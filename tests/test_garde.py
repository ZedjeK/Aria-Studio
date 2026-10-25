"""Tests du garde-VRAM : coupables probables, attente, reprise, forçage, annulation, branchement dans l'orchestrateur.

Rien ne touche la vraie carte : NVML et les processus sont simulés.
"""

import asyncio
import unittest
from types import SimpleNamespace
from unittest import mock

import pynvml

from app import garde, mesures
from app.garde import ErreurGarde, Garde
from app.orchestrateur import Orchestrateur

GO = 2**30
PROCESSUS = {  # pid : (nom, exe, RAM en Go) — ceux vus sur la machine de Jérémy cette semaine
    10: ("explorer.exe", r"C:\Windows\explorer.exe", 0.6),
    11: ("opera.exe", r"C:\Users\zeman\AppData\Local\Programs\Opera GX\opera.exe", 0.3),
    12: ("llama-server.exe", r"C:\Users\zeman\Desktop\Aria-Pilote\Aria-Pilote-0.12.0\engines\llama.cpp\llama-server.exe", 10.3),
    13: ("wallpaper64.exe", r"D:\SteamLibrary\steamapps\common\wallpaper_engine\wallpaper64.exe", 0.3),
    14: ("python.exe", r"C:\Users\zeman\Desktop\Aria - Studio\services\comfyui\python_embeded\python.exe", 3.0),
}


class FauxProcess:
    def __init__(self, pid):
        self.nom, self.chemin, self.ram = PROCESSUS[pid]

    def name(self):
        return self.nom

    def exe(self):
        return self.chemin

    def memory_info(self):
        return SimpleNamespace(rss=int(self.ram * GO))


def simuler_processus():
    return [mock.patch.object(garde.pynvml, "nvmlDeviceGetComputeRunningProcesses",
                              return_value=[SimpleNamespace(pid=p, usedGpuMemory=None) for p in PROCESSUS]),
            mock.patch.object(garde.psutil, "Process", FauxProcess),
            mock.patch.object(mesures, "_gpu", return_value=None)]


class Coupables(unittest.TestCase):
    def setUp(self):
        self.patchs = simuler_processus()
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()

    def test_appli_d_apres_le_dossier(self):
        self.assertEqual(garde.appli(PROCESSUS[12][1]), "Aria-Pilote")
        self.assertEqual(garde.appli(r"C:\Program Files\LM Studio\LM Studio.exe"), "LM Studio")
        self.assertEqual(garde.appli(PROCESSUS[13][1]), "Steam")
        self.assertEqual(garde.appli(r"D:\outils\ollama\ollama.exe"), "ollama")

    def test_moteurs_et_gros_consommateurs_hors_aria_et_windows(self):
        self.assertEqual([c["nom"] for c in garde.coupables(exclus=set())],
                         ["llama-server (Aria-Pilote)", "python (Aria - Studio)"])
        # Le ComfyUI d'Aria (son enfant) n'est pas un coupable ; un autre serveur Aria - Studio, si.
        self.assertEqual([c["nom"] for c in garde.coupables(exclus={14})], ["llama-server (Aria-Pilote)"])
        self.assertEqual(garde.coupables(exclus={14})[0]["ram_go"], 10.3)


