"""Moteurs d'Aria - Studio (ComfyUI portable, llama-server) aux versions épinglées de catalogue/moteurs.yaml.

    python -m app.moteurs                           état des moteurs et de la carte graphique
    python -m app.moteurs installer [id ...]        installe (défaut : les moteurs obligatoires absents)
    python -m app.moteurs revenir <id>              remet la version précédente (un second appel la redonne)
Options : --archives DOSSIER   où chercher et ranger les archives (défaut : moteurs\\_archives)
          --dans DOSSIER       installer ailleurs que dans le dossier des moteurs (essai)

Installer un moteur : l'espace disque est vérifié ; chaque archive est reprise du dossier des archives si son
SHA-256 est le bon, sinon téléchargée depuis la release GitHub du catalogue (un .part interrompu reprend où il
s'était arrêté), puis vérifiée ; tout est extrait dans <dossier>.new, et l'échange se fait d'un coup : la version
en place devient <dossier>.prev. Refusé tant que le moteur répond sur son port. Les adresses ne viennent que du
catalogue, et rien n'est exécuté pendant l'installation.
"""

import asyncio
import contextlib
import hashlib
import json
import os
import shutil
import socket
import subprocess
import sys
import zipfile
from datetime import datetime
from pathlib import Path

import httpx
import yaml

from app.config import CONFIG, ROOT
from app.emplacements import MOTEURS

CATALOGUE = ROOT / "catalogue" / "moteurs.yaml"
FICHE = ".aria-moteur.json"  # dans le dossier d'un moteur installé par Aria : id, version, archives
BLOC = 1024 * 1024
TAR = Path(os.environ.get("WINDIR", r"C:\Windows")) / "System32" / "tar.exe"  # bsdtar : lit aussi le 7z
SEPT_ZIP = Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "7-Zip" / "7z.exe"
EXPANSION = 3  # place à prévoir une fois extrait, par octet d'archive (mesuré : ComfyUI ×2,3, llama.cpp ×1,3)


class ErreurMoteur(RuntimeError):
    pass


def catalogue() -> dict:
    return yaml.safe_load(CATALOGUE.read_text(encoding="utf-8"))


def _moteur(id_moteur: str) -> dict:
    moteurs = catalogue()["moteurs"]
    if id_moteur not in moteurs:
        raise ErreurMoteur(f"Moteur inconnu : {id_moteur} (connus : {', '.join(moteurs)})")
    return moteurs[id_moteur]


