"""LICENCES-TIERS.txt cite chaque dépendance directe et chaque moteur épinglé, aux versions du dépôt.

Un paquet ajouté ou monté de version dans requirements.txt, ou un moteur épinglé autrement dans
catalogue/moteurs.yaml, fait échouer ce test tant que le fichier n'est pas mis à jour.
"""

import re
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
TEXTE = (ROOT / "LICENCES-TIERS.txt").read_text(encoding="utf-8")


class TestLicencesTiers(unittest.TestCase):
    def test_chaque_dependance_directe_est_citee_a_sa_version(self):
        lignes = (ROOT / "requirements.txt").read_text(encoding="utf-8").splitlines()
        for ligne in lignes:
            if not ligne.strip() or ligne.startswith("#"):
                continue
            nom, version = re.split(r"==", ligne.strip(), maxsplit=1)
            with self.subTest(nom=nom):
                self.assertRegex(TEXTE, rf"(?im)^\s*{re.escape(nom)} {re.escape(version)}\b")

    def test_chaque_moteur_epingle_est_cite_a_sa_version(self):
        moteurs = yaml.safe_load((ROOT / "catalogue" / "moteurs.yaml").read_text(encoding="utf-8"))["moteurs"]
        for cle, moteur in moteurs.items():
            with self.subTest(moteur=cle):
                self.assertIn(moteur["version"], TEXTE)
                for archive in moteur["archives"]:
                    self.assertIn(archive["depot"], TEXTE)


if __name__ == "__main__":
    unittest.main()
