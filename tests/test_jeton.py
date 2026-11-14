"""Jeton par lancement (app/jeton.py, garde de app/main.py) : sans lui, l'interface et l'API refusent ; « /?jeton= »
le range dans un cookie ; /api/sante reste ouverte au lanceur. Et l'écoute réservée à 127.0.0.1.

Rien n'est lancé : le cycle de vie (ComfyUI, Llama, téléchargements) ne démarre pas sans « with TestClient ».
"""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from fastapi.testclient import TestClient

from app import jeton, main
from app.config import CONFIG

LOCAL = f"http://127.0.0.1:{CONFIG['app']['port']}"


class Garde(unittest.TestCase):
    def setUp(self):
        self.client = TestClient(main.app, base_url=LOCAL, follow_redirects=False)

    def test_sans_jeton_tout_est_refuse(self):
        self.assertEqual(self.client.get("/api/fond").status_code, 403)
        self.assertEqual(self.client.post("/api/quitter").status_code, 403)
        self.assertEqual(self.client.get("/static/commun.js").status_code, 403)
        r = self.client.get("/api/fond")
        self.assertIn("lanceur", r.json()["detail"])

    def test_la_page_sans_jeton_dit_d_ouvrir_le_lanceur(self):
        r = self.client.get("/")
        self.assertEqual(r.status_code, 403)
        self.assertIn("Aria-Studio.exe", r.text)
        self.assertNotIn("/static/", r.text)  # rien de l'appli

    def test_sante_reste_ouverte(self):
        self.assertEqual(self.client.get("/api/sante").status_code, 200)

    def test_jeton_dans_l_adresse_pose_le_cookie_et_disparait(self):
        r = self.client.get(f"/?jeton={jeton.JETON}")
        self.assertEqual(r.status_code, 303)
        self.assertEqual(r.headers["location"], "/")
        cookie = r.headers["set-cookie"]
        for attendu in (f"{jeton.COOKIE}=", "HttpOnly", "SameSite=strict", "Path=/"):
            self.assertIn(attendu.lower(), cookie.lower())
        self.assertEqual(self.client.get("/").status_code, 200)  # le cookie suit
        self.assertEqual(self.client.get("/api/fond").status_code, 200)

    def test_mauvais_jeton_ou_jeton_d_un_autre_lancement(self):
        self.assertEqual(self.client.get("/?jeton=faux").status_code, 403)
        self.client.cookies.set(jeton.COOKIE, "jeton-d-un-lancement-precedent")
        self.assertEqual(self.client.get("/api/fond").status_code, 403)
        self.assertEqual(self.client.get("/api/fond", headers={jeton.EN_TETE: "faux"}).status_code, 403)

    def test_en_tete_pour_les_scripts(self):
        self.assertEqual(self.client.get("/api/fond", headers={jeton.EN_TETE: jeton.JETON}).status_code, 200)

    def test_autre_site_toujours_refuse_meme_avec_le_jeton(self):
        r = self.client.post("/api/quitter", headers={jeton.EN_TETE: jeton.JETON, "origin": "https://evil.example"})
        self.assertEqual(r.status_code, 403)


class Session(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.fichier = Path(self.tmp.name) / "cache" / "session.json"

    def tearDown(self):
        self.tmp.cleanup()

    def test_ecrite_puis_effacee(self):
        jeton.ecrire_session(8075, self.fichier)
        d = json.loads(self.fichier.read_text(encoding="utf-8"))
        self.assertEqual((d["port"], d["jeton"]), (8075, jeton.JETON))
        jeton.effacer_session(self.fichier)
        self.assertFalse(self.fichier.exists())

    def test_celle_d_un_autre_lancement_n_est_pas_effacee(self):
        self.fichier.parent.mkdir(parents=True)
        self.fichier.write_text(json.dumps({"jeton": "autre"}), encoding="utf-8")
        jeton.effacer_session(self.fichier)
        self.assertTrue(self.fichier.exists())


class Ecoute(unittest.TestCase):
    def test_seulement_127_0_0_1(self):
        for hote in ("127.0.0.1", "localhost"):
            with mock.patch.dict(CONFIG["app"], {"host": hote}):
                self.assertEqual(main.hote_d_ecoute(), hote)
        for hote in ("0.0.0.0", "::", "192.168.1.10", ""):
            with mock.patch.dict(CONFIG["app"], {"host": hote}), self.assertRaises(SystemExit):
                main.hote_d_ecoute()


class MiseAJourEnLecture(unittest.TestCase):
    def test_get_n_interroge_pas_github(self):
        client = TestClient(main.app, base_url=LOCAL, headers={jeton.EN_TETE: jeton.JETON})
        with mock.patch.object(main.mises_a_jour, "verifier", side_effect=AssertionError("pas de réseau")):
            self.assertEqual(client.get("/api/mise-a-jour").status_code, 200)


if __name__ == "__main__":
    unittest.main()
