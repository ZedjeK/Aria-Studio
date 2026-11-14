"""Tous les réglages de l'utilisateur dans un seul fichier : <dossier d'Aria>\\reglages.json (contrat des applis Aria).

    {
        "migration": {"date": "…", "depuis": ["aria.yaml", "data/reglages.yaml", …]},
        "emplacements": {"modeles": "D:\\\\IA\\\\modeles", "comfyui": "…"},   dossier des modèles, ComfyUI externe
        "favoris": {"generation": {…}, "llama": {…}},                     favoris du Casting
        "assistant": {…},                                                  licences acceptées, fin de l'assistant
        "installation": {"proposee": false},                               la question du lanceur
        "fond": {"actif": true, "vitesse": "normale", "minimum": 2, "maximum": 5},
        "notifications": {"active": true, "son": true},
        "mises_a_jour": {"active": true},
        "loras": {"<clé de la LoRA>": {"declencheurs": "…", "force": 0.8}},
        "llm": {…}, "workflows": {…}                                       réglages fins (app/reglages.py)
    }

Chaque module garde ses valeurs par défaut et ses contrôles ; ce module ne fait que lire et écrire les sections, d'un
bloc (verrou, fichier temporaire puis remplacement), et relit le fichier dès qu'il a changé sur le disque.

Migration (une fois) : tant que le fichier n'a pas de clé « migration », les anciens fichiers de réglages (aria.yaml à
la racine, data/reglages.yaml, data/fond.json, data/notifications.json, data/mise_a_jour.json, data/loras.json) sont
lus et versés dans reglages.json ; ce qui y est déjà (le lanceur peut l'avoir créé pour « installation ») l'emporte.
Les anciens fichiers sont copiés dans sauvegardes/avant-migration-AAAA-MM-JJ/ et laissés à leur place, sans être
modifiés : une version d'avant d'Aria les retrouve. Le résultat de la dernière recherche de mise à jour part dans
cache/mise_a_jour.json.

Sauvegardes : avant chaque écriture qui change quelque chose, l'ancien reglages.json est copié dans
sauvegardes/reglages-AAAAMMJJ-HHMMSS-micro.json ; les SAUVEGARDES_GARDEES dernières sont gardées. restaurer() remet
l'une d'elles, en sauvegardant d'abord l'état actuel (rien ne se perd).

Un reglages.json illisible n'est jamais écrasé sans copie : il est mis de côté dans sauvegardes/ avant la première
écriture, et Aria repart en attendant des valeurs d'origine.

La racine (dossier de l'utilisateur, ou à côté du programme en mode portable) est donnée par app/emplacements.py,
qui importe ce module : RACINE_DE y est remplacée par emplacements._racine.
"""

import copy
import json
import logging
import re
import shutil
import threading
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path

import yaml

log = logging.getLogger(__name__)

NOM = "reglages.json"
SECTIONS = ("emplacements", "favoris", "assistant", "installation", "fond", "notifications", "mises_a_jour", "loras",
            "llm", "workflows")
# Anciens fichiers (relatifs à la racine) : leur contenu, versé dans les sections de reglages.json.
ANCIENS = ("aria.yaml", "data/reglages.yaml", "data/fond.json", "data/notifications.json", "data/mise_a_jour.json",
           "data/loras.json")


SAUVEGARDES_GARDEES = 20
_SAUVEGARDE = re.compile(r"^reglages-(\d{8})-(\d{6})-\d{6}\.json$")


class ErreurSauvegarde(ValueError):
    pass


def RACINE_DE() -> Path:  # noqa: N802 : remplacée par app/emplacements.py à son import
    return Path.home() / ".Aria-Studio"


_verrou = threading.RLock()
_cache: tuple[Path, tuple[int, int], dict] | None = None
_illisible: Path | None = None  # le fichier qu'on n'a pas su lire : mis de côté avant la première écriture


def fichier() -> Path:
    return RACINE_DE() / NOM


def _empreinte(chemin: Path) -> tuple[int, int] | None:
    try:
        stat = chemin.stat()
    except OSError:
        return None
    # Date ET taille : sous Windows l'horloge des fichiers est grossière, deux écritures rapprochées peuvent
    # porter la même date et un cache clé sur la date seule resservirait l'ancien contenu.
    return stat.st_mtime_ns, stat.st_size


def _lire_disque(chemin: Path) -> dict:
    global _illisible
    try:
        d = json.loads(chemin.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}
    except (OSError, ValueError) as e:
        log.warning("%s illisible (%s) : valeurs d'origine en attendant ; il sera mis de côté avant d'être réécrit",
                    chemin, e)
        _illisible = chemin
        return {}
    if not isinstance(d, dict):
        log.warning("%s ne contient pas un objet JSON : valeurs d'origine en attendant", chemin)
        _illisible = chemin
        return {}
    return d


