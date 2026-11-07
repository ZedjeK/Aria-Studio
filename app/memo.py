"""Caches « tant que les fichiers n'ont pas changé » : une fiche n'est relue que si sa date ou sa taille change.

Avant, les fiches des workflows et le catalogue des LLM étaient relus et revalidés à chaque appel, parfois des
dizaines de fois pour une seule requête : la route des options du Plateau prenait 0,78 s (mesuré le 2026-11-07).
"""

from pathlib import Path

import yaml

# Le chargeur YAML en C quand PyYAML a été construit avec libyaml (8 fois plus rapide), sinon le chargeur Python.
CHARGEUR_YAML = getattr(yaml, "CSafeLoader", yaml.SafeLoader)


def lire_yaml(chemin: Path):
    return yaml.load(chemin.read_text(encoding="utf-8"), Loader=CHARGEUR_YAML)


def empreinte(chemins) -> tuple:
    """Ce qui dit qu'un ensemble de fichiers a changé : chemin, date de modification et taille de chacun."""
    sortie = []
    for chemin in chemins:
        try:
            infos = Path(chemin).stat()
            sortie.append((str(chemin), infos.st_mtime_ns, infos.st_size))
        except OSError:
            sortie.append((str(chemin), None, None))
    return tuple(sortie)


class SelonFichiers:
    """Garde la dernière valeur calculée tant que l'empreinte de ses fichiers ne change pas."""

    def __init__(self):
        self._cle = None
        self._valeur = None

    def lire(self, cle: tuple, calcul):
        if cle != self._cle:
            self._valeur = calcul()  # une erreur ne remplace rien : le calcul sera retenté au prochain appel
            self._cle = cle
        return self._valeur
