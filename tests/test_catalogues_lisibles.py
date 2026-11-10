"""Les fichiers YAML livrés se lisent : un « : » non protégé dans un texte (catalogue/moteurs.yaml, 2026-11-10) faisait
échouer /api/moteurs et /api/assistant, sans qu'aucun test ne le voie (ils lisent tous un faux catalogue)."""

import unittest
from pathlib import Path

import yaml

from app import moteurs

ROOT = Path(__file__).resolve().parent.parent


class CataloguesLisibles(unittest.TestCase):
    def test_chaque_yaml_livre_se_lit(self):
        fichiers = [ROOT / "config.yaml", *sorted((ROOT / "catalogue").glob("*.yaml")),
                    *sorted((ROOT / "workflows").glob("*/fiche.yaml"))]
        for fichier in fichiers:
            with self.subTest(fichier=fichier.relative_to(ROOT).as_posix()):
                self.assertIsInstance(yaml.safe_load(fichier.read_text(encoding="utf-8")), dict)

    def test_le_vrai_catalogue_des_moteurs(self):
        self.assertIn("llama-prism", moteurs.catalogue()["moteurs"])


if __name__ == "__main__":
    unittest.main()
