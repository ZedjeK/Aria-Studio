"""tools/zip_release.py : le zip de la release a le dossier racine Aria-Studio-X.Y.Z, et son .sha256 est celui du README."""

import hashlib
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from tools import zip_release  # noqa: E402


class ZipReleaseTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dossier = Path(self.tmp.name)
        self.staging = self.dossier / "staging"
        (self.staging / "app").mkdir(parents=True)
        (self.staging / "Aria-Studio.exe").write_bytes(b"exe")
        (self.staging / "app" / "a.py").write_text("1", encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def test_dossier_racine_et_empreinte(self):
        zip_, somme = zip_release.faire_zip(self.staging, self.dossier / "sortie", "2.0.0")
        self.assertEqual(zip_.name, "Aria-Studio-2.0.0.zip")
        self.assertEqual(sorted(zipfile.ZipFile(zip_).namelist()),
                         ["Aria-Studio-2.0.0/Aria-Studio.exe", "Aria-Studio-2.0.0/app/a.py"])
        empreinte, nom = somme.read_text(encoding="ascii").split()
        self.assertEqual(nom, "Aria-Studio-2.0.0.zip")
        self.assertEqual(empreinte, hashlib.sha256(zip_.read_bytes()).hexdigest())

    def test_sans_construction_on_s_arrete(self):
        (self.staging / "Aria-Studio.exe").unlink()
        with self.assertRaises(SystemExit):
            zip_release.faire_zip(self.staging, self.dossier / "sortie", "2.0.0")


if __name__ == "__main__":
    unittest.main()
