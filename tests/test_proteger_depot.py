"""tools/proteger-depot.ps1 : le script du dépôt modèle des applis Aria, repris tel quel. Il exige l'analyse CodeQL
avant fusion, une fois qu'elle a tourné sur la branche par défaut.

Lecture du script : rien n'est lancé, GitHub n'est pas appelé. Le script se vérifie pour de bon par son essai à blanc
(`.\\tools\\proteger-depot.ps1 -Montrer`), qui affiche ce qu'il enverrait.
"""

import unittest
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
SCRIPT = RACINE / "tools" / "proteger-depot.ps1"


class AnalyseExigee(unittest.TestCase):
    def setUp(self):
        self.brut = SCRIPT.read_bytes()
        self.script = self.brut.decode("utf-8-sig")

    def test_le_script_garde_son_encodage(self):
        """UTF-8 avec BOM et fins de ligne CRLF : sans BOM, Windows PowerShell 5.1 lit mal les accents."""
        self.assertTrue(self.brut.startswith(b"\xef\xbb\xbf"))
        self.assertEqual(self.brut.count(b"\n"), self.brut.count(b"\r\n"))

    def test_la_regle_exige_codeql_aux_alertes_elevees(self):
        self.assertIn("type = 'code_scanning'", self.script)
        self.assertIn("tool = 'CodeQL'; security_alerts_threshold = 'high_or_higher'; alerts_threshold = 'errors'",
                      self.script)

    def test_seulement_quand_codeql_a_deja_analyse_la_branche(self):
        """On n'exige jamais une analyse qui n'a pas encore rendu de résultat : aucune PR ne pourrait fusionner."""
        self.assertIn("code-scanning/analyses?ref=refs/heads/", self.script)
        self.assertIn("$codeqlPret = @($analyses | Where-Object { $_ -and $_ -isnot [bool] }).Count -gt 0", self.script)
        avant_la_regle = self.script[:self.script.index("type = 'code_scanning'")].rstrip().splitlines()
        self.assertEqual(avant_la_regle[-2].strip(), "if ($codeqlPret) {")

    def test_un_ruleset_sans_l_analyse_n_est_pas_dit_complet(self):
        complet = self.script[self.script.index("$complet = "):self.script.index("# Le ruleset posé par ce script")]
        self.assertIn("(-not $codeqlPret -or $types -contains 'code_scanning')", complet)

    def test_l_essai_a_blanc_n_ecrit_rien(self):
        self.assertIn("[switch]$Montrer", self.script)
        essai = self.script[self.script.index("if ($Montrer) {"):self.script.index('$reponse = (Read-Host')]
        self.assertFalse("Ecrire " in essai)
        self.assertFalse("$etape.Action" in essai)
        self.assertRegex(essai, r"exit 0\s+\}\s*$")


if __name__ == "__main__":
    unittest.main()