def _fiche(dossier: Path) -> dict | None:
    try:
        return json.loads((dossier / FICHE).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def _archives_uniques(ids) -> list[dict]:
    vues = {}
    for i in ids:
        for a in _moteur(i)["archives"]:
            vues.setdefault(a["sha256"], a)
    return list(vues.values())


def _deja_la(archive: dict, dossier_archives: Path) -> bool:
    """Présente avec la bonne taille (le SHA-256 est vérifié au moment de s'en servir)."""
    f = dossier_archives / archive["fichier"]
    return f.is_file() and f.stat().st_size == archive["taille"]


def etat(dans: Path = MOTEURS, dossier_archives: Path | None = None) -> list[dict]:
    """Chaque moteur du catalogue : absent, à jour (version épinglée), différent, ou installé à la main."""
    dossier_archives = dossier_archives or dans / "_archives"
    sortie = []
    for id_moteur, m in catalogue()["moteurs"].items():
        dossier = dans / m["dossier"]
        fiche = _fiche(dossier)
        if not (dossier / m["preuve"]).is_file():
            statut = "absent"
        elif fiche is None:
            statut = "manuel"  # déposé à la main : version inconnue
        else:
            statut = "a_jour" if fiche.get("version") == m["version"] else "different"
        sortie.append({
            "id": id_moteur, "nom": m["nom"], "role": m["role"], "facultatif": bool(m.get("facultatif")),
            "dossier": m["dossier"],
            "version": m["version"], "version_installee": (fiche or {}).get("version"), "statut": statut,
            "chemin": str(dossier), "precedente": dossier.with_name(dossier.name + ".prev").is_dir(),
            "a_telecharger": sum(a["taille"] for a in m["archives"] if not _deja_la(a, dossier_archives)),
            "occupe": occupe(id_moteur),
        })
    return sortie


def carte() -> dict:
    """La carte graphique et son pilote face à ce que demandent les moteurs (CUDA 13 : branche 580 au moins)."""
    minimum = catalogue().get("pilote_min")
    try:
        import pynvml

        pynvml.nvmlInit()
        try:
            nom = pynvml.nvmlDeviceGetName(pynvml.nvmlDeviceGetHandleByIndex(0))
            pilote = pynvml.nvmlSystemGetDriverVersion()
        finally:
            pynvml.nvmlShutdown()
    except Exception:  # pas de carte NVIDIA, ou pas de pilote : NVML ne se charge pas
        return {"nvidia": False, "nom": None, "pilote": None, "ok": False,
                "message": "Aucune carte NVIDIA détectée : les moteurs d'Aria demandent une carte NVIDIA (CUDA 13)."}
    nom, pilote = (x.decode() if isinstance(x, bytes) else x for x in (nom, pilote))
    ok = minimum is None or int(pilote.split(".")[0]) >= minimum
    return {"nvidia": True, "nom": nom, "pilote": pilote, "ok": ok,
            "message": None if ok else f"Pilote {pilote} trop ancien : installe un pilote NVIDIA {minimum} ou plus récent."}


def occupe(id_moteur: str) -> bool:
    port = CONFIG[_moteur(id_moteur)["port"]]["port"]
    with socket.socket() as s:
        s.settimeout(0.3)
        return s.connect_ex(("127.0.0.1", port)) == 0


def _sha256(fichier: Path) -> str:
    h = hashlib.sha256()
    with fichier.open("rb") as f:
        while bloc := f.read(BLOC):
            h.update(bloc)
    return h.hexdigest()


def _url(archive: dict) -> str:
    return f"https://github.com/{archive['depot']}/releases/download/{archive['tag']}/{archive['fichier']}"


async def _telecharger(archive: dict, part: Path, progression=None):
    """Télécharge dans le .part, en reprenant là où il s'était arrêté."""
    deja = part.stat().st_size if part.exists() else 0
    entetes = {"User-Agent": "Aria-Studio", **({"Range": f"bytes={deja}-"} if deja else {})}
    async with httpx.AsyncClient(follow_redirects=True, timeout=httpx.Timeout(30, read=120)) as client:
        async with client.stream("GET", _url(archive), headers=entetes) as r:
            if r.status_code == 200:
                deja = 0  # le serveur repart du début
            elif r.status_code != 206:
                raise ErreurMoteur(f"GitHub a répondu {r.status_code} pour {archive['fichier']}")
            with part.open("ab" if deja else "wb") as f:
                async for bloc in r.aiter_bytes():  # tels qu'ils arrivent : la progression suit le réseau
                    f.write(bloc)
                    deja += len(bloc)
                    if progression:
                        progression(f"Téléchargement de {archive['fichier']}", deja, archive["taille"])


async def obtenir(archive: dict, dossier_archives: Path, progression=None) -> Path:
    """L'archive vérifiée : reprise du dossier des archives si elle y est intacte, sinon téléchargée."""
    cible = dossier_archives / archive["fichier"]
    if _deja_la(archive, dossier_archives):
        if progression:
            progression(f"Vérification de {archive['fichier']}", 0, archive["taille"])
        if await asyncio.to_thread(_sha256, cible) == archive["sha256"]:
            return cible
        cible.unlink()  # même taille, mauvais contenu : on la remplace
    dossier_archives.mkdir(parents=True, exist_ok=True)
    part = cible.with_name(cible.name + ".part")
    await _telecharger(archive, part, progression)
    obtenu = await asyncio.to_thread(_sha256, part)
    if obtenu != archive["sha256"]:
        part.unlink()
        raise ErreurMoteur(f"SHA-256 de {archive['fichier']} invalide ({obtenu}) : fichier supprimé, rien n'est installé")
    part.replace(cible)
    return cible


def _extraire(archive: Path, vers: Path):
    vers.mkdir(parents=True, exist_ok=True)
    if archive.suffix.lower() == ".zip":
        racine = vers.resolve()
        with zipfile.ZipFile(archive) as z:
            for nom in z.namelist():
                if racine not in (racine / nom).resolve().parents and (racine / nom).resolve() != racine:
                    raise ErreurMoteur(f"{archive.name} contient un chemin dangereux : {nom}")
            z.extractall(vers)
        return
    # .7z : le tar de Windows (libarchive refuse les chemins absolus et « .. »), sinon 7-Zip
    for commande in ([str(TAR), "-xf", str(archive), "-C", str(vers)], [str(SEPT_ZIP), "x", str(archive), f"-o{vers}", "-y"]):
        if Path(commande[0]).is_file():
            r = subprocess.run(commande, capture_output=True, text=True, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            if r.returncode == 0:
                return
            erreur = (r.stderr or r.stdout).strip().splitlines()[-1:] or [f"code {r.returncode}"]
            raise ErreurMoteur(f"Extraction de {archive.name} impossible : {erreur[0]}")
    raise ErreurMoteur(f"Pour extraire {archive.name}, il faut le tar de Windows 11 ou 7-Zip")


def _verser(source: Path, cible: Path):
    """Déplace le contenu de source dans cible (le runtime CUDA rejoint llama-server)."""
    cible.mkdir(parents=True, exist_ok=True)
    for element in source.iterdir():
        destination = cible / element.name
        if destination.is_dir() and element.is_dir():
            _verser(element, destination)
        else:
            if destination.exists():
                destination.unlink()
            element.rename(destination)


def _echanger(dossier: Path, nouveau: Path):
    precedent = dossier.with_name(dossier.name + ".prev")
    if precedent.exists():
        shutil.rmtree(precedent)
    if dossier.exists():
        try:
            dossier.rename(precedent)
        except OSError as e:
            raise ErreurMoteur(f"{dossier.name} est en cours d'utilisation (un fichier y est ouvert) : rien n'a changé") from e
    nouveau.rename(dossier)


def place_necessaire(id_moteur: str, dossier_archives: Path) -> int:
    archives = _moteur(id_moteur)["archives"]
    return (sum(a["taille"] for a in archives if not _deja_la(a, dossier_archives))
            + EXPANSION * sum(a["taille"] for a in archives))


async def installer(id_moteur: str, dans: Path = MOTEURS, dossier_archives: Path | None = None, progression=None,
                    verrou=None) -> dict:
    """`verrou` : fabrique d'un contexte asynchrone qui arrête le moteur et le garde arrêté le temps de l'échange
    (l'orchestrateur, depuis l'interface) ; sans lui, un moteur qui tourne est refusé d'emblée."""
    m = _moteur(id_moteur)
    dossier_archives = dossier_archives or dans / "_archives"
    if verrou is None and occupe(id_moteur):
        raise ErreurMoteur(f"{m['nom']} tourne : arrête-le avant de l'installer")
    dans.mkdir(parents=True, exist_ok=True)
    libre, besoin = shutil.disk_usage(dans).free, place_necessaire(id_moteur, dossier_archives)
    if libre < besoin:
        raise ErreurMoteur(f"Place insuffisante : {besoin / 1024 ** 3:.1f} Go nécessaires, {libre / 1024 ** 3:.1f} Go libres")

    fichiers = [await obtenir(a, dossier_archives, progression) for a in m["archives"]]
    dossier = dans / m["dossier"]
    nouveau, extrait = dossier.with_name(dossier.name + ".new"), dossier.with_name(dossier.name + ".extrait")
    for reste in (nouveau, extrait):
        if reste.exists():
            shutil.rmtree(reste)  # une installation précédente interrompue
    try:
        for archive, fichier in zip(m["archives"], fichiers):
            if progression:
                progression(f"Extraction de {fichier.name}", 0, 0)
            await asyncio.to_thread(_extraire, fichier, extrait)
            source = extrait / archive["racine"] if archive.get("racine") else extrait
            if not source.is_dir():
                raise ErreurMoteur(f"{fichier.name} ne contient pas le dossier {archive['racine']}")
            _verser(source, nouveau)
            shutil.rmtree(extrait)
        if not (nouveau / m["preuve"]).is_file():
            raise ErreurMoteur(f"{m['preuve']} absent après extraction : l'archive ne ressemble pas à {m['nom']}")
        fiche = {"id": id_moteur, "version": m["version"], "installe_le": datetime.now().isoformat(timespec="seconds"),
                 "archives": {a["fichier"]: a["sha256"] for a in m["archives"]}}
        (nouveau / FICHE).write_text(json.dumps(fiche, indent=4, ensure_ascii=False), encoding="utf-8")
        if progression:
            progression("Mise en place (attend la fin d'une génération en cours)", 0, 0)
        async with verrou() if verrou else contextlib.nullcontext():
            if occupe(id_moteur):
                raise ErreurMoteur(f"{m['nom']} tourne encore (lancé par une autre appli ?) : rien n'a changé")
            _echanger(dossier, nouveau)
    finally:
        for reste in (nouveau, extrait):
            if reste.exists():
                shutil.rmtree(reste, ignore_errors=True)
    return fiche


def revenir(id_moteur: str, dans: Path = MOTEURS) -> dict | None:
    """Échange la version en place et la précédente ; renvoie la fiche de celle qui revient (None : posée à la main)."""
    m = _moteur(id_moteur)
    dossier = dans / m["dossier"]
    precedent = dossier.with_name(dossier.name + ".prev")
    if not precedent.is_dir():
        raise ErreurMoteur(f"Pas de version précédente de {m['nom']}")
    if occupe(id_moteur):
        raise ErreurMoteur(f"{m['nom']} tourne : arrête-le avant")
    temporaire = dossier.with_name(dossier.name + ".echange")
    try:
        dossier.rename(temporaire)
        precedent.rename(dossier)
        temporaire.rename(precedent)
    except OSError as e:
        if temporaire.exists() and not dossier.exists():
            temporaire.rename(dossier)
        raise ErreurMoteur(f"{m['nom']} est en cours d'utilisation : rien n'a changé") from e
    global _tache
    if _tache and _tache["fini"] and _tache["id"] == id_moteur:
        _tache = None  # « Installé ✓ » ne vaut plus pour la version qui revient
    return _fiche(dossier)


# --- installation en tâche de fond (interface) ----------------------------------------

_tache: dict | None = None  # la dernière installation : id, nom, etape, fait, total, erreur, fini
_execution: asyncio.Task | None = None


def tache() -> dict | None:
    return dict(_tache) if _tache else None


def lancer(id_moteur: str, verrou=None, dans: Path = MOTEURS) -> dict:
    """Démarre l'installation d'un moteur en arrière-plan ; une seule à la fois."""
    global _tache, _execution
    if _execution and not _execution.done():
        raise ErreurMoteur(f"Une installation est déjà en cours ({_tache['nom']})")
    m = _moteur(id_moteur)
    _tache = {"id": id_moteur, "nom": m["nom"], "etape": "Préparation", "fait": 0, "total": 0, "erreur": None, "fini": False}
    suivi = _tache

    def progression(etape, fait, total):
        suivi.update(etape=etape, fait=fait, total=total)

    async def executer():
        try:
            await installer(id_moteur, dans, progression=progression, verrou=verrou)
            suivi.update(etape="Installé ✓", fait=0, total=0)
        except asyncio.CancelledError:
            suivi["erreur"] = "Annulé : le téléchargement reprendra où il s'est arrêté"
        except (ErreurMoteur, OSError, httpx.HTTPError) as e:
            suivi["erreur"] = str(e) or type(e).__name__
        finally:
            suivi["fini"] = True

    _execution = asyncio.create_task(executer())
    return tache()


def annuler() -> bool:
    """Seul un téléchargement s'annule (une extraction en cours va au bout)."""
    if _execution and not _execution.done() and _tache["etape"].startswith("Téléchargement"):
        _execution.cancel()
        return True
    return False


# --- ligne de commande ---------------------------------------------------------------

STATUTS = {"absent": "absent", "a_jour": "à jour", "different": "autre version", "manuel": "installé à la main"}


def _afficher_progression():
    dernier = {"texte": None}

    def afficher(texte, fait, total):
        if texte != dernier["texte"]:
            print(("\n" if dernier["texte"] else "") + f"  {texte}", end="", flush=True)
            dernier["texte"] = texte
        if total:
            print(f"\r  {texte} : {fait * 100 // total} %", end="", flush=True)
    return afficher


def main(args: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    options = {k: Path(args[args.index(k) + 1]) for k in ("--archives", "--dans") if k in args and args.index(k) + 1 < len(args)}
    mots = [a for i, a in enumerate(args) if not a.startswith("--") and (i == 0 or args[i - 1] not in ("--archives", "--dans"))]
    dans = options.get("--dans", MOTEURS)
    archives = options.get("--archives", dans / "_archives")
    try:
        if mots[:1] == ["installer"]:
            ids = mots[1:] or [e["id"] for e in etat(dans, archives) if e["statut"] == "absent" and not e["facultatif"]]
            if not ids:
                print("Rien à installer : les moteurs obligatoires sont là.")
            for i in ids:
                print(f"{_moteur(i)['nom']} {_moteur(i)['version']} → {dans / _moteur(i)['dossier']}")
                asyncio.run(installer(i, dans, archives, _afficher_progression()))
                print("\n  installé ✓")
            return 0
        if mots[:1] == ["revenir"] and len(mots) == 2:
            fiche = revenir(mots[1], dans)
            print(f"Version remise en place : {(fiche or {}).get('version', 'posée à la main')}")
            return 0
        if mots:
            print(__doc__)
            return 1
    except ErreurMoteur as e:
        print(f"\n  ✗ {e}")
        return 1
    c = carte()
    print(f"Carte : {c['nom'] or 'aucune carte NVIDIA'}" + (f" · pilote {c['pilote']}" if c["pilote"] else "")
          + ("" if c["ok"] else f"\n  ✗ {c['message']}"))
    for e in etat(dans, archives):
        print(f"{e['nom']:<32} {STATUTS[e['statut']]:<20} épinglé {e['version']}"
              + (f" · {e['a_telecharger'] / 1024 ** 2:.0f} Mo à télécharger" if e["statut"] != "a_jour" and e["a_telecharger"] else "")
              + (" · facultatif" if e["facultatif"] else "") + (" · tourne" if e["occupe"] else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
