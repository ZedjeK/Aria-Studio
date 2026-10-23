"""Tests du Montage : commandes de clips, AS-18, Bobines (genre clips) et dossiers partagés, consigne vidéo.

Rien n'est envoyé à ComfyUI ni à Llama ; les rushes et clips vivent dans un dossier temporaire.
"""

import asyncio
import tempfile
import unittest
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from app import plateau, prompt, reglages, rushes, workflows
from app.plateau import ErreurPlateau, Plateau
from app.securite import ErreurSecurite

PNG = bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010806000000") + bytes(16)


class Base(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json")]
        for p in self.patchs:
            p.start()
        self.plateau = Plateau(orchestrateur=None)

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def rush(self, idee="", prompt_=""):
        return plateau.enregistrer(PNG, {"type": "creation", "idee": idee, "prompt": prompt_})

    def clip(self, **options):
        valeurs = dict(idee="x", prompt_force="A slow push-in.", llm=None, mode="texte", source=None, qualite="compromis",
                       duree=5, format="16:9 (Widescreen)", seed=None, nombre=1)
        valeurs.update(options)
        return self.plateau.commander_clip(**valeurs)


class Commandes(Base):
    def test_texte_et_animer(self):
        c = self.clip()
        self.assertEqual((c.type, c.mode_video, c.workflow, c.parent), ("clip", "texte", "minimax_h3_t2v", None))
        r = self.rush("une pilote")
        c = self.clip(mode="animer", source=r, qualite="qualite", duree=10, nombre=2)
        self.assertEqual((c.workflow, c.parent, c.qualite, c.duree, c.nombre), ("minimax_h3_i2v", r, "qualite", 10, 2))

    def test_validations(self):
        for options, message in (({"mode": "collage"}, "Mode"), ({"qualite": "ultra"}, "Qualité"), ({"duree": 7}, "Durée"),
                                 ({"nombre": 4}, "clips"), ({"mode": "animer"}, "rush"), ({"format": "5:4"}, "Format")):
            with self.assertRaisesRegex(ErreurPlateau, message, msg=options):
                self.clip(**options)

    def test_as18_avec_le_rush_anime(self):
        ado = self.rush("une adolescente de 15 ans", "A teenage girl, 15 years old")
        with self.assertRaises(ErreurSecurite):
            self.clip(mode="animer", source=ado, prompt_force="She slowly takes off her clothes, naked.")
        self.clip(mode="animer", source=ado, prompt_force="She waves and smiles at the camera.")
        with self.assertRaises(ErreurSecurite):
            self.clip(prompt_force="A 12-year-old girl in lingerie.")

    def test_qualites_appliquees_au_graphe(self):
        wf = workflows.obtenir("minimax_h3_t2v")
        for cle, q in plateau.QUALITES_VIDEO.items():
            api = wf.preparer({"prompt": "x", "seed": 1, "duree": 5, "megapixels": q["megapixels"], "steps": q["steps"]})
            noeud, entree = wf.fiche["entrees"]["steps"]
            self.assertEqual(api[noeud]["inputs"][entree], q["steps"], cle)
            noeud, entree = wf.fiche["entrees"]["megapixels"]
            self.assertEqual(api[noeud]["inputs"][entree], q["megapixels"], cle)

    def test_vignette_ajoutee(self):
        wf = workflows.obtenir("minimax_h3_i2v")
        api = wf.preparer({"prompt": "x", "seed": 1, "image": "a.png"})
        wf.ajouter_affiche(api, "aria_montage")
        cv = reglages.noeud_unique(api, "CreateVideo")
        self.assertEqual(api["aria_affiche_lot"]["inputs"]["image"], api[cv]["inputs"]["images"])
        self.assertEqual(api["aria_affiche"]["class_type"], "SaveImage")
        with self.assertRaises(workflows.ErreurWorkflow):
            workflows.obtenir("krea2_turbo").ajouter_affiche(workflows.obtenir("krea2_turbo").preparer({"prompt": "x"}), "p")


class Bobines(Base):
    def test_clips_et_images_separes_dossiers_communs(self):
        image = self.rush("une image")
        clip = plateau.enregistrer_clip(b"mp4", PNG, {"type": "clip", "idee": "un clip"})
        self.assertTrue(plateau.chemin_video(clip).is_file())
        self.assertEqual([f["id"] for f in rushes.liste(genre="images")], [image])
        self.assertEqual([f["id"] for f in rushes.liste(genre="clips")], [clip])
        rushes.creer_dossier("Manga ch.1")
        rushes.agir([image, clip], "ranger", "Manga ch.1")
        self.assertEqual(rushes.etat("images")["dossiers"], [{"nom": "Manga ch.1", "nombre": 1}])
        self.assertEqual(rushes.etat("clips")["dossiers"], [{"nom": "Manga ch.1", "nombre": 1}])

    def test_corbeille_par_genre(self):
        image = self.rush("une image")
        clip = plateau.enregistrer_clip(b"mp4", PNG, {"type": "clip"})
        rushes.agir([image, clip], "corbeille")
        self.assertEqual(rushes.vider_corbeille("clips"), 1)  # l'image reste dans sa corbeille
        self.assertFalse((plateau.DOSSIER / f"{clip}.mp4").exists())
        self.assertEqual(rushes.etat("images")["corbeille"], 1)
        with self.assertRaises(rushes.ErreurRushes):
            rushes.liste(genre="sons")


class Consigne(unittest.TestCase):
    def test_consigne_video(self):
        appels = []

        @asynccontextmanager
        async def llm(modele=None):
            async def generer(messages, **_):
                appels.append(messages)
                return SimpleNamespace(texte='Medium shot. She whispers in French: "On décolle ce soir."',
                                       tokens_sortie=12, ecriture_tps=1.0)
            yield SimpleNamespace(generer=generer, modele=None)

        asyncio.run(prompt.reecrire(SimpleNamespace(llm=llm), "elle murmure « On décolle ce soir. »", None,
                                    video="animer", duree=8))
        systeme, utilisateur = appels[0]
        self.assertIn("Never translate the lines", systeme["content"])
        self.assertTrue(systeme["content"].endswith(prompt.CLAUSE_MINEURS))
        self.assertIn("(Clip: 8 s, animate the provided image)", utilisateur["content"])


if __name__ == "__main__":
    unittest.main()
