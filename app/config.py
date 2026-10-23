"""Chargement de config.yaml et résolution des chemins du projet."""

from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
CONFIG_FILE = ROOT / "config.yaml"


def charger() -> dict:
    with CONFIG_FILE.open(encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def chemin(relatif: str) -> Path:
    """Résout un chemin de config.yaml par rapport à la racine du projet."""
    p = Path(relatif)
    return p if p.is_absolute() else ROOT / p


CONFIG = charger()
