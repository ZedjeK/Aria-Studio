"""Casting LLM : les contrôles d'un prompt écrit suivent la longueur du modèle de rendu.

Avant, les candidats écrivaient sans les règles ni la longueur du modèle de rendu (120 mots au plus), et le contrôle
de longueur restait à 40-120 mots, même pour un modèle réglé à 200. Rien n'est lancé.
"""

import unittest

from app import casting, prompt

IDEE = "Une renarde rousse dans la neige"


def texte(mots: int) -> str:
    return " ".join(["the fox in the snow"] * (mots // 5))


class Controles(unittest.TestCase):
    def test_longueur_selon_le_modele_de_rendu(self):
        self.assertTrue(prompt.controler(texte(150), IDEE, 200)["longueur"])
        self.assertFalse(prompt.controler(texte(150), IDEE)["longueur"])  # sans modèle : 120 au plus
        self.assertFalse(prompt.controler(texte(250), IDEE, 200)["longueur"])
        self.assertFalse(prompt.controler(texte(30), IDEE, 200)["longueur"])  # 40 au moins, comme la consigne

    def test_le_banc_controle_avec_la_longueur_du_rendu(self):
        banc = object.__new__(casting.BancLLM)  # sans __init__ : ni config, ni GGUF sur le disque
        banc.mots_max = 200
        g = banc._generation("a", {"id": "p1", "prompt": IDEE}, 42, texte(150))
        self.assertTrue(g["controles"]["longueur"])
        self.assertEqual(g["controles"]["mots"], 150)


if __name__ == "__main__":
    unittest.main()
