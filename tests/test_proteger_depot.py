"""tools/proteger-depot.ps1 : l'analyse CodeQL est exigée avant fusion, sur un dépôt qui a ce workflow.

Lecture du script : rien n'est lancé, GitHub n'est pas appelé. Le script se vérifie pour de bon par son essai à blanc
(`.\\tools\\proteger-depot.ps1 -Montrer`), qui affiche ce qu'il enverrait.
"""

import re
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
        self.assertIn("tool = $outilAnalyse; security_alerts_threshold = 'high_or_higher'; alerts_threshold = 'errors'",
                      self.script)
        self.assertIn("$outilAnalyse = 'CodeQL'", self.script)

    def test_seulement_si_le_depot_a_le_workflow_et_qu_il_a_deja_tourne(self):
        """On n'exige jamais une analyse qui n'a pas encore rendu de résultat : aucune PR ne pourrait fusionner."""
        self.assertIn('Lire "repos/$Depot/contents/.github/workflows/codeql.yml"', self.script)
        self.assertIn("code-scanning/analyses?tool_name=$outilAnalyse&ref=$refBranche&per_page=1", self.script)
        self.assertIn("$analyseAPoser = $aCodeQL -and $codeqlATourne -and -not $analyseExigee", self.script)
        self.assertIn("if ($aCodeQL -and ($codeqlATourne -or $analyseExigee)) {", self.script)

    def test_un_ruleset_incomplet_sans_l_analyse_n_est_pas_dit_complet(self):
        complet = self.script[self.script.index("$complet = "):self.script.index("# Le ruleset posé par ce script")]
        self.assertIn("-not $analyseAPoser", complet)

    def test_completer_garde_les_regles_deja_en_place(self):
        """Les règles lues sont renvoyées telles quelles : GitHub y ajoute des paramètres que le script ne connaît pas."""
        fonction = self.script[self.script.index("function ReglesCompletees("):self.script.index("# 1. Méthode de fusion")]
        self.assertIn("$regle.parameters = $r.parameters", fonction)
        self.assertIn("if ($presentes -notcontains $v.type) { $resultat += $v }", fonction)
        self.assertIn("rules = (ReglesCompletees $actuel.rules $corpsBranche.rules $checkLie)", self.script)

    def test_l_essai_a_blanc_n_ecrit_rien(self):
        self.assertIn("[switch]$Montrer", self.script)
        essai = self.script[self.script.index("if ($Montrer) {"):self.script.index('$reponse = (Read-Host')]
        self.assertFalse("Ecrire " in essai)
        self.assertFalse("$etape.Action" in essai)
        self.assertRegex(essai, r"exit 0\s+\}\s*$")


if __name__ == "__main__":
    unittest.main()
