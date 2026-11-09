"""Estimation du tournage (app/estimation.py) : calculée d'après le checkpoint, la qualité, la durée, le nombre de clips
et la VRAM libre ; identique au tableau d'origine sur la machine de référence.

Origine : l'estimation du Montage ne collait pas au temps réel d'une bobine (tableau mesuré une fois, sur une RTX 4080,
sans le checkpoint ni la VRAM). Rien n'est lancé : la VRAM libre est donnée, jamais lue.
"""

import unittest
from unittest import mock

from app import estimation, plateau

CONFIG = {"generation": {"image_vers_video": "minimax_h3_i2v", "texte_vers_video": "minimax_h3_t2v"}}
ORIGINE = "minimax_h3_fl2va_pruned_int8_convrot.safetensors"


class Estimation(unittest.TestCase):
    def estimer(self, qualite="compromis", duree=5, nombre=1, libre=estimation.LIBRE_REF_GO, mode="animer",
                modele=ORIGINE, steps=None, poids=estimation.POIDS_REF_GO):
        variante = {"steps": steps} if steps else {}
        with (mock.patch.object(estimation, "CONFIG", CONFIG),
              mock.patch.object(estimation, "reglages_variante", return_value=variante),
              mock.patch.object(estimation, "_poids_go", return_value=poids)):
            return estimation.estimer(mode, modele, qualite, duree, nombre, libre_go=libre)

    def test_machine_de_reference_redonne_le_tableau(self):
        for cle, q in plateau.QUALITES_VIDEO.items():
            r = self.estimer(cle)
            self.assertEqual((r["clip_s"], r["facteur_vram"]), (q["estimation_s"], 1.0), cle)
            self.assertEqual(r["source"], "calcul")

    def test_la_duree_garde_l_exposant_mesure(self):
        e = plateau.EXPOSANT_DUREE_VIDEO
        self.assertEqual(self.estimer("rapide", duree=10)["clip_s"], round(80 * 2 ** e))
        self.assertEqual(self.estimer("rapide", duree=15)["clip_s"], round(80 * 3 ** e))

    def test_un_clip_reel_de_15_s_en_compromis(self):
        """Relevé le 2026-11-09, checkpoint d'origine : 15 s en Compromis (0,6 MP, 12 steps) ont pris 15 min 57 (957 s).
        L'exposant de durée est ajusté sur ce clip : un seul réglage, à 5 % près."""
        self.assertAlmostEqual(self.estimer("compromis", duree=15)["clip_s"], 957, delta=957 * 0.05)

    def test_le_nombre_de_clips_multiplie(self):
        un, trois = self.estimer("qualite"), self.estimer("qualite", nombre=3)
        self.assertEqual((trois["clip_s"], trois["total_s"]), (un["clip_s"], 3 * un["clip_s"]))

    def test_les_steps_du_checkpoint_comptent(self):
        base = self.estimer("compromis")["clip_s"]  # 12 steps
        self.assertEqual(self.estimer("compromis", steps=8)["clip_s"], round(base * 8 / 12))
        self.assertEqual(self.estimer("compromis", steps=8)["steps"], 8)

    def test_la_vram_libre_compte(self):
        moins, ref, plus = (self.estimer("qualite", libre=v)["clip_s"] for v in (10, 15, 24))
        self.assertGreater(moins, ref)
        self.assertGreater(ref, plus)
        # Tout tient en VRAM : plus de ralentissement à enlever, une carte plus grande n'y gagne plus.
        self.assertEqual(self.estimer("qualite", libre=48)["clip_s"], self.estimer("qualite", libre=80)["clip_s"])

    def test_un_checkpoint_plus_leger_va_plus_vite(self):
        self.assertLess(self.estimer("qualite", poids=12.0)["clip_s"], self.estimer("qualite")["clip_s"])

    def test_sans_lecture_de_vram_le_tableau(self):
        with (mock.patch.object(estimation, "CONFIG", CONFIG),
              mock.patch.object(estimation, "reglages_variante", return_value={}),
              mock.patch.object(estimation, "_poids_go", return_value=19.5),
              mock.patch.object(estimation, "_vram_libre_go", return_value=None)):
            r = estimation.estimer("animer", ORIGINE, "compromis", 5, 2)
        self.assertEqual((r["source"], r["clip_s"], r["total_s"]), ("tableau_d_origine", 170, 340))

    def test_entrees_inconnues(self):
        with self.assertRaises(ValueError):
            estimation.estimer("animer", None, "inconnue", 5)
        with self.assertRaises(ValueError):
            estimation.estimer("sequence", None, "rapide", 5)


if __name__ == "__main__":
    unittest.main()
