"""Tests des clés d'accès (app/cles.py) : le vrai coffre Windows, sous un nom d'essai effacé à la fin ; les sources
sont simulées (httpx.MockTransport). Seules des clés factices sont utilisées."""

import asyncio
import sys
import unittest
import uuid
from unittest import mock

import httpx

from app import cles


@unittest.skipUnless(sys.platform == "win32", "coffre Windows")
class Coffre(unittest.TestCase):
    def setUp(self):
        self.cible = f"Aria-Studio-test-{uuid.uuid4().hex[:8]}/{{}}"
        self.patchs = [mock.patch.object(cles, "CIBLE", self.cible),
                       mock.patch.dict(cles.os.environ, {"CIVITAI_TOKEN": "", "HF_TOKEN": "hf-variable-factice"})]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for source in cles.SOURCES:
            cles._effacer(self.cible.format(source))
        for p in self.patchs:
            p.stop()

    def test_aria_passe_devant_la_variable_et_ne_rend_que_l_origine(self):
        self.assertEqual({s: e["origine"] for s, e in cles.etat().items()}, {"civitai": None, "huggingface": "variable"})
        self.assertEqual(cles.entetes("huggingface"), {"Authorization": "Bearer hf-variable-factice"})
        etat = cles.enregistrer("huggingface", "  hf-aria-factice \n")
        self.assertEqual(etat["huggingface"]["origine"], "aria")
        self.assertNotIn("hf-aria-factice", str(etat))  # la clé ne repart jamais vers l'interface
        self.assertEqual(cles.cle("huggingface"), "hf-aria-factice")
        cles.supprimer("huggingface")
        self.assertEqual(cles.cle("huggingface"), "hf-variable-factice")  # retour à la variable
        cles.supprimer("huggingface")  # déjà absente : rien à redire

    def test_saisie_refusee(self):
        for valeur in ("", "deux morceaux", "x" * 600):
            with self.subTest(valeur=valeur[:10]), self.assertRaises(cles.ErreurCle):
                cles.enregistrer("civitai", valeur)
        with self.assertRaisesRegex(cles.ErreurCle, "Source inconnue"):
            cles.enregistrer("autre", "abc")


class Test(unittest.TestCase):
    def essayer(self, source, statut, corps):
        vus = []

        def repondre(requete):
            vus.append(requete)
            return httpx.Response(statut, json=corps)

        vrai = httpx.AsyncClient
        client = lambda **kw: vrai(transport=httpx.MockTransport(repondre), **kw)  # noqa: E731
        with mock.patch.object(cles.httpx, "AsyncClient", client), mock.patch.object(cles, "cle", lambda s: "cle-factice"):
            return asyncio.run(cles.tester(source)), vus[0]

    def test_cle_acceptee_avec_le_compte(self):
        r, requete = self.essayer("huggingface", 200, {"name": "compte-essai"})
        self.assertEqual(r, {"ok": True, "message": "Clé acceptée : compte compte-essai"})
        self.assertEqual(requete.headers["Authorization"], "Bearer cle-factice")
        r, requete = self.essayer("civitai", 200, {"username": "compte-essai"})
        self.assertEqual((r["ok"], requete.url.path), (True, "/api/v1/me"))

    def test_cle_refusee(self):
        r, _ = self.essayer("civitai", 401, {})
        self.assertFalse(r["ok"])
        self.assertIn("refusée (401)", r["message"])

    def test_rien_a_tester(self):
        with mock.patch.object(cles, "cle", lambda s: None):
            self.assertEqual(asyncio.run(cles.tester("civitai")), {"ok": False, "message": "Aucune clé à tester"})


if __name__ == "__main__":
    unittest.main()
