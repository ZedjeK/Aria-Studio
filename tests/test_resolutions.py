"""Résolutions de MiniMax H3 réglables (Réglages) : trois valeurs de mégapixels, d'accord entre les deux workflows."""

import unittest
from unittest import mock

from app import estimation, plateau, reglages, sequences, workflows
from app.reglages import ErreurReglages
from tests import steps_connus
from tests.test_reglages import AvecFichierTemporaire

setUpModule, tearDownModule = steps_connus.module()

I2V, T2V = "minimax_h3_i2v", "minimax_h3_t2v"


class Reglage(AvecFichierTemporaire):
    def test_trois_champs_pour_les_workflows_video_seulement(self):
        for id_ in (I2V, T2V):
            cles = reglages.params_workflow(workflows.obtenir(id_))
            for cle in plateau.QUALITES_VIDEO:
                self.assertIn(f"resolution_{cle}", cles)
        self.assertFalse([c for c in reglages.params_workflow(workflows.obtenir("krea2_turbo")) if c.startswith("resolution_")])

    def test_valeurs_d_origine(self):
        v = reglages.valeurs_workflow(workflows.obtenir(I2V))
        self.assertEqual([v[f"resolution_{c}"] for c in plateau.QUALITES_VIDEO], [0.4, 0.6, 0.98])
        self.assertEqual(plateau.qualites_video(), plateau.QUALITES_VIDEO)  # rien de changé : le tableau d'origine

    def test_un_changement_suit_dans_les_deux_workflows(self):
        reglages.modifier_workflow(T2V, {"resolution_compromis": 0.5})
        for id_ in (I2V, T2V):
            self.assertEqual(reglages.valeurs_workflow(workflows.obtenir(id_))["resolution_compromis"], 0.5)

    def test_retour_a_l_origine(self):
        reglages.modifier_workflow(I2V, {"resolution_rapide": 0.3})
        reglages.modifier_workflow(I2V, {"resolution_rapide": 0.4})
        self.assertNotIn("resolution_rapide", reglages.lire()["workflows"].get(I2V, {}))
        self.assertNotIn("resolution_rapide", reglages.lire()["workflows"].get(T2V, {}))

    def test_bornes(self):
        for valeur in (0.1, 2.5, "grand"):
            with self.assertRaises(ErreurReglages):
                reglages.modifier_workflow(I2V, {"resolution_qualite": valeur})

    def test_les_autres_workflows_refusent_ces_champs(self):
        with self.assertRaises(ErreurReglages):
            reglages.modifier_workflow("krea2_turbo", {"resolution_rapide": 0.5})


class Effets(AvecFichierTemporaire):
    def test_nom_aide_et_mp_suivent(self):
        reglages.modifier_workflow(I2V, {"resolution_compromis": 0.75})
        q = plateau.qualites_video()
        self.assertEqual(q["compromis"]["nom"], "0,75 MP")
        self.assertEqual(q["compromis"]["megapixels"], 0.75)
        self.assertEqual(q["compromis"]["aide"], "réglé dans Réglages")
        self.assertEqual(q["rapide"], plateau.QUALITES_VIDEO["rapide"])  # les autres boutons ne bougent pas

    def test_le_temps_est_celui_de_la_mesure_a_la_valeur_d_origine_et_croit_avec_la_taille(self):
        for cle, q in plateau.QUALITES_VIDEO.items():
            self.assertAlmostEqual(plateau._temps_a_12_steps(q["megapixels"]) * q["steps"] / 12, q["estimation_s"], places=6)
        temps = [plateau._temps_a_12_steps(mp) for mp in (0.25, 0.4, 0.5, 0.6, 0.8, 0.98, 1.5, 2.0)]
        self.assertEqual(temps, sorted(temps))

    def test_l_estimation_du_montage_suit(self):
        base = estimation.estimer("animer", None, "compromis", 5, libre_go=24)["clip_s"]
        reglages.modifier_workflow(I2V, {"resolution_compromis": 0.98})
        plus = estimation.estimer("animer", None, "compromis", 5, libre_go=24)["clip_s"]
        self.assertGreater(plus, base * 1.5)

    def test_les_dimensions_des_sequences_suivent(self):
        s = {"qualite": "rapide", "format": "16:9 (Widescreen)", "plans": [{"enchainement": "texte"}]}
        avant = sequences.dimensions_sequence(s)
        reglages.modifier_workflow(I2V, {"resolution_rapide": 0.8})
        apres = sequences.dimensions_sequence(s)
        self.assertGreater(apres[0] * apres[1], avant[0] * avant[1])

    def test_noms_des_trois_boutons(self):
        reglages.modifier_workflow(I2V, {"resolution_qualite": 1.2})
        self.assertEqual([q["nom"] for q in plateau.qualites_video().values()], ["0,4 MP", "0,6 MP", "1,2 MP"])

    def test_workflow_absent_garde_l_origine(self):
        with mock.patch.object(workflows, "obtenir", side_effect=KeyError("x")):
            self.assertEqual(plateau.mp_video_reglees(), {"rapide": 0.4, "compromis": 0.6, "qualite": 0.98})


if __name__ == "__main__":
    unittest.main()
