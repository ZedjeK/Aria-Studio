"""Démarrer avec Windows (app/demarrage.py) et les routes de l'icône (app/main.py) : la valeur « Aria Studio » de la
clé Run, relue dans le registre ; « Libérer la carte graphique » ; l'ouverture sur une page (« Aller à »).

La clé Run n'est jamais la vraie : une clé d'essai sous HKCU\\Software\\Aria-Studio-tests, effacée à la fin.
"""

import os
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest import mock

from fastapi.testclient import TestClient

from app import demarrage, emplacements, jeton, main
from app.config import CONFIG

LOCAL = f"http://127.0.0.1:{CONFIG['app']['port']}"
RACINE_ESSAIS = r"Software\Aria-Studio-tests"


def effacer_cle(chemin: str):
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, chemin, 0, winreg.KEY_ALL_ACCESS) as k:
            while True:
                try:
                    effacer_cle(chemin + "\\" + winreg.EnumKey(k, 0))
                except OSError:
                    break
        winreg.DeleteKey(winreg.HKEY_CURRENT_USER, chemin)
    except FileNotFoundError:
        pass


@unittest.skipUnless(os.name == "nt", "registre Windows")
class CleRun(unittest.TestCase):
    def setUp(self):
        self.cle = RACINE_ESSAIS + "\\" + uuid.uuid4().hex
        self.temp = tempfile.TemporaryDirectory()
        self.programme = Path(self.temp.name) / "programme"
        self.patchs = [mock.patch.dict(os.environ, {"ARIA_ESSAI_CLE_RUN": self.cle, "ARIA_TESTS_ISOLES": "1"}),
                       mock.patch.object(emplacements, "programme", lambda: self.programme),
                       mock.patch.object(emplacements, "portable", lambda: False),
                       mock.patch.object(demarrage.journal, "noter")]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        effacer_cle(self.cle)
        effacer_cle(RACINE_ESSAIS) if not self._reste() else None
        self.temp.cleanup()

    @staticmethod
    def _reste() -> bool:
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RACINE_ESSAIS) as k:
                winreg.EnumKey(k, 0)
                return True
        except OSError:
            return False

    def installer(self):
        self.programme.mkdir()
        (self.programme / "Aria-Studio.exe").write_bytes(b"")

    def test_jamais_la_vraie_cle_sans_isolation(self):
        with mock.patch.dict(os.environ, {"ARIA_TESTS_ISOLES": ""}):
            self.assertEqual(demarrage.cle(), r"Software\Microsoft\Windows\CurrentVersion\Run")
        self.assertEqual(demarrage.cle(), self.cle)

    def test_desactive_par_defaut(self):
        self.installer()
        e = demarrage.etat()
        self.assertTrue(e["possible"])
        self.assertFalse(e["actif"])

    def test_activer_puis_desactiver(self):
        self.installer()
        e = demarrage.regler(True)
        self.assertTrue(e["actif"])
        self.assertFalse(e["ailleurs"])
        self.assertEqual(e["valeur"], f'"{self.programme / "Aria-Studio.exe"}" --silencieux')
        e = demarrage.regler(False)
        self.assertFalse(e["actif"])
        self.assertIsNone(demarrage._lire())
        demarrage.regler(False)  # déjà retirée : rien à faire, pas d'erreur

    def test_seulement_pour_aria_installee(self):
        self.assertFalse(demarrage.etat()["possible"])
        with self.assertRaises(ValueError):
            demarrage.regler(True)
        self.assertIsNone(demarrage._lire())
        self.installer()
        with mock.patch.object(emplacements, "portable", lambda: True):
            self.assertFalse(demarrage.possible())

    def test_une_valeur_qui_pointe_ailleurs_se_voit_et_se_corrige(self):
        import winreg
        self.installer()
        with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, self.cle, 0, winreg.KEY_SET_VALUE) as k:
            winreg.SetValueEx(k, demarrage.NOM, 0, winreg.REG_SZ, r'"D:\ancien\Aria-Studio.exe" --silencieux')
        e = demarrage.etat()
        self.assertTrue(e["actif"])
        self.assertTrue(e["ailleurs"])
        self.assertFalse(demarrage.regler(True)["ailleurs"])

    def test_routes(self):
        client = TestClient(main.app, base_url=LOCAL, headers={jeton.EN_TETE: jeton.JETON})
        self.assertFalse(client.get("/api/demarrage").json()["possible"])
        self.assertEqual(client.put("/api/demarrage", json={"actif": True}).status_code, 409)
        self.installer()
        self.assertTrue(client.put("/api/demarrage", json={"actif": True}).json()["actif"])
        self.assertFalse(client.put("/api/demarrage", json={"actif": False}).json()["actif"])


class RoutesIcone(unittest.TestCase):
    def setUp(self):
        self.client = TestClient(main.app, base_url=LOCAL, headers={jeton.EN_TETE: jeton.JETON}, follow_redirects=False)

    def test_aller_a_une_page(self):
        r = self.client.get(f"/?jeton={jeton.JETON}&vue=modeles")
        self.assertEqual(r.status_code, 303)
        self.assertEqual(r.headers["location"], "/#modeles")
        self.assertEqual(self.client.get(f"/?jeton={jeton.JETON}&vue=mise-a-jour").headers["location"], "/#mise-a-jour")

    def test_une_page_inconnue_ouvre_l_accueil(self):
        for vue in ("inconnue", "javascript:alert(1)", "//ailleurs.example"):
            self.assertEqual(self.client.get(f"/?jeton={jeton.JETON}&vue={vue}").headers["location"], "/")

    def test_liberer_refuse_pendant_une_generation(self):
        arreter = mock.AsyncMock()
        with mock.patch.object(main, "commande_en_cours", return_value={"id": "c1"}), \
                mock.patch.object(main.orchestrateur, "arreter", arreter):
            r = self.client.post("/api/moteurs/liberer")
        self.assertEqual(r.status_code, 409)
        arreter.assert_not_called()

    def test_liberer_arrete_les_moteurs(self):
        arreter = mock.AsyncMock()
        with mock.patch.object(main, "commande_en_cours", return_value=None), \
                mock.patch.object(main.services, "banc_en_cours", return_value=False), \
                mock.patch.object(main.orchestrateur, "arreter", arreter), \
                mock.patch.object(main.journal, "noter"):
            r = self.client.post("/api/moteurs/liberer")
        self.assertEqual(r.status_code, 200)
        arreter.assert_awaited_once()


if __name__ == "__main__":
    unittest.main()
