"""Un reglages.json jetable pour les tests (app/fichier_reglages.py) : il vit dans le dossier donné, vide au départ,
et le cache du module est oublié avant et après."""

from pathlib import Path
from unittest import mock

from app import fichier_reglages


def patch(dossier: Path):
    """À démarrer comme les autres patchs : `reglages_jetables.patch(d).start()` ou dans un `with`."""
    return mock.patch.multiple(fichier_reglages, RACINE_DE=lambda: Path(dossier), _cache=None, _illisible=None)


def fichier(dossier: Path) -> Path:
    return Path(dossier) / fichier_reglages.NOM
