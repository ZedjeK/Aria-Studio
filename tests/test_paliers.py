"""Tests du banc de palier (app/paliers.py) : taille du lest, conclusions, report dans les fiches. Sans carte graphique."""

import asyncio
import copy
import shutil
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

import yaml

from app import fiches, paliers, workflows

GO = 2**30
MESURES = {16: {"etat": "ok", "temps_s": 40.0}, 12: {"etat": "ok", "temps_s": 55.0}, 8: {"etat": "trop_lent", "temps_s": 1800}}


class Calculs(unittest.TestCase):
    def test_libre_vise(self):
        totale = 16376 * 2**20  # la RTX 4080 SUPER de l'auteur
        self.assertEqual(paliers.libre_vise(8, totale, int(1.5 * GO)), 8188 * 2**20 - int(1.5 * GO))
        self.assertEqual(paliers.libre_vise(12, totale, 0), 12282 * 2**20)

    def test_conclure(self):
        self.assertEqual(paliers.conclure(MESURES), (12, 12))
        lent = {16: {"etat": "ok", "temps_s": 40.0}, 12: {"etat": "ok", "temps_s": 90.0}, 8: {"etat": "ok", "temps_s": 300.0}}
        self.assertEqual(paliers.conclure(lent), (8, 16))  # passe à 8 Go, mais confortable seulement à 16
        self.assertEqual(paliers.conclure({16: {"etat": "memoire", "temps_s": 3.0}}), (None, None))

    def test_etat_erreur(self):
        self.assertEqual(paliers.etat_erreur("torch.OutOfMemoryError: CUDA out of memory. Tried to allocate"), "memoire")
        self.assertEqual(paliers.etat_erreur("KSampler : shapes mismatch"), "erreur")


class SansLest:
    def __init__(self, palier):
        pass

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        return False


class Mesures(unittest.TestCase):
    """Le banc du 2026-10-26 était faux : 2e génération servie par le cache de ComfyUI, 1re vidéo lue sur le disque."""

    def test_chauffe_puis_mesure_avec_une_autre_seed(self):
        seeds = []

        async def generer(comfy, wf, limite, seed):
            seeds.append(seed)
            return {"etat": "ok", "temps_s": 80.0 if seed == paliers.SEED else 40.0}

        class Comfy:
            def __init__(self, *_):
                pass

            async def demarrer(self):
                pass

            async def arreter(self):
                pass

        for wf in (workflows.obtenir("qwen_image_2_1"), workflows.obtenir("minimax_h3_t2v")):  # image et vidéo
            seeds.clear()
            with mock.patch.object(paliers, "Lest", SansLest), mock.patch.object(paliers, "ComfyUI", Comfy), \
                    mock.patch.object(paliers, "_generer", generer):
                r = asyncio.run(paliers.mesurer_workflow(wf, lambda _: None))
            self.assertEqual(seeds, [paliers.SEED, paliers.SEED + 1] * len(paliers.PALIERS), wf.id)
            self.assertEqual((r[16]["temps_s"], r[16]["essais"]), (40.0, [80.0, 40.0]))  # la chauffe ne compte pas

    def test_llm_lance_comme_aria(self):
        lances = []

        class Llama:
            def __init__(self, serveur, *_):
                self.serveur = serveur

            async def demarrer(self, modele, **options):
                lances.append((self.serveur, options))

            async def generer(self, messages, **_):
                return SimpleNamespace(ecriture_tps=60.0)

            async def arreter(self):
                pass

        lancement = {"serveur": Path("llama-prism/llama-server.exe"), "contexte": 8192, "reflexion": True,
                     "vision": Path("mmproj.gguf")}
        with mock.patch.object(paliers, "Lest", SansLest), mock.patch.object(paliers, "Llama", Llama), \
                mock.patch.object(paliers, "lancement_llm", lambda _: dict(lancement)):
            r = asyncio.run(paliers.mesurer_llm("Ternary-Bonsai-2-27B-PTQ1_0.gguf", lambda _: None))
        self.assertEqual(lances[0], (lancement["serveur"], lancement))
        self.assertIn("chargement_s", r[16])


class Report(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        shutil.copytree(workflows.DOSSIER / "qwen_image_2_1", d / "workflows" / "qwen_image_2_1")
        shutil.copy(fiches.CATALOGUE_LLM, d / "llm.yaml")
        self.patchs = [mock.patch.object(workflows, "DOSSIER", d / "workflows"),
                       mock.patch.object(fiches, "CATALOGUE_LLM", d / "llm.yaml")]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def test_fiche_de_workflow(self):
        avant = (workflows.DOSSIER / "qwen_image_2_1" / "fiche.yaml").read_text(encoding="utf-8")
        resultats = {"carte": "NVIDIA GeForce RTX 4080 SUPER (16 Go), 64 Go de RAM", "date": "2026-10-26",
                     "workflows": {"qwen_image_2_1": {str(k): v for k, v in MESURES.items()}}, "llm": {}}
        paliers.ecrire(resultats)
        apres = (workflows.DOSSIER / "qwen_image_2_1" / "fiche.yaml").read_text(encoding="utf-8")
        materiel = workflows.obtenir("qwen_image_2_1").materiel  # la fiche reste valide
        self.assertEqual((materiel["vram_min_go"], materiel["vram_confort_go"]), (12, 12))
        self.assertEqual(materiel["mesure"]["paliers"], "16 Go : 40.0 s, 12 Go : 55.0 s, 8 Go : trop_lent")
        self.assertIn("lest", materiel["mesure"]["methode"])
        self.assertIn("# Recommandations officielles", apres)  # les commentaires restent
        self.assertEqual(len(apres.splitlines()) - len(avant.splitlines()), 4)  # seul le bloc mesure s'ajoute

    def test_catalogue_llm(self):
        resultats = {"carte": "RTX", "date": "2026-10-26", "workflows": {},
                     "llm": {"Ternary-Bonsai-2-27B-PTQ1_0.gguf": {"16": {"etat": "ok", "temps_s": 8.0},
                                                                  "12": {"etat": "ok", "temps_s": 9.0},
                                                                  "8": {"etat": "ok", "temps_s": 30.0}}}}
        paliers.ecrire(resultats)
        bonsai = fiches.fiche_llm("Ternary-Bonsai-2-27B-PTQ1_0.gguf")["materiel"]
        autre = fiches.fiche_llm("Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf")["materiel"]
        self.assertEqual((bonsai["vram_min_go"], bonsai["vram_confort_go"]), (8, 12))
        self.assertIsNone(autre["vram_min_go"])  # seul le GGUF mesuré change

    def test_deja_mesuree(self):
        resultats = {"carte": "RTX", "date": "2026-10-26", "llm": {},
                     "workflows": {"qwen_image_2_1": {str(k): v for k, v in MESURES.items()}}}
        paliers.ecrire(resultats)
        with self.assertRaisesRegex(paliers.ErreurPalier, "Bloc materiel introuvable"):
            paliers.ecrire(copy.deepcopy(resultats))  # on ne réécrit pas par-dessus une mesure


if __name__ == "__main__":
    unittest.main()
