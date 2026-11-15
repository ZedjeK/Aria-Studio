"""Identifiants de créations, de séquences et de voix : le contrôle est un fullmatch, jamais un « $ ».

« $ » accepte un retour à la ligne final : « 20261111-101010-abcdef\\n » passait le motif. Aucun fichier de ce nom ne
peut exister, donc rien n'était lu, mais un identifiant qui sert de nom de fichier doit être exactement celui qu'Aria
a fabriqué. Le test fait croire que le fichier existe : seul le motif décide.
"""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import plateau, sequences, voix

BON = "20261111-101010-abcdef"
MAUVAIS = (BON + "\n", BON + "\r\n", " " + BON, BON + " ", BON + ".json", "x" + BON, BON[:-1], BON.upper())


def fichier_present():
    """Tout fichier « existe » : le refus ne peut venir que du motif."""
    return mock.patch.object(Path, "is_file", return_value=True)


class Identifiants(unittest.TestCase):
    def test_creations(self):
        with fichier_present():
            plateau._verifier_id(BON)
            for mauvais in MAUVAIS:
                with self.subTest(id=mauvais), self.assertRaises(plateau.ErreurPlateau):
                    plateau._verifier_id(mauvais)

    def test_sequences(self):
        with fichier_present():
            self.assertEqual(sequences._chemin(BON).name, f"{BON}.json")
            for mauvais in MAUVAIS + ("", None):
                with self.subTest(id=mauvais), self.assertRaises(sequences.ErreurSequence):
                    sequences._chemin(mauvais)

    def test_sequences_de_la_corbeille(self):
        with tempfile.TemporaryDirectory() as tmp, fichier_present(), \
                mock.patch.object(sequences, "_corbeille", return_value=Path(tmp)):
            for mauvais in MAUVAIS:
                with self.subTest(id=mauvais), self.assertRaisesRegex(sequences.ErreurSequence, "inconnue dans la corbeille"):
                    sequences.restaurer(mauvais)

    def test_voix(self):
        with fichier_present():
            self.assertEqual(voix._chemin(BON).name, f"{BON}.json")
            for mauvais in MAUVAIS + ("", None):
                with self.subTest(id=mauvais), self.assertRaises(voix.ErreurVoix):
                    voix._chemin(mauvais)


if __name__ == "__main__":
    unittest.main()
