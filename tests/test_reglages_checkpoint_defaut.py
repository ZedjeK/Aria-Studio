"""Le checkpoint par défaut d'un workflow se choisit dans Modèles (☆ Par défaut) ; l'onglet Réglages le montre en
lecture et mène à la carte du workflow, dans l'onglet d'usage de sa tâche. La table tâche → onglet de web/modeles.js
doit suivre celle du serveur (app/collection.py, USAGES) : une tâche oubliée ouvrirait le mauvais onglet."""

import re
import unittest
from pathlib import Path

from app import collection, reglages, workflows

WEB = Path(__file__).resolve().parent.parent / "web"


def usage_de_tache_du_script() -> dict[str, str]:
    script = (WEB / "modeles.js").read_text(encoding="utf-8")
    bloc = re.search(r"const USAGE_DE_TACHE = \{(.*?)\};", script, re.DOTALL).group(1)
    return dict(re.findall(r'(\w+): "(\w+)"', bloc))


class OuvrirDansModeles(unittest.TestCase):
    def test_chaque_tache_mene_a_l_onglet_d_usage_du_serveur(self):
        attendu = {tache: usage for usage, u in collection.USAGES.items() for tache in u["taches"]}
        self.assertEqual(usage_de_tache_du_script(), attendu)

    def test_chaque_workflow_a_checkpoint_a_son_onglet(self):
        table = usage_de_tache_du_script()
        for wf in workflows.catalogue():
            if "modele" in reglages.params_workflow(wf):
                self.assertIn(wf.fiche["tache"], table, wf.id)

    def test_chaque_onglet_d_usage_existe_dans_la_page(self):
        page = (WEB / "index.html").read_text(encoding="utf-8")
        for usage in set(usage_de_tache_du_script().values()):
            self.assertIn(f'data-sous="{usage}"', page, usage)
            self.assertIn(f'data-usage="{usage}"', page, usage)

    def test_la_carte_d_un_workflow_se_retrouve_par_son_identifiant(self):
        script = (WEB / "modeles.js").read_text(encoding="utf-8")
        self.assertIn('class="carte workflow-usage" data-carte-workflow="${echapper(wf.id)}"', script)
        self.assertIn('.workflow-usage[data-carte-workflow="${CSS.escape(workflowAMontrer)}"]', script)


class EnLectureDansLesReglages(unittest.TestCase):
    def test_le_reglage_porte_le_nom_affiche(self):
        for wf in workflows.catalogue():
            self.assertEqual(reglages.params_workflow(wf)["modele"].libelle, "Checkpoint par défaut", wf.id)

    def test_la_page_a_son_emplacement_et_le_script_ne_le_range_plus_dans_les_champs(self):
        page = (WEB / "index.html").read_text(encoding="utf-8")
        self.assertEqual(page.count('id="defaut-workflow"'), 1)
        script = (WEB / "reglages.js").read_text(encoding="utf-8")
        self.assertIn('const reglables = (w) => w.params.filter((p) => p.cle !== "modele");', script)
        # Les champs de la carte, le compteur de changements et l'enregistrement ne voient que les réglages réglables ici.
        self.assertNotIn("workflowAffiche().params)", script)
        self.assertNotIn("w.params.map(", script)


if __name__ == "__main__":
    unittest.main()
