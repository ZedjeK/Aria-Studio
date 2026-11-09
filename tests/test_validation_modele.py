"""Un checkpoint téléchargé n'est proposé nulle part tant que l'utilisateur n'a pas validé ses steps ; la suggestion
(lue dans le texte de la source) n'est jamais appliquée seule et se déclare peu fiable dès qu'elle n'est pas univoque."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import collection, modeles, reglages, workflows
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
    """Un checkpoint n'est proposé que si ses steps sont connus : validés par l'utilisateur, ou clairs dans la fiche de
    son workflow (une seule valeur). L'origine de H3 dit 8 par défaut et 12 recommandé : elle attend aussi."""

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

    def test_rien_n_est_propose_avant_validation(self):
        self.assertEqual(self.wf.variantes(), [])
        self.assertEqual(self.wf.en_attente(), [ORIGINE, NEUF])
        # Présents mais à valider : on ne retélécharge pas l'origine pour autant.
        self.assertFalse(any(f["dossier"] == "MiniMax-H3/modeles" for f in self.wf.manquants()))

    def test_fiche_claire_ou_pas(self):
        self.assertEqual(self.wf.steps_clairs(), 0)  # H3 : 8 par défaut, 12 recommandé
        self.assertEqual(workflows.obtenir("krea2_turbo").steps_clairs(), 8)  # 8 et 8
        self.assertEqual(workflows.obtenir("qwen_image_2_1").steps_clairs(), 0)  # 25 et 40

    def test_un_checkpoint_a_valider_est_refuse_au_tournage(self):
        with self.assertRaisesRegex(workflows.ErreurWorkflow, "à valider"):
            self.wf.preparer({"prompt": "A pilot smiles.", "seed": 1, "image": "a.png"})
        with self.assertRaisesRegex(workflows.ErreurWorkflow, "à valider"):
            self.wf.preparer({"prompt": "A pilot smiles.", "seed": 1, "image": "a.png", "modele": NEUF})

    def test_valide_il_est_propose_partout_avec_ses_steps(self):
        workflows.valider_checkpoint(CLE, 8)
        for id_ in (H3, "minimax_h3_t2v"):  # toute la famille
            wf = workflows.obtenir(id_)
            self.assertEqual(wf.variantes(), [NEUF])
            self.assertEqual(wf.steps_de(NEUF), 8)
        api = self.wf.preparer({"prompt": "A pilot smiles.", "seed": 1, "image": "a.png", "modele": NEUF})
        self.assertEqual(next(v for v in api.values() if v["class_type"] == "BasicScheduler")["inputs"]["steps"], 8)
        self.assertIsNone(modeles.suggestion_steps(CLE))  # la suggestion a servi

    def test_l_origine_se_valide_comme_les_autres(self):
        workflows.valider_checkpoint(f"MiniMax-H3/modeles/{ORIGINE}", 12)
        self.assertEqual(self.wf.variantes(), [ORIGINE])
        self.assertEqual(self.wf.steps_de(ORIGINE), 12)

    def test_steps_invalides_ne_mettent_rien_en_service(self):
        for steps in (0, 151, -1):
            with self.assertRaises(workflows.ErreurWorkflow):
                workflows.valider_checkpoint(CLE, steps)
        self.assertEqual(self.wf.variantes(), [])
        self.assertIsNotNone(modeles.suggestion_steps(CLE))

    def test_checkpoint_absent_refuse(self):
        with self.assertRaises(workflows.ErreurWorkflow):
            workflows.valider_checkpoint("MiniMax-H3/modeles/inconnu.safetensors", 8)

    def test_remettre_les_steps_a_zero_le_remet_en_attente(self):
        workflows.valider_checkpoint(CLE, 8)
        reglages.modifier_variante(H3, NEUF, {"steps": 0})
        self.assertIn(NEUF, self.wf.en_attente())

    def test_ma_collection_dit_quoi_decider(self):
        etat = collection._steps_des_checkpoints([self.wf])
        origine = etat[f"MiniMax-H3/modeles/{ORIGINE}"]["a_valider"]
        self.assertEqual([f["valeur"] for f in origine["fiche"]], [8, 12])  # la fiche se contredit : à toi
        self.assertIsNone(origine["steps"])
        neuf = etat[CLE]["a_valider"]
        self.assertEqual((neuf["fiche"], neuf["steps"]["valeurs"]), ([], [8]))  # suggestion lue sur sa page
        workflows.valider_checkpoint(CLE, 8)
        self.assertEqual(collection._steps_des_checkpoints([self.wf])[CLE], {"valides": 8})

    def test_seuls_les_checkpoints_de_famille_attendent(self):
        self.assertTrue(modeles.est_checkpoint("MiniMax-H3/modeles"))
        self.assertFalse(modeles.est_checkpoint("MiniMax-H3/loras"))
        self.assertFalse(modeles.est_checkpoint("llm"))


if __name__ == "__main__":
    unittest.main()
