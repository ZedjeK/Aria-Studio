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


class FauxWebsocket:
    """Connexion de ComfyUI qui annonce tout de suite la fin de l'exécution du prompt « p1 »."""

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        return False

    def __aiter__(self):
        self.restant = [json.dumps({"type": "executing", "data": {"node": None, "prompt_id": "p1"}})]
        return self

    async def __anext__(self):
        if not self.restant:
            raise StopAsyncIteration
        return self.restant.pop()


class Rejeu(Base):
    """Une seed rejouée (même prompt, même modèle, même format) renvoie le même workflow à ComfyUI. Vu le 2026-11-16 :
    il ne le refait pas, tout sort de son cache, et son historique annonce les fichiers de la première fois, déjà
    effacés par Aria après les avoir récupérés : « 404 Not Found » sur /view."""

    def comfyui_simule(self):
        """Un ComfyUI qui se comporte ainsi : un workflow déjà exécuté n'écrit rien et annonce ses anciens fichiers
        (un fichier effacé reprend le numéro 00001 d'après) ; /view ne sert que ce qui existe."""
        deja = {}  # workflow exécuté (en texte) -> sorties annoncées
        sorties = self.travail / "output"

        def repondre(requete: httpx.Request) -> httpx.Response:
            if requete.url.path == "/prompt":
                workflow = json.loads(requete.content)["prompt"]
                cle = json.dumps(workflow, sort_keys=True)
                if cle not in deja:
                    annonce = []
                    for noeud in workflow.values():
                        prefixe = noeud.get("inputs", {}).get("filename_prefix")
                        if prefixe is None:
                            continue
                        sous_dossier, _, base = prefixe.rpartition("/")
                        nom = f"{base}_00001_.png"
                        (sorties / sous_dossier).mkdir(parents=True, exist_ok=True)
                        (sorties / sous_dossier / nom).write_bytes(b"image")
                        annonce.append({"filename": nom, "subfolder": sous_dossier, "type": "output"})
                    deja[cle] = {"9": {"images": annonce}}
                self.derniere = deja[cle]
                return httpx.Response(200, json={"prompt_id": "p1"})
            if requete.url.path == "/history/p1":
                return httpx.Response(200, json={"p1": {"outputs": self.derniere}})
            if requete.url.path == "/view":
                p = requete.url.params
                chemin = sorties / p["subfolder"] / p["filename"]
                return httpx.Response(200, content=chemin.read_bytes()) if chemin.is_file() else httpx.Response(404)
            return httpx.Response(404)

        client = lambda **kw: VRAI_CLIENT(transport=httpx.MockTransport(repondre), **kw)  # noqa: E731
        return mock.patch.multiple(comfy.httpx, AsyncClient=client), \
            mock.patch.object(comfy.websockets, "connect", lambda *a, **kw: FauxWebsocket())

    def rejouer(self, workflow: dict, fois: int = 2) -> list[list[tuple[str, bytes]]]:
        async def lancer():
            return [await self.c.executer(workflow) for _ in range(fois)]
        client, ws = self.comfyui_simule()
        with client, ws:
            return asyncio.run(lancer())

    def test_meme_workflow_deux_fois_rend_deux_fois_l_image(self):
        workflow = {"1": {"class_type": "KSampler", "inputs": {"seed": 42}},
                    "2": {"class_type": "SaveImage", "inputs": {"images": ["1", 0], "filename_prefix": "aria_plateau"}}}
        premiere, seconde = self.rejouer(workflow)
        self.assertEqual([octets for _, octets in premiere], [b"image"])
        self.assertEqual([octets for _, octets in seconde], [b"image"])
        # Le fichier reste un .png (les appelants lisent l'extension) et les deux passages ne partagent pas de nom.
        self.assertTrue(all(nom.endswith(".png") for nom, _ in premiere + seconde))
        self.assertNotEqual(premiere[0][0], seconde[0][0])
        self.assertEqual(list((self.travail / "output").rglob("*.png")), [])  # tout est récupéré, rien ne reste

    def test_prefixe_avec_sous_dossier(self):
        """Les bancs rangent leurs sorties sous aria_banc/<id>/<modèle>."""
        workflow = {"2": {"class_type": "SaveImage", "inputs": {"images": ["1", 0],
                                                                "filename_prefix": "aria_banc/b1/krea2"}}}
        premiere, seconde = self.rejouer(workflow)
        self.assertEqual(len(premiere), 1)
        self.assertEqual(len(seconde), 1)

    def test_video_et_affiche_rejoues(self):
        """Un clip : la vidéo et son affiche sont deux enregistrements, chacun avec son préfixe."""
        workflow = {"3": {"class_type": "SaveVideo", "inputs": {"video": ["1", 0], "filename_prefix": "aria_montage"}},
                    "4": {"class_type": "SaveImage", "inputs": {"images": ["2", 0],
                                                                "filename_prefix": "aria_montage_affiche"}}}
        premiere, seconde = self.rejouer(workflow)
        self.assertEqual(len(premiere), 2)
        self.assertEqual(len(seconde), 2)

    def test_le_workflow_de_l_appelant_n_est_pas_modifie(self):
        workflow = {"2": {"class_type": "SaveImage", "inputs": {"images": ["1", 0], "filename_prefix": "aria_plateau"}}}
        self.rejouer(workflow, fois=1)
        self.assertEqual(workflow["2"]["inputs"]["filename_prefix"], "aria_plateau")

    def test_noeuds_sans_prefixe_ou_au_prefixe_relie(self):
        """Rien d'autre n'est touché : un nœud sans entrées, sans préfixe, ou dont le préfixe vient d'un autre nœud."""
        workflow = {"1": {}, "2": {"inputs": {"seed": 1}}, "3": {"inputs": {"filename_prefix": ["7", 0]}},
                    "4": {"inputs": {"filename_prefix": "a"}}}
        resultat = comfy._avec_prefixes_uniques(workflow)
        self.assertEqual(resultat["1"], {})
        self.assertEqual(resultat["2"], {"inputs": {"seed": 1}})
        self.assertEqual(resultat["3"], {"inputs": {"filename_prefix": ["7", 0]}})
        self.assertRegex(resultat["4"]["inputs"]["filename_prefix"], r"^a_[0-9a-f]{8}$")

    def test_un_jeton_par_envoi(self):
        workflow = {"4": {"inputs": {"filename_prefix": "a"}}}
        self.assertNotEqual(comfy._avec_prefixes_uniques(workflow), comfy._avec_prefixes_uniques(workflow))


if __name__ == "__main__":
    unittest.main()
