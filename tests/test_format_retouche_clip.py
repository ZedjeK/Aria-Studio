"""Le format choisi au Scénario pour une Retouche ou un Clip animé : l'image de départ est recadrée au centre.

ComfyUI est simulé : on regarde l'image qui lui serait envoyée.
"""

import io
import unittest

from PIL import Image

from app import plateau, prompt, retouche
from app.plateau import ErreurPlateau, FORMAT_IMAGE
from tests import steps_connus
from tests.test_retouche_options import Base as BaseRetouche, FauxComfy, png

setUpModule, tearDownModule = steps_connus.module()


class ComfyQuiGarde(FauxComfy):
    def __init__(self):
        super().__init__()
        self.envoyees = []

    async def televerser(self, fichier):
        self.envoyees.append(fichier)
        return await super().televerser(fichier)


class Recadrage(unittest.TestCase):
    def test_rapports(self):
        image = Image.new("RGB", (64, 48))
        for format_, taille in (("1:1 (Square)", (48, 48)), ("16:9 (Widescreen)", (64, 36)),
                                ("9:16 (Portrait Widescreen)", (27, 48)), ("4:3 (Standard)", (64, 48))):
            self.assertEqual(retouche.recadrer_centre(image, format_).size, taille, format_)

    def test_au_centre_sans_deformer(self):
        image = Image.new("RGB", (30, 10), (255, 0, 0))
        image.paste((0, 0, 255), (10, 0, 20, 10))  # le tiers du milieu est bleu
        carre = retouche.recadrer_centre(image, "1:1 (Square)")
        self.assertEqual(carre.size, (10, 10))
        self.assertEqual(carre.getpixel((5, 5)), (0, 0, 255))

    def test_format_inconnu(self):
        with self.assertRaises(retouche.ErreurRetouche):
            retouche.recadrer_centre(Image.new("RGB", (4, 4)), "n'importe quoi")


class Retouche(BaseRetouche):
    def setUp(self):
        super().setUp()
        self.comfy = ComfyQuiGarde()

    def test_recadree_avant_d_etre_envoyee(self):
        c = self.retouche(seed=1, format="1:1 (Square)")
        fiches = self.tourner(c)
        _, octets = self.comfy.envoyees[0]
        self.assertEqual(Image.open(io.BytesIO(octets)).size, (48, 48))
        self.assertEqual(Image.open(plateau.chemin_image(c.images[0])).size, (48, 48))
        self.assertEqual(fiches[0]["cadrage"], "1:1 (Square)")

    def test_celui_de_l_image_ne_touche_a_rien(self):
        for format_ in (None, FORMAT_IMAGE):
            c = self.retouche(seed=1, format=format_)
            fiche = self.tourner(c)[0]
            self.assertIsNone(c.cadrage)
            self.assertNotIn("cadrage", fiche)
            self.assertEqual(Image.open(io.BytesIO(self.comfy.envoyees[-1][1])).size, (64, 48))

    def test_refus(self):
        with self.assertRaisesRegex(ErreurPlateau, "Format inconnu"):
            self.retouche(format="5:4")
        with self.assertRaisesRegex(ErreurPlateau, "masque"):
            self.plateau.commander_retouche(self.source, "add a moon", False, None, None, png(), 24, 1,
                                            format="1:1 (Square)")


class ClipAnime(BaseRetouche):
    def clip(self, **options):
        valeurs = dict(idee="x", prompt_clip="A slow push-in.", mode="animer", source=self.source, qualite="compromis",
                       duree=5, format=FORMAT_IMAGE, seed=None, nombre=1)
        valeurs.update(options)
        return self.plateau.commander_clip(**valeurs)

    def test_cadrage_selon_le_choix(self):
        self.assertIsNone(self.clip().cadrage)
        self.assertEqual(self.clip(format="9:16 (Portrait Widescreen)").cadrage, "9:16 (Portrait Widescreen)")
        self.assertIsNone(self.clip(mode="texte", source=None, format="16:9 (Widescreen)").cadrage)

    def test_formats_refuses(self):
        with self.assertRaisesRegex(ErreurPlateau, "Format inconnu"):
            self.clip(format="5:4")
        with self.assertRaisesRegex(ErreurPlateau, "Format inconnu"):
            self.clip(mode="texte", source=None, format=FORMAT_IMAGE)  # « celui de l'image » n'a pas de sens sans image

    def test_image_de_depart(self):
        self.assertEqual(plateau.Plateau._image_de_depart(self.clip()), plateau.chemin_image(self.source))
        nom, octets = plateau.Plateau._image_de_depart(self.clip(format="1:1 (Square)"))
        self.assertTrue(nom.endswith(".png"))
        self.assertEqual(Image.open(io.BytesIO(octets)).size, (48, 48))


class DansLaConsigneDuLlm(unittest.TestCase):
    def test_retouche_dit_que_l_image_est_recadree(self):
        texte = prompt.cadre_format("9:16 (Portrait Widescreen)", "retouche")
        self.assertIn("cropped", texte)
        self.assertIn("portrait", texte)
        self.assertNotIn("cropped", prompt.cadre_format("9:16 (Portrait Widescreen)", "image"))
        self.assertEqual(prompt.cadre_format(None, "retouche"), "")


if __name__ == "__main__":
    unittest.main()
