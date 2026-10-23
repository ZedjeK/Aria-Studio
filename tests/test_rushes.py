"""Tests du rangement des rushes (app/rushes.py) : dossiers, archives, corbeille, vues.

Les rushes et la liste des dossiers vivent dans un dossier temporaire.
"""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import plateau, rushes
from app.rushes import ErreurRushes

PNG = bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010806000000") + bytes(16)


class Rushes(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json")]
        for p in self.patchs:
            p.start()
        self.ids = [plateau.enregistrer(PNG, {"type": "creation", "idee": f"image {i}"}) for i in range(4)]

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def vue(self, vue="tous", dossier=None):
        return sorted(f["id"] for f in rushes.liste(vue, dossier))

    def test_dossiers(self):
        a, b, c, d = self.ids
        rushes.creer_dossier("Manga ch.1")
        rushes.agir([a, b], "ranger", "Manga ch.1")
        self.assertEqual(self.vue(dossier="Manga ch.1"), sorted([a, b]))
        self.assertEqual(self.vue("non_ranges"), sorted([c, d]))
        self.assertEqual(len(self.vue()), 4)  # « Tous » montre aussi ce qui est rangé
        rushes.renommer_dossier("Manga ch.1", "Chapitre 1")
        self.assertEqual(self.vue(dossier="Chapitre 1"), sorted([a, b]))
        self.assertEqual(rushes.supprimer_dossier("Chapitre 1"), 2)  # les images restent, non rangées
        self.assertEqual(len(self.vue("non_ranges")), 4)
        self.assertEqual(rushes.dossiers(), [])

    def test_un_seul_dossier_par_rush(self):
        a = self.ids[0]
        for nom in ("Dragons", "Pilotes"):
            rushes.creer_dossier(nom)
            rushes.agir([a], "ranger", nom)
        self.assertEqual(plateau.fiche(a)["dossier"], "Pilotes")
        rushes.agir([a], "ranger", None)
        self.assertNotIn("dossier", plateau.fiche(a))

    def test_archives(self):
        a, b, *_ = self.ids
        rushes.creer_dossier("Dragons")
        rushes.agir([a], "ranger", "Dragons")
        rushes.agir([a, b], "archiver")
        self.assertEqual(self.vue("archives"), sorted([a, b]))
        self.assertNotIn(a, self.vue())
        self.assertEqual(self.vue(dossier="Dragons"), [])  # archivé : sorti aussi de son dossier
        rushes.agir([a], "desarchiver")
        self.assertEqual(self.vue(dossier="Dragons"), [a])

    def test_corbeille(self):
        a, b, *_ = self.ids
        rushes.agir([a, b], "corbeille")
        self.assertEqual(self.vue("corbeille"), sorted([a, b]))
        self.assertNotIn(a, self.vue())
        rushes.agir([b], "restaurer")
        self.assertIn(b, self.vue())
        self.assertEqual(rushes.vider_corbeille(), 1)  # seule suppression définitive
        self.assertFalse((plateau.DOSSIER / f"{a}.png").exists())
        self.assertTrue((plateau.DOSSIER / f"{b}.png").exists())
        self.assertEqual(rushes.etat()["corbeille"], 0)

    def test_compteurs(self):
        a, b, c, _ = self.ids
        rushes.creer_dossier("Dragons")
        rushes.agir([a], "ranger", "Dragons")
        rushes.agir([b], "archiver")
        rushes.agir([c], "corbeille")
        e = rushes.etat()
        self.assertEqual((e["tous"], e["non_ranges"], e["archives"], e["corbeille"]), (2, 1, 1, 1))
        self.assertEqual(e["dossiers"], [{"nom": "Dragons", "nombre": 1}])

    def test_refus(self):
        a = self.ids[0]
        for nom in ("", "../x", "a" * 41, "<b>"):
            with self.assertRaises(ErreurRushes):
                rushes.creer_dossier(nom)
        rushes.creer_dossier("Dragons")
        with self.assertRaises(ErreurRushes):
            rushes.creer_dossier("Dragons")
        with self.assertRaises(ErreurRushes):
            rushes.agir([a], "ranger", "Inconnu")
        with self.assertRaises(ErreurRushes):
            rushes.agir([a], "teleporter")
        with self.assertRaises(plateau.ErreurPlateau):  # rien n'est modifié si un id est faux
            rushes.agir([a, "20990101-000000-abcdef"], "corbeille")
        self.assertNotIn("corbeille", plateau.fiche(a))
        with self.assertRaises(ErreurRushes):
            rushes.liste("partout")
        with self.assertRaises(ErreurRushes):
            rushes.valider_dossier("Inconnu")

    def test_import_range_dans_le_dossier_ouvert(self):
        import io
        from PIL import Image
        tampon = io.BytesIO()
        Image.new("RGB", (32, 24), (255, 46, 136)).save(tampon, format="PNG")
        rushes.creer_dossier("Dragons")
        id_import = plateau.importer(tampon.getvalue(), "photo.png", rushes.valider_dossier("Dragons"))
        self.assertEqual(self.vue(dossier="Dragons"), [id_import])
        self.assertIsNone(rushes.valider_dossier(""))


if __name__ == "__main__":
    unittest.main()
