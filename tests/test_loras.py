"""LoRA : mots déclencheurs et force. Civitai n'a pas de champ « force conseillée » : la force vient des images
d'exemple de l'auteur (civitai.usage_auteur) ; l'utilisateur les règle dans Ma collection (modeles.regler_lora).
Rien n'est téléchargé : registre, mémoire et dossier des modèles sont temporaires."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import modeles
from app.sources import civitai

FICHIER = "pubes_krea2_loraholic.safetensors"
CLE = f"image/Krea2/loras/{FICHIER}"


def image(*ressources, prompt="a woman, film photo"):
    return {"meta": {"prompt": prompt, "additionalResources": list(ressources)}}


class UsageAuteur(unittest.TestCase):
    def test_force_mediane_des_images_d_exemple(self):
        version = {"images": [image({"name": FICHIER, "type": "lora", "strength": 0.8}),
                              image({"name": FICHIER, "type": "lora", "strength": 1.0}, prompt=""),
                              image({"name": FICHIER, "type": "lora", "strength": 0.7})]}
        self.assertEqual(civitai.usage_auteur(version, FICHIER),
                         {"force_auteur": 0.8, "images_force": 3, "prompt_exemple": "a woman, film photo"})

    def test_plusieurs_lora_sur_l_image(self):
        """Seule celle qui porte le nom du fichier compte ; une valeur absurde est ignorée."""
        version = {"images": [image({"name": "detail_slider.safetensors", "type": "lora", "strength": 2.0},
                                    {"name": FICHIER, "type": "LORA", "strength": 0.65}),
                              image({"name": FICHIER, "type": "lora", "strength": 5}),
                              image({"name": "checkpoint.safetensors", "type": "checkpoint", "strength": 1})]}
        self.assertEqual(civitai.usage_auteur(version, FICHIER)["force_auteur"], 0.65)
        self.assertEqual(civitai.usage_auteur({"images": []}, FICHIER),
                         {"force_auteur": None, "images_force": 0, "prompt_exemple": None})

    def test_id_de_version_de_la_page(self):
        self.assertEqual(civitai.id_version("https://civitai.red/models/2617090?modelVersionId=3136749"), "3136749")
        self.assertIsNone(civitai.id_version("https://huggingface.co/x/y"))


class Reglage(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        (d / "modeles" / "image" / "Krea2" / "loras").mkdir(parents=True)
        (d / "modeles" / "image" / "Krea2" / "loras" / FICHIER).write_bytes(b"x")
        (d / "modeles.json").write_text(json.dumps({CLE: {
            "source": "civitai", "page": "https://civitai.red/models/2617090?modelVersionId=3136749",
            "mots_declencheurs": [], "force_auteur": 0.8, "images_force": 3, "prompt_exemple": "a woman"}}),
            encoding="utf-8")
        self.patchs = [mock.patch.object(modeles, "DOSSIER", d / "modeles"),
                       mock.patch.object(modeles, "REGISTRE", d / "modeles.json"),
                       mock.patch.object(modeles, "MEMOIRE_LORAS", d / "loras.json")]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        self.dossier.cleanup()

    def test_la_source_d_abord_puis_ton_reglage(self):
        r = modeles.reglage_lora(CLE)
        self.assertEqual((r["force"], r["declencheurs"], r["regle"], r["civitai"]), (0.8, "", False, True))
        self.assertEqual(modeles.loras("Krea2")[0]["force"], 0.8)  # ce que le Plateau proposera
        r = modeles.regler_lora(CLE, "  pubic hair ,  natural  ", 0.65)
        self.assertEqual((r["force"], r["declencheurs"], r["regle"]), (0.65, "pubic hair , natural", True))
        self.assertEqual(modeles.loras("Krea2")[0]["declencheurs"], "pubic hair , natural")
        r = modeles.oublier_reglage_lora(CLE)
        self.assertEqual((r["force"], r["regle"]), (0.8, False))

    def test_relire_civitai_remplace_la_source(self):
        modeles.mettre_a_jour_source(CLE, {"mots_declencheurs": ["loraholic"], "force_auteur": None,
                                           "images_force": 0, "prompt_exemple": None})
        r = modeles.reglage_lora(CLE)
        self.assertEqual((r["declencheurs"], r["force"]), ("loraholic", 1.0))  # plus de force d'auteur : 1,0

    def test_validation(self):
        for cle, force in (("image/Krea2/loras/absente.safetensors", 1.0), ("image/_base/loras/x.safetensors", 1.0),
                           ("../../secret", 1.0), (CLE, 2.5)):
            with self.assertRaises(modeles.ErreurModele, msg=cle):
                modeles.regler_lora(cle, "", force)
        with self.assertRaises(modeles.ErreurModele):
            modeles.regler_lora(CLE, "x" * 301, 1.0)


if __name__ == "__main__":
    unittest.main()
