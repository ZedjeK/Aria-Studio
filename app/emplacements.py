"""Emplacements des affaires d'Aria : données, modèles et moteurs, décidés une fois au démarrage.

Par défaut, tout vit dans le dossier de l'utilisateur, comme ~/.ollama ou ~/.claude :
    %USERPROFILE%\\.Aria-Studio\\aria.yaml   choix de l'utilisateur (dossier des modèles, favoris du Casting)
    %USERPROFILE%\\.Aria-Studio\\data\\      séquences, rushes, réglages, voix, journaux
    %USERPROFILE%\\.Aria-Studio\\modeles\\   modèles (ailleurs si l'utilisateur l'a choisi dans aria.yaml)
    %USERPROFILE%\\.Aria-Studio\\moteurs\\   ComfyUI portable, llama-server
Mode portable : si un dossier data\\ se trouve à côté du code (installation d'origine de Jérémy, clé USB…),
tout reste à côté du code : aria.yaml, data\\, modeles\\ et services\\.

Les modules lisent ces chemins à l'import : un changement de dossier des modèles vaut au prochain démarrage.
Ce module ne dépend pas d'app.config : c'est config qui lui emprunte les favoris (une mise à jour remplace le code,
config.yaml compris, sans toucher à aria.yaml).
"""

import os
import shutil
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent  # dossier du code (celui de app\)
DEFAUT = Path.home() / ".Aria-Studio"  # Windows ignore la casse : .aria-studio désigne le même dossier
NOMS = {"racine": "Dossier d'Aria", "data": "Données", "modeles": "Modèles", "moteurs": "Moteurs"}


class ErreurEmplacement(ValueError):
    pass


def portable() -> bool:
    return (ROOT / "data").is_dir()


def _racine() -> Path:
    return ROOT if portable() else DEFAUT


def _fichier_choix() -> Path:
    return _racine() / "aria.yaml"


def _choix() -> dict:
    try:
        return yaml.safe_load(_fichier_choix().read_text(encoding="utf-8")) or {}
    except (OSError, yaml.YAMLError):
        return {}


def _ecrire_choix(choix: dict):
    _fichier_choix().parent.mkdir(parents=True, exist_ok=True)
    _fichier_choix().write_text(
        "# Choix de l'utilisateur pour Aria - Studio (onglet Modèles, favoris du Casting) : lu au démarrage.\n"
        + yaml.safe_dump(choix, allow_unicode=True, sort_keys=False), encoding="utf-8")


def favoris() -> dict:
    """Favoris du Casting, par section de config.yaml : {"generation": {"texte_vers_image": "…"}, "llama": {…}}."""
    f = _choix().get("favoris")
    return {s: dict(v) for s, v in f.items() if isinstance(v, dict)} if isinstance(f, dict) else {}


def retenir_favori(section: str, cle: str, valeur: str):
    choix = _choix()
    choix.setdefault("favoris", {}).setdefault(section, {})[cle] = valeur
    _ecrire_choix(choix)


def _modeles(choix: dict) -> Path:
    return Path(choix["modeles"]) if choix.get("modeles") else _racine() / "modeles"


RACINE = _racine()
DATA = RACINE / "data"
MODELES = _modeles(_choix())
MOTEURS = RACINE / ("services" if portable() else "moteurs")


def moteur(relatif: str) -> Path:
    """Chemin d'un moteur de config.yaml (« comfyui », « llama/llama-server.exe »), relatif au dossier des moteurs."""
    p = Path(relatif)
    return p if p.is_absolute() else MOTEURS / p


def _taille_libre(dossier: Path) -> float | None:
    for d in (dossier, *dossier.parents):
        if d.exists():
            return round(shutil.disk_usage(d).free / 1024 ** 3, 1)
    return None


def etat() -> dict:
    """Ce que montrent l'onglet Modèles et l'onglet Réglages."""
    prochain = _modeles(_choix())
    return {
        "portable": portable(),
        "dossiers": {cle: {"nom": NOMS[cle], "chemin": str(chemin), "existe": chemin.is_dir(),
                           "libre_go": _taille_libre(chemin)}
                     for cle, chemin in (("racine", RACINE), ("data", DATA), ("modeles", MODELES), ("moteurs", MOTEURS))},
        "modeles_choisi": _choix().get("modeles"),
        "modeles_defaut": str(_racine() / "modeles"),
        "modeles_prochain": str(prochain),
        "redemarrage": prochain != MODELES,  # choix fait, pris en compte au prochain démarrage
    }


def ouvrir(cle: str, sous: str | None = None) -> Path:
    """Ouvre l'un des dossiers connus dans l'Explorateur (jamais un chemin venu de l'interface).

    `sous` : un sous-dossier des modèles déjà validé par l'appelant (« MiniMax-H3/modeles »)."""
    dossiers = {"racine": RACINE, "data": DATA, "modeles": MODELES, "moteurs": MOTEURS}
    if cle not in dossiers:
        raise ErreurEmplacement(f"Dossier inconnu : {cle}")
    dossier = dossiers[cle]
    if sous:
        dossier = (dossier / sous).resolve()
        if dossiers[cle].resolve() not in dossier.parents:
            raise ErreurEmplacement(f"Sous-dossier invalide : {sous}")
    dossier.mkdir(parents=True, exist_ok=True)
    os.startfile(dossier)  # Windows : l'Explorateur s'ouvre sur le dossier
    return dossier


def choisir_modeles(chemin: str | None) -> dict:
    """Enregistre le dossier des modèles dans aria.yaml (None : revenir au dossier par défaut).

    Les fichiers ne sont pas déplacés ; le nouveau dossier vaut au prochain démarrage d'Aria."""
    choix = _choix()
    if chemin:
        dossier = Path(chemin).expanduser()
        if not dossier.is_absolute():
            raise ErreurEmplacement("Donne un chemin complet, par exemple D:\\IA\\Aria-modeles")
        if dossier.exists() and not dossier.is_dir():
            raise ErreurEmplacement(f"{dossier} est un fichier, pas un dossier")
        for interdit in (DATA, MOTEURS, ROOT / "app"):
            if dossier == interdit or interdit in dossier.parents:
                raise ErreurEmplacement(f"Les modèles ne peuvent pas aller dans {interdit}")
        try:
            dossier.mkdir(parents=True, exist_ok=True)
            essai = dossier / ".aria-ecriture"
            essai.write_text("ok", encoding="utf-8")
            essai.unlink()
        except OSError as e:
            raise ErreurEmplacement(f"Impossible d'écrire dans {dossier} : {e.strerror or e}") from e
        choix["modeles"] = str(dossier)
    else:
        choix.pop("modeles", None)
    _ecrire_choix(choix)
    return etat()


def parcourir(depart: str | None = None) -> str | None:
    """Sélecteur de dossier natif de Windows, ouvert par le serveur local (None si annulé)."""
    try:
        import tkinter
        from tkinter import filedialog
    except ImportError as e:
        raise ErreurEmplacement("Sélecteur de dossier indisponible (tkinter absent)") from e
    fenetre = tkinter.Tk()
    fenetre.withdraw()
    fenetre.attributes("-topmost", True)  # au-dessus du navigateur
    try:
        choisi = filedialog.askdirectory(parent=fenetre, title="Dossier des modèles d'Aria - Studio",
                                         initialdir=depart or str(MODELES), mustexist=False)
    finally:
        fenetre.destroy()
    return str(Path(choisi)) if choisi else None
