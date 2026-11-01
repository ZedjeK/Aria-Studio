"""Tests des emplacements (app/emplacements.py) : le code et le dossier de l'utilisateur sont simulés dans un dossier temporaire."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

from app import emplacements


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.code, self.maison = base / "code", base / "maison" / ".Aria-Studio"
        self.code.mkdir()
        self.patchs = [mock.patch.object(emplacements, "ROOT", self.code),
                       mock.patch.object(emplacements, "DEFAUT", self.maison)]
        for p in self.patchs:
            p.start()
        self.figer()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.tmp.cleanup()

    def figer(self):
        """Refait ce que le module calcule à l'import (RACINE, DATA, MODELES, MOTEURS)."""
        racine = emplacements._racine()
        for nom, valeur in (("RACINE", racine), ("DATA", racine / "data"),
                            ("MODELES", emplacements._modeles(emplacements._choix())),
                            ("MOTEURS", racine / ("services" if emplacements.portable() else "moteurs"))):
            p = mock.patch.object(emplacements, nom, valeur)
            p.start()
            self.patchs.append(p)


class Modes(Base):
    def test_par_defaut_dans_le_dossier_utilisateur(self):
        self.assertFalse(emplacements.portable())
        self.assertEqual(emplacements.DATA, self.maison / "data")
        self.assertEqual(emplacements.MODELES, self.maison / "modeles")
        self.assertEqual(emplacements.moteur("comfyui"), self.maison / "moteurs" / "comfyui")
        self.assertFalse(self.code.joinpath("data").exists())  # le choix du mode ne crée rien à côté du code

    def test_portable_si_data_a_cote_du_code(self):
        (self.code / "data").mkdir()
        self.figer()
        self.assertTrue(emplacements.portable())
        self.assertEqual(emplacements.DATA, self.code / "data")
        self.assertEqual(emplacements.MODELES, self.code / "modeles")
        self.assertEqual(emplacements.moteur("llama/llama-server.exe"), self.code / "services" / "llama" / "llama-server.exe")

    def test_moteur_absolu_garde_tel_quel(self):
        absolu = Path(self.tmp.name) / "ailleurs" / "comfyui"
        self.assertEqual(emplacements.moteur(str(absolu)), absolu)


class DossierModeles(Base):
    def test_choix_valable_au_prochain_demarrage(self):
        nouveau = Path(self.tmp.name) / "D" / "IA" / "modeles"
        e = emplacements.choisir_modeles(str(nouveau))
        self.assertTrue(nouveau.is_dir())
        self.assertEqual(list(nouveau.iterdir()), [])  # le fichier d'essai d'écriture est parti
        self.assertTrue(e["redemarrage"])
        self.assertEqual(e["modeles_prochain"], str(nouveau))
        self.assertEqual(e["dossiers"]["modeles"]["chemin"], str(self.maison / "modeles"))  # pas avant le redémarrage
        self.assertEqual(yaml.safe_load((self.maison / "aria.yaml").read_text(encoding="utf-8")), {"modeles": str(nouveau)})
        self.figer()  # redémarrage
        self.assertEqual(emplacements.MODELES, nouveau)
        self.assertFalse(emplacements.etat()["redemarrage"])

    def test_retour_au_defaut(self):
        emplacements.choisir_modeles(str(Path(self.tmp.name) / "ailleurs"))
        e = emplacements.choisir_modeles(None)
        self.assertIsNone(e["modeles_choisi"])
        self.assertFalse(e["redemarrage"])

    def test_refus(self):
        fichier = Path(self.tmp.name) / "un_fichier.txt"
        fichier.write_text("x", encoding="utf-8")
        for chemin, attendu in (("modeles", "chemin complet"), (str(fichier), "pas un dossier"),
                                (str(self.maison / "data" / "modeles"), "ne peuvent pas aller"),
                                (str(self.maison / "moteurs"), "ne peuvent pas aller")):
            with self.subTest(chemin=chemin), self.assertRaisesRegex(emplacements.ErreurEmplacement, attendu):
                emplacements.choisir_modeles(chemin)
        self.assertFalse((self.maison / "aria.yaml").exists())

    def test_aria_yaml_illisible(self):
        self.maison.mkdir(parents=True)
        (self.maison / "aria.yaml").write_text("modeles: [pas fermé", encoding="utf-8")
        self.assertEqual(emplacements._modeles(emplacements._choix()), self.maison / "modeles")


class Favoris(Base):
    def test_retenus_dans_aria_yaml_avec_le_dossier_des_modeles(self):
        emplacements.choisir_modeles(str(Path(self.tmp.name) / "ailleurs"))
        emplacements.retenir_favori("generation", "texte_vers_image", "krea2")
        emplacements.retenir_favori("llama", "modele", "")
        self.assertEqual(emplacements.favoris(), {"generation": {"texte_vers_image": "krea2"}, "llama": {"modele": ""}})
        self.assertEqual(emplacements._choix()["modeles"], str(Path(self.tmp.name) / "ailleurs"))  # rien d'écrasé

    def test_passent_devant_config_yaml(self):
        from app import config
        emplacements.retenir_favori("generation", "retouche", "mon_favori")
        c = config.charger()
        self.assertEqual(c["generation"]["retouche"], "mon_favori")
        self.assertIn("texte_vers_image", c["generation"])  # le reste de config.yaml est gardé

    def test_comfyui_externe_remplace_celui_d_aria(self):
        from app import config
        sien = Path(self.tmp.name) / "D" / "ComfyUI_windows_portable"
        emplacements.choisir_modeles(str(Path(self.tmp.name) / "ailleurs"))
        emplacements.choisir_comfyui(sien)
        self.assertEqual(emplacements.comfyui_externe(), sien)
        self.assertEqual(config.charger()["comfyui"]["dossier"], str(sien))
        self.assertEqual(emplacements.moteur(config.charger()["comfyui"]["dossier"]), sien)  # absolu : gardé tel quel
        emplacements.choisir_comfyui(None)
        self.assertIsNone(emplacements.comfyui_externe())
        self.assertEqual(config.charger()["comfyui"]["dossier"], config.dossier_comfyui_livre())
        self.assertIn("modeles", emplacements._choix())  # le reste d'aria.yaml est gardé

    def test_aria_yaml_bizarre_ignore(self):
        self.maison.mkdir(parents=True)
        (self.maison / "aria.yaml").write_text("favoris: [1, 2]", encoding="utf-8")
        self.assertEqual(emplacements.favoris(), {})


class Ouvrir(Base):
    def test_dossiers_connus_seulement(self):
        with mock.patch.object(emplacements.os, "startfile", create=True) as startfile:
            self.assertEqual(emplacements.ouvrir("data"), self.maison / "data")
            startfile.assert_called_once_with(self.maison / "data")
            self.assertTrue((self.maison / "data").is_dir())
            ouvert = emplacements.ouvrir("modeles", "MiniMax-H3/modeles")
            self.assertEqual(ouvert, (self.maison / "modeles" / "MiniMax-H3" / "modeles").resolve())
            for cle, sous in (("C:\\Windows", None), ("modeles", "../../.."), ("modeles", "..")):
                with self.subTest(cle=cle, sous=sous), self.assertRaises(emplacements.ErreurEmplacement):
                    emplacements.ouvrir(cle, sous)
            self.assertEqual(startfile.call_count, 2)


if __name__ == "__main__":
    unittest.main()
