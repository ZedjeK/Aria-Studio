"""Migration des modèles vers deux côtés indépendants, image et vidéo (2026-11-10), et retour arrière.

Avant : modeles/_base/<type> commun à tous, et modeles/<Famille>/{modeles,loras}. Après : modeles/image/… et
modeles/video/…, chacun avec son _base et ses familles (app/modeles.py).

    python -m app.migration_modeles            essai à blanc : ce qui bougerait
    python -m app.migration_modeles --go       migre (Aria le fait seule au démarrage)
    python -m app.migration_modeles --retour   remet l'ancienne organisation (essai à blanc, --go pour le faire)

Chaque fichier est renommé, sans copie (même disque : instantané). Les clés des fichiers de data/ qui citent un chemin
(registre, empreintes, réglages des LoRA) sont réécrites, une copie « .avant-migration » de chacun gardée. Si un
renommage échoue (fichier ouvert par un ComfyUI resté lancé…), ceux déjà faits sont défaits : l'ancienne organisation
reste en place, Aria le dit, et réessaie au démarrage suivant. Un fichier de _base qu'aucune fiche ne cite (upscaler,
controlnet déposés à la main…) va côté image et figure dans le rapport (data/migration_modeles.json).
"""

import json
import logging
import sys
from datetime import datetime
from pathlib import Path

import yaml

from app import modeles
from app.config import ROOT

log = logging.getLogger(__name__)
SUFFIXE_SAUVEGARDE = ".avant-migration"


def _journal() -> Path:
    return modeles.REGISTRE.parent / "migration_modeles.json"


def _fichiers_data() -> list[Path]:
    """Les fichiers de data/ dont les clés sont des chemins sous modeles/."""
    return [modeles.REGISTRE, modeles.EMPREINTES, modeles.MEMOIRE_LORAS]


def _natures_des_fiches() -> dict[tuple[str, str], str]:
    """(type de _base, nom du fichier) → nature, d'après les fiches de workflow (déjà au nouveau format)."""
    connus = {}
    for chemin in sorted((ROOT / "workflows").glob("*/fiche.yaml")):
        for f in (yaml.safe_load(chemin.read_text(encoding="utf-8")) or {}).get("fichiers", []):
            nature, famille, sous = modeles.decouper(f["dossier"])
            if nature and famille is None:
                connus[(sous, f["nom"])] = nature
    return connus


def plan(racine: Path | None = None) -> dict:
    """{"deplacements": [(ancien, nouveau)] en chemins relatifs posix, "a_trier": fichiers de _base envoyés côté image
    faute de fiche qui les cite, "conflits": fichiers déjà présents à l'arrivée}. Vide si rien n'est à l'ancienne."""
    racine = racine or modeles.DOSSIER
    deplacements, a_trier, conflits = [], [], []
    natures = _natures_des_fiches()

    def ajouter(fichier: Path, nouveau: str):
        ancien = fichier.relative_to(racine).as_posix()
        (conflits if (racine / nouveau).exists() else deplacements).append((ancien, nouveau))

    base = racine / modeles.BASE
    if base.is_dir():
        for fichier in sorted(p for p in base.rglob("*") if p.is_file()):
            relatif = fichier.relative_to(base)
            type_ = relatif.parts[0] if len(relatif.parts) > 1 else ""
            nature = natures.get((type_, fichier.name))
            if nature is None:
                nature = "image"
                a_trier.append(f"image/{modeles.BASE}/{relatif.as_posix()}")  # là où il arrive
            ajouter(fichier, f"{nature}/{modeles.BASE}/{relatif.as_posix()}")
    for famille in modeles.familles():
        dossier = racine / famille
        if dossier.is_dir():
            for fichier in sorted(p for p in dossier.rglob("*") if p.is_file()):
                ajouter(fichier, f"{modeles.nature(famille)}/{fichier.relative_to(racine).as_posix()}")
    return {"deplacements": deplacements, "a_trier": a_trier, "conflits": conflits}


def _renommer(paires: list[tuple[Path, Path]]) -> list[tuple[Path, Path]]:
    """Renomme dans l'ordre ; au premier échec, défait ce qui a été fait et relance l'erreur."""
    faits = []
    try:
        for ancien, nouveau in paires:
            nouveau.parent.mkdir(parents=True, exist_ok=True)
            ancien.rename(nouveau)
            faits.append((ancien, nouveau))
    except OSError:
        for ancien, nouveau in reversed(faits):
            nouveau.rename(ancien)
        raise
    return faits


def _supprimer_dossiers_vides(dossier: Path):
    """Les dossiers vidés par la migration (jamais un fichier : rmdir refuse un dossier qui n'est pas vide)."""
    if not dossier.is_dir():
        return
    for d in sorted((p for p in dossier.rglob("*") if p.is_dir()), key=lambda p: len(p.parts), reverse=True):
        try:
            d.rmdir()
        except OSError:
            pass
    try:
        dossier.rmdir()
    except OSError:
        pass


