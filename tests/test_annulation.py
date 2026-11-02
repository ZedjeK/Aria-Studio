"""✕ Annuler une commande du Plateau (création, clip, séquence) : app/plateau.py et l'interruption de ComfyUI.

Rien n'est lancé : les commandes simulées attendent un signal, ComfyUI est simulé (httpx.MockTransport).
"""

import asyncio
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

import httpx

from app import comfy, plateau
from app.comfy import ComfyUI
from app.plateau import Commande, Plateau

VRAI_CLIENT = httpx.AsyncClient  # capturé avant les remplacements (comfy.httpx est le même module)


def commande(idee="un chat") -> Commande:
    return Commande(idee=idee, prompt="a cat", llm=None, workflow="wf", format="1:1", seed=1, nombre=2)


class FilePlateau(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.plateau = Plateau(SimpleNamespace())
        self.liberer = asyncio.Event()
        self.executees = []

        async def executer(c):
            self.executees.append(c.idee)
            c.images.append("20261102-120000-aaaaaa")  # une première image déjà enregistrée
            await self.liberer.wait()

        self.patch = mock.patch.object(self.plateau, "_executer_type", executer)
        self.patch.start()
        self.plateau.demarrer()

    async def asyncTearDown(self):
        self.patch.stop()
        await self.plateau.arreter()

    async def attendre(self, c, *etats):
        for _ in range(200):
            if c.etat in etats:
                return
            await asyncio.sleep(0.01)
        self.fail(f"{c.idee} est resté {c.etat}")

    async def test_annuler_la_commande_en_cours_puis_la_suivante_part(self):
        a, b = self.plateau._mettre_en_file(commande("a")), self.plateau._mettre_en_file(commande("b"))
        await self.attendre(a, "en_cours")
        self.assertIs(self.plateau.annuler(a.id), a)
        await self.attendre(a, "annule")
        self.assertEqual((a.erreur, a.images), (None, ["20261102-120000-aaaaaa"]))  # ce qui est fait reste
        await self.attendre(b, "en_cours")  # la file continue
        self.liberer.set()
        await self.attendre(b, "termine")

    async def test_annuler_une_commande_en_attente(self):
        a, b = self.plateau._mettre_en_file(commande("a")), self.plateau._mettre_en_file(commande("b"))
        await self.attendre(a, "en_cours")
        self.plateau.annuler(b.id)
        self.assertEqual(b.etat, "annule")
        self.liberer.set()
        await self.attendre(a, "termine")
        await asyncio.sleep(0.05)
        self.assertEqual(self.executees, ["a"])  # b n'est jamais partie

    async def test_commande_finie_ou_inconnue(self):
        a = self.plateau._mettre_en_file(commande("a"))
        self.liberer.set()
        await self.attendre(a, "termine")
        self.assertEqual(self.plateau.annuler(a.id).etat, "termine")
        with self.assertRaises(plateau.ErreurPlateau):
            self.plateau.annuler("inconnue")

    async def test_l_arret_du_plateau_n_est_pas_une_annulation(self):
        a = self.plateau._mettre_en_file(commande("a"))
        await self.attendre(a, "en_cours")
        await self.plateau.arreter()
        self.assertTrue(self.plateau._boucle_tache.done())
        self.assertEqual(a.etat, "en_cours")  # rien de faux affiché : le serveur s'arrête


class FauxWebsocket:
    """Connexion de ComfyUI qui ne dit jamais rien : la génération « tourne » jusqu'à l'annulation."""

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        return False

    def __aiter__(self):
        return self

    async def __anext__(self):
        await asyncio.Event().wait()


class InterruptionComfy(unittest.IsolatedAsyncioTestCase):
    async def test_annuler_retire_et_interrompt_le_prompt(self):
        recus = []

        def repondre(requete):
            corps = json.loads(requete.content) if requete.content else None
            recus.append((requete.url.path, corps))
            return httpx.Response(200, json={"prompt_id": "p1"} if requete.url.path == "/prompt" else {})

        client = lambda **kw: VRAI_CLIENT(transport=httpx.MockTransport(repondre), **kw)  # noqa: E731
        with mock.patch.object(comfy.httpx, "AsyncClient", client), \
                mock.patch.object(comfy.websockets, "connect", lambda *a, **kw: FauxWebsocket()):
            moteur = ComfyUI(Path("comfyui"), 8195, Path("chemins.yaml"))
            tache = asyncio.create_task(moteur.executer({"1": {}}))
            await asyncio.sleep(0.05)
            tache.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await tache
        self.assertEqual(recus[1:], [("/queue", {"delete": ["p1"]}), ("/interrupt", {"prompt_id": "p1"})])


if __name__ == "__main__":
    unittest.main()
