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
        config.setdefault(section, {}).update(valeurs)
    return config


def chemin(relatif: str) -> Path:
    """Résout un chemin de config.yaml par rapport à la racine du projet."""
    p = Path(relatif)
    return p if p.is_absolute() else ROOT / p


CONFIG = charger()
