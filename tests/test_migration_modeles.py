"""Modèles rangés côté image et côté vidéo (app/modeles.py) et migration de l'ancienne organisation à plat
(app/migration_modeles.py) : renommages, clés de data/ réécrites, échec défait, retour arrière. Tout se passe dans un
dossier temporaire."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

from app import migration_modeles, modeles

VAE_KREA = "qwen_image_vae.safetensors"  # cité par la fiche de Krea 2 : côté image
VAE_H3 = "minimax_h3_video_vae_int8_convrot.safetensors"  # cité par les fiches de MiniMax H3 : côté vidéo
UPSCALER = "4x_maison.safetensors"  # déposé à la main, cité par aucune fiche : côté image, à trier


class Rangement(unittest.TestCase):
    def test_chaque_famille_de_son_cote(self):
        self.assertEqual(modeles.nature("Krea2"), "image")
        self.assertEqual(modeles.nature("Qwen-Image2.1"), "image")  # génération et retouche : image
        self.assertEqual(modeles.nature("MiniMax-H3"), "video")
        self.assertEqual(modeles.dossier_famille("MiniMax-H3", "loras"), "video/MiniMax-H3/loras")
        with self.assertRaises(modeles.ErreurModele):
            modeles.nature("Inconnue")

    def test_dossiers_et_lecture(self):
        d = modeles.dossiers()
        for attendu in ("image/_base/vae", "video/_base/vae", "image/Krea2/modeles", "video/MiniMax-H3/loras", "llm"):
            self.assertIn(attendu, d)
        self.assertFalse([x for x in d if x.startswith("_base/") or x.startswith("Krea2/")])  # plus rien à plat
        self.assertEqual(modeles.decouper("image/Krea2/loras"), ("image", "Krea2", "loras"))
        self.assertEqual(modeles.decouper("video/_base/vae"), ("video", None, "vae"))
        self.assertEqual(modeles.decouper("llm"), (None, None, "llm"))
        self.assertTrue(modeles.est_checkpoint("image/Krea2/modeles"))
        self.assertFalse(modeles.est_checkpoint("image/_base/diffusion_models"))

    def test_les_fiches_rangent_chaque_fichier_du_cote_de_leur_famille(self):
        racine = Path(__file__).resolve().parent.parent / "workflows"
        for chemin in racine.glob("*/fiche.yaml"):
            fiche = yaml.safe_load(chemin.read_text(encoding="utf-8"))
            for f in fiche["fichiers"]:
                self.assertEqual(modeles.decouper(f["dossier"])[0], modeles.nature(fiche["famille"]), (chemin, f))


class AvecDossier(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        d = Path(self.temp.name)
        self.racine, data = d / "modeles", d / "data"
        data.mkdir()
        self.patchs = [mock.patch.object(modeles, "DOSSIER", self.racine),
                       mock.patch.object(modeles, "REGISTRE", data / "modeles.json"),
                       mock.patch.object(modeles, "EMPREINTES", data / "empreintes.json"),
                       mock.patch.object(modeles, "MEMOIRE_LORAS", data / "loras.json"),
                       mock.patch.object(modeles, "CHEMINS_COMFYUI", data / "comfyui_chemins.yaml")]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        self.temp.cleanup()

    def poser(self, relatif: str, contenu: bytes = b"x") -> Path:
        f = self.racine / relatif
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_bytes(contenu)
        return f


class ComfyUI(AvecDossier):
    def test_une_racine_par_nature(self):
        declaration = yaml.safe_load(modeles.ecrire_chemins_comfyui().read_text(encoding="utf-8"))
        self.assertEqual(set(declaration), {"aria_image", "aria_video"})
        image, video = declaration["aria_image"], declaration["aria_video"]
        self.assertEqual(Path(image["base_path"]), self.racine / "image")
        self.assertTrue(image.get("is_default"))
        self.assertIn("Krea2/modeles/", image["diffusion_models"])
        self.assertNotIn("MiniMax-H3", image["diffusion_models"] + image["loras"])
        self.assertIn("MiniMax-H3/loras/", video["loras"])
        self.assertEqual(video["vae"], "_base/vae/")
        self.assertTrue((self.racine / "video" / "MiniMax-H3" / "modeles").is_dir())

    def test_homonymes_signales(self):
        self.poser(f"image/_base/vae/{VAE_KREA}")
        self.assertEqual(modeles.homonymes(), [])
        self.poser(f"video/_base/vae/{VAE_KREA}")
        self.assertEqual(modeles.homonymes(), [VAE_KREA])


class Migration(AvecDossier):
    def ancienne_organisation(self):
        self.poser(f"_base/vae/{VAE_KREA}")
        self.poser(f"_base/vae/{VAE_H3}")
        self.poser(f"_base/upscale_models/{UPSCALER}")
        self.poser("Krea2/loras/style.safetensors")
        self.poser("MiniMax-H3/modeles/dasiwa.safetensors")
        self.poser("llm/Bonsai.gguf")  # hors migration
        ecrire = lambda f, d: f.write_text(json.dumps(d), encoding="utf-8")  # noqa: E731
        ecrire(modeles.REGISTRE, {"Krea2/loras/style.safetensors": {"source": "civitai"},
                                  f"_base/vae/{VAE_H3}": {"source": "aria"}, "llm/Bonsai.gguf": {"source": "hf"}})
        ecrire(modeles.EMPREINTES, {"MiniMax-H3/modeles/dasiwa.safetensors": {"sha256": "ab"}})
        ecrire(modeles.MEMOIRE_LORAS, {"Krea2/loras/style.safetensors": {"declencheurs": "x", "force": 0.8}})

    def lire(self, fichier: Path) -> dict:
        return json.loads(fichier.read_text(encoding="utf-8"))

    def test_rien_a_faire(self):
        self.assertIsNone(migration_modeles.migrer())
        self.poser("image/Krea2/modeles/a.safetensors")
        self.assertIsNone(migration_modeles.migrer())

    def test_essai_a_blanc(self):
        self.ancienne_organisation()
        p = migration_modeles.plan()
        self.assertIn((f"_base/vae/{VAE_H3}", f"video/_base/vae/{VAE_H3}"), p["deplacements"])
        self.assertEqual(p["a_trier"], [f"image/_base/upscale_models/{UPSCALER}"])
        self.assertTrue((self.racine / "_base" / "vae" / VAE_KREA).is_file())  # rien n'a bougé

    def test_migration(self):
        self.ancienne_organisation()
        r = migration_modeles.migrer()
        self.assertEqual(r["statut"], "faite")
        for relatif in (f"image/_base/vae/{VAE_KREA}", f"video/_base/vae/{VAE_H3}",
                        f"image/_base/upscale_models/{UPSCALER}", "image/Krea2/loras/style.safetensors",
                        "video/MiniMax-H3/modeles/dasiwa.safetensors", "llm/Bonsai.gguf"):
            self.assertTrue((self.racine / relatif).is_file(), relatif)
        for ancien in ("_base", "Krea2", "MiniMax-H3"):
            self.assertFalse((self.racine / ancien).exists(), ancien)  # dossiers vidés, supprimés
        self.assertEqual(r["a_trier"], [f"image/_base/upscale_models/{UPSCALER}"])
        self.assertEqual(set(self.lire(modeles.REGISTRE)), {"image/Krea2/loras/style.safetensors",
                                                            f"video/_base/vae/{VAE_H3}", "llm/Bonsai.gguf"})
        self.assertEqual(self.lire(modeles.EMPREINTES), {"video/MiniMax-H3/modeles/dasiwa.safetensors": {"sha256": "ab"}})
        self.assertEqual(modeles.loras("Krea2")[0]["force"], 0.8)  # le réglage de la LoRA a suivi
        sauvegarde = modeles.REGISTRE.with_name("modeles.json" + migration_modeles.SUFFIXE_SAUVEGARDE)
        self.assertIn("Krea2/loras/style.safetensors", self.lire(sauvegarde))
        self.assertEqual(migration_modeles.etat()["statut"], "faite")
        self.assertIsNone(migration_modeles.migrer())  # relancée : plus rien à faire

    def test_echec_tout_est_defait(self):
        self.ancienne_organisation()
        avant = self.lire(modeles.REGISTRE)
        vrai, appels = Path.rename, []

        def rename(chemin, cible):
            appels.append(chemin)
            if len(appels) == 3:
                raise PermissionError("fichier ouvert par un autre processus")
            return vrai(chemin, cible)

        with mock.patch.object(Path, "rename", rename):
            r = migration_modeles.migrer()
        self.assertEqual(r["statut"], "echec")
        self.assertIn("PermissionError", r["erreur"])
        self.assertTrue((self.racine / "_base" / "vae" / VAE_KREA).is_file())
        self.assertTrue((self.racine / "Krea2" / "loras" / "style.safetensors").is_file())
        self.assertFalse(list((self.racine / "image").rglob("*.safetensors")) if (self.racine / "image").exists() else [])
        self.assertEqual(self.lire(modeles.REGISTRE), avant)  # data/ intact
        self.assertEqual(migration_modeles.migrer()["statut"], "faite")  # réessayée au démarrage suivant

    def test_deja_present_a_l_arrivee_laisse_en_place(self):
        self.ancienne_organisation()
        self.poser(f"video/_base/vae/{VAE_H3}", b"neuf")
        r = migration_modeles.migrer()
        self.assertEqual(r["conflits"], [f"_base/vae/{VAE_H3}"])
        self.assertEqual((self.racine / "video" / "_base" / "vae" / VAE_H3).read_bytes(), b"neuf")
        self.assertTrue((self.racine / "_base" / "vae" / VAE_H3).is_file())  # rien d'écrasé
        self.assertIsNone(migration_modeles.migrer())  # au démarrage suivant : le journal de la migration reste
        self.assertEqual(len(migration_modeles.etat()["deplacements"]), 4)

    def test_retour(self):
        self.ancienne_organisation()
        avant = self.lire(modeles.REGISTRE)
        migration_modeles.migrer()
        self.assertTrue(migration_modeles.retour())  # essai à blanc : rien ne bouge
        self.assertTrue((self.racine / "video" / "_base" / "vae" / VAE_H3).is_file())
        migration_modeles.retour(essai=False)
        self.assertTrue((self.racine / "_base" / "vae" / VAE_H3).is_file())
        self.assertTrue((self.racine / "Krea2" / "loras" / "style.safetensors").is_file())
        self.assertEqual(self.lire(modeles.REGISTRE), avant)
        self.assertEqual(migration_modeles.etat()["statut"], "defaite")
        with self.assertRaises(RuntimeError):
            migration_modeles.retour(essai=False)


if __name__ == "__main__":
    unittest.main()
