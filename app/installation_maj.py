"""Mises à jour intégrées (contrat des applis Aria) : sur un clic, télécharger la dernière release, la vérifier et
l'installer en gardant l'ancienne version. La recherche quotidienne (app/mises_a_jour.py) ne fait que lire.

1. Le clic vaut accord (après une confirmation dans l'interface) : rien ne se télécharge ni ne s'installe tout seul.
2. La dernière release est relue sur api.github.com ; le zip « Aria-Studio-X.Y.Z.zip » et son « .sha256 » sont
   téléchargés en HTTPS depuis ses seules adresses. Les redirections ne sont suivies que vers les hôtes de GitHub (HOTES),
   à la main, une par une. Le zip va dans cache/mise-a-jour/, avec reprise si la connexion coupe.
3. Double vérification : la taille annoncée, et le SHA-256 du zip comparé à l'empreinte que GitHub calcule lui-même
   (« digest » de l'API) et au fichier .sha256 publié avec la release. Les deux doivent concorder.
4. Décompression dans cache/mise-a-jour/Aria-Studio-X.Y.Z/, sans chemin qui sorte du dossier ; il doit contenir
   Aria-Studio.exe et annoncer la version attendue dans app/__init__.py.
5. Relais : Aria lance le NOUVEAU lanceur depuis ce dossier (--mise-a-jour <pid> <ancien lanceur>), puis s'arrête.
   Le lanceur attend la fin du serveur, installe comme « Installer » (programme.precedent gardé) et relance Aria ; en
   cas d'échec, il relance l'ancien lanceur (tools/launcher/Launcher.cs).
   En mode portable, rien n'est déplacé : la nouvelle version est copiée à côté du dossier actuel et l'Explorateur
   s'ouvre dessus ; l'utilisateur y recopie ses affaires.
6. Au démarrage suivant, bilan_au_demarrage() note au journal si la version attendue tourne, puis vide
   cache/mise-a-jour/.

Les empreintes prouvent l'intégrité, pas la provenance : celle-ci repose sur le compte GitHub de l'auteur, les releases
immuables et l'attestation jointe (dit dans le README, « Limites à connaître »).
"""

import asyncio
import hashlib
import json
import logging
import os
import re
import shutil
import subprocess
import zipfile
from datetime import datetime
from pathlib import Path
from urllib.parse import urljoin, urlparse

import httpx

from app import VERSION, journal, mises_a_jour
from app.emplacements import CACHE, ROOT, portable

DOSSIER = CACHE / "mise-a-jour"
RELAIS = DOSSIER / "relais.json"
EXE = "Aria-Studio.exe"
HOTES = frozenset({"api.github.com", "github.com", "release-assets.githubusercontent.com",
                   "objects.githubusercontent.com"})
REDIRECTIONS_MAX = 5
TAILLE_SHA256_MAX = 1024
ETAPES_ACTIVES = ("recherche", "telechargement", "verification", "installation", "redemarrage")
log = logging.getLogger(__name__)


class ErreurMiseAJour(RuntimeError):
    pass


# --- essais ------------------------------------------------------------------------------------------------------

def _essai_github() -> str | None:
    """Un faux GitHub sur http://127.0.0.1, seulement pendant les essais (tools/tester.py pose ARIA_TESTS_ISOLES) :
    jamais pris en compte autrement."""
    if not os.environ.get("ARIA_TESTS_ISOLES"):
        return None
    url = os.environ.get("ARIA_ESSAI_GITHUB") or ""
    return url.rstrip("/") if urlparse(url).scheme == "http" and urlparse(url).hostname == "127.0.0.1" else None


def _api() -> str:
    return _essai_github() or mises_a_jour.API


def adresse_permise(url: str) -> bool:
    u = urlparse(url)
    if _essai_github() and u.scheme == "http" and u.hostname == "127.0.0.1":
        return True
    return u.scheme == "https" and u.hostname in HOTES


