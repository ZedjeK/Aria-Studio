"""Chargement de config.yaml (valeurs d'origine, livrées avec le code) et des favoris de l'utilisateur (aria.yaml)."""

from pathlib import Path

import yaml

from app import emplacements

ROOT = Path(__file__).resolve().parent.parent
CONFIG_FILE = ROOT / "config.yaml"


def charger() -> dict:
    with CONFIG_FILE.open(encoding="utf-8") as f:
        config = yaml.safe_load(f) or {}
    for section, valeurs in emplacements.favoris().items():  # les favoris du Casting passent devant
        if section == "generation":  # un workflow retiré d'Aria ne reste pas favori : la valeur de config.yaml reprend
            valeurs = {tache: id_wf for tache, id_wf in valeurs.items() if (ROOT / "workflows" / str(id_wf)).is_dir()}
        config.setdefault(section, {}).update(valeurs)
    if emplacements.comfyui_externe():  # « J'ai déjà ComfyUI » : un chemin absolu, que moteur() garde tel quel
        config.setdefault("comfyui", {})["dossier"] = str(emplacements.comfyui_externe())
    return config


def dossier_comfyui_livre() -> str:
    """Le ComfyUI d'Aria, celui de config.yaml (relatif au dossier des moteurs)."""
    with CONFIG_FILE.open(encoding="utf-8") as f:
        return (yaml.safe_load(f) or {})["comfyui"]["dossier"]


def chemin(relatif: str) -> Path:
    """Résout un chemin de config.yaml par rapport à la racine du projet."""
    p = Path(relatif)
    return p if p.is_absolute() else ROOT / p


CONFIG = charger()
