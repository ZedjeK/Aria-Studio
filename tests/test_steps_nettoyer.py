"""Nettoyage du texte d'une page de modèle (app/sources/steps.py) : retirer les balises HTML sans geler.

Le texte vient de tiers (description Civitai, README Hugging Face). Avec « <[^>]+> », chaque « < » sans « > » relisait
tout le reste du texte : le temps était quadratique (40 000 « < » : 0,55 s ; 400 000 : près d'une minute). Sans « < »
dans la classe, un « < » ne relit que jusqu'au prochain.
"""

import time
import unittest

from app.sources import steps


class Nettoyer(unittest.TestCase):
    def test_les_balises_partent_comme_avant(self):
        self.assertEqual(steps._nettoyer("<p>Use <b>8</b> steps</p>"), "Use 8 steps")
        self.assertEqual(steps._nettoyer("a  <br/>\n b"), "a b")
        self.assertEqual(steps._nettoyer('<a href="x?y=1">lien</a>'), "lien")
        self.assertEqual(steps._nettoyer(None), "")
        self.assertEqual(steps._nettoyer(""), "")

    def test_un_chevron_perdu_ne_mange_pas_le_texte_qui_suit(self):
        """Avant : « < 2 and <b> » partait en un bloc et emportait « 2 and »."""
        self.assertEqual(steps._nettoyer("1 < 2 and <b>8</b> steps"), "1 < 2 and 8 steps")

    def test_un_texte_de_chevrons_ne_gele_pas(self):
        for nom, texte in (("« < » seuls", "<" * 200_000), ("« <a » répétés", "<a " * 70_000),
                           ("chevrons puis un « > »", "<" * 100_000 + ">")):
            debut = time.perf_counter()
            steps._nettoyer(texte)
            duree = time.perf_counter() - debut
            self.assertLess(duree, 2.0, f"{nom} : {duree:.1f} s")

    def test_la_suggestion_reste_lisible_dans_un_texte_piege(self):
        texte = "<" * 50_000 + " Use 8 steps for speed"
        self.assertEqual(steps.suggerer({"description": texte})["valeurs"], [8])


if __name__ == "__main__":
    unittest.main()
