"""Page Casting : deux sous-onglets (lancer, comparer), chacun son commentaire « À savoir », son panneau, et un seul
bouton orange. Lecture de web/index.html et de web/casting.js : rien n'est lancé."""

import re
import unittest
from pathlib import Path

WEB = Path(__file__).resolve().parent.parent / "web"


def vue_casting() -> str:
    page = (WEB / "index.html").read_text(encoding="utf-8")
    debut = page.index('<div class="vue" id="vue-casting"')
    return page[debut:page.index("</main>", debut)]


class SousOnglets(unittest.TestCase):
    def setUp(self):
        self.vue = vue_casting()
        self.script = (WEB / "casting.js").read_text(encoding="utf-8")

    def test_deux_sous_onglets_avec_leur_commentaire(self):
        onglets = re.findall(r'<button class="sous-onglet[^"]*" type="button" data-sous="(\w+)" data-savoir="([^"]+)"', self.vue)
        self.assertEqual([nom for nom, _ in onglets], ["lancer", "comparer"])
        self.assertTrue(all(len(texte) > 40 for _, texte in onglets))
        self.assertIn('<div class="savoir" data-pour="#vue-casting .sous-onglets">', self.vue)

    def test_un_panneau_par_sous_onglet_et_un_seul_visible_au_depart(self):
        panneaux = re.findall(r'<div data-panneau-casting="(\w+)"( hidden)?>', self.vue)
        self.assertEqual(panneaux, [("lancer", ""), ("comparer", " hidden")])
        self.assertEqual(self.vue.count('class="sous-onglet actif"'), 1)

    def test_le_panneau_accentue_est_celui_du_bouton_orange(self):
        lancer, comparer = self.vue.split('<div data-panneau-casting="comparer"')
        self.assertEqual(lancer.count('class="carte accent"'), 1)
        self.assertEqual(lancer.count('class="principal"'), 1)
        self.assertIn('id="lancer-banc" class="principal"', lancer)
        # Comparer : l'accent suit « Révéler les modèles », posé par le script tant que le casting est à l'aveugle.
        self.assertNotIn("accent", comparer)
        self.assertIn('id="carte-banc"', comparer)
        self.assertIn('getElementById("carte-banc").classList.toggle("accent", !b.revele)', self.script)
        self.assertIn('<button id="reveler" class="principal"', self.script)

    def test_plus_de_liste_cachee(self):
        self.assertNotIn("<select", self.vue)
        for ancien in ("tache-banc", "liste-bancs"):
            self.assertNotIn(ancien, self.script)
        self.assertNotIn("liste-bancs", (WEB / "style.css").read_text(encoding="utf-8"))

    def test_la_fin_d_un_casting_mene_aux_resultats(self):
        self.assertIn('afficherSousOngletCasting("comparer");', self.script)
        self.assertIn('id="a-noter-casting"', self.vue)


if __name__ == "__main__":
    unittest.main()
