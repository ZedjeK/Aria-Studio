"""Tests du filtre AS-18 : mineur seul autorisé, mineur + terme sensible bloqué."""

import unittest

from app import securite
from app.securite import ErreurSecurite


class Deshabillage(unittest.TestCase):
    MINEUR = "A teenage girl, 15 years old"

    def test_variantes_bloquees_avec_un_mineur(self):
        for texte in ("She undresses slowly.", "Undress her.", "He disrobes.", "She is disrobing by the bed.",
                      "She stands unclothed.", "She takes off her clothes.", "She pulls her clothes off.",
                      "Elle se déshabiller lentement.", "Ils se déshabillent.", "Elle enlève ses vêtements.",
                      "Elle retire ses vêtements.", "Elle est dévêtue."):
            with self.assertRaises(ErreurSecurite, msg=texte):
                securite.verifier(self.MINEUR, texte)

    def test_autorise_sans_mineur(self):
        securite.verifier("A woman in her thirties", "She undresses slowly.")

    def test_mots_ambigus_toleres(self):
        securite.verifier(self.MINEUR, "She reads a comic strip, lit by strips of neon light.")


if __name__ == "__main__":
    unittest.main()
