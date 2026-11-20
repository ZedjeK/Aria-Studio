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
        for ancien in ('getElementById("tache-banc")', "liste-bancs"):
            self.assertFalse(ancien in self.script, ancien)
        self.assertFalse("liste-bancs" in (WEB / "style.css").read_text(encoding="utf-8"))

    def test_la_tache_se_choisit_par_cartes_a_onglet(self):
        """Un choix exclusif : des cartes à onglet (boutons radio), dans l'ordre des tâches du serveur."""
        from app import casting
        valeurs = re.findall(r'<label class="carte-mode"><input type="radio" name="tache-banc" value="(\w+)"( checked)?>', self.vue)
        self.assertEqual([v for v, _ in valeurs], list(casting.TACHES))
        self.assertEqual([coche for _, coche in valeurs].count(" checked"), 1)
        self.assertIn('role="radiogroup"', self.vue)
        for nom in casting.TACHES:  # le script sait nommer chaque tâche
            self.assertIn(f"{nom}:", self.script)

    def test_ce_casting_remplace_le_volet_d_options_vide(self):
        self.assertNotIn("<details", self.vue)
        self.assertNotIn("Options du casting", self.vue)
        for ident in ("carte-preparation", "candidats-banc", "essais-banc", "sources-banc", "compte-rendu-banc", "blocage-banc"):
            self.assertEqual(self.vue.count(f'id="{ident}"'), 1, ident)
        self.assertIn('api("GET", "/api/bancs/preparation")', self.script)

    def test_un_casting_bloque_eteint_le_bouton(self):
        self.assertIn('getElementById("lancer-banc").disabled = bancEnCours || Boolean(preparation?.taches[tacheBanc]?.blocage);',
                      self.script)
        self.assertEqual(self.script.count('getElementById("lancer-banc").disabled'), 1)

    def test_la_tache_lue_dans_la_page_n_est_pas_reecrite_dans_le_html(self):
        """La tâche choisie vient d'une valeur de la page : elle sert de clé, elle n'est jamais interpolée dans du HTML
        (alerte CodeQL js/xss-through-dom). Le bouton « Ouvrir dans Modèles » ne porte que l'identifiant du modèle."""
        self.assertFalse('data-tache="${' in self.script)
        self.assertFalse("dataset.tache" in self.script)
        self.assertIn('"rendu" in b.dataset ? "texte_vers_image" : tacheBanc', self.script)
        for ligne in self.script.splitlines():
            if "${tacheBanc}" in ligne:
                self.fail(f"tacheBanc interpolée : {ligne.strip()[:80]}")

    def test_les_anciennes_pastilles_ont_disparu(self):
        for fichier in ("index.html", "casting.js", "style.css", "habillage.css"):
            contenu = (WEB / fichier).read_text(encoding="utf-8")
            self.assertFalse("pastilles-taches" in contenu, fichier)
            self.assertFalse("options-casting" in contenu, fichier)

    def test_la_fin_d_un_casting_mene_aux_resultats(self):
        self.assertIn('afficherSousOngletCasting("comparer");', self.script)
        self.assertIn('id="a-noter-casting"', self.vue)


class Comparer(unittest.TestCase):
    """Les castings en cartes à onglet, et des résultats dont l'en-tête reste visible."""

    def setUp(self):
        self.vue = vue_casting().split('<div data-panneau-casting="comparer"')[1]
        self.script = (WEB / "casting.js").read_text(encoding="utf-8")

    def test_les_castings_sont_des_cartes_a_onglet(self):
        self.assertIn('id="historique-bancs" class="cartes-mode" role="radiogroup"', self.vue)
        self.assertIn('<input type="radio" name="banc-affiche"', self.script)
        self.assertIn("${b.notees} / ${b.total}", self.script)  # où en sont les notes, sur chaque carte
        self.assertFalse(".carte-banc" in (WEB / "style.css").read_text(encoding="utf-8"))

    def test_sans_casting_une_invitation_a_en_lancer_un(self):
        self.assertIn('id="carte-vide-casting" hidden', self.vue)
        self.assertIn('id="vers-nouveau-casting"', self.vue)
        self.assertIn('getElementById("vers-nouveau-casting").addEventListener("click", () => afficherSousOngletCasting("lancer"))',
                      self.script)

    def test_l_en_tete_des_resultats_porte_l_avancement_les_sauts_et_reveler(self):
        tete = self.script[self.script.index('<div class="tete-banc">'):self.script.index("${b.revele ? tableauResume(b)")]
        for morceau in ('id="reveler" class="principal"', 'class="avancement-banc"', 'class="jauge-notes"', "${sauts}"):
            self.assertIn(morceau, tete)
        self.assertRegex((WEB / "habillage.css").read_text(encoding="utf-8"),
                         r"#vue-casting \.tete-banc \{\s+position: sticky;")

    def test_un_saut_mene_au_prompt(self):
        self.assertIn('data-saut="${echapper(p.id)}"', self.script)
        self.assertIn('data-prompt-banc="${echapper(p.id)}"', self.script)
        self.assertIn('[data-prompt-banc="${CSS.escape(cible.dataset.saut)}"]', self.script)

    def test_noter_ne_fait_pas_bouger_la_page(self):
        self.assertIn("const hauteur = window.scrollY;", self.script)
        self.assertIn("window.scrollTo(0, hauteur);", self.script)
        self.assertIn("majCarteBanc(b);", self.script)


if __name__ == "__main__":
    unittest.main()
