"""reglages.json (app/fichier_reglages.py) : lecture par section, écriture d'un bloc, migration des anciens fichiers
de réglages, fichier illisible. Tout vit dans un dossier temporaire."""

import json
import tempfile
import threading
import unittest
from pathlib import Path

import yaml

from app import fichier_reglages
from tests import reglages_jetables


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.racine = Path(self.tmp.name)
        self.patch = reglages_jetables.patch(self.racine)
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.tmp.cleanup()

    def ecrire(self, relatif: str, contenu):
        chemin = self.racine / relatif
        chemin.parent.mkdir(parents=True, exist_ok=True)
        if relatif.endswith(".yaml"):
            chemin.write_text(contenu if isinstance(contenu, str) else yaml.safe_dump(contenu, allow_unicode=True),
                              encoding="utf-8")
        else:
            chemin.write_text(contenu if isinstance(contenu, str) else json.dumps(contenu), encoding="utf-8")
        return chemin

    def sur_le_disque(self) -> dict:
        return json.loads((self.racine / "reglages.json").read_text(encoding="utf-8"))


class LectureEcriture(Base):
    def test_rien_n_est_ecrit_avant_le_premier_reglage(self):
        self.assertEqual(fichier_reglages.section("fond"), {})
        self.assertFalse((self.racine / "reglages.json").exists())
        fichier_reglages.ecrire("fond", {"actif": False})
        self.assertEqual(self.sur_le_disque()["fond"], {"actif": False})
        self.assertIn("migration", self.sur_le_disque())

    def test_une_section_vide_disparait(self):
        fichier_reglages.ecrire("fond", {"actif": False})
        fichier_reglages.ecrire("notifications", {"son": False})
        fichier_reglages.ecrire("fond", {})
        self.assertNotIn("fond", self.sur_le_disque())
        self.assertEqual(self.sur_le_disque()["notifications"], {"son": False})

    def test_un_changement_sur_le_disque_est_relu(self):
        fichier_reglages.ecrire("fond", {"actif": False})
        d = self.sur_le_disque()
        d["fond"] = {"actif": True, "vitesse": "lente"}
        (self.racine / "reglages.json").write_text(json.dumps(d), encoding="utf-8")
        self.assertEqual(fichier_reglages.section("fond"), {"actif": True, "vitesse": "lente"})

    def test_ecritures_simultanees_sans_perte(self):
        def ecrire(i):
            with fichier_reglages.transaction() as d:
                d.setdefault("loras", {})[f"lora{i}"] = {"force": i}
        fils = [threading.Thread(target=ecrire, args=(i,)) for i in range(20)]
        for f in fils:
            f.start()
        for f in fils:
            f.join()
        self.assertEqual(len(self.sur_le_disque()["loras"]), 20)