# --- état --------------------------------------------------------------------------------------------------------

_etat: dict = {"etape": "repos"}


def installable() -> bool:
    """Un lanceur à côté du code (zip, programme\\ ou portable) ; pas en mode développeur (run.ps1)."""
    return (ROOT / EXE).is_file()


def etat() -> dict:
    """Ce que l'interface suit pendant une mise à jour (relu chaque seconde)."""
    return {**_etat, "possible": installable(), "portable": portable()}


def _changer(**valeurs):
    _etat.update(valeurs)


def commencer():
    """Avant de lancer installer() en tâche de fond : la réponse au clic dit déjà qu'elle a commencé."""
    _changer(etape="recherche", erreur=None, recu=0, total=0, version=None, chemin=None)


# --- téléchargement ----------------------------------------------------------------------------------------------

async def _ouvrir(client: httpx.AsyncClient, url: str, entetes: dict | None = None) -> httpx.Response:
    """GET en flux, redirections suivies à la main : chaque adresse doit être celle de GitHub, en HTTPS."""
    for _ in range(REDIRECTIONS_MAX + 1):
        if not adresse_permise(url):
            raise ErreurMiseAJour(f"Adresse refusée : {urlparse(url).hostname or url} n'est pas un hôte de GitHub")
        reponse = await client.send(client.build_request("GET", url, headers=entetes or {}), stream=True,
                                    follow_redirects=False)
        if reponse.is_redirect:
            suivante = urljoin(url, reponse.headers.get("location", ""))
            await reponse.aclose()
            url = suivante
            continue
        return reponse
    raise ErreurMiseAJour("Trop de redirections")


async def _lire_json(client, url) -> dict:
    r = await _ouvrir(client, url)
    try:
        if r.status_code != 200:
            raise ErreurMiseAJour(f"GitHub a répondu {r.status_code}")
        return json.loads(await r.aread())
    finally:
        await r.aclose()


async def _lire_petit(client, url, limite: int) -> bytes:
    r = await _ouvrir(client, url)
    try:
        if r.status_code != 200:
            raise ErreurMiseAJour(f"GitHub a répondu {r.status_code} pour {url.rsplit('/', 1)[-1]}")
        corps = b""
        async for morceau in r.aiter_bytes():
            corps += morceau
            if len(corps) > limite:
                raise ErreurMiseAJour("Fichier d'empreinte trop long")
        return corps
    finally:
        await r.aclose()


async def _telecharger(client, url: str, cible: Path, taille: int):
    """Vers cible.part, repris là où il s'était arrêté (Range) ; renommé en cible quand la taille est atteinte."""
    part = cible.with_name(cible.name + ".part")
    deja = part.stat().st_size if part.is_file() else 0
    if deja > taille:
        part.unlink()
        deja = 0
    entetes = {"Range": f"bytes={deja}-"} if deja else {}
    r = await _ouvrir(client, url, entetes)
    try:
        if r.status_code == 200:
            deja = 0  # la reprise n'est pas acceptée : on repart du début
        elif r.status_code != 206 or not deja:
            raise ErreurMiseAJour(f"GitHub a répondu {r.status_code} pour {cible.name}")
        _changer(recu=deja)
        with open(part, "ab" if deja else "wb") as f:
            async for morceau in r.aiter_bytes(1024 * 1024):
                f.write(morceau)
                deja += len(morceau)
                if deja > taille:
                    raise ErreurMiseAJour(f"{cible.name} dépasse la taille annoncée")
                _changer(recu=deja)
    finally:
        await r.aclose()
    if deja != taille:
        raise ErreurMiseAJour(f"{cible.name} incomplet : {deja} octets sur {taille}")
    part.replace(cible)


def _sha256(fichier: Path) -> str:
    h = hashlib.sha256()
    with open(fichier, "rb") as f:
        for bloc in iter(lambda: f.read(1024 * 1024), b""):
            h.update(bloc)
    return h.hexdigest()


