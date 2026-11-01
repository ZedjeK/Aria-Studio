"""Tests du déménagement (app/demenagement.py) : le code et le dossier de l'utilisateur sont simulés dans un dossier temporaire."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import demenagement


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.code, self.maison = base / "code", base / "maison" / ".Aria-Studio"
        for d in ("data/sequences", "modeles/llm", "services/comfyui"):
            (self.code / d).mkdir(parents=True)
        (self.code / "data" / "reglages.yaml").write_text("llm: {}", encoding="utf-8")
        (self.code / "modeles" / "llm" / "petit.gguf").write_bytes(b"GGUF")
        (self.code / "aria.yaml").write_text("{}", encoding="utf-8")
        self.ports = mock.patch.object(demenagement, "_ports_occupes", return_value=[])
        self.ports.start()

    def tearDown(self):
        self.ports.stop()
        self.tmp.cleanup()

    def plan(self, retour=False):
        return demenagement.plan(retour, self.code, self.maison)

    def demenager(self, retour=False):
        return demenagement.demenager(retour, self.code, self.maison)


class Aller(Base):
    def test_essai_a_blanc_ne_touche_a_rien(self):
        p = self.plan()
        self.assertEqual(p["blocages"], [])
        self.assertEqual([(s.name, c.name) for s, c in p["mouvements"]],
                         [("aria.yaml", "aria.yaml"), ("services", "moteurs"), ("modeles", "modeles"), ("data", "data")])
        self.assertTrue((self.code / "data").is_dir())
        self.assertFalse(self.maison.exists())

    def test_demenage_puis_revient(self):
        self.demenager()
        self.assertEqual(sorted(p.name for p in self.code.iterdir()), [])
        self.assertEqual((self.maison / "modeles" / "llm" / "petit.gguf").read_bytes(), b"GGUF")
        self.assertTrue((self.maison / "moteurs" / "comfyui").is_dir())
        self.assertEqual((self.maison / "data" / "reglages.yaml").read_text(encoding="utf-8"), "llm: {}")
        self.demenager(retour=True)
        self.assertEqual(sorted(p.name for p in self.code.iterdir()), ["aria.yaml", "data", "modeles", "services"])
        self.assertEqual(list(self.maison.iterdir()), [])

    def test_dossier_vide_deja_la_accepte(self):
        (self.maison / "modeles").mkdir(parents=True)
        self.demenager()
        self.assertTrue((self.maison / "modeles" / "llm" / "petit.gguf").is_file())


class Blocages(Base):
    def test_destination_occupee(self):
        (self.maison / "data").mkdir(parents=True)
        (self.maison / "data" / "autre.json").write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(demenagement.ErreurDemenagement, "existe déjà"):
            self.demenager()
        self.assertTrue((self.code / "modeles").is_dir())  # rien n'a bougé

    def test_aria_en_marche(self):
        with mock.patch.object(demenagement, "_ports_occupes", return_value=["ComfyUI (port 8195)"]), \
                self.assertRaisesRegex(demenagement.ErreurDemenagement, "Arrête d'abord : ComfyUI"):
            self.demenager()

    def test_autre_disque(self):
        with mock.patch.object(demenagement, "_volume", side_effect=lambda c: "C:" if c == self.code else "D:"), \
                self.assertRaisesRegex(demenagement.ErreurDemenagement, "même disque"):
            self.demenager()

    def test_rien_a_demenager(self):
        self.demenager()
        self.assertIn("Rien à déménager à côté du code", self.plan()["blocages"])

    def test_echec_au_milieu_tout_est_defait(self):
        vrai_rename = Path.rename

        def rename(source, cible):
            if source.name == "modeles":
                raise PermissionError(13, "Accès refusé")
            return vrai_rename(source, cible)

        with mock.patch.object(Path, "rename", rename), \
                self.assertRaisesRegex(demenagement.ErreurDemenagement, "modeles n'a pas pu partir.*rien n'a bougé"):
            self.demenager()
        self.assertEqual(sorted(p.name for p in self.code.iterdir()), ["aria.yaml", "data", "modeles", "services"])
        self.assertFalse((self.maison / "moteurs").exists())


if __name__ == "__main__":
    unittest.main()
