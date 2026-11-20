"""Fenêtre de confirmation de l'appli (web/commun.js, demander) : la question se pose dans une fenêtre d'Aria, pas dans
celle du navigateur. Premier usage : révéler un casting avant d'avoir tout noté. Lecture des fichiers de web/ : rien
n'est lancé."""

import re
import unittest
from pathlib import Path

WEB = Path(__file__).resolve().parent.parent / "web"


def lire(nom: str) -> str:
    return (WEB / nom).read_text(encoding="utf-8")


def corps_de_demander() -> str:
    script = lire("commun.js")
    debut = script.index("function demander(")
    return script[debut:script.index("\nfunction ", debut + 1)]


class Fenetre(unittest.TestCase):
    def setUp(self):
        page = lire("index.html")
        self.assertEqual(page.count('id="demande"'), 1)
        debut = page.index('<div id="demande" class="voile" hidden>')
        self.fenetre = page[debut:page.index('<div id="assistant"', debut)]

    def test_un_dialogue_avec_son_titre_et_sa_question(self):
        self.assertIn('role="alertdialog" aria-modal="true" aria-labelledby="titre-demande"', self.fenetre)
        self.assertIn('aria-describedby="texte-demande"', self.fenetre)
        for ident in ("titre-demande", "texte-demande", "detail-demande"):
            self.assertEqual(self.fenetre.count(f'id="{ident}"'), 1, ident)

    def test_un_seul_panneau_accentue_et_l_orange_en_dernier(self):
        self.assertEqual(self.fenetre.count("panneau-assistant accent"), 1)
        boutons = re.findall(r'<button type="button" id="([\w-]+)"( class="a-faire")?>', self.fenetre)
        self.assertEqual(boutons, [("refuser-demande", ""), ("accepter-demande", ' class="a-faire"')])

    def test_les_pieces_du_dialogue_valent_pour_l_assistant_et_pour_la_fenetre(self):
        css = lire("habillage.css")
        for piece in (".fenetre.assistant {", ".titre-assistant {", ".panneau-assistant.accent::before {",
                      ".pied-assistant {", "button {"):
            self.assertIn(f"body.large :is(#assistant, #demande):not(#_) {piece}", css, piece)
        self.assertIn("[data-annuler-moteur], #refuser-demande) {", css)  # le bouton qui n'engage rien : contour discret


class Demander(unittest.TestCase):
    def setUp(self):
        self.corps = corps_de_demander()

    def test_le_texte_n_est_jamais_lu_comme_du_html(self):
        self.assertFalse("innerHTML" in self.corps)
        self.assertEqual(self.corps.count(".textContent = "), 5)  # titre, texte, détail, deux boutons

    def test_le_focus_va_d_abord_sur_le_bouton_qui_n_engage_rien(self):
        self.assertIn("refuser.focus();", self.corps)
        self.assertFalse("accepter.focus();" in self.corps)

    def test_echap_et_le_voile_referment_sans_valider(self):
        self.assertRegex(self.corps, r'e\.key === "Escape"\) \{\s+e\.stopPropagation\(\);\s+fermer\(false\);')
        self.assertIn("e.target === refuser || e.target === voile) fermer(false);", self.corps)
        self.assertIn("if (e.target === accepter) fermer(true);", self.corps)

    def test_les_ecouteurs_sont_retires_a_la_fermeture(self):
        self.assertIn('voile.removeEventListener("click", clic);', self.corps)
        self.assertIn('document.removeEventListener("keydown", touche, true);', self.corps)


class RevelerUnCasting(unittest.TestCase):
    def test_la_revelation_passe_par_la_fenetre_de_l_appli(self):
        script = lire("casting.js")
        self.assertFalse("confirm(" in script)
        self.assertIn("!await demander({", script)
        self.assertIn('titre: "Révéler les modèles",', script)
        self.assertIn('valider: "Révéler quand même",', script)
        self.assertIn('annuler: "Continuer à noter",', script)


if __name__ == "__main__":
    unittest.main()
