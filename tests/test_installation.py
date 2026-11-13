"""Installation par l'exe, côté Python : l'état et la proposition (app/emplacements.py), la mise en place du build
(tools/build_portable.py) et le refus du « retour à côté du code » quand Aria est installée (app/demenagement.py).

Le lanceur lui-même est essayé dans tests/test_lanceur.py.
"""

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

from app import demenagement, emplacements
from tools import build_portable as build


class Installation(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.maison = Path(self.tmp.name) / ".Aria-Studio"
        self.patchs = [mock.patch.object(emplacements, "DEFAUT", self.maison)]

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        self.tmp.cleanup()

    def code(self, chemin: Path):
        chemin.mkdir(parents=True, exist_ok=True)
        p = mock.patch.object(emplacements, "ROOT", chemin)
        p.start()
        self.patchs.append(p)
        for q in self.patchs[:-1]:
            q.start()

    def test_lancee_depuis_un_zip_propose_par_defaut(self):
        self.code(Path(self.tmp.name) / "Telechargements" / "Aria-Studio-2.1.0")
        i = emplacements.installation()
        self.assertFalse(i["installe"])
        self.assertFalse(i["portable"])
        self.assertTrue(i["proposee"])
        self.assertEqual(i["programme"], str(self.maison / "programme"))

    def test_installee_dans_programme(self):
        self.code(self.maison / "programme")
        self.assertTrue(emplacements.installation()["installe"])

    def test_ne_plus_proposer_est_ecrit_dans_aria_yaml_sans_perdre_le_reste(self):
        self.code(Path(self.tmp.name) / "zip")
        self.maison.mkdir(parents=True)
        (self.maison / "aria.yaml").write_text("modeles: D:\\IA\\modeles\n", encoding="utf-8")
        self.assertFalse(emplacements.proposer_installation(False)["proposee"])
        choix = yaml.safe_load((self.maison / "aria.yaml").read_text(encoding="utf-8"))
        self.assertEqual(choix, {"modeles": "D:\\IA\\modeles", "installation_proposee": False})
        self.assertTrue(emplacements.proposer_installation(True)["proposee"])

    def test_la_ligne_ecrite_par_le_lanceur_se_relit(self):
        # Launcher.cs ajoute « installation_proposee: false » à la fin du fichier, sans passer par PyYAML
        self.code(Path(self.tmp.name) / "zip")
        self.maison.mkdir(parents=True)
        (self.maison / "aria.yaml").write_text("# Choix\nfavoris: {}\ninstallation_proposee: false\n", encoding="utf-8")
        self.assertFalse(emplacements.installation()["proposee"])


class MiseEnPlace(unittest.TestCase):
    """tools/build_portable.py : staging devient programme\\, l'ancienne disposition est rangée sans rien perdre."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.maison = base / ".Aria-Studio"
        self.staging = base / "staging"
        self.silence = contextlib.redirect_stdout(io.StringIO())  # le build raconte ce qu'il fait
        self.silence.__enter__()

    def tearDown(self):
        self.silence.__exit__(None, None, None)
        self.tmp.cleanup()

    def construire(self, version: str):
        self.staging.mkdir()
        (self.staging / build.EXE).write_text(version, encoding="utf-8")
        (self.staging / "app").mkdir()
        (self.staging / "app" / "__init__.py").write_text(f'VERSION = "{version}"', encoding="utf-8")

    def affaires(self):
        for d in ("data/logs", "modeles/llm", "moteurs/comfyui"):
            (self.maison / d).mkdir(parents=True)
        (self.maison / "aria.yaml").write_text("favoris: {}\n", encoding="utf-8")

    def test_premiere_installation(self):
        self.construire("2.1.0")
        programme = build.mettre_en_place(self.staging, self.maison)
        self.assertEqual(programme, self.maison / "programme")
        self.assertEqual((programme / build.EXE).read_text(encoding="utf-8"), "2.1.0")
        self.assertFalse(self.staging.exists())
        self.assertFalse((self.maison / "programme.precedent").exists())

    def test_la_version_d_avant_devient_precedent(self):
        self.affaires()
        self.construire("2.1.0")
        build.mettre_en_place(self.staging, self.maison)
        self.construire("2.2.0")
        build.mettre_en_place(self.staging, self.maison)
        self.assertEqual((self.maison / "programme" / build.EXE).read_text(encoding="utf-8"), "2.2.0")
        self.assertEqual((self.maison / "programme.precedent" / build.EXE).read_text(encoding="utf-8"), "2.1.0")
        self.construire("2.3.0")
        build.mettre_en_place(self.staging, self.maison)
        self.assertEqual((self.maison / "programme.precedent" / build.EXE).read_text(encoding="utf-8"), "2.2.0")
        self.assertEqual((self.maison / "aria.yaml").read_text(encoding="utf-8"), "favoris: {}\n")
        self.assertTrue((self.maison / "modeles" / "llm").is_dir())

    def test_ancienne_disposition_rangee_dans_precedent(self):
        self.affaires()
        for nom in (build.EXE, "config.yaml"):
            (self.maison / nom).write_text("ancien", encoding="utf-8")
        for nom in ("app", "python", "web"):
            (self.maison / nom).mkdir()
        (self.maison / "a-moi.txt").write_text("à l'utilisateur", encoding="utf-8")  # absent de .programme : reste
        entrees = [build.EXE, "app", "config.yaml", "python", "web", "data"]  # data : jamais déplacé
        (self.maison / build.MANIFESTE).write_text(json.dumps({"version": "2.0.0", "entrees": entrees}), encoding="utf-8")
        self.construire("2.1.0")
        build.mettre_en_place(self.staging, self.maison)
        precedent = self.maison / "programme.precedent"
        self.assertEqual(sorted(p.name for p in precedent.iterdir()),
                         sorted([build.EXE, "app", "config.yaml", "python", "web", build.MANIFESTE]))
        self.assertEqual(sorted(p.name for p in self.maison.iterdir()),
                         ["a-moi.txt", "aria.yaml", "data", "modeles", "moteurs", "programme", "programme.precedent"])
        self.assertEqual((self.maison / "programme" / build.EXE).read_text(encoding="utf-8"), "2.1.0")

    def test_un_dossier_programme_etranger_n_est_pas_touche(self):
        (self.maison / "programme").mkdir(parents=True)
        (self.maison / "programme" / "autre.txt").write_text("x", encoding="utf-8")
        self.construire("2.1.0")
        with self.assertRaises(build.ArretMiseEnPlace):
            build.mettre_en_place(self.staging, self.maison)
        self.assertEqual([p.name for p in (self.maison / "programme").iterdir()], ["autre.txt"])
        self.assertTrue(self.staging.is_dir())

    def test_les_deux_dispositions_a_la_fois_arretent_tout(self):
        (self.maison / "programme").mkdir(parents=True)
        (self.maison / "programme" / build.EXE).write_text("2.1.0", encoding="utf-8")
        (self.maison / build.EXE).write_text("ancien", encoding="utf-8")
        (self.maison / build.MANIFESTE).write_text(json.dumps({"entrees": [build.EXE]}), encoding="utf-8")
        self.construire("2.2.0")
        with self.assertRaises(build.ArretMiseEnPlace):
            build.mettre_en_place(self.staging, self.maison)
        self.assertEqual((self.maison / build.EXE).read_text(encoding="utf-8"), "ancien")
        self.assertEqual((self.maison / "programme" / build.EXE).read_text(encoding="utf-8"), "2.1.0")

    def test_programme_ouvert_refuse(self):
        self.construire("2.1.0")
        build.mettre_en_place(self.staging, self.maison)
        self.construire("2.2.0")
        with open(self.maison / "programme" / build.EXE, encoding="utf-8"):  # Aria lancée depuis là
            with self.assertRaises(build.ArretMiseEnPlace):
                build.mettre_en_place(self.staging, self.maison)
        self.assertEqual((self.maison / "programme" / build.EXE).read_text(encoding="utf-8"), "2.1.0")


class DemenagementInstalle(unittest.TestCase):
    def test_retour_refuse_quand_aria_est_installee(self):
        with tempfile.TemporaryDirectory() as tmp, \
                mock.patch.object(demenagement, "_ports_occupes", return_value=[]):
            maison = Path(tmp) / ".Aria-Studio"
            (maison / "data").mkdir(parents=True)
            p = demenagement.plan(True, maison / "programme", maison)
            self.assertTrue(any("installée" in b for b in p["blocages"]), p["blocages"])


if __name__ == "__main__":
    unittest.main()
