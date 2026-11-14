"""Choix du moteur d'un LLM : en-tête GGUF (app/gguf.py) et réglage « Moteur » (app/reglages.py).

Les GGUF sont fabriqués à la volée (en-tête seul) ; data/reglages.yaml est remplacé par un fichier temporaire.
"""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import gguf, modeles, reglages
from tests import reglages_jetables


def _chaine(texte: str) -> bytes:
    octets = texte.encode()
    return len(octets).to_bytes(8, "little") + octets


def ecrire_gguf(chemin: Path, types: list[int]):
    """Un en-tête GGUF v3 : trois métadonnées (chaîne, tableau de chaînes, entier) puis un tenseur par type."""
    u32 = lambda n: n.to_bytes(4, "little")  # noqa: E731
    u64 = lambda n: n.to_bytes(8, "little")  # noqa: E731
    d = b"GGUF" + u32(3) + u64(len(types)) + u64(3)
    d += _chaine("general.architecture") + u32(8) + _chaine("qwen35")
    d += _chaine("tokenizer.ggml.tokens") + u32(9) + u32(8) + u64(2) + _chaine("a") + _chaine("b")
    d += _chaine("general.file_type") + u32(4) + u32(1)
    for i, t in enumerate(types):
        d += _chaine(f"blk.{i}.weight") + u32(2) + u64(4) + u64(4) + u32(t) + u64(0)
    chemin.write_bytes(d)


class EnTete(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.d = Path(self.dossier.name)

    def tearDown(self):
        self.dossier.cleanup()

    def test_types_lus(self):
        ecrire_gguf(self.d / "a.gguf", [0, 30, 12])
        self.assertEqual(gguf.types_tenseurs(self.d / "a.gguf"), {0, 12, 30})
        self.assertEqual(gguf.types_inconnus(self.d / "a.gguf"), [])

    def test_type_d_un_fork(self):
        """Bonsai 2 Heretic PQ2_0 : type 142, refusé par le llama.cpp officiel (« should be in [0, 43) »)."""
        ecrire_gguf(self.d / "b.gguf", [0, 142, 30])
        self.assertEqual(gguf.types_inconnus(self.d / "b.gguf"), [142])

    def test_fichier_abime(self):
        (self.d / "c.gguf").write_bytes(b"GGUF" + (3).to_bytes(4, "little") + b"\x01")
        with self.assertRaises(gguf.ErreurGguf):
            gguf.types_tenseurs(self.d / "c.gguf")
        (self.d / "d.gguf").write_bytes(b"PK\x03\x04")
        with self.assertRaises(gguf.ErreurGguf):
            gguf.types_tenseurs(self.d / "d.gguf")

    def test_relu_si_le_fichier_change(self):
        chemin = self.d / "e.gguf"
        ecrire_gguf(chemin, [0])
        self.assertEqual(gguf.types_inconnus(chemin), [])
        ecrire_gguf(chemin, [0, 143, 143])  # même nom, autre taille
        self.assertEqual(gguf.types_inconnus(chemin), [143])


class Moteur(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        (d / "llm").mkdir()
        ecrire_gguf(d / "llm" / "standard.gguf", [0, 12])
        ecrire_gguf(d / "llm" / "bonsai-finetune.gguf", [0, 142])
        self.patchs = [reglages_jetables.patch(d),
                       mock.patch.object(reglages, "_cache", None),
                       mock.patch.object(modeles, "DOSSIER", d),
                       mock.patch.object(modeles, "llm_disponibles",
                                         return_value=["standard.gguf", "bonsai-finetune.gguf"])]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def test_automatique_selon_le_format(self):
        self.assertEqual(reglages.moteur_llm("standard.gguf")[0], "llama")
        moteur, raison = reglages.moteur_llm("bonsai-finetune.gguf")
        self.assertEqual(moteur, "llama-prism")
        self.assertIn("142", raison)

    def test_choix_manuel_prioritaire(self):
        reglages.modifier_llm("standard.gguf", {"moteur": "llama-prism"})
        reglages.modifier_llm("bonsai-finetune.gguf", {"moteur": "llama"})
        self.assertEqual(reglages.moteur_llm("standard.gguf"), ("llama-prism", "choisi à la main"))
        self.assertEqual(reglages.moteur_llm("bonsai-finetune.gguf")[0], "llama")
        reglages.modifier_llm("standard.gguf", {"moteur": "auto"})  # retour à l'automatique : rien de gardé
        self.assertNotIn("standard.gguf", reglages.lire()["llm"])

    def test_moteur_inconnu_refuse(self):
        with self.assertRaises(reglages.ErreurReglages):
            reglages.modifier_llm("standard.gguf", {"moteur": "ollama"})

    def test_nomme_dans_config_ou_illisible(self):
        with mock.patch.dict(reglages.CONFIG["llama"], {"serveurs_specifiques": {"standard.gguf": "x"}}):
            self.assertEqual(reglages.moteur_llm("standard.gguf")[0], "llama-prism")
        self.assertEqual(reglages.moteur_llm("absent.gguf")[0], "llama")  # en-tête illisible : l'officiel

    def test_serveurs(self):
        self.assertEqual(reglages.serveur_llm("llama"), reglages.CONFIG["llama"]["server"])
        self.assertEqual(reglages.serveur_llm("llama-prism"), "llama-prism/llama-server.exe")

    def test_libelles_et_moteur_en_service_pour_reglages(self):
        llms = {m["fichier"]: m for m in reglages.etat()["llm"]}
        moteur = next(p for p in llms["bonsai-finetune.gguf"]["params"] if p["cle"] == "moteur")
        self.assertEqual(moteur["valeur"], "auto")
        self.assertEqual(moteur["libelles"]["llama-prism"], "PrismML (ternaire)")
        self.assertTrue(moteur["aide"].startswith("En service : PrismML (ternaire)"))


if __name__ == "__main__":
    unittest.main()