def _extraire(archive: Path, dossier: Path, version: str):
    """Dans `dossier`, depuis la racine « Aria-Studio-X.Y.Z/ » du zip, sans rien écrire hors de lui."""
    racine = f"Aria-Studio-{version}/"
    if dossier.exists():
        shutil.rmtree(dossier)
    dossier.mkdir(parents=True)
    base = dossier.resolve()
    with zipfile.ZipFile(archive) as z:
        for membre in z.infolist():
            nom = membre.filename.replace("\\", "/")
            if not nom.startswith(racine):
                raise ErreurMiseAJour(f"Le zip contient un chemin inattendu : {nom}")
            cible = (dossier / nom[len(racine):]).resolve()
            if cible != base and not cible.is_relative_to(base):
                raise ErreurMiseAJour(f"Le zip contient un chemin qui sort de son dossier : {nom}")
            if membre.is_dir():
                cible.mkdir(parents=True, exist_ok=True)
                continue
            cible.parent.mkdir(parents=True, exist_ok=True)
            with z.open(membre) as source, open(cible, "wb") as destination:
                shutil.copyfileobj(source, destination)
    if not (dossier / EXE).is_file():
        raise ErreurMiseAJour(f"Le zip ne contient pas {EXE}")
    init = dossier / "app" / "__init__.py"
    annoncee = re.search(r'^VERSION\s*=\s*"([^"]+)"', init.read_text(encoding="utf-8"), re.M) if init.is_file() else None
    if not annoncee or annoncee.group(1) != version:
        raise ErreurMiseAJour(f"Le zip n'annonce pas la version {version}")


async def preparer() -> Path:
    """Télécharge, vérifie et décompresse la dernière release ; rend le dossier décompressé."""
    commencer()
    entetes = {"Accept": "application/vnd.github+json", "User-Agent": f"Aria-Studio/{VERSION}"}
    async with httpx.AsyncClient(timeout=httpx.Timeout(30, read=60), headers=entetes) as client:
        release = await _lire_json(client, f"{_api()}/repos/{mises_a_jour.depot()}/releases/latest")
        version = (release.get("tag_name") or "").lstrip("v")
        if mises_a_jour.numero(version) is None or not mises_a_jour.plus_recente(version):
            raise ErreurMiseAJour(f"Pas de version plus récente que la {VERSION} sur GitHub")
        fichiers = {a.get("name"): a for a in release.get("assets") or []}
        nom_zip = f"Aria-Studio-{version}.zip"
        zip_, somme = fichiers.get(nom_zip), fichiers.get(f"{nom_zip}.sha256")
        if not zip_ or not somme:
            raise ErreurMiseAJour(f"La release {version} ne contient pas {nom_zip} et son .sha256")
        digest = str(zip_.get("digest") or "")
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            raise ErreurMiseAJour("GitHub ne donne pas l'empreinte SHA-256 du zip : rien n'est installé")
        attendu = digest.split(":", 1)[1]
        ligne = (await _lire_petit(client, somme["browser_download_url"], TAILLE_SHA256_MAX)).decode("ascii", "replace")
        morceaux = ligne.split()
        if len(morceaux) < 2 or morceaux[1].lstrip("*") != nom_zip or morceaux[0].lower() != attendu:
            raise ErreurMiseAJour("Les deux empreintes (GitHub et .sha256 de la release) ne concordent pas")
        taille = int(zip_.get("size") or 0)
        DOSSIER.mkdir(parents=True, exist_ok=True)
        archive = DOSSIER / nom_zip
        _changer(etape="telechargement", version=version, total=taille)
        if not (archive.is_file() and archive.stat().st_size == taille):
            await _telecharger(client, zip_["browser_download_url"], archive, taille)
    _changer(etape="verification")
    if await asyncio.to_thread(_sha256, archive) != attendu:
        archive.unlink(missing_ok=True)
        raise ErreurMiseAJour("Empreinte SHA-256 incorrecte : le zip téléchargé est rejeté")
    dossier = DOSSIER / f"Aria-Studio-{version}"
    await asyncio.to_thread(_extraire, archive, dossier, version)
    journal.noter("Mise à jour téléchargée et vérifiée", f"{VERSION} → {version}")
    return dossier


