"""Les tests tournent dans un dossier utilisateur jetable (tools/tester.py), jamais sur le vrai ~/.Aria-Studio."""

import os
import unittest
from pathlib import Path

from app import emplacements


class Isolation(unittest.TestCase):
    def test_lances_par_tools_tester(self):
        maison = os.environ.get("ARIA_TESTS_ISOLES")
        self.assertTrue(maison, "Lance les tests avec : python tools/tester.py (ils touchaient sinon ton vrai "
                                "~/.Aria-Studio : modèles installés, réglages, créations)")
        self.assertTrue(emplacements.RACINE.is_relative_to(Path(maison)), emplacements.RACINE)


if __name__ == "__main__":
    unittest.main()