def _reecrire_cles(correspondance: dict[str, str]):
    for fichier in _fichiers_data():
        if not fichier.is_file():
            continue
        donnees = json.loads(fichier.read_text(encoding="utf-8"))
        if not isinstance(donnees, dict) or not any(c in correspondance for c in donnees):
            continue
        sauvegarde = fichier.with_name(fichier.name + SUFFIXE_SAUVEGARDE)
        if not sauvegarde.exists():
            sauvegarde.write_bytes(fichier.read_bytes())
        nouvelles = {correspondance.get(c, c): v for c, v in donnees.items()}
        temporaire = fichier.with_suffix(fichier.suffix + ".tmp")
        temporaire.write_text(json.dumps(nouvelles, indent=4, ensure_ascii=False), encoding="utf-8")
        temporaire.replace(fichier)


def _ecrire_journal(rapport: dict):
    journal = _journal()
    journal.parent.mkdir(parents=True, exist_ok=True)
    journal.write_text(json.dumps(rapport, indent=4, ensure_ascii=False), encoding="utf-8")


def etat() -> dict | None:
    """Le dernier rapport (onglet Modèles), None si aucune migration n'a eu lieu."""
    try:
        return json.loads(_journal().read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def migrer(racine: Path | None = None) -> dict | None:
    """Migre si l'ancienne organisation est là ; None s'il n'y a rien à faire. Ne lève jamais : un échec est rapporté
    (et journalisé) et l'ancienne organisation reste en place."""
    racine = racine or modeles.DOSSIER
    p = plan(racine)
    if not p["deplacements"]:  # rien, ou seulement des fichiers déjà signalés laissés en place
        return None
    rapport = {"date": datetime.now().isoformat(timespec="seconds"), "deplacements": [], "a_trier": p["a_trier"],
               "conflits": [a for a, _ in p["conflits"]], "erreur": None}
    try:
        faits = _renommer([(racine / a, racine / n) for a, n in p["deplacements"]])
    except OSError as e:
        rapport.update(statut="echec", erreur=f"{type(e).__name__} : {e}")
        log.error("Migration des modèles annulée, rien n'a bougé : %s", rapport["erreur"])
        _ecrire_journal(rapport)
        return rapport
    _reecrire_cles(dict(p["deplacements"]))
    for ancien in [modeles.BASE, *modeles.familles()]:
        _supprimer_dossiers_vides(racine / ancien)
    rapport.update(statut="faite", deplacements=[[a.relative_to(racine).as_posix(), n.relative_to(racine).as_posix()]
                                                 for a, n in faits])
    if rapport["conflits"]:
        log.warning("Migration des modèles : déjà présents à l'arrivée, laissés en place : %s",
                    ", ".join(rapport["conflits"]))
    log.info("Migration des modèles : %d fichiers rangés côté image ou vidéo", len(faits))
    precedent = etat()
    if precedent and precedent.get("statut") == "faite":  # une seconde vague : le retour arrière défait les deux
        rapport["deplacements"] = precedent["deplacements"] + rapport["deplacements"]
        rapport["a_trier"] = precedent.get("a_trier", []) + rapport["a_trier"]
    _ecrire_journal(rapport)
    return rapport


def retour(racine: Path | None = None, essai: bool = True) -> list[tuple[str, str]]:
    """Remet l'ancienne organisation d'après le journal, et les fichiers de data/ d'avant la migration."""
    racine = racine or modeles.DOSSIER
    rapport = etat()
    if not rapport or rapport.get("statut") != "faite":
        raise RuntimeError("Aucune migration faite à défaire (data/migration_modeles.json)")
    paires = [(n, a) for a, n in rapport["deplacements"] if (racine / n).is_file() and not (racine / a).exists()]
    if essai:
        return paires
    _renommer([(racine / n, racine / a) for n, a in paires])
    for fichier in _fichiers_data():
        sauvegarde = fichier.with_name(fichier.name + SUFFIXE_SAUVEGARDE)
        if sauvegarde.is_file():
            sauvegarde.replace(fichier)
    rapport["statut"] = "defaite"
    _ecrire_journal(rapport)
    return paires


def main(args: list[str]) -> int:
    go = "--go" in args
    if "--retour" in args:
        try:
            paires = retour(essai=not go)
        except RuntimeError as e:
            print(e)
            return 1
        for n, a in paires:
            print(f"{n}  →  {a}")
        print(f"{len(paires)} fichiers {'remis' if go else 'à remettre (essai à blanc : --go pour le faire)'}")
        return 0
    if not go:
        p = plan()
        for a, n in p["deplacements"]:
            print(f"{a}  →  {n}")
        for a, _ in p["conflits"]:
            print(f"{a}  ✗ déjà présent à l'arrivée, laissé en place")
        print(f"{len(p['deplacements'])} fichiers à ranger (essai à blanc : --go pour le faire)")
        return 0
    rapport = migrer()
    if rapport is None:
        print("Rien à migrer : les modèles sont déjà rangés côté image et vidéo.")
        return 0
    print(f"Migration {rapport['statut']} : {len(rapport['deplacements'])} fichiers"
          + (f" · {rapport['erreur']}" if rapport["erreur"] else ""))
    return 0 if rapport["statut"] == "faite" else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