# --- installation ------------------------------------------------------------------------------------------------

def _relayer(dossier: Path, version: str):
    """Lance le nouveau lanceur, qui attend la fin de ce processus pour installer et relancer."""
    RELAIS.write_text(json.dumps({"de": VERSION, "vers": version, "date": datetime.now().isoformat(timespec="seconds")}),
                      encoding="utf-8")
    subprocess.Popen([str(dossier / EXE), "--mise-a-jour", str(os.getpid()), str(ROOT / EXE)], cwd=dossier,
                     creationflags=getattr(subprocess, "DETACHED_PROCESS", 0)
                     | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0),
                     close_fds=True)
    journal.noter("Mise à jour : relais au nouveau lanceur", f"{VERSION} → {version}")


def _copier_a_cote(dossier: Path, version: str) -> Path:
    """Mode portable : la nouvelle version à côté du dossier actuel ; rien de l'utilisateur n'est déplacé."""
    cible = ROOT.parent / f"Aria-Studio-{version}"
    if cible.exists():
        raise ErreurMiseAJour(f"{cible} existe déjà : rien n'est écrasé")
    shutil.copytree(dossier, cible)
    return cible


async def installer(arreter) -> None:
    """Toute la mise à jour, en tâche de fond. `arreter` : arrête le serveur (après le relais)."""
    try:
        dossier = await preparer()
        version = _etat["version"]
        _changer(etape="installation")
        if portable():
            cible = await asyncio.to_thread(_copier_a_cote, dossier, version)
            journal.noter("Mise à jour décompressée à côté (mode portable)", str(cible))
            _changer(etape="portable", chemin=str(cible))
            if hasattr(os, "startfile"):
                os.startfile(cible)  # l'Explorateur, pour y recopier data, modeles, services et reglages.json
            return
        _relayer(dossier, version)
        _changer(etape="redemarrage")
        arreter()
    except Exception as e:  # noqa: BLE001 : tout échec est montré et noté, rien n'est installé
        log.exception("Mise à jour impossible")
        message = str(e) if isinstance(e, ErreurMiseAJour) else f"{type(e).__name__} : {e}"
        _changer(etape="erreur", erreur=message)
        journal.noter("Mise à jour en échec", f"{VERSION} → {_etat.get('version') or '?'}", "erreur", message)


def en_cours() -> bool:
    return _etat.get("etape") in ETAPES_ACTIVES


# --- au démarrage ------------------------------------------------------------------------------------------------

def bilan_au_demarrage() -> str | None:
    """Après un relais : la version attendue tourne-t-elle ? Noté au journal. Rend « installee », « echec » ou None."""
    try:
        relais = json.loads(RELAIS.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    RELAIS.unlink(missing_ok=True)
    if relais.get("vers") == VERSION:
        journal.noter("Mise à jour installée", f"{relais.get('de')} → {VERSION}")
        return "installee"
    journal.noter("Mise à jour non installée", f"{relais.get('de')} → {relais.get('vers')}", "erreur",
                  f"Aria a redémarré en {VERSION} ; voir data/logs/aria.log")
    return "echec"


async def vider_plus_tard(delai: float = 30):
    """cache/mise-a-jour/ se vide une fois le lanceur du relais fermé (son exe y est encore ouvert un instant)."""
    await asyncio.sleep(delai)
    if DOSSIER.is_dir() and not en_cours():
        await asyncio.to_thread(shutil.rmtree, DOSSIER, True)
