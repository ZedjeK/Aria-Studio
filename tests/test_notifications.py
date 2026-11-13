"""Tests des préférences de fin de job (app/notifications.py) : section « notifications » d'un reglages.json temporaire."""

import json
import tempfile
import unittest
from pathlib import Path

from app import fichier_reglages, notifications
from tests import reglages_jetables


class TestNotifications(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.patch = reglages_jetables.patch(Path(self.dossier.name))
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.dossier.cleanup()

    def test_actif_avec_son_par_defaut(self):
        self.assertEqual(notifications.etat(), {"active": True, "son": True})

    def test_regler_garde_l_autre_choix(self):
        notifications.regler(son=False)
        self.assertEqual(notifications.regler(active=False), {"active": False, "son": False})
        self.assertEqual(notifications.etat(), {"active": False, "son": False})
        notifications.regler(active=True)
        self.assertEqual(notifications.etat(), {"active": True, "son": False})

    def test_ecrit_dans_reglages_json(self):
        notifications.regler(son=False)
        ecrit = json.loads(fichier_reglages.fichier().read_text(encoding="utf-8"))
        self.assertEqual(ecrit["notifications"], {"active": True, "son": False})

    def test_fichier_illisible(self):
        fichier_reglages.fichier().write_text("pas du json", encoding="utf-8")
        self.assertEqual(notifications.etat(), {"active": True, "son": True})


if __name__ == "__main__":
    unittest.main()
