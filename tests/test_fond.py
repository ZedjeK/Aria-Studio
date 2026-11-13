"""Tests des réglages du fond animé (app/fond.py) : section « fond » d'un reglages.json temporaire."""

import json
import tempfile
import unittest
from pathlib import Path

from app import fichier_reglages, fond
from tests import reglages_jetables


class TestFond(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.patch = reglages_jetables.patch(Path(self.dossier.name))
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.dossier.cleanup()

    def test_actif_normal_de_2_a_5_par_defaut(self):
        self.assertEqual(fond.etat(), {"actif": True, "vitesse": "normale", "minimum": 2, "maximum": 5})

    def test_le_fichier_porte_les_noms_du_contrat(self):
        fond.regler(actif=False, vitesse="rapide", minimum=3, maximum=8)
        ecrit = json.loads(fichier_reglages.fichier().read_text(encoding="utf-8"))
        self.assertEqual(ecrit["fond"], {"actif": False, "vitesse": "rapide", "minimum": 3, "maximum": 8})

    def test_regler_garde_les_autres_choix(self):
        fond.regler(vitesse="lente")
        self.assertEqual(fond.regler(actif=False), {"actif": False, "vitesse": "lente", "minimum": 2, "maximum": 5})
        self.assertEqual(fond.etat()["vitesse"], "lente")

    def test_les_nombres_restent_entre_1_et_12(self):
        self.assertEqual(fond.regler(minimum=0, maximum=99)["minimum"], 1)
        self.assertEqual(fond.etat()["maximum"], 12)

    def test_des_bornes_inversees_sont_remises_dans_l_ordre(self):
        d = fond.regler(minimum=9, maximum=4)
        self.assertEqual((d["minimum"], d["maximum"]), (4, 9))

    def test_une_vitesse_inconnue_est_refusee_et_ne_change_rien(self):
        fond.regler(vitesse="rapide")
        with self.assertRaises(ValueError):
            fond.regler(vitesse="turbo")
        self.assertEqual(fond.etat()["vitesse"], "rapide")

    def test_fichier_illisible(self):
        defauts = {"actif": True, "vitesse": "normale", "minimum": 2, "maximum": 5}
        fichier_reglages.fichier().write_text("pas du json", encoding="utf-8")
        self.assertEqual(fond.etat(), defauts)

    def test_section_modifiee_a_la_main(self):
        fichier_reglages.ecrire("fond", {"actif": "oui", "vitesse": "turbo", "minimum": "3", "maximum": True})
        self.assertEqual(fond.etat(), {"actif": True, "vitesse": "normale", "minimum": 2, "maximum": 5})

    def test_un_decimal_n_est_pas_un_nombre_de_gouttes(self):
        fichier_reglages.ecrire("fond", {"minimum": 3.5, "maximum": 6})
        self.assertEqual(fond.etat()["minimum"], 2)
        self.assertEqual(fond.etat()["maximum"], 6)


if __name__ == "__main__":
    unittest.main()
