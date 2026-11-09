"""Tests des préférences de fin de job (app/notifications.py) : data/notifications.json temporaire."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import notifications


class TestNotifications(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.patch = mock.patch.object(notifications, "FICHIER", Path(self.dossier.name) / "notifications.json")
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

    def test_fichier_illisible(self):
        notifications.FICHIER.write_text("pas du json", encoding="utf-8")
        self.assertEqual(notifications.etat(), {"active": True, "son": True})
        notifications.FICHIER.write_text(json.dumps([1, 2]), encoding="utf-8")
        self.assertEqual(notifications.etat(), {"active": True, "son": True})


if __name__ == "__main__":
    unittest.main()