class Migration(Base):
    def anciens(self):
        self.ecrire("aria.yaml", {"modeles": "D:\\IA\\modeles", "comfyui": "E:\\ComfyUI",
                                  "favoris": {"generation": {"retouche": "krea2"}},
                                  "assistant": {"fini": True}, "installation_proposee": False})
        self.ecrire("data/reglages.yaml", "llm:\n  bonsai.gguf:\n    consignes:\n      retouche: |\n        Ligne 1.\n"
                                          "        Ligne 2.\nworkflows:\n  krea2:\n    steps: 12\n")
        self.ecrire("data/fond.json", {"actif": False, "vitesse": "lente", "minimum": 1, "maximum": 3})
        self.ecrire("data/notifications.json", {"active": True, "son": False})
        self.ecrire("data/mise_a_jour.json", {"active": False, "verifie_le": "2026-11-13T10:00:00",
                                              "derniere": {"version": "2.0.0"}})
        self.ecrire("data/loras.json", {"image/Krea2/loras/style.safetensors": {"declencheurs": "x", "force": 0.8}})

    def test_tout_est_reuni_dans_reglages_json(self):
        self.anciens()
        d = fichier_reglages.tout()
        self.assertEqual(d["emplacements"], {"modeles": "D:\\IA\\modeles", "comfyui": "E:\\ComfyUI"})
        self.assertEqual(d["favoris"], {"generation": {"retouche": "krea2"}})
        self.assertEqual(d["assistant"], {"fini": True})
        self.assertEqual(d["installation"], {"proposee": False})
        self.assertEqual(d["llm"]["bonsai.gguf"]["consignes"]["retouche"], "Ligne 1.\nLigne 2.\n")
        self.assertEqual(d["workflows"], {"krea2": {"steps": 12}})
        self.assertEqual(d["fond"], {"actif": False, "vitesse": "lente", "minimum": 1, "maximum": 3})
        self.assertEqual(d["notifications"], {"active": True, "son": False})
        self.assertEqual(d["mises_a_jour"], {"active": False})
        self.assertEqual(d["loras"], {"image/Krea2/loras/style.safetensors": {"declencheurs": "x", "force": 0.8}})
        self.assertEqual(sorted(d["migration"]["depuis"]), sorted(fichier_reglages.ANCIENS))
        self.assertEqual(self.sur_le_disque()["fond"]["vitesse"], "lente")

    def test_les_anciens_restent_intacts_et_sont_copies(self):
        self.anciens()
        avant = {r: (self.racine / r).read_bytes() for r in fichier_reglages.ANCIENS}
        fichier_reglages.tout()
        sauvegardes = list((self.racine / "sauvegardes").glob("avant-migration-*"))
        self.assertEqual(len(sauvegardes), 1)
        for relatif, contenu in avant.items():
            self.assertEqual((self.racine / relatif).read_bytes(), contenu)
            self.assertEqual((sauvegardes[0] / relatif).read_bytes(), contenu)

    def test_le_resultat_de_la_recherche_de_mise_a_jour_va_dans_cache(self):
        self.anciens()
        fichier_reglages.tout()
        cache = json.loads((self.racine / "cache" / "mise_a_jour.json").read_text(encoding="utf-8"))
        self.assertEqual(cache, {"verifie_le": "2026-11-13T10:00:00", "derniere": {"version": "2.0.0"}})

    def test_une_seule_fois(self):
        self.anciens()
        fichier_reglages.tout()
        self.ecrire("data/fond.json", {"actif": True, "vitesse": "rapide"})  # une version d'avant l'a changé
        self.ecrire("reglages.json", self.sur_le_disque())  # nouvelle empreinte : relu
        self.assertEqual(fichier_reglages.section("fond")["vitesse"], "lente")

    def test_ce_que_le_lanceur_a_ecrit_l_emporte(self):
        self.anciens()
        self.ecrire("reglages.json", '{"installation":{"proposee":true}}')
        self.assertEqual(fichier_reglages.section("installation"), {"proposee": True})
        self.assertEqual(fichier_reglages.section("fond")["vitesse"], "lente")  # le reste est bien migré
        self.assertIn("migration", self.sur_le_disque())

    def test_un_ancien_fichier_illisible_est_passe(self):
        self.ecrire("aria.yaml", "modeles: [pas fermé")
        self.ecrire("data/fond.json", "pas du json")
        self.ecrire("data/notifications.json", [1, 2])
        self.ecrire("data/loras.json", {"a": {"force": 1}})
        d = fichier_reglages.tout()
        self.assertEqual(d["migration"]["depuis"], ["data/loras.json"])
        self.assertNotIn("fond", d)


class Illisible(Base):
    def test_jamais_ecrase_sans_copie(self):
        self.ecrire("reglages.json", "{ cassé")
        self.assertEqual(fichier_reglages.section("fond"), {})
        fichier_reglages.ecrire("fond", {"actif": False})
        copies = list((self.racine / "sauvegardes").glob("reglages-illisible-*.json"))
        self.assertEqual([c.read_text(encoding="utf-8") for c in copies], ["{ cassé"])
        self.assertEqual(self.sur_le_disque()["fond"], {"actif": False})

    def test_illisible_n_est_pas_migre_par_dessus(self):
        self.ecrire("data/fond.json", {"actif": False})
        self.ecrire("reglages.json", "[1, 2]")
        self.assertEqual(fichier_reglages.section("fond"), {})
        self.assertEqual((self.racine / "reglages.json").read_text(encoding="utf-8"), "[1, 2]")


if __name__ == "__main__":
    unittest.main()
