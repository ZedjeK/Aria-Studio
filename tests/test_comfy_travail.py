"""ComfyUI travaille dans le cache d'Aria : ses entrées, sorties et temporaires ne vont jamais dans le dossier de
ComfyUI, qui peut être celui de l'utilisateur (« J'ai déjà ComfyUI »). Les entrées et les temporaires sont vidés avant
chaque lancement ; une sortie est effacée dès qu'Aria l'a récupérée."""

import asyncio
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import httpx

from app import comfy
from app.emplacements import CACHE

VRAI_CLIENT = httpx.AsyncClient


class Base(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.dossier = Path(self.temp.name)
        self.travail = self.dossier / "cache" / "comfyui"
        self.c = comfy.ComfyUI(self.dossier / "moteur", 8195, self.dossier / "chemins.yaml", travail=self.travail)

    def tearDown(self):
        if self.c._log is not None:
            self.c._log.close()
        self.temp.cleanup()

    def demarrer(self, pret: bool = False) -> list[list[str]]:
        lancements = []
        with mock.patch.object(comfy, "LOGS", self.dossier / "logs"), \
                mock.patch.object(comfy.ComfyUI, "est_pret", return_value=pret), \
                mock.patch.object(comfy.ComfyUI, "adopter", return_value=False), \
                mock.patch.object(comfy.ComfyUI, "vivant", return_value=False), \
                mock.patch.object(comfy.ComfyUI, "_port_pris", return_value=False), \
                mock.patch.object(comfy.ComfyUI, "_attendre", return_value=None), \
                mock.patch.object(comfy.subprocess, "Popen", side_effect=lambda a, **k: lancements.append(a)):
            asyncio.run(self.c.demarrer())
        return lancements

    def poser(self, *relatifs: str) -> list[Path]:
        chemins = []
        for r in relatifs:
            p = self.travail / r
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(b"x")
            chemins.append(p)
        return chemins


class Lancement(Base):
    def test_trois_dossiers_dans_le_cache(self):
        arguments = self.demarrer()[0]
        valeur = lambda option: arguments[arguments.index(option) + 1]  # noqa: E731
        self.assertEqual(valeur("--input-directory"), str(self.travail / "input"))
        self.assertEqual(valeur("--output-directory"), str(self.travail / "output"))
        # ComfyUI ajoute lui-même « temp » au dossier donné.
        self.assertEqual(valeur("--temp-directory"), str(self.travail))
        for nom in ("input", "output", "temp"):
            self.assertTrue((self.travail / nom).is_dir(), nom)

    def test_par_defaut_dans_le_cache_d_aria(self):
        self.assertEqual(comfy.TRAVAIL, CACHE / "comfyui")
        with mock.patch.object(comfy, "TRAVAIL", self.dossier / "ailleurs"):
            self.assertEqual(comfy.ComfyUI(self.dossier, 8195, self.dossier / "c.yaml").travail, self.dossier / "ailleurs")

    def test_entrees_et_temporaires_vides_avant_le_lancement(self):
        entree, sous_dossier, temporaire, sortie = self.poser(
            "input/aria_1.png", "input/clips/a.mp4", "temp/apercu.png", "output/Krea2_turbo_00001_.png")
        self.demarrer()
        self.assertFalse(entree.exists())
        self.assertFalse(sous_dossier.exists())
        self.assertFalse(temporaire.exists())
        # Une sortie restée là (Aria fermée en pleine génération) n'est pas effacée.
        self.assertTrue(sortie.exists())

    def test_rien_n_est_vide_si_comfyui_tourne_deja(self):
        entree, = self.poser("input/aria_1.png")
        self.assertEqual(self.demarrer(pret=True), [])
        self.assertTrue(entree.exists())


class Sorties(Base):
    def suivre(self, sorties: dict) -> list[tuple[str, bytes]]:
        async def ws():
            yield json.dumps({"type": "executing", "data": {"node": None, "prompt_id": "p1"}})

        def repondre(requete: httpx.Request) -> httpx.Response:
            if requete.url.path == "/history/p1":
                return httpx.Response(200, json={"p1": {"outputs": sorties}})
            if requete.url.path == "/view":
                return httpx.Response(200, content=requete.url.params["filename"].encode())
            return httpx.Response(404)

        async def lancer():
            async with VRAI_CLIENT(transport=httpx.MockTransport(repondre)) as client:
                return await self.c._suivre(ws(), client, "p1", None)
        return asyncio.run(lancer())

    def test_sortie_effacee_une_fois_recuperee(self):
        image, video = self.poser("output/Krea2_turbo_00001_.png", "output/video/MiniMax_H3_00001_.mp4")
        fichiers = self.suivre({"9": {"images": [
            {"filename": "Krea2_turbo_00001_.png", "subfolder": "", "type": "output"},
            {"filename": "MiniMax_H3_00001_.mp4", "subfolder": "video", "type": "output"}]}})
        self.assertEqual(fichiers, [("Krea2_turbo_00001_.png", b"Krea2_turbo_00001_.png"),
                                    ("MiniMax_H3_00001_.mp4", b"MiniMax_H3_00001_.mp4")])
        self.assertFalse(image.exists())
        self.assertFalse(video.exists())

    def test_rien_hors_des_sorties(self):
        dehors, = self.poser("input/aria_1.png")
        voisin = self.dossier / "voisin.png"
        voisin.write_bytes(b"x")
        self.suivre({"9": {"images": [
            {"filename": "aria_1.png", "subfolder": "../input", "type": "output"},
            {"filename": "voisin.png", "subfolder": "../../..", "type": "output"}]}})
        self.assertTrue(dehors.exists())
        self.assertTrue(voisin.exists())

    def test_apercus_temporaires_ni_lus_ni_effaces(self):
        apercu, = self.poser("temp/ComfyUI_temp_00001_.png")
        self.assertEqual(self.suivre({"9": {"images": [
            {"filename": "ComfyUI_temp_00001_.png", "subfolder": "", "type": "temp"}]}}), [])
        self.assertTrue(apercu.exists())


if __name__ == "__main__":
    unittest.main()
