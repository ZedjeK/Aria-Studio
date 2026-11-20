"""Le socle commun : les fichiers que cette appli garde identiques à ceux du dépôt modèle des applis Aria.

La comparaison au modèle se fait par `.\\tools\\verifier-socle.ps1`, qui lit le modèle sur GitHub. Ici, hors ligne,
seulement ce qui se vérifie sans réseau : la liste est là, et chaque fichier qu'elle nomme existe dans le dépôt.
"""

import re
import unittest
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
LISTE = RACINE / "tools" / "socle.txt"


def fichiers_du_socle() -> list[str]:
    lignes = (ligne.strip() for ligne in LISTE.read_text(encoding="utf-8").splitlines())
    return [ligne for ligne in lignes if ligne and not ligne.startswith("#")]


class Socle(unittest.TestCase):
    def test_la_liste_nomme_des_fichiers(self):
        fichiers = fichiers_du_socle()
        self.assertGreaterEqual(len(fichiers), 5)
        for chemin in ("tools/socle.txt", "tools/verifier-socle.ps1", "tools/proteger-depot.ps1"):
            self.assertIn(chemin, fichiers)

    def test_chaque_chemin_est_relatif_et_sans_remontee(self):
        for chemin in fichiers_du_socle():
            self.assertRegex(chemin, r"\A[\w.-]+(/[\w.-]+)*\Z", chemin)
            self.assertNotIn("..", chemin.split("/"), chemin)

    def test_chaque_fichier_du_socle_est_dans_le_depot(self):
        absents = [chemin for chemin in fichiers_du_socle() if not (RACINE / chemin).is_file()]
        self.assertEqual(absents, [])

    def test_le_script_de_controle_garde_son_bom(self):
        """Sans BOM, Windows PowerShell 5.1 lit mal les accents du script."""
        self.assertTrue((RACINE / "tools" / "verifier-socle.ps1").read_bytes().startswith(b"\xef\xbb\xbf"))

    def test_le_script_ne_modifie_rien(self):
        script = (RACINE / "tools" / "verifier-socle.ps1").read_text(encoding="utf-8-sig")
        for ecriture in ("WriteAllText", "WriteAllBytes", "Set-Content", "Out-File", "Copy-Item", "Remove-Item", "-X "):
            self.assertFalse(ecriture in script, ecriture)
        self.assertIsNone(re.search(r"gh api [^\n]*(POST|PUT|PATCH|DELETE)", script))


if __name__ == "__main__":
    unittest.main()
