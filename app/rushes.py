"""Rushes du Plateau : dossiers, archives et corbeille.

Aucun fichier n'est déplacé : chaque rush garde son image et sa fiche dans data/creations, et sa fiche note
où il est rangé. Ainsi, parents, variantes et références restent valides quoi qu'on range.
    dossier   : nom du dossier (un seul par rush), absent = « Non rangés » ;
    archive   : true = sorti de « Tous » et de son dossier, visible dans « Archives » ;
    corbeille : date de mise à la corbeille ; seul « Vider la corbeille » supprime pour de bon.
La liste des dossiers (et leur ordre) est dans data/dossiers.json, pour garder aussi les dossiers vides.
"""

import json
import re
from datetime import datetime

from app import plateau
from app.config import ROOT

FICHIER_DOSSIERS = ROOT / "data" / "dossiers.json"
VUES = ("tous", "non_ranges", "archives", "corbeille")
GENRES = ("images", "clips")  # Rushes du Plateau, Bobines du Montage : mêmes dossiers, chacun son onglet
ACTIONS = ("corbeille", "restaurer", "archiver", "desarchiver", "ranger")
_NOM = re.compile(r"[\w][\w .'’&()-]{0,39}")


class ErreurRushes(ValueError):
    pass


def _fiches() -> list[dict]:
    if not plateau.DOSSIER.is_dir():
        return []
    return [json.loads(f.read_text(encoding="utf-8")) for f in sorted(plateau.DOSSIER.glob("*.json"), reverse=True)]


def _ecrire(f: dict):
    (plateau.DOSSIER / f"{f['id']}.json").write_text(json.dumps(f, indent=4, ensure_ascii=False), encoding="utf-8")


def dossiers() -> list[str]:
    noms = json.loads(FICHIER_DOSSIERS.read_text(encoding="utf-8")) if FICHIER_DOSSIERS.is_file() else []
    # Un dossier noté dans une fiche mais absent de la liste (fichier effacé à la main) réapparaît.
    return noms + sorted({f["dossier"] for f in _fiches() if f.get("dossier") and f["dossier"] not in noms})


def _ecrire_dossiers(noms: list[str]):
    FICHIER_DOSSIERS.parent.mkdir(parents=True, exist_ok=True)
    FICHIER_DOSSIERS.write_text(json.dumps(noms, ensure_ascii=False, indent=2), encoding="utf-8")


def _du_genre(f: dict, genre: str) -> bool:
    return (f.get("type") == "clip") == (genre == "clips")


def _visible(f: dict, vue: str, dossier: str | None) -> bool:
    if vue == "corbeille":
        return bool(f.get("corbeille"))
    if f.get("corbeille"):
        return False
    if vue == "archives":
        return bool(f.get("archive"))
    if f.get("archive"):
        return False
    if dossier is not None:
        return f.get("dossier") == dossier
    return vue == "tous" or not f.get("dossier")


def _genre_valide(genre: str) -> str:
    if genre not in GENRES:
        raise ErreurRushes(f"Genre inconnu : {genre}")
    return genre


def liste(vue: str = "tous", dossier: str | None = None, limite: int = 500, genre: str = "images") -> list[dict]:
    if vue not in VUES:
        raise ErreurRushes(f"Vue inconnue : {vue}")
    _genre_valide(genre)
    return [f for f in _fiches() if _du_genre(f, genre) and _visible(f, vue, dossier)][:limite]


def etat(genre: str = "images") -> dict:
    """Compteurs pour les pastilles de rangement (les dossiers sont communs aux deux genres)."""
    fiches = [f for f in _fiches() if _du_genre(f, _genre_valide(genre))]
    compte = lambda vue, dossier=None: sum(_visible(f, vue, dossier) for f in fiches)
    return {"tous": compte("tous"), "non_ranges": compte("non_ranges"), "archives": compte("archives"),
            "corbeille": compte("corbeille"),
            "dossiers": [{"nom": d, "nombre": compte("tous", d)} for d in dossiers()]}


def _nom_valide(nom: str) -> str:
    nom = (nom or "").strip()
    if not _NOM.fullmatch(nom):
        raise ErreurRushes("Nom de dossier invalide (lettres, chiffres, espaces, 40 caractères au plus)")
    return nom


def agir(ids: list[str], action: str, dossier: str | None = None) -> int:
    """Applique une action à une sélection de rushes ; renvoie le nombre de rushes modifiés."""
    if action not in ACTIONS:
        raise ErreurRushes(f"Action inconnue : {action}")
    if action == "ranger" and dossier is not None and dossier not in dossiers():
        raise ErreurRushes(f"Dossier inconnu : {dossier}")
    fiches = [plateau.fiche(i) for i in dict.fromkeys(ids)]  # tout est vérifié avant de modifier quoi que ce soit
    for f in fiches:
        if action == "corbeille":
            f["corbeille"] = datetime.now().isoformat(timespec="seconds")
        elif action == "restaurer":
            f.pop("corbeille", None)
        elif action == "archiver":
            f["archive"] = True
        elif action == "desarchiver":
            f.pop("archive", None)
        elif dossier is None:
            f.pop("dossier", None)
        else:
            f["dossier"] = dossier
        _ecrire(f)
    return len(fiches)


def vider_corbeille(genre: str = "images") -> int:
    """Seule suppression définitive : fichiers et fiche des rushes (ou clips) mis à la corbeille."""
    a_supprimer = [f["id"] for f in _fiches() if f.get("corbeille") and _du_genre(f, _genre_valide(genre))]
    for id_rush in a_supprimer:
        plateau.supprimer(id_rush)
    return len(a_supprimer)


def creer_dossier(nom: str) -> str:
    nom, noms = _nom_valide(nom), dossiers()
    if nom in noms:
        raise ErreurRushes(f"Le dossier « {nom} » existe déjà")
    _ecrire_dossiers(noms + [nom])
    return nom


def renommer_dossier(ancien: str, nouveau: str) -> str:
    nouveau, noms = _nom_valide(nouveau), dossiers()
    if ancien not in noms:
        raise ErreurRushes(f"Dossier inconnu : {ancien}")
    if nouveau != ancien and nouveau in noms:
        raise ErreurRushes(f"Le dossier « {nouveau} » existe déjà")
    for f in _fiches():
        if f.get("dossier") == ancien:
            f["dossier"] = nouveau
            _ecrire(f)
    _ecrire_dossiers([nouveau if n == ancien else n for n in noms])
    return nouveau


def supprimer_dossier(nom: str) -> int:
    """Le dossier disparaît, pas ses images : elles retournent dans « Non rangés ». Renvoie leur nombre."""
    noms = dossiers()
    if nom not in noms:
        raise ErreurRushes(f"Dossier inconnu : {nom}")
    liberes = 0
    for f in _fiches():
        if f.get("dossier") == nom:
            f.pop("dossier")
            _ecrire(f)
            liberes += 1
    _ecrire_dossiers([n for n in noms if n != nom])
    return liberes


def valider_dossier(nom: str | None) -> str | None:
    """Pour ranger une nouvelle création : None (non rangée) ou un dossier existant."""
    if nom and nom not in dossiers():
        raise ErreurRushes(f"Dossier inconnu : {nom}")
    return nom or None
