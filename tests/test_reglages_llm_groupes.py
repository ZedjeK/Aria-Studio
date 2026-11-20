"""Réglages du LLM : rangés en groupes à l'écran (web/reglages.js, GROUPES_LLM), et « Enregistrer » est l'action
orange de sa page, sur le panneau accentué. Un réglage d'app/reglages.py oublié dans les groupes tomberait dans
« Autres réglages » ; un libellé raccourci en double dans un groupe ne dirait plus lequel est lequel."""

import re
import unittest
from html.parser import HTMLParser
from pathlib import Path

from app import reglages

WEB = Path(__file__).resolve().parent.parent / "web"
TITRES = ["Écriture sans réflexion", "Écriture avec réflexion", "Autres tâches", "Capacité et moteur"]


def groupes_du_script() -> list[dict]:
    """Les groupes de GROUPES_LLM, dans l'ordre : titre, court (oui ou non), clés."""
    script = (WEB / "reglages.js").read_text(encoding="utf-8")
    bloc = re.search(r"const GROUPES_LLM = \[(.*?)\n\];", script, re.DOTALL).group(1)
    return [{"titre": titre, "court": "court: true" in entre, "cles": re.findall(r'"([^"]+)"', cles)}
            for titre, entre, cles in re.findall(r'\{ titre: "([^"]+)",(.*?)cles: \[([^\]]*)\]', bloc, re.DOTALL)]


class Balises(HTMLParser):
    """Les attributs des balises ouvrantes de la page, par balise."""

    def __init__(self):
        super().__init__()
        self.vues = []

    def handle_starttag(self, balise, attributs):
        self.vues.append((balise, dict(attributs)))


class GroupesDuLlm(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.groupes = groupes_du_script()
        cls.params = reglages.params_llm()

    def test_quatre_groupes_dans_l_ordre(self):
        self.assertEqual([g["titre"] for g in self.groupes], TITRES)

    def test_chaque_reglage_est_dans_un_groupe_et_un_seul(self):
        rangees = [cle for g in self.groupes for cle in g["cles"]]
        self.assertEqual(sorted(rangees), sorted(self.params))

    def test_un_libelle_raccourci_reste_unique_dans_son_groupe(self):
        for g in self.groupes:
            if not g["court"]:
                continue
            courts = [self.params[cle].libelle.split(" · ")[0] for cle in g["cles"]]
            self.assertEqual(len(courts), len(set(courts)), g["titre"])

    def test_un_libelle_raccourci_perd_bien_sa_fin(self):
        for g in self.groupes:
            for cle in g["cles"] if g["court"] else []:
                self.assertIn(" · ", self.params[cle].libelle, cle)


class EnregistrerEstLActionDeLaPage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        lecture = Balises()
        lecture.feed((WEB / "index.html").read_text(encoding="utf-8"))
        cls.vues = lecture.vues

    def test_les_deux_cartes_des_reglages_sont_accentuees(self):
        for section in ("llm", "workflows video"):
            classes = next(a["class"] for b, a in self.vues if b == "section" and a.get("data-section") == section)
            self.assertIn("accent", classes.split(), section)

    def test_enregistrer_est_un_bouton_principal(self):
        for bouton in ("enregistrer-llm", "enregistrer-workflow"):
            classes = next(a["class"] for b, a in self.vues if b == "button" and a.get("id") == bouton)
            self.assertEqual(classes.split(), ["principal"], bouton)

    def test_l_habillage_ne_les_remet_pas_en_contour_cyan(self):
        habillage = (WEB / "habillage.css").read_text(encoding="utf-8")
        for ligne in habillage.splitlines():
            if "#optimiser-scenario" in ligne:  # la liste des « principales » ramenées au contour cyan
                self.assertNotIn("#enregistrer-", ligne)


if __name__ == "__main__":
    unittest.main()
