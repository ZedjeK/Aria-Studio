"""Garde « local seulement » (app/main.py) : DNS rebinding (en-tête Host) et requêtes d'un autre site (en-tête Origin).

Même règle qu'Aria Pilote. Rien n'est lancé : le cycle de vie (ComfyUI, Llama, téléchargements) ne démarre pas sans
« with TestClient », et la requête témoin (PUT /api/mise-a-jour sans corps) est refusée par la validation (422) dès
qu'elle passe la garde : aucun réglage n'est touché.
"""

import unittest
from unittest import mock

from fastapi.testclient import TestClient

from app import jeton, main
from app.config import CONFIG

PORT = CONFIG["app"]["port"]
LOCAL = f"http://127.0.0.1:{PORT}"


class LocalSeulement(unittest.TestCase):
    def setUp(self):
        self.client = TestClient(main.app, base_url=LOCAL, headers={jeton.EN_TETE: jeton.JETON})

    def essai(self, **entetes):
        return self.client.put("/api/mise-a-jour", headers=entetes).status_code

    def test_interface_locale_acceptee(self):
        for hote in (f"127.0.0.1:{PORT}", f"localhost:{PORT}", f"[::1]:{PORT}"):
            self.assertEqual(self.essai(host=hote), 422, hote)
            self.assertEqual(self.essai(host=hote, origin=f"http://{hote}"), 422, hote)

    def test_sans_origine_accepte(self):
        # Le lanceur (Aria-Studio.exe), curl, et le navigateur pour une page de l'interface en GET.
        self.assertEqual(self.essai(), 422)
        self.assertEqual(self.client.get("/").status_code, 200)

    def test_rebinding_refuse(self):
        for hote in ("evil.example", f"evil.example:{PORT}", "127.0.0.1", f"127.0.0.1:{PORT + 1}",
                     f"localhost.evil.example:{PORT}", f"192.168.1.10:{PORT}"):
            r = self.client.get("/", headers={"host": hote})
            self.assertEqual(r.status_code, 403, hote)
            self.assertEqual(r.json()["detail"], "Hôte refusé.")

    def test_rebinding_refuse_aussi_sur_les_fichiers(self):
        self.assertEqual(self.client.get("/static/index.html", headers={"host": "evil.example"}).status_code, 403)
        self.assertEqual(self.client.get("/api/etat", headers={"host": "evil.example"}).status_code, 403)

    def test_autre_site_refuse(self):
        for origine in ("http://evil.example", f"http://evil.example:{PORT}", "null", f"https://127.0.0.1:{PORT}",
                        f"http://127.0.0.1:{PORT + 1}", f"http://127.0.0.1:{PORT}.evil.example"):
            r = self.client.post("/api/quitter", headers={"origin": origine})
            self.assertEqual(r.status_code, 403, origine)
            self.assertEqual(r.json()["detail"], "Origine refusée.")
        self.assertIsNone(main.serveur)  # /api/quitter n'a jamais été atteint

    def test_autre_site_refuse_en_lecture(self):
        # Plus strict qu'Aria Pilote : une origine étrangère est refusée même en GET.
        self.assertEqual(self.client.get("/api/jetons", headers={"origin": "http://evil.example"}).status_code, 403)

    def test_hote_de_config_yaml(self):
        with mock.patch.dict(CONFIG["app"], {"host": "aria.localhost"}):
            self.assertEqual(self.essai(host=f"aria.localhost:{PORT}"), 422)
        with mock.patch.dict(CONFIG["app"], {"host": "0.0.0.0"}):
            self.assertEqual(main.hotes_locaux(), {f"127.0.0.1:{PORT}", f"localhost:{PORT}", f"[::1]:{PORT}"})

    def test_port_de_config_yaml(self):
        with mock.patch.dict(CONFIG["app"], {"port": 9123}):
            self.assertEqual(self.essai(host="127.0.0.1:9123"), 422)
            self.assertEqual(self.essai(host=f"127.0.0.1:{PORT}"), 403)


if __name__ == "__main__":
    unittest.main()
