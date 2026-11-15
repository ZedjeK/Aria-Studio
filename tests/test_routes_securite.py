"""Routes de la carte Sécurité de Réglages (/api/securite) : lire, essayer, ajouter, retirer, promouvoir.

Aucune route ne retire ni ne modifie un terme du socle. Les fichiers sont des copies jetables.
"""

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from fastapi.testclient import TestClient

from app import jeton, main, securite
from app.config import CONFIG
from tests import reglages_jetables

LOCAL = f"http://127.0.0.1:{CONFIG['app']['port']}"


class RoutesSecurite(unittest.TestCase):
    def setUp(self):
        dossier = Path(tempfile.mkdtemp(prefix="aria-as18-"))
        self.addCleanup(shutil.rmtree, dossier, ignore_errors=True)
        self.socle = dossier / "socle.yaml"
        shutil.copy(securite.FICHIER_SOCLE, self.socle)
        for p in (mock.patch.object(securite, "FICHIER_SOCLE", self.socle), reglages_jetables.patch(dossier)):
            p.start()
            self.addCleanup(p.stop)
        self.client = TestClient(main.app, base_url=LOCAL, headers={jeton.EN_TETE: jeton.JETON})

    def ajouts(self, etat, liste="sensibles"):
        return next(l for l in etat["listes"] if l["cle"] == liste)["ajouts"]

    def test_etat(self):
        etat = self.client.get("/api/securite").json()
        self.assertIsNone(etat["erreur"])
        self.assertEqual([l["cle"] for l in etat["listes"]], ["mineurs", "sensibles", "toujours_bloques"])
        self.assertTrue(all(l["socle"] and l["ajouts"] == [] for l in etat["listes"]))
        self.assertTrue(all(t["ok"] for t in etat["temoins"]))

    def test_essayer(self):
        r = self.client.post("/api/securite/essayer", json={"texte": "a child, nude"}).json()
        self.assertTrue(r["bloque"])
        self.assertFalse(self.client.post("/api/securite/essayer", json={"texte": "a child in a garden"}).json()["bloque"])

    def test_ajouter_retirer_promouvoir(self):
        r = self.client.post("/api/securite/ajouts", json={"liste": "sensibles", "terme": "zorglub"})
        self.assertEqual(self.ajouts(r.json()), ["zorglub"])
        self.assertEqual(self.client.post("/api/securite/ajouts", json={"liste": "sensibles", "terme": "zorglub"}).status_code, 400)
        r = self.client.post("/api/securite/ajouts/retirer", json={"liste": "sensibles", "terme": "zorglub"})
        self.assertEqual(self.ajouts(r.json()), [])
        self.client.post("/api/securite/ajouts", json={"liste": "sensibles", "terme": "zorglub"})
        r = self.client.post("/api/securite/ajouts/promouvoir", json={"liste": "sensibles", "terme": "zorglub"}).json()
        self.assertEqual(self.ajouts(r), [])
        self.assertIn("zorglub", next(l for l in r["listes"] if l["cle"] == "sensibles")["socle"])

    def test_un_terme_du_socle_ne_se_retire_par_aucune_route(self):
        for route in ("/api/securite/ajouts/retirer", "/api/securite/ajouts/promouvoir"):
            r = self.client.post(route, json={"liste": "sensibles", "terme": "nude"})
            self.assertEqual(r.status_code, 400, route)
        chemins = {r.path for r in main.app.routes if getattr(r, "path", "").startswith("/api/securite")}
        self.assertEqual(chemins, {"/api/securite", "/api/securite/essayer", "/api/securite/ajouts",
                                   "/api/securite/ajouts/retirer", "/api/securite/ajouts/promouvoir",
                                   "/api/securite/temoins", "/api/securite/temoins/retirer"})
        self.assertTrue(self.client.post("/api/securite/essayer", json={"texte": "a child, nude"}).json()["bloque"])

    def test_un_temoin_casse_demande_confirmation(self):
        self.client.post("/api/securite/temoins", json={"texte": "a child with a kite", "attendu": "passe"})
        r = self.client.post("/api/securite/ajouts", json={"liste": "sensibles", "terme": "kite"})
        self.assertEqual(r.status_code, 409)
        self.assertIn("a child with a kite", r.json()["detail"])
        r = self.client.post("/api/securite/ajouts", json={"liste": "sensibles", "terme": "kite", "forcer": True})
        self.assertEqual(self.ajouts(r.json()), ["kite"])
        r = self.client.post("/api/securite/temoins/retirer", json={"texte": "a child with a kite"})
        self.assertEqual(r.status_code, 200)

    def test_socle_illisible(self):
        self.socle.write_text("mineurs: [", encoding="utf-8")
        self.assertTrue(self.client.get("/api/securite").json()["erreur"])
        self.assertEqual(self.client.post("/api/securite/essayer", json={"texte": "a red apple"}).status_code, 400)


if __name__ == "__main__":
    unittest.main()
