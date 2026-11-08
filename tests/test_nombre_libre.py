"""Le nombre d'images (Plateau) et de clips (Montage) se tape : le serveur annonce son maximum, le garde-fou refuse
au-delà, et une commande de 20 clips ou de 50 images est acceptée. Rien n'est envoyé à ComfyUI."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import plateau, rushes, workflows
from app.plateau import ErreurPlateau, Plateau


class Base(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        d = Path(self.temp.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json"),
                       mock.patch.object(workflows.Workflow, "manquants", lambda self: [])]
        for p in self.patchs:
            p.start()
        self.plateau = Plateau(orchestrateur=None)

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.temp.cleanup()

    def images(self, nombre):
        return self.plateau.commander("un chat", "A cat.", None, "krea2_turbo", "1:1 (Square)", None, nombre)

    def clips(self, nombre):
        return self.plateau.commander_clip("un chat", "A slow push-in.", "texte", None, "rapide", 5,
                                           "16:9 (Widescreen)", None, nombre)


class Images(Base):
    def test_le_plafond_est_celui_que_la_page_recoit(self):
        self.assertEqual((plateau.NOMBRE_MAX, self.plateau.options()["nombre_max"]), (50, 50))

    def test_de_1_a_50(self):
        for n in (1, 7, plateau.NOMBRE_MAX):
            self.assertEqual(self.images(n).nombre, n)

    def test_au_dela_ou_en_dessous_refuse(self):
        for n in (0, -3, plateau.NOMBRE_MAX + 1, 400):
            with self.assertRaisesRegex(ErreurPlateau, f"entre 1 et {plateau.NOMBRE_MAX}"):
                self.images(n)


class Clips(Base):
    def test_le_plafond_est_celui_que_la_page_recoit(self):
        with mock.patch.object(plateau.modeles, "llm_disponibles", return_value=[]):
            options = self.plateau.options()
        self.assertEqual((plateau.CLIPS_MAX, options["video"]["nombre_max"]), (20, 20))

    def test_de_1_a_20(self):
        for n in (1, 5, plateau.CLIPS_MAX):
            self.assertEqual(self.clips(n).nombre, n)

    def test_au_dela_ou_en_dessous_refuse(self):
        for n in (0, -1, plateau.CLIPS_MAX + 1, 100):
            with self.assertRaisesRegex(ErreurPlateau, f"De 1 à {plateau.CLIPS_MAX} clips"):
                self.clips(n)


if __name__ == "__main__":
    unittest.main()
