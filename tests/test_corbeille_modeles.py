"""Corbeille des modèles (app/modeles.py) : un modèle téléchargé part dans <dossier des modèles>\\.corbeille\\ avec sa
fiche, revient à l'identique, et n'est effacé que par « Vider la corbeille ». Dossier des modèles et registres
temporaires ; rien n'est téléchargé.
"""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import modeles

CLE = "image/Qwen-Image-2.1/modeles/qwen_image_2.1_fp8.safetensors"
FICHE = {"source": "huggingface", "url": "https://huggingface.co/Qwen/x", "sha256": "ab" * 32, "licence": "Qwen"}


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.dossier = base / "modeles"
        self.patchs = [mock.patch.object(modeles, "DOSSIER", self.dossier),
                       mock.patch.object(modeles, "REGISTRE", base / "data" / "modeles.json"),
                       mock.patch.object(modeles, "CORBEILLE_REGISTRE", base / "data" / "corbeille_modeles.json")]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        self.tmp.cleanup()

    def telecharge(self, cle=CLE, contenu=b"poids"):
        fichier = self.dossier / cle
        fichier.parent.mkdir(parents=True, exist_ok=True)
        fichier.write_bytes(contenu)
        registre = modeles.charger_registre()
        registre[cle] = dict(FICHE)
        modeles._sauver_registre(registre)
        return fichier


class Corbeille(Base):
    def test_a_la_corbeille_puis_restaure_avec_sa_fiche(self):
        fichier = self.telecharge()
        modeles.mettre_a_la_corbeille(CLE)
        self.assertFalse(fichier.exists())
        self.assertEqual((self.dossier / ".corbeille" / CLE).read_bytes(), b"poids")
        self.assertNotIn(CLE, modeles.charger_registre())
        c = modeles.corbeille()
        self.assertEqual(c["taille"], 5)
        self.assertEqual([(m["cle"], m["present"], m["ailleurs"]) for m in c["modeles"]], [(CLE, True, False)])
        modeles.restaurer(CLE)
        self.assertEqual(fichier.read_bytes(), b"poids")
        self.assertEqual(modeles.charger_registre()[CLE], FICHE)
        self.assertEqual(modeles.corbeille(), {"modeles": [], "taille": 0})
        self.assertFalse((self.dossier / ".corbeille").exists())  # les sous-dossiers vidés ne restent pas

    def test_ni_l_inventaire_ni_comfyui_ne_voient_la_corbeille(self):
        self.telecharge()
        with mock.patch.object(modeles, "dossiers", return_value=["image/Qwen-Image-2.1/modeles"]):
            self.assertEqual(len(modeles.inventaire()), 1)
            modeles.mettre_a_la_corbeille(CLE)
            self.assertEqual(modeles.inventaire(), [])
        self.assertEqual(modeles.fichiers("image/Qwen-Image-2.1/modeles", modeles.EXTENSIONS_MODELES), [])
        self.assertEqual(modeles.homonymes(), [])

    def test_un_fichier_depose_a_la_main_reste_refuse(self):
        fichier = self.dossier / "image" / "_base" / "vae" / "a-moi.safetensors"
        fichier.parent.mkdir(parents=True)
        fichier.write_bytes(b"x")
        with self.assertRaisesRegex(modeles.ErreurModele, "téléchargés par l'appli"):
            modeles.mettre_a_la_corbeille("image/_base/vae/a-moi.safetensors")
        self.assertTrue(fichier.exists())

    def test_chemin_hors_du_dossier_refuse(self):
        registre = {"../../data/modeles.json": dict(FICHE)}
        modeles._sauver_registre(registre)
        with self.assertRaises(modeles.ErreurModele):
            modeles.mettre_a_la_corbeille("../../data/modeles.json")

    def test_restauration_refusee_si_retelecharge(self):
        self.telecharge()
        modeles.mettre_a_la_corbeille(CLE)
        self.telecharge(contenu=b"nouveau")
        with self.assertRaisesRegex(modeles.ErreurModele, "déjà présent"):
            modeles.restaurer(CLE)
        self.assertEqual((self.dossier / CLE).read_bytes(), b"nouveau")
        self.assertTrue((self.dossier / ".corbeille" / CLE).is_file())

    def test_deux_fois_le_meme_modele_a_la_corbeille_refuse(self):
        self.telecharge()
        modeles.mettre_a_la_corbeille(CLE)
        self.telecharge(contenu=b"nouveau")
        with self.assertRaisesRegex(modeles.ErreurModele, "déjà dans la corbeille"):
            modeles.mettre_a_la_corbeille(CLE)
        self.assertEqual((self.dossier / CLE).read_bytes(), b"nouveau")

    def test_vider_efface_pour_de_bon(self):
        self.telecharge()
        autre = "image/Qwen-Image-2.1/loras/style.safetensors"
        self.telecharge(autre, b"lora")
        modeles.mettre_a_la_corbeille(CLE)
        modeles.mettre_a_la_corbeille(autre)
        self.assertEqual(modeles.vider_corbeille(), {"effaces": 2, "liberes": 9, "restes": 0})
        self.assertFalse((self.dossier / ".corbeille").exists())
        self.assertEqual(modeles.corbeille()["modeles"], [])
        with self.assertRaisesRegex(modeles.ErreurModele, "pas dans la corbeille"):
            modeles.restaurer(CLE)

    def test_fichier_ouvert_rien_ne_bouge(self):
        fichier = self.telecharge()
        with mock.patch.object(Path, "rename", side_effect=PermissionError(13, "Accès refusé")):
            with self.assertRaisesRegex(modeles.ErreurModele, "ComfyUI"):
                modeles.mettre_a_la_corbeille(CLE)
        self.assertTrue(fichier.is_file())
        self.assertIn(CLE, modeles.charger_registre())
        self.assertEqual(modeles.corbeille()["modeles"], [])

    def test_ancien_dossier_des_modeles(self):
        self.telecharge()
        modeles.mettre_a_la_corbeille(CLE)
        nouveau = Path(self.tmp.name) / "ailleurs"
        with mock.patch.object(modeles, "DOSSIER", nouveau):
            c = modeles.corbeille()
            self.assertTrue(c["modeles"][0]["ailleurs"])
            self.assertEqual(c["modeles"][0]["dossier_modeles"], str(self.dossier))
            with self.assertRaisesRegex(modeles.ErreurModele, "ancien dossier"):
                modeles.restaurer(CLE)
            self.assertEqual(modeles.vider_corbeille()["effaces"], 1)  # le vidage touche aussi l'ancien dossier
        self.assertFalse((self.dossier / ".corbeille" / CLE).exists())

    def test_registre_de_corbeille_illisible(self):
        modeles.CORBEILLE_REGISTRE.parent.mkdir(parents=True, exist_ok=True)
        modeles.CORBEILLE_REGISTRE.write_text("pas du json", encoding="utf-8")
        self.assertEqual(modeles.corbeille(), {"modeles": [], "taille": 0})
        modeles.CORBEILLE_REGISTRE.write_text(json.dumps([1]), encoding="utf-8")
        self.assertEqual(modeles.corbeille(), {"modeles": [], "taille": 0})


if __name__ == "__main__":
    unittest.main()
