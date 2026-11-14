"""Place libre pour les modèles (app/modeles.py) : dans un profil neuf, le dossier des modèles n'existe pas encore.
Vu le 2026-11-14 : l'assistant échouait alors à l'étape « Ce que tu veux faire » (erreur 500 au calcul du plan), et
un téléchargement aurait échoué de même. La place se lit sur le premier dossier parent qui existe."""

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import modeles


class EspaceLibre(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.racine = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def libre(self, dossier: Path) -> int:
        with mock.patch.object(modeles, "DOSSIER", dossier):
            return modeles.espace_libre()

    def test_dossier_existant(self):
        self.assertEqual(self.libre(self.racine), shutil.disk_usage(self.racine).free)

    def test_dossier_pas_encore_cree(self):
        neuf = self.racine / ".Aria-Studio" / "modeles"
        self.assertEqual(self.libre(neuf), shutil.disk_usage(self.racine).free)
        self.assertFalse(neuf.exists())  # lire la place ne crée rien

    def test_disque_absent(self):
        # Un dossier des modèles sur un disque débranché : aucune place, plutôt qu'une erreur
        with mock.patch.object(modeles.shutil, "disk_usage", side_effect=FileNotFoundError):
            self.assertEqual(self.libre(self.racine / "modeles"), 0)


if __name__ == "__main__":
    unittest.main()
