"""Vision des LLM : projecteur d'images par famille (app/reglages.py), images ou fiches montrées au LLM (app/prompt.py,
app/plateau.py).

Les projecteurs et les images sont fabriqués dans un dossier temporaire ; le LLM est simulé.
"""

import asyncio
import io
import json
import tempfile
import unittest
from contextlib import asynccontextmanager
from pathlib import Path
from unittest import mock

from PIL import Image

from app import fiches, modeles, plateau, prompt, reglages
from app.llm import Reponse

BONSAI = "Ternary-Bonsai-2-27B-PTQ1_0.gguf"
HERETIC = "Ternary-Bonsai-2-27B-Uncensored-Heretic-PQ2_0.gguf"


def png(largeur: int, hauteur: int) -> bytes:
    sortie = io.BytesIO()
    Image.new("RGB", (largeur, hauteur), "orange").save(sortie, "PNG")
    return sortie.getvalue()


class Projecteur(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        (d / "llm").mkdir()
        for nom, taille in ((BONSAI, 10), (HERETIC, 10), ("Autre-LLM-14B.gguf", 10),
                            ("Ternary-Bonsai-2-27B-mmproj-BF16.gguf", 9), ("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf", 6)):
            (d / "llm" / nom).write_bytes(b"x" * taille)
        self.patchs = [mock.patch.object(reglages, "FICHIER", d / "reglages.yaml"),
                       mock.patch.object(reglages, "_cache", None),
                       mock.patch.object(modeles, "DOSSIER", d)]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def test_le_plus_leger_de_la_famille_pour_le_modele_et_ses_derives(self):
        for llm in (BONSAI, HERETIC):
            self.assertEqual(reglages.projecteur_llm(llm)[0], "Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf", llm)
        self.assertEqual(reglages.projecteur_llm("Autre-LLM-14B.gguf")[0], None)  # pas de sa famille
        self.assertNotIn("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf", modeles.llm_disponibles())  # pas un LLM

    def test_choix_a_la_main(self):
        reglages.modifier_llm(HERETIC, {"vision": "aucune"})
        self.assertEqual(reglages.projecteur_llm(HERETIC), (None, "coupée à la main"))
        reglages.modifier_llm(BONSAI, {"vision": "Ternary-Bonsai-2-27B-mmproj-BF16.gguf"})
        self.assertEqual(reglages.projecteur_llm(BONSAI), ("Ternary-Bonsai-2-27B-mmproj-BF16.gguf", "choisi à la main"))
        with self.assertRaises(reglages.ErreurReglages):
            reglages.modifier_llm(BONSAI, {"vision": "absent-mmproj.gguf"})

    def test_reglages_dit_ce_qui_voit(self):
        llms = {m["fichier"]: m for m in reglages.etat()["llm"]}
        vision = next(p for p in llms[HERETIC]["params"] if p["cle"] == "vision")
        self.assertTrue(vision["aide"].startswith("En service : Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf"))
        aveugle = next(p for p in llms["Autre-LLM-14B.gguf"]["params"] if p["cle"] == "vision")
        self.assertTrue(aveugle["aide"].startswith("Aveugle"))


class Montrer(unittest.TestCase):
    def test_reduite_a_un_megapixel(self):
        vue = prompt.Vue.depuis(png(2048, 2048), "a cat")
        with Image.open(io.BytesIO(vue.png)) as im:
            self.assertLessEqual(im.width * im.height, prompt.PIXELS_VUE)
            self.assertEqual(im.width, im.height)
        petite = png(512, 512)
        self.assertEqual(prompt.Vue.depuis(petite).png, petite)  # déjà assez petite : telle quelle

    def test_le_llm_voit_l_image(self):
        contenu = prompt.montrer("Il se réveille.", [prompt.Vue(b"PNG", "a cat")], "animer", voit=True)
        self.assertEqual(contenu[0]["type"], "image_url")
        self.assertTrue(contenu[0]["image_url"]["url"].startswith("data:image/png;base64,"))
        self.assertIn("first frame", contenu[-1]["text"])
        self.assertNotIn("a cat", contenu[-1]["text"])  # il la voit : pas besoin de sa fiche

    def test_sinon_il_lit_la_fiche(self):
        texte = prompt.montrer("Il se réveille.", [prompt.Vue(b"PNG", "An orange cat asleep.")], "animer", voit=False)
        self.assertIn("created from this prompt: An orange cat asleep.", texte)
        refs = prompt.montrer("la femme de l'image 1", [prompt.Vue(b"", "A woman."), prompt.Vue(b"", None)],
                              "references", voit=False)
        self.assertIn("Picture 1: A woman.", refs)
        self.assertNotIn("Picture 2:", refs)  # importée, sans fiche
        self.assertEqual(prompt.montrer("idée", [prompt.Vue(b"", None)], "retouche", voit=False), "idée")
        self.assertEqual(prompt.montrer("idée", None, "animer", voit=True), "idée")


class FauxLlama:
    def __init__(self, vision):
        self.vision, self.modele, self.appels = vision, None, []

    async def generer(self, messages, **_):
        self.appels.append(messages)
        return Reponse("An orange cat wakes up.", 10, 5, 100.0, 50.0)


class Reecriture(unittest.TestCase):
    def reecrire(self, vision, **options):
        llama = FauxLlama(vision)

        class Orchestrateur:
            @asynccontextmanager
            async def llm(self, modele=None):
                yield llama

        with mock.patch.object(fiches, "specialiste", lambda _: None):
            asyncio.run(prompt.reecrire(Orchestrateur(), "Il se réveille.", "bonsai.gguf", **options))
        return llama.appels[0][-1]["content"]

    def test_montage_animer_avec_ou_sans_vision(self):
        vue = prompt.Vue(b"PNG", "An orange cat asleep on a windowsill.")
        contenu = self.reecrire(Path("mmproj.gguf"), video="animer", vues=[vue])
        self.assertEqual(contenu[0]["type"], "image_url")
        self.assertIn("(Clip: 5 s, animate the provided image)", contenu[-1]["text"])
        self.assertIn("An orange cat asleep", self.reecrire(None, video="animer", vues=[vue]))
        self.assertEqual(self.reecrire(None, video="texte"), "Il se réveille.\n\n(Clip: 5 s, from text only)")


class Description(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.patch = mock.patch.object(plateau, "DOSSIER", Path(self.dossier.name))
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.dossier.cleanup()

    def ecrire(self, id_, f):
        (Path(self.dossier.name) / f"{id_}.json").write_text(json.dumps(f), encoding="utf-8")
        (Path(self.dossier.name) / f"{id_}.png").write_bytes(png(64, 64))

    def test_creation_retouche_et_import(self):
        origine, retouchee, importee = "20261102-010000-aaaaaa", "20261102-020000-bbbbbb", "20261102-030000-cccccc"
        self.ecrire(origine, {"type": "creation", "prompt": "An orange cat asleep."})
        self.ecrire(retouchee, {"type": "retouche", "prompt": "Add a red scarf.", "parent": origine})
        self.ecrire(importee, {"type": "import", "idee": ""})
        self.assertEqual(plateau.vue(origine).texte, "An orange cat asleep.")
        self.assertEqual(plateau.vue(retouchee).texte, "An orange cat asleep. Then edited: Add a red scarf.")
        self.assertIsNone(plateau.vue(importee).texte)
        self.assertIsNone(plateau.description({"type": "retouche", "parent": "20261102-090000-dddddd"}))  # supprimée


class Catalogue(unittest.TestCase):
    def test_bonsai_livre_avec_ses_yeux(self):
        bonsai = fiches.fiche_llm(BONSAI)
        self.assertEqual(bonsai["vision"]["nom"], "Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf")
        self.assertEqual(fiches.problemes_llm(bonsai), [])
        from app import assistant, collection
        self.assertIn("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf", [f["nom"] for f in collection._fichiers_llm(bonsai)])
        self.assertIn("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf",
                      [f["nom"] for f in assistant._fichiers({"llm": [BONSAI]})])


if __name__ == "__main__":
    unittest.main()
