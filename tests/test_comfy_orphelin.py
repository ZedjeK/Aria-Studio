"""ComfyUI : jamais deux à la fois, et arrêté en quittant même quand il est adopté (orphelin d'une Aria fermée
brutalement). Vu le 2026-11-06 : occupé, ComfyUI a mis plus de 2 s à répondre, Aria en a lancé un second (mort sur le
port pris) et a laissé le vrai tourner après sa fermeture. Un faux ComfyUI (petit serveur HTTP de la bibliothèque
standard, lent sur demande) tient le rôle : rien de lourd n'est lancé, rien ne survit au test."""

import asyncio
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

import httpx
import psutil

from app import comfy, garde

PYTHON = getattr(sys, "_base_executable", None) or sys.executable  # pas le lanceur du venv, qui ferait un petit-fils
FAUX = """
import http.server, pathlib, sys, time
port = int(sys.argv[sys.argv.index("--port") + 1])
lent = pathlib.Path("lent")  # contient une heure : jusque-là, chaque réponse prend 3 s (ComfyUI occupé)

class Reponse(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if lent.exists() and time.time() < float(lent.read_text()):
            time.sleep(3)
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"{}")

    def log_message(self, *args):
        pass

http.server.ThreadingHTTPServer(("127.0.0.1", port), Reponse).serve_forever()
"""


def port_libre() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Base(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.dossier = Path(self.temp.name)
        (self.dossier / "ComfyUI").mkdir()
        (self.dossier / "ComfyUI" / "main.py").write_text(FAUX, encoding="utf-8")
        self.port = port_libre()
        self.patch = mock.patch.object(comfy, "LOGS", self.dossier / "logs")  # jamais le vrai comfyui.log
        self.patch.start()

    def tearDown(self):
        for p in self.faux():  # rien ne doit survivre au test
            try:
                p.kill()
                p.wait(5)
            except psutil.Error:
                pass
        self.patch.stop()
        garde.ADOPTES.clear()
        self.temp.cleanup()

    def faux(self) -> list[psutil.Process]:
        sortie = []
        for p in psutil.process_iter():
            try:
                cmd = p.cmdline()
                if "ComfyUI/main.py" in cmd and str(self.port) in cmd:
                    sortie.append(p)
            except psutil.Error:
                continue
        return sortie

    def comfy(self) -> comfy.ComfyUI:
        c = comfy.ComfyUI(self.dossier, self.port, self.dossier / "chemins.yaml")
        c.python = Path(PYTHON)
        return c

    def attendre_pret(self):
        fin = time.time() + 20
        while time.time() < fin:
            try:
                if httpx.get(f"http://127.0.0.1:{self.port}/system_stats", timeout=1).status_code == 200:
                    return
            except httpx.HTTPError:
                pass
            time.sleep(0.2)
        self.fail("le faux ComfyUI ne répond pas")

    def lancer_orphelin(self):
        """Un faux ComfyUI lancé, comme par Aria, par un processus qui se termine aussitôt (fermeture brutale)."""
        commande = [PYTHON, "-s", "ComfyUI/main.py", "--listen", "127.0.0.1", "--port", str(self.port)]
        subprocess.run([PYTHON, "-c", f"import subprocess; subprocess.Popen({commande!r}, cwd={str(self.dossier)!r})"],
                       check=True)
        self.attendre_pret()


class JamaisDeux(Base):
    def test_occupe_il_est_attendu_pas_relance(self):
        c = self.comfy()
        with mock.patch.object(comfy.subprocess, "Popen", wraps=subprocess.Popen) as popen:
            asyncio.run(c.demarrer(timeout=30))
            premier = c.process.pid
            (self.dossier / "lent").write_text(str(time.time() + 4))  # occupé : plus de 2 s pour répondre
            debut = time.time()
            asyncio.run(c.demarrer(timeout=30))
        self.assertGreater(time.time() - debut, 2)  # il a bien attendu
        self.assertEqual(popen.call_count, 1)  # avant : un second ComfyUI, mort sur le port pris
        self.assertEqual(c.process.pid, premier)
        self.assertEqual(len(self.faux()), 1)
        asyncio.run(c.arreter())
        self.assertEqual(self.faux(), [])  # celui qui tournait vraiment est bien arrêté

    def test_port_pris_par_autre_chose(self):
        with socket.socket() as s:  # quelque chose tient le port sans répondre comme ComfyUI
            s.bind(("127.0.0.1", self.port))
            s.listen()
            c = self.comfy()
            with mock.patch.object(comfy.subprocess, "Popen") as popen:
                with self.assertRaisesRegex(comfy.ErreurComfy, "ne répond plus"):
                    asyncio.run(c.demarrer(timeout=3))
            popen.assert_not_called()


class Orphelins(Base):
    def test_adopte_compte_par_le_garde_puis_arrete(self):
        self.lancer_orphelin()
        [orphelin] = self.faux()
        c = self.comfy()
        with mock.patch.object(comfy.subprocess, "Popen") as popen:
            asyncio.run(c.demarrer(timeout=10))
        popen.assert_not_called()
        self.assertEqual(c.adopte.pid, orphelin.pid)
        self.assertTrue(c.vivant())
        self.assertIn(orphelin.pid, garde.pids_aria())  # sa VRAM n'est plus « une autre appli »
        asyncio.run(c.arreter())
        self.assertEqual(self.faux(), [])
        self.assertNotIn(orphelin.pid, garde.ADOPTES)

    def test_celui_d_une_aria_vivante_n_est_jamais_adopte(self):
        a = self.comfy()
        asyncio.run(a.demarrer(timeout=30))  # son parent (ce test) est vivant
        b = self.comfy()
        self.assertEqual(b.orphelins(), [])
        self.assertFalse(b.adopter())
        asyncio.run(b.arreter())  # n'y touche pas
        self.assertTrue(a.vivant())
        asyncio.run(a.arreter())
        self.assertEqual(self.faux(), [])

    def test_ailleurs_que_dans_le_dossier_d_aria(self):
        autre = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        try:
            (Path(autre.name) / "ComfyUI").mkdir()
            (Path(autre.name) / "ComfyUI" / "main.py").write_text(FAUX, encoding="utf-8")
            commande = [PYTHON, "-s", "ComfyUI/main.py", "--port", str(self.port)]
            subprocess.run([PYTHON, "-c", f"import subprocess; subprocess.Popen({commande!r}, cwd={autre.name!r})"],
                           check=True)
            self.attendre_pret()
            self.assertFalse(self.comfy().adopter())  # un autre ComfyUI que celui d'Aria : jamais à elle
        finally:
            for p in self.faux():
                p.kill()
                p.wait(5)
            autre.cleanup()
