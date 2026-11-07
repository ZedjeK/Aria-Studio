"""Consignes v2 (app/prompt.py) : fidélité d'abord, contenu adulte gardé, longueur de la fiche du modèle cible.

Origine : le LLM déformait le brouillon (médium, sons et caméra inventés, idée adoucie, consignes de l'utilisateur
recopiées ou ignorées). Rien n'est lancé : on lit seulement le texte envoyé au LLM.
"""

import unittest

from app import prompt


class Consignes(unittest.TestCase):
    def test_fidelite_dans_les_reecritures(self):
        for nom in ("reecriture", "video", "references"):
            texte = prompt.consigne(nom)
            self.assertIn("FIXED", texte, nom)
            self.assertIn("never copy them into the prompt", texte, nom)  # ses consignes s'appliquent, sans écho
            self.assertIn(prompt.ADULTES, texte, nom)

    def test_contenu_adulte_jamais_adouci(self):
        for nom in ("peaufinage", "decoupage"):
            self.assertIn(prompt.ADULTES, prompt.consigne(nom), nom)
        self.assertIn("never make a character younger", prompt.CLAUSE_MINEURS)  # la clause mineurs reste la dernière
        self.assertTrue(prompt.consigne("reecriture").endswith(prompt.CLAUSE_MINEURS))

    def test_medium_jamais_change(self):
        self.assertIn("never switch to painting or illustration on your own", prompt.consigne("reecriture"))

    def test_pas_de_musique_inventee(self):
        self.assertIn("No background music unless the user asks for it", prompt.consigne("video"))
        self.assertIn("No background music unless the user asks for it", prompt.consigne("decoupage"))

    def test_longueur_de_la_fiche(self):
        """Qwen-Image 2.1 conseille 200 mots : la consigne en imposait 40 à 120 à tous les modèles."""
        self.assertEqual(prompt.mots_max_de("qwen_image_2_1"), 200)
        self.assertIn("40 to 200 words", prompt.messages("une pomme", None, None, 200)[0]["content"])
        self.assertIn("40 to 120 words", prompt.consigne("reecriture"))  # sans modèle cible
        self.assertIsNone(prompt.mots_max_de("inconnu"))

    def test_animer_ancre_la_premiere_image(self):
        """Guide d'écriture de MiniMax H3 : la première image est d'abord ancrée, puis l'action."""
        self.assertIn("anchoring it as it really is", prompt.NOTES_VUES["animer"])


if __name__ == "__main__":
    unittest.main()
