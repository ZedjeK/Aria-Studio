"""Un checkpoint téléchargé n'est proposé nulle part tant que l'utilisateur n'a pas validé ses steps ; la suggestion
(lue dans le texte de la source) n'est jamais appliquée seule et se déclare peu fiable dès qu'elle n'est pas univoque."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import modeles, reglages, workflows
from app.sources import steps

H3 = "minimax_h3_i2v"
NEUF = "10Eros_Max_h3_8stepTURBO_beta7.safetensors"
ORIGINE = "minimax_h3_fl2va_pruned_int8_convrot.safetensors"
CLE = f"MiniMax-H3/modeles/{NEUF}"


def deposes(dossier, extensions):
    return sorted([ORIGINE, NEUF]) if dossier == "MiniMax-H3/modeles" else []


class Suggestion(unittest.TestCase):
    def test_un_seul_nombre_est_unique(self):
        s = steps.suggerer({"nom du fichier": NEUF})
        self.assertEqual((s["statut"], s["valeurs"]), ("unique", [8]))

    def test_plusieurs_nombres_sont_ambigus(self):
        s = steps.suggerer({"description": "Use 4 steps for speed, 8 steps for quality"})
        self.assertEqual((s["statut"], s["valeurs"]), ("ambigu", [4, 8]))

    def test_rien_trouve(self):
        self.assertEqual(steps.suggerer({"description": "Great model, enjoy"})["statut"], "aucun")

    def test_un_tutoriel_n_est_pas_un_nombre_de_steps(self):
        self.assertEqual(steps.suggerer({"readme": "Step 1: download. Step 2: install."})["statut"], "aucun")

    def test_valeurs_hors_bornes_ignorees(self):
        self.assertEqual(steps.suggerer({"d": "trained for 5000 steps, 0 steps"})["statut"], "aucun")

    def test_balises_html_retirees(self):
        s = steps.suggerer({"d": "<p>Use <b>8</b> steps</p>"})
        self.assertEqual(s["valeurs"], [8])


class Validation(unittest.TestCase):
    def setUp(self):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        dossier = Path(d.name)
        for p in (mock.patch.object(reglages, "FICHIER", dossier / "reglages.yaml"),
                  mock.patch.object(reglages, "_cache", None),
                  mock.patch.object(modeles, "fichiers", side_effect=deposes),
                  mock.patch.object(modeles, "REGISTRE", dossier / "registre.json")):
            p.start()
            self.addCleanup(p.stop)
        modeles.REGISTRE.write_text(
            '{"%s": {"source": "civitai", "a_valider": {"steps": {"statut": "unique", "valeurs": [8], "extraits": []}}}}' % CLE,
            encoding="utf-8")
        self.wf = workflows.obtenir(H3)

    def test_en_attente_il_n_est_pas_propose(self):
        self.assertNotIn(NEUF, self.wf.variantes())
        self.assertIn(ORIGINE, self.wf.variantes())

    def test_valide_il_est_propose_avec_ses_steps(self):
        workflows.valider_checkpoint(CLE, 8)
        self.assertIn(NEUF, self.wf.variantes())
        self.assertEqual(reglages.valeurs_variante(self.wf, NEUF)["steps"], 8)
        self.assertFalse(modeles.en_attente("MiniMax-H3/modeles", NEUF))

    def test_valider_avec_zero_garde_les_steps_du_reglage(self):
        workflows.valider_checkpoint(CLE, 0)
        self.assertIn(NEUF, self.wf.variantes())
        self.assertEqual(reglages.valeurs_variante(self.wf, NEUF)["steps"], 0)

    def test_steps_invalides_ne_mettent_rien_en_service(self):
        with self.assertRaises(workflows.ErreurWorkflow):
            workflows.valider_checkpoint(CLE, 500)
        self.assertNotIn(NEUF, self.wf.variantes())

    def test_valider_deux_fois_est_refuse(self):
        workflows.valider_checkpoint(CLE, 8)
        with self.assertRaises(modeles.ErreurModele):
            workflows.valider_checkpoint(CLE, 8)

    def test_un_fichier_depose_a_la_main_n_attend_rien(self):
        self.assertFalse(modeles.en_attente("MiniMax-H3/modeles", ORIGINE))

    def test_seuls_les_checkpoints_de_famille_attendent(self):
        self.assertTrue(modeles.est_checkpoint("MiniMax-H3/modeles"))
        self.assertFalse(modeles.est_checkpoint("MiniMax-H3/loras"))
        self.assertFalse(modeles.est_checkpoint("llm"))


if __name__ == "__main__":
    unittest.main()
