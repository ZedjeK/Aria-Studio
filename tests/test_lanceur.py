"""Le lanceur (tools/launcher/Launcher.cs) compilé pour de vrai, puis essayé sans fenêtre dans un dossier utilisateur
jetable (USERPROFILE redirigé) : la proposition d'installer, l'installation, la version précédente, le raccourci du
Bureau, la marque « téléchargé depuis Internet » et « Ne plus me proposer ».

Les options --essai-* du lanceur n'ouvrent aucune fenêtre et ne lancent pas Aria. Sans le compilateur C# de Windows,
ces tests sont sautés.
"""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CSC = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Microsoft.NET" / "Framework64" / "v4.0.30319" / "csc.exe"
EXE = "Aria-Studio.exe"


@unittest.skipUnless(os.name == "nt" and CSC.exists(), "compilateur C# de Windows absent")
class Lanceur(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.compile = tempfile.TemporaryDirectory()
        cls.exe = Path(cls.compile.name) / EXE
        r = subprocess.run([str(CSC), "/nologo", "/target:winexe", f"/out:{cls.exe}", "/codepage:65001",
                            "/reference:System.Windows.Forms.dll", "/reference:System.Drawing.dll",
                            str(ROOT / "tools" / "launcher" / "Launcher.cs")], capture_output=True)
        if r.returncode:
            raise AssertionError(r.stdout.decode("utf-8", "replace"))

    @classmethod
    def tearDownClass(cls):
        cls.compile.cleanup()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.profil = base / "profil"
        self.maison = self.profil / ".Aria-Studio"
        self.programme = self.maison / "programme"
        self.zip = base / "Telechargements" / "Aria-Studio-2.1.0"
        (self.zip / "app").mkdir(parents=True)
        (self.zip / "python").mkdir()
        shutil.copy2(self.exe, self.zip / EXE)
        (self.zip / "app" / "__init__.py").write_text('VERSION = "2.1.0"', encoding="utf-8")
        (self.zip / "config.yaml").write_text("app:\n    port: 8075\n", encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def lancer(self, exe: Path, *options) -> int:
        env = {**os.environ, "USERPROFILE": str(self.profil)}
        return subprocess.run([str(exe), *options], env=env, capture_output=True, timeout=60).returncode

    def proposerait(self, exe: Path, *options) -> bool:
        return self.lancer(exe, "--essai-proposer", *options) == 0

    def test_propose_depuis_un_zip(self):
        self.assertTrue(self.proposerait(self.zip / EXE))

    def test_ne_propose_pas_en_portable_ni_avec_l_option(self):
        self.assertFalse(self.proposerait(self.zip / EXE, "--portable"))
        (self.zip / "data").mkdir()
        self.assertFalse(self.proposerait(self.zip / EXE))

    def test_ne_plus_me_proposer(self):
        self.maison.mkdir(parents=True)
        (self.maison / "aria.yaml").write_text("# Choix\nfavoris: {}\ninstallation_proposee: true\n", encoding="utf-8")
        self.assertTrue(self.proposerait(self.zip / EXE))
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-ne-plus-proposer"), 0)
        self.assertEqual((self.maison / "aria.yaml").read_text(encoding="utf-8"),
                         "# Choix\nfavoris: {}\ninstallation_proposee: false\n")
        self.assertFalse(self.proposerait(self.zip / EXE))

    def test_ne_plus_me_proposer_sans_aria_yaml(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-ne-plus-proposer"), 0)
        self.assertEqual((self.maison / "aria.yaml").read_text(encoding="utf-8"), "installation_proposee: false\n")

    def test_installe_dans_programme_sans_la_marque_internet(self):
        with open(str(self.zip / "config.yaml") + ":Zone.Identifier", "w", encoding="utf-8") as marque:
            marque.write("[ZoneTransfer]\nZoneId=3\n")
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        self.assertTrue((self.programme / EXE).is_file())
        self.assertEqual((self.programme / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.1.0"')
        self.assertTrue((self.programme / "python").is_dir())
        with self.assertRaises(OSError):
            open(str(self.programme / "config.yaml") + ":Zone.Identifier", encoding="utf-8").close()
        self.assertTrue((self.zip / EXE).is_file())  # le zip décompressé reste tel quel
        self.assertFalse(self.proposerait(self.programme / EXE))  # depuis programme\ : plus de proposition
        self.assertFalse((self.profil / "Desktop" / "Aria Studio.lnk").exists())  # case décochée

    def test_reinstaller_garde_la_version_d_avant(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        (self.zip / "app" / "__init__.py").write_text('VERSION = "2.2.0"', encoding="utf-8")
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        self.assertEqual((self.programme / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.2.0"')
        precedent = self.maison / "programme.precedent"
        self.assertEqual((precedent / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.1.0"')
        self.assertFalse((self.maison / "programme.nouveau").exists())

    def test_la_version_d_avant_ne_propose_rien_et_ne_s_installe_pas_sur_elle_meme(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        precedent = self.maison / "programme.precedent"
        self.assertFalse(self.proposerait(precedent / EXE))
        self.assertEqual(self.lancer(precedent / EXE, "--essai-installer"), 1)
        self.assertTrue((precedent / EXE).is_file())
        self.assertTrue((self.programme / EXE).is_file())

    def test_programme_ouvert_rien_ne_change(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        (self.zip / "app" / "__init__.py").write_text('VERSION = "2.2.0"', encoding="utf-8")
        with open(self.programme / "config.yaml", encoding="utf-8"):  # Aria lancée depuis programme\
            self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 1)
        self.assertEqual((self.programme / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.1.0"')
        self.assertFalse((self.maison / "programme.precedent").exists())

    def test_raccourci_sur_le_bureau_de_l_utilisateur(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer", "--raccourci"), 0)
        lien = self.profil / "Desktop" / "Aria Studio.lnk"
        self.assertTrue(lien.is_file())
        self.assertIn(str(self.programme / EXE).encode("utf-16-le"), lien.read_bytes())

    def test_raccourci_deja_la_jamais_remplace_en_silence(self):
        (self.profil / "Desktop").mkdir(parents=True)
        lien = self.profil / "Desktop" / "Aria Studio.lnk"
        lien.write_bytes(b"a moi")
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer", "--raccourci"), 0)
        self.assertEqual(lien.read_bytes(), b"a moi")


if __name__ == "__main__":
    unittest.main()
