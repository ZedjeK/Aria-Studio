"""Réglages : sept sous-onglets à plat (web/index.html, web/reglages.js). Chaque sous-onglet a sa page et son
commentaire « À savoir », et chaque panneau du système est rangé dans une seule page : un onglet sans page, ou un
panneau resté hors des pages, ne s'afficherait jamais."""

import re
import unittest
from html.parser import HTMLParser
from pathlib import Path

WEB = Path(__file__).resolve().parent.parent / "web"
SYSTEME = {"moteurs": ["carte-moteurs", "carte-emplacements", "carte-cles"],
           "application": ["carte-maj", "carte-demarrage", "carte-notifications", "carte-fond", "carte-mcp"],
           "securite": ["carte-securite"],
           "historique": ["carte-sauvegardes", "carte-journal"]}
GENERATION = ["llm", "workflows", "video"]
VIDES = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"}


class Lecture(HTMLParser):
    """Les boutons de .nav-reglages, les pages de .contenu-reglages, et la page qui contient chaque panneau « carte-… »."""

    def __init__(self):
        super().__init__()
        self.pile = []  # (balise, attributs) des éléments ouverts
        self.onglets = []
        self.pages = []
        self.cartes = {}

    def dans(self, classe):
        return any(classe in (a.get("class") or "").split() for _, a in self.pile)

    def handle_starttag(self, balise, attributs):
        if balise in VIDES:
            return
        a = dict(attributs)
        if balise == "button" and self.dans("nav-reglages"):
            self.onglets.append(a)
        if "data-section" in a and self.dans("contenu-reglages"):
            self.pages.append(a["data-section"])
        if balise == "section" and (a.get("id") or "").startswith("carte-"):
            self.cartes[a["id"]] = next((x["data-section"] for _, x in reversed(self.pile) if "data-section" in x), None)
        self.pile.append((balise, a))

    def handle_startendtag(self, balise, attributs):
        pass  # <path … /> : rien à ouvrir

    def handle_endtag(self, balise):
        if any(b == balise for b, _ in self.pile):
            while self.pile.pop()[0] != balise:
                pass


class SousOngletsDesReglages(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.page = (WEB / "index.html").read_text(encoding="utf-8")
        cls.lu = Lecture()
        cls.lu.feed(cls.page)
        cls.sections = [o.get("data-section") for o in cls.lu.onglets]

    def test_sept_sous_onglets_l_appli_puis_ce_qui_genere(self):
        self.assertEqual(self.sections, [*SYSTEME, *GENERATION])

    def test_chaque_sous_onglet_a_son_commentaire(self):
        for onglet in self.lu.onglets:
            self.assertTrue((onglet.get("data-savoir") or "").strip(), onglet.get("data-section"))

    def test_chaque_sous_onglet_a_sa_page_et_chaque_page_son_sous_onglet(self):
        rangees = [s for page in self.lu.pages for s in page.split()]
        self.assertEqual(sorted(rangees), sorted(self.sections))

    def test_chaque_panneau_du_systeme_est_dans_sa_page(self):
        attendu = {carte: page for page, cartes in SYSTEME.items() for carte in cartes}
        self.assertEqual({c: p for c, p in self.lu.cartes.items() if c in attendu}, attendu)

    def test_le_script_range_les_memes_pages_dans_le_systeme(self):
        script = (WEB / "reglages.js").read_text(encoding="utf-8")
        liste = re.search(r"const SYSTEME = \[([^\]]*)\]", script).group(1)
        self.assertEqual(re.findall(r'"([^"]+)"', liste), list(SYSTEME))

    def test_le_volet_systeme_n_existe_plus(self):
        for fichier in ("index.html", "reglages.js", "app.js"):
            self.assertNotIn("systeme-reglages", (WEB / fichier).read_text(encoding="utf-8"), fichier)


if __name__ == "__main__":
    unittest.main()
