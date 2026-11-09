"""Tests des options de l'onglet Retouche : nombre de variantes, seed, dossier de rangement.

ComfyUI est simulé : rien n'est envoyé, les rushes de test vivent dans un dossier temporaire.
"""

import asyncio
import io
import json
import tempfile
import unittest
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from PIL import Image

from app import plateau, rushes
from app.plateau import ErreurPlateau, Plateau
from tests import steps_connus

setUpModule, tearDownModule = steps_connus.module()  # ces tests ne portent pas sur la validation des steps


def png(taille=(64, 48), couleur=(200, 60, 60)) -> bytes:
    tampon = io.BytesIO()
    Image.new("RGB", taille, couleur).save(tampon, format="PNG")
    return tampon.getvalue()


class FauxComfy:
    def __init__(self):
        self.seeds = []
        self.televerses = 0

    async def televerser(self, _fichier):
        self.televerses += 1
        return "aria_entree.png"

    async def executer(self, api, progression=None):
        self.seeds.append(next(n["inputs"]["seed"] for n in api.values() if "seed" in n.get("inputs", {})))
        if progression:
            progression(1, 1)
        return [("sortie.png", png((64, 48), (60, 60, 200)))]


class Base(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json")]
        for p in self.patchs:
            p.start()
        self.comfy = FauxComfy()

        @asynccontextmanager
        async def comfyui():
            yield self.comfy

        self.plateau = Plateau(orchestrateur=SimpleNamespace(comfyui=comfyui))
        self.source = plateau.enregistrer(png(), {"type": "creation", "idee": "une pilote", "prompt": "a pilot"})

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def tourner(self, commande):
        asyncio.run(self.plateau._retoucher(commande))
        return [plateau.fiche(i) for i in commande.images]

    def retouche(self, **options):
        return self.plateau.commander_retouche(self.source, "add a moon", False, None, None, None, 24, options.pop("seed", None),
                                               **options)


class Retouche(Base):
    def test_nombre_de_variantes_seeds_qui_se_suivent(self):
        fiches = self.tourner(self.retouche(seed=100, nombre=3))
        self.assertEqual([f["seed"] for f in fiches], [100, 101, 102])
        self.assertEqual(self.comfy.seeds, [100, 101, 102])
        self.assertEqual(self.comfy.televerses, 1)  # l'image d'entrée n'est envoyée qu'une fois
        self.assertTrue(all(f["parent"] == self.source and f["type"] == "retouche" for f in fiches))

    def test_une_image_par_defaut(self):
        self.assertEqual(len(self.tourner(self.retouche(seed=5))), 1)

    def test_nombre_hors_limites(self):
        for n in (0, plateau.NOMBRE_MAX + 1):
            with self.assertRaisesRegex(ErreurPlateau, "entre 1 et"):
                self.retouche(nombre=n)

    def test_dossier_choisi(self):
        rushes.creer_dossier("Retouches")
        fiches = self.tourner(self.retouche(seed=1, nombre=2, dossier="Retouches"))
        self.assertEqual([f["dossier"] for f in fiches], ["Retouches", "Retouches"])

    def test_sans_choix_le_resultat_reste_avec_sa_source(self):
        rushes.creer_dossier("Pilotes")
        rushes.agir([self.source], "ranger", "Pilotes")
        self.assertEqual(self.tourner(self.retouche(seed=1))[0]["dossier"], "Pilotes")

    def test_sans_dossier_du_tout(self):
        self.assertNotIn("dossier", self.tourner(self.retouche(seed=1))[0])


class Agrandissement(Base):
    def agrandir(self, **options):
        return self.plateau.commander_agrandissement(self.source, 96, 48, "gauche", "", False, None, None, 96,
                                                     options.pop("seed", None), **options)

    def test_variantes_et_dossier(self):
        rushes.creer_dossier("Larges")
        fiches = self.tourner(self.agrandir(seed=7, nombre=2, dossier="Larges"))
        self.assertEqual([f["seed"] for f in fiches], [7, 8])
        self.assertEqual({f["dossier"] for f in fiches}, {"Larges"})
        self.assertEqual({(f["largeur"], f["hauteur"]) for f in fiches}, {(96, 48)})
        self.assertTrue(all(f["type"] == "agrandissement" for f in fiches))

    def test_nombre_hors_limites(self):
        with self.assertRaisesRegex(ErreurPlateau, "entre 1 et"):
            self.agrandir(nombre=0)


class Dossier(unittest.TestCase):
    def test_un_dossier_de_bobines_ne_sert_pas_aux_rushes(self):
        with tempfile.TemporaryDirectory() as d, mock.patch.object(rushes, "FICHIER_DOSSIERS", Path(d) / "dossiers.json"), \
                mock.patch.object(plateau, "DOSSIER", Path(d) / "creations"):
            rushes.creer_dossier("Bobine A", "clips")
            with self.assertRaises(rushes.ErreurRushes):
                rushes.valider_dossier("Bobine A")  # genre images par défaut : c'est la règle de la route de retouche
            self.assertEqual(json.loads((Path(d) / "dossiers.json").read_text(encoding="utf-8")),
                             {"images": [], "clips": ["Bobine A"]})


if __name__ == "__main__":
    unittest.main()
