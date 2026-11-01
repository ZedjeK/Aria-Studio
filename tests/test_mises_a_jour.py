"""Tests de la recherche de mise à jour (app/mises_a_jour.py) : GitHub est simulé, data/mise_a_jour.json temporaire."""

import asyncio
import json
import tempfile
import unittest
from datetime import datetime, timedelta
from pathlib import Path
from unittest import mock

import httpx

from app import VERSION, mises_a_jour

RELEASE = {"tag_name": "v9.9.9", "name": "Aria - Studio 9.9.9", "html_url": "https://github.com/ZedjeK/Aria-Studio/releases/tag/v9.9.9",
           "body": "- Nouveautés", "published_at": "2026-11-02T10:00:00Z"}


class FauxGitHub:
    """Remplace httpx.AsyncClient : compte les appels et rend la réponse donnée (ou lève l'erreur)."""

    def __init__(self, statut=200, corps=None, erreur=None):
        self.statut, self.corps, self.erreur, self.urls = statut, corps, erreur, []

    def __call__(self, **_):
        return self

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        pass

    async def get(self, url):
        self.urls.append(url)
        if self.erreur:
            raise self.erreur
        return httpx.Response(self.statut, json=self.corps if self.corps is not None else {"message": "Not Found"})


class Base(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.patch = mock.patch.object(mises_a_jour, "FICHIER", Path(self.dossier.name) / "mise_a_jour.json")
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.dossier.cleanup()

    def verifier(self, github, force=False):
        with mock.patch.object(mises_a_jour.httpx, "AsyncClient", github):
            return asyncio.run(mises_a_jour.verifier(force))


class Versions(unittest.TestCase):
    def test_comparaison(self):
        self.assertEqual(mises_a_jour.numero("v0.10.2"), (0, 10, 2))
        self.assertIsNone(mises_a_jour.numero("nightly"))
        self.assertTrue(mises_a_jour.plus_recente("0.10.0", "0.9.9"))  # pas un tri alphabétique
        self.assertFalse(mises_a_jour.plus_recente("0.9.0", "0.9.0"))
        self.assertFalse(mises_a_jour.plus_recente("0.8.5", "0.9.0"))
        self.assertFalse(mises_a_jour.plus_recente(None))
        self.assertIsNotNone(mises_a_jour.numero(VERSION))


class Verification(Base):
    def test_nouvelle_version(self):
        github = FauxGitHub(corps=RELEASE)
        e = self.verifier(github)
        self.assertEqual(github.urls, ["https://api.github.com/repos/ZedjeK/Aria-Studio/releases/latest"])
        self.assertTrue(e["disponible"])
        self.assertEqual((e["derniere"]["version"], e["derniere"]["date"]), ("9.9.9", "2026-11-02"))
        self.assertIsNone(e["erreur"])

    def test_deja_a_jour(self):
        e = self.verifier(FauxGitHub(corps={**RELEASE, "tag_name": f"v{VERSION}"}))
        self.assertFalse(e["disponible"])

    def test_au_plus_une_fois_par_jour(self):
        github = FauxGitHub(corps=RELEASE)
        self.verifier(github)
        self.verifier(github)
        self.assertEqual(len(github.urls), 1)
        self.verifier(github, force=True)  # « Vérifier maintenant »
        self.assertEqual(len(github.urls), 2)
        d = json.loads(mises_a_jour.FICHIER.read_text(encoding="utf-8"))
        d["verifie_le"] = (datetime.now() - timedelta(days=2)).isoformat()
        mises_a_jour.FICHIER.write_text(json.dumps(d), encoding="utf-8")
        self.verifier(github)  # la réponse a vieilli
        self.assertEqual(len(github.urls), 3)

    def test_desactivee(self):
        mises_a_jour.activer(False)
        github = FauxGitHub(corps=RELEASE)
        self.assertFalse(self.verifier(github)["active"])
        self.assertEqual(github.urls, [])  # aucune requête
        self.verifier(github, force=True)  # le bouton, lui, interroge quand même
        self.assertEqual(len(github.urls), 1)

    def test_silencieuse_en_cas_de_probleme(self):
        e = self.verifier(FauxGitHub(statut=404))
        self.assertEqual((e["disponible"], e["erreur"]), (False, "Aucune release publiée sur ce dépôt pour l'instant"))
        e = self.verifier(FauxGitHub(erreur=httpx.ConnectError("hors ligne")), force=True)
        self.assertEqual((e["disponible"], e["erreur"]), (False, "GitHub injoignable : ConnectError"))
        e = self.verifier(FauxGitHub(corps={**RELEASE, "tag_name": "nightly"}), force=True)
        self.assertIn("sans numéro de version", e["erreur"])
        e = self.verifier(FauxGitHub(statut=403), force=True)  # limite de requêtes anonymes
        self.assertEqual(e["erreur"], "GitHub a répondu 403")


if __name__ == "__main__":
    unittest.main()