def tout() -> dict:
    """Le contenu de reglages.json (migration faite au besoin). Ne pas le modifier : passer par ecrire() ou
    transaction()."""
    global _cache
    with _verrou:
        chemin = fichier()
        empreinte = _empreinte(chemin)
        if _cache is not None and _cache[0] == chemin and _cache[1] == empreinte:
            return _cache[2]  # (aussi quand le fichier n'existe pas encore : rien à relire)
        d = _lire_disque(chemin)
        if "migration" not in d and _illisible != chemin:
            d = _migrer(chemin.parent, d)
            empreinte = _empreinte(chemin)
        _cache = (chemin, empreinte, d)
        return d


def section(nom: str) -> dict:
    """Une section (un dictionnaire vide si elle n'existe pas). Ne pas la modifier : voir tout()."""
    valeur = tout().get(nom)
    return valeur if isinstance(valeur, dict) else {}


def _ecrire(chemin: Path, d: dict):
    global _cache, _illisible
    chemin.parent.mkdir(parents=True, exist_ok=True)
    if _illisible == chemin and chemin.is_file():
        cote = chemin.parent / "sauvegardes" / f"reglages-illisible-{datetime.now():%Y%m%d-%H%M%S}.json"
        cote.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(chemin, cote)
        log.warning("%s illisible mis de côté dans %s avant d'être réécrit", chemin, cote)
    elif chemin.is_file():
        if _lire_disque(chemin) == {k: v for k, v in d.items() if v not in (None, {}, [])}:
            return  # rien ne change : ni écriture ni sauvegarde
        _sauvegarder(chemin)
    _illisible = None
    propre = {k: v for k, v in d.items() if v not in (None, {}, [])}  # pas de section vide
    temporaire = chemin.with_suffix(".tmp")
    temporaire.write_text(json.dumps(propre, indent=4, ensure_ascii=False), encoding="utf-8")
    temporaire.replace(chemin)
    _cache = (chemin, _empreinte(chemin), propre)


@contextmanager
def transaction():
    """Lire, modifier et écrire tout le fichier d'un bloc : `with transaction() as d: d["fond"] = {…}`."""
    with _verrou:
        d = copy.deepcopy(tout())
        yield d
        _ecrire(fichier(), d)


def ecrire(nom: str, valeur: dict | None):
    """Remplace une section (None ou {} : la retire, ses valeurs reviennent à celles d'origine)."""
    with transaction() as d:
        if valeur:
            d[nom] = valeur
        else:
            d.pop(nom, None)


# --- migration ---------------------------------------------------------------------------------------------------

def _lire_ancien(chemin: Path):
    texte = chemin.read_text(encoding="utf-8")
    return (yaml.safe_load(texte) if chemin.suffix == ".yaml" else json.loads(texte)) or {}


def _depuis_anciens(racine: Path) -> tuple[dict, dict, list[str]]:
    """(sections, cache de la recherche de mise à jour, fichiers lus) à partir des anciens fichiers de réglages."""
    sections: dict = {}
    cache_maj: dict = {}
    lus = []
    for relatif in ANCIENS:
        chemin = racine / relatif
        if not chemin.is_file():
            continue
        try:
            d = _lire_ancien(chemin)
        except (OSError, ValueError, yaml.YAMLError) as e:
            log.warning("Migration des réglages : %s illisible, passé (%s)", chemin, e)
            continue
        if not isinstance(d, dict):
            log.warning("Migration des réglages : %s ne contient pas de réglages, passé", chemin)
            continue
        lus.append(relatif)
        if relatif == "aria.yaml":
            emplacements = {k: d[k] for k in ("modeles", "comfyui") if d.get(k)}
            if emplacements:
                sections["emplacements"] = emplacements
            for cle in ("favoris", "assistant"):
                if isinstance(d.get(cle), dict) and d[cle]:
                    sections[cle] = d[cle]
            if isinstance(d.get("installation_proposee"), bool):
                sections["installation"] = {"proposee": d["installation_proposee"]}
        elif relatif == "data/reglages.yaml":
            for cle in ("llm", "workflows"):
                if isinstance(d.get(cle), dict) and d[cle]:
                    sections[cle] = d[cle]
        elif relatif == "data/mise_a_jour.json":
            if "active" in d:
                sections["mises_a_jour"] = {"active": bool(d["active"])}
            cache_maj = {k: v for k, v in d.items() if k != "active"}
        else:  # fond, notifications, loras : la section porte le nom du fichier
            sections[{"data/fond.json": "fond", "data/notifications.json": "notifications",
                      "data/loras.json": "loras"}[relatif]] = d
    return sections, cache_maj, lus


