"""Clé d'accès des moteurs (écart E14) : llama-server reçoit une clé neuve à chaque démarrage, par la variable
d'environnement LLAMA_API_KEY et jamais par la ligne de commande, et Aria la présente à chaque appel. ComfyUI n'a pas de
clé : Aria ne lui passe jamais --enable-cors-header, qui désactiverait son refus des autres sites du navigateur.

Rien n'est lancé : subprocess.Popen et llama-server sont simulés.
"""

import asyncio
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import httpx

from app import comfy, llm
from app.llm import Llama

VRAI_CLIENT = httpx.AsyncClient


class FauxProcessus:
    returncode = None

    def poll(self):
        return None

    def terminate(self):
        self.returncode = 0

    def wait(self, *a):
        return 0


class LlamaServer(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        d = Path(self.tmp.name)
        self.serveur, self.modele = d / "llama-server.exe", d / "petit.gguf"
        self.serveur.write_bytes(b"")
        self.modele.write_bytes(b"GGUF")
        self.lancements, self.llamas = [], []
        self.patchs = [mock.patch.object(llm, "LOGS", d / "logs"),
                       mock.patch.object(llm.subprocess, "Popen", side_effect=self.popen)]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for llama in self.llamas:
            if llama._log is not None:
                llama._log.close()  # llama.log, ouvert par demarrer() et fermé d'habitude par arreter()
        for p in reversed(self.patchs):
            p.stop()
        self.tmp.cleanup()

    def popen(self, arguments, **options):
        self.lancements.append((arguments, options))
        return FauxProcessus()

    def demarrer(self, llama):
        self.llamas.append(llama)
        with mock.patch.object(Llama, "est_pret", return_value=True):
            asyncio.run(llama.demarrer(self.modele))

    def test_cle_par_l_environnement_jamais_sur_la_ligne_de_commande(self):
        llama = Llama(self.serveur, 8095, 4096, False)
        self.demarrer(llama)
        arguments, options = self.lancements[0]
        cle = options["env"]["LLAMA_API_KEY"]
        self.assertGreaterEqual(len(cle), 40)
        self.assertEqual(cle, llama.cle)
        self.assertNotIn(cle, " ".join(arguments))
        self.assertNotIn("--api-key", arguments)
        self.assertNotIn("--api-key-file", arguments)
        self.assertIn("PATH", options["env"])  # le reste de l'environnement est gardé

    def test_une_cle_neuve_a_chaque_demarrage_oubliee_a_l_arret(self):
        llama = Llama(self.serveur, 8095, 4096, False)
        self.demarrer(llama)
        premiere = llama.cle
        asyncio.run(llama.arreter())
        self.assertIsNone(llama.cle)
        self.demarrer(llama)
        self.assertNotEqual(llama.cle, premiere)

    def test_chaque_appel_presente_la_cle(self):
        llama = Llama(self.serveur, 8095, 4096, False)
        llama.cle = "cle-de-test"
        vues = []

        def repondre(requete):
            vues.append((requete.url.path, requete.headers.get("authorization")))
            if requete.url.path == "/health":
                return httpx.Response(200, json={"status": "ok"})
            flux = "data: " + json.dumps({"choices": [{"delta": {"content": "ok"}, "finish_reason": "stop"}]}) \
                   + "\n\ndata: [DONE]\n\n"
            return httpx.Response(200, text=flux, headers={"content-type": "text/event-stream"})

        client = lambda **kw: VRAI_CLIENT(transport=httpx.MockTransport(repondre), **kw)  # noqa: E731
        with mock.patch.object(llm.httpx, "AsyncClient", client):
            self.assertTrue(asyncio.run(llama.est_pret()))
            asyncio.run(llama.generer([{"role": "user", "content": "x"}]))
        self.assertEqual(vues, [("/health", "Bearer cle-de-test"), ("/v1/chat/completions", "Bearer cle-de-test")])


class ComfyUISansCors(unittest.TestCase):
    def test_jamais_enable_cors_header(self):
        lancements = []
        with tempfile.TemporaryDirectory() as d:
            c = comfy.ComfyUI(Path(d), 8195, Path(d) / "chemins.yaml", travail=Path(d) / "travail")
            with mock.patch.object(comfy, "LOGS", Path(d) / "logs"), \
                    mock.patch.object(comfy.ComfyUI, "est_pret", return_value=False), \
                    mock.patch.object(comfy.ComfyUI, "vivant", return_value=False), \
                    mock.patch.object(comfy.ComfyUI, "_port_pris", return_value=False), \
                    mock.patch.object(comfy.ComfyUI, "_attendre", return_value=None), \
                    mock.patch.object(comfy.subprocess, "Popen", side_effect=lambda a, **k: lancements.append(a)):
                asyncio.run(c.demarrer())
                if c._log is not None:
                    c._log.close()
        arguments = lancements[0]
        self.assertNotIn("--enable-cors-header", arguments)
        self.assertEqual(arguments[arguments.index("--listen") + 1], "127.0.0.1")


if __name__ == "__main__":
    unittest.main()