class Attente(unittest.TestCase):
    def setUp(self):
        self.g = Garde(seuil=3.5 * GO, intervalle=0.05)
        self.patchs = [mock.patch.object(garde, "coupables",
                                         return_value=[{"pid": 12, "nom": "llama-server (Aria-Pilote)", "ram_go": 10.3}])]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()

    def vram(self, *lectures):
        valeurs = iter([int(v * GO) for v in lectures])
        dernier = [int(lectures[-1] * GO)]
        return mock.patch.object(mesures, "vram_utilisee", side_effect=lambda: next(valeurs, dernier[0]))

    def test_carte_libre_rien_n_attend(self):
        with self.vram(1.9):
            asyncio.run(self.g.attendre("ComfyUI", exclus=set()))
        self.assertIsNone(self.g.attente)

    def test_attend_puis_repart_seul(self):
        c = SimpleNamespace(etape="Tournage")
        self.g.commande = c
        vues = []

        async def scenario():
            tache = asyncio.create_task(self.g.attendre("ComfyUI", exclus=set()))
            await asyncio.sleep(0.02)
            vues.append((dict(self.g.attente), c.etape))
            await tache

        with self.vram(12.1, 12.1, 12.1, 1.9):
            asyncio.run(scenario())
        attente, etape = vues[0]
        self.assertEqual((attente["moteur"], attente["vram_go"], attente["seuil_go"]), ("ComfyUI", 12.1, 3.5))
        self.assertEqual(etape, "⏸ En attente de la carte : 12.1 Go occupés hors Aria "
                                "(llama-server (Aria-Pilote), probablement)")
        self.assertEqual(c.etape, "Tournage")  # l'étape revient une fois la carte libérée
        self.assertIsNone(self.g.attente)

    def test_lancer_quand_meme(self):
        async def scenario():
            tache = asyncio.create_task(self.g.attendre("ComfyUI", exclus=set()))
            await asyncio.sleep(0.02)
            self.assertTrue(self.g.forcer())
            await asyncio.wait_for(tache, 1)

        with self.vram(12.1):
            asyncio.run(scenario())
        self.assertFalse(self.g.forcer())  # plus rien n'attend : le prochain départ n'est pas forcé d'avance

    def test_annuler(self):
        async def scenario():
            tache = asyncio.create_task(self.g.attendre("Llama", exclus=set()))
            await asyncio.sleep(0.02)
            self.g.annuler()
            await asyncio.wait_for(tache, 1)

        with self.vram(12.1), self.assertRaisesRegex(ErreurGarde, "Annulé"):
            asyncio.run(scenario())
        self.assertIsNone(self.g.attente)

    def test_sans_gpu_lisible_rien_ne_bloque(self):
        with mock.patch.object(mesures, "vram_utilisee", side_effect=pynvml.NVMLError(1)):
            asyncio.run(self.g.attendre("ComfyUI", exclus=set()))


class DansLOrchestrateur(unittest.TestCase):
    """Le garde passe à chaque bascule de la carte vers un moteur, pas entre deux tournages d'affilée."""

    def setUp(self):
        self.o = Orchestrateur()
        self.appels = []

        async def attendre(moteur, exclus=None):
            self.appels.append(moteur)

        self.o.garde = SimpleNamespace(attendre=attendre)
        self.patchs = [mock.patch.object(self.o.llama, "arreter", new=mock.AsyncMock()),
                       mock.patch.object(self.o.llama, "demarrer", new=mock.AsyncMock()),
                       mock.patch.object(self.o.comfy, "demarrer", new=mock.AsyncMock()),
                       mock.patch.object(self.o.comfy, "est_pret", new=mock.AsyncMock(return_value=True)),
                       mock.patch.object(self.o.comfy, "liberer_vram", new=mock.AsyncMock()),
                       mock.patch.object(self.o, "_lire_samplers", new=mock.AsyncMock()),
                       mock.patch("app.orchestrateur.modeles.ecrire_chemins_comfyui")]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()

    def test_bascules(self):
        async def scenario():
            async with self.o.comfyui():
                pass
            async with self.o.comfyui():  # déjà sur ComfyUI : pas de nouvelle vérification
                pass
            async with self.o.llm():
                pass
            async with self.o.llm():
                pass
            async with self.o.comfyui():
                pass

        asyncio.run(scenario())
        self.assertEqual(self.appels, ["ComfyUI", "Llama", "ComfyUI"])
        self.o.llama.arreter.assert_awaited()  # le Llama d'Aria est rendu AVANT la vérification


class File(unittest.TestCase):
    def test_la_commande_est_confiee_au_garde_pendant_son_execution(self):
        from app.plateau import Commande, Plateau
        vue = []
        g = Garde()
        p = Plateau(orchestrateur=SimpleNamespace(garde=g))
        c = Commande("idée", None, None, "wf", "1:1", None, 1, type="creation")

        async def creer(commande):
            vue.append(g.commande)

        with mock.patch.object(p, "_creer", side_effect=creer):
            asyncio.run(p._executer(c))
        self.assertIs(vue[0], c)
        self.assertIsNone(g.commande)


if __name__ == "__main__":
    unittest.main()