def _migrer(racine: Path, existant: dict) -> dict:
    """Verse les anciens fichiers dans reglages.json (une fois). Ne lève pas : un échec laisse les anciens fichiers et
    reglages.json tels quels, et la migration est retentée au prochain démarrage."""
    sections, cache_maj, lus = _depuis_anciens(racine)
    d = {**sections, **existant}  # ce que reglages.json avait déjà (le lanceur) l'emporte
    d["migration"] = {"date": datetime.now().isoformat(timespec="seconds"), "depuis": lus}
    try:
        if lus:
            sauvegarde = racine / "sauvegardes" / f"avant-migration-{datetime.now():%Y-%m-%d}"
            for relatif in lus:
                cible = sauvegarde / relatif
                if not cible.exists():
                    cible.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(racine / relatif, cible)
        if cache_maj:
            cache = racine / "cache" / "mise_a_jour.json"
            if not cache.exists():
                cache.parent.mkdir(parents=True, exist_ok=True)
                cache.write_text(json.dumps(cache_maj, indent=4, ensure_ascii=False), encoding="utf-8")
        if lus or existant:
            _ecrire(racine / NOM, d)
        # Sans ancien fichier ni reglages.json, rien n'est écrit : la clé « migration » partira avec la première
        # écriture, et il n'y avait rien à verser.
    except OSError as e:
        log.error("Migration des réglages impossible (%s) : les anciens fichiers restent lus au prochain essai", e)
        return {**sections, **existant}
    if lus:
        log.info("Réglages réunis dans %s depuis %s", racine / NOM, ", ".join(lus))
    return d


# --- sauvegardes ---------------------------------------------------------------------------------------------------

def dossier_sauvegardes() -> Path:
    return RACINE_DE() / "sauvegardes"


def _fichiers_sauvegardes() -> list[Path]:
    """Les sauvegardes de reglages.json, la plus récente en premier (le nom porte la date, à la microseconde)."""
    dossier = dossier_sauvegardes()
    if not dossier.is_dir():
        return []
    return sorted((f for f in dossier.iterdir() if f.is_file() and _SAUVEGARDE.match(f.name)),
                  key=lambda f: f.name, reverse=True)


def _sauvegarder(chemin: Path):
    """Copie reglages.json avant de le réécrire, puis ne garde que les SAUVEGARDES_GARDEES dernières copies."""
    dossier = dossier_sauvegardes()
    dossier.mkdir(parents=True, exist_ok=True)
    shutil.copy2(chemin, dossier / f"reglages-{datetime.now():%Y%m%d-%H%M%S-%f}.json")
    for ancienne in _fichiers_sauvegardes()[SAUVEGARDES_GARDEES:]:
        try:
            ancienne.unlink()
        except OSError as e:
            log.warning("Sauvegarde %s non effacée (%s)", ancienne.name, e)


def sauvegardes() -> list[dict]:
    """Ce que montre Réglages → Système : chaque sauvegarde, sa date et sa taille, la plus récente en premier."""
    liste = []
    for f in _fichiers_sauvegardes():
        jour, heure = _SAUVEGARDE.match(f.name).groups()
        liste.append({"nom": f.name, "taille": f.stat().st_size,
                      "date": f"{jour[:4]}-{jour[4:6]}-{jour[6:]}T{heure[:2]}:{heure[2:4]}:{heure[4:]}"})
    return liste


def restaurer(nom: str) -> dict:
    """Remet une sauvegarde à la place de reglages.json ; l'état actuel est sauvegardé d'abord."""
    if not _SAUVEGARDE.match(nom or ""):
        raise ErreurSauvegarde(f"Sauvegarde inconnue : {nom}")
    source = dossier_sauvegardes() / nom
    if not source.is_file():
        raise ErreurSauvegarde(f"Sauvegarde introuvable : {nom}")
    try:
        d = json.loads(source.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise ErreurSauvegarde(f"Sauvegarde illisible : {nom}") from e
    if not isinstance(d, dict):
        raise ErreurSauvegarde(f"Sauvegarde illisible : {nom}")
    with _verrou:
        actuel = tout()
        d.setdefault("migration", actuel.get("migration") or {"date": datetime.now().isoformat(timespec="seconds"),
                                                              "depuis": []})
        _ecrire(fichier(), d)
    log.info("Réglages restaurés depuis %s", nom)
    return {"restaure": nom}
