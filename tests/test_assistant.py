"""Tests de l'assistant de premier lancement (app/assistant.py) : faux packs, faux workflows, aria.yaml temporaire."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

from app import assistant, emplacements
from app.telechargement import ErreurTelechargement

GO = 1024 ** 3
COMMUN = {"dossier": "_base/vae", "nom": "vae.safetensors", "url": "https://huggingface.co/org/depot/resolve/main/vae.safetensors",
          "taille": 1 * GO, "sha256": "a" * 64}
IMAGE = {"dossier": "Image/modeles", "nom": "image.safetensors", "url": "https://huggingface.co/org/image/resolve/main/image.safetensors",
         "taille": 10 * GO, "sha256": "b" * 64}
VIDEO = {"dossier": "Video/modeles", "nom": "video.safetensors", "url": "https://huggingface.co/org/video/resolve/main/video.safetensors",
         "taille": 20 * GO, "sha256": "c" * 64}
LICENCE_LIBRE = {"id": "apache-2.0", "nom": "Apache 2.0", "url": "https://apache.org", "resume": "libre", "commercial": True,
                 "territoires_exclus": []}
LICENCE_VIDEO = {"id": "video", "nom": "Licence vidéo", "url": "https://exemple.org", "resume": "restreinte", "commercial": True,
                 "territoires_exclus": ["Union européenne"], "composants": [{"fichier": "turbo", "licence": "non déclarée"}]}

PACKS = {"format": 1, "packs": {
    "images": {"nom": "Images", "usage": "images", "workflows": ["image"], "moteurs": ["comfyui"], "conseille": True,
               "favoris": {"generation": {"texte_vers_image": "image", "retouche": "image"}}},
    "video": {"nom": "Vidéo", "usage": "vidéo", "workflows": ["video"], "moteurs": ["comfyui"],
              "favoris": {"generation": {"image_vers_video": "video", "retouche": "video"}}},
    "llm": {"nom": "LLM", "usage": "prompts", "llm": ["petit.gguf"], "moteurs": ["llama"],
            "favoris": {"llama": {"modele": "petit.gguf"}}},
}}


class FauxWorkflow:
    def __init__(self, id_, fichiers, licence, modeles_dir):
        self.id, self.licence, self._dir = id_, licence, modeles_dir
        self.fiche = {"fichiers": fichiers, "nom": id_.capitalize()}

    def manquants(self):
        return [f for f in self.fiche["fichiers"] if not (self._dir / f["dossier"] / f["nom"]).is_file()]


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.modeles = base / "modeles"
        (base / "packs.yaml").write_text(yaml.safe_dump(PACKS, sort_keys=False), encoding="utf-8")  # l'ordre compte
        self.wfs = {"image": FauxWorkflow("image", [COMMUN, IMAGE], LICENCE_LIBRE, self.modeles),
                    "video": FauxWorkflow("video", [COMMUN, VIDEO], LICENCE_VIDEO, self.modeles)}
        llm = {"id": "petit.gguf", "url": "https://huggingface.co/org/llm/resolve/main/petit.gguf", "taille": 2 * GO,
               "sha256": "d" * 64, "licence": LICENCE_LIBRE}
        self.libre = 500 * GO
        self.etat_moteurs = [{"id": "comfyui", "nom": "ComfyUI", "statut": "absent", "facultatif": False},
                             {"id": "llama", "nom": "llama.cpp", "statut": "a_jour", "facultatif": False}]
        self.config = {"generation": {}, "llama": {}}
        self.patchs = [
            mock.patch.object(assistant, "CATALOGUE", base / "packs.yaml"),
            mock.patch.object(assistant.workflows, "obtenir", lambda i: self.wfs[i]),
            mock.patch.object(assistant.workflows, "catalogue", lambda: list(self.wfs.values())),
            mock.patch.object(assistant.fiches, "fiche_llm", lambda g: llm if g == "petit.gguf" else None),
            mock.patch.object(assistant.modeles, "destination", lambda d, n: self.modeles / d / n),
            mock.patch.object(assistant.modeles, "espace_libre", lambda: self.libre),
            mock.patch.object(assistant.moteurs, "etat", lambda: self.etat_moteurs),
            mock.patch.object(emplacements, "_fichier_choix", lambda: base / "aria.yaml"),
            mock.patch.object(assistant, "CONFIG", self.config),
        ]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.tmp.cleanup()

    def poser(self, f):
        chemin = self.modeles / f["dossier"] / f["nom"]
        chemin.parent.mkdir(parents=True, exist_ok=True)
        chemin.write_bytes(b"x")


class Packs(Base):
    def test_tailles_licences_et_moteurs(self):
        self.poser(COMMUN)
        p = {x["id"]: x for x in assistant.packs()}
        self.assertEqual((p["images"]["taille"], p["images"]["a_telecharger"]), (11 * GO, 10 * GO))
        self.assertFalse(p["images"]["installe"])
        self.assertEqual(p["images"]["moteurs_absents"], ["ComfyUI"])
        self.assertEqual(p["llm"]["moteurs_absents"], [])
        video = p["video"]["licences"][0]
        self.assertEqual((video["territoires_exclus"], video["composants"]), (["Union européenne"], ["turbo : non déclarée"]))

    def test_plan_compte_les_fichiers_communs_une_fois(self):
        p = assistant.plan(["video", "images"])
        self.assertEqual(p["a_telecharger"], 31 * GO)  # 1 + 10 + 20 : le VAE commun une seule fois
        self.assertEqual([f["nom"] for f in p["fichiers"]], ["vae.safetensors", "image.safetensors", "video.safetensors"])
        self.assertEqual(p["licences_a_accepter"], ["images", "video"])
        self.assertTrue(p["place_ok"])
        self.libre = 35 * GO  # 31 Go + 10 de marge ne tiennent pas
        self.assertFalse(assistant.plan(["video", "images"])["place_ok"])

    def test_pack_inconnu_ou_aucun(self):
        with self.assertRaisesRegex(assistant.ErreurAssistant, "Pack inconnu : krea"):
            assistant.plan(["images", "krea"])
        with self.assertRaisesRegex(assistant.ErreurAssistant, "Aucun pack"):
            assistant.plan([])


class Telechargement(Base):
    def test_refuse_avant_les_licences_puis_met_en_file(self):
        telechargeur = mock.Mock()
        telechargeur.ajouter.side_effect = lambda fiche, dossier: mock.Mock(id=f"t-{fiche.nom}")
        with self.assertRaisesRegex(assistant.ErreurAssistant, "Accepte d'abord la licence de : Images"):
            assistant.telecharger(["images"], telechargeur)
        telechargeur.ajouter.assert_not_called()
        self.poser(COMMUN)
        assistant.accepter(["images", "llm"])
        r = assistant.telecharger(["images", "llm"], telechargeur)
        self.assertEqual(r["ajoutes"], ["t-image.safetensors", "t-petit.gguf"])  # le VAE déjà là est sauté
        fiche, dossier = telechargeur.ajouter.call_args_list[0].args
        self.assertEqual((fiche.url, fiche.sha256, fiche.taille, dossier), (IMAGE["url"], IMAGE["sha256"], IMAGE["taille"], "Image/modeles"))
        self.assertEqual((fiche.page, fiche.licence, fiche.jeton_requis), ("https://huggingface.co/org/image", "Apache 2.0", False))
        self.assertEqual(telechargeur.ajouter.call_args_list[1].args[1], "llm")

    def test_deja_dans_la_file_toleres_autres_erreurs_non(self):
        assistant.accepter(["images"])
        telechargeur = mock.Mock()
        telechargeur.ajouter.side_effect = ErreurTelechargement("Ce fichier est déjà dans la file")
        self.assertEqual(assistant.telecharger(["images"], telechargeur)["deja_en_file"], ["vae.safetensors", "image.safetensors"])
        telechargeur.ajouter.side_effect = ErreurTelechargement("Espace disque insuffisant")
        with self.assertRaisesRegex(assistant.ErreurAssistant, "vae.safetensors : Espace disque insuffisant"):
            assistant.telecharger(["images"], telechargeur)

    def test_place_insuffisante(self):
        assistant.accepter(["video"])
        self.libre = 5 * GO
        with self.assertRaisesRegex(assistant.ErreurAssistant, "Place insuffisante"):
            assistant.telecharger(["video"], mock.Mock())

    def test_licences_datees_et_aria_yaml_garde_le_reste(self):
        emplacements.retenir_choix("modeles", "D:/IA")
        assistant.accepter(["video"])
        assistant.accepter(["images"])
        memoire = emplacements.lire_choix("assistant")
        self.assertEqual(sorted(memoire["licences"]), ["images", "video"])
        self.assertEqual(emplacements.lire_choix("modeles"), "D:/IA")


class Fin(Base):
    def test_favoris_seulement_ce_qui_est_installe_et_dans_l_ordre(self):
        for f in (COMMUN, IMAGE, VIDEO):
            self.poser(f)
        with mock.patch.object(emplacements, "retenir_favori") as retenir:
            r = assistant.terminer(["video", "images", "llm"])
        self.assertEqual(r["favoris"], {"generation.texte_vers_image": "image", "generation.retouche": "video",
                                        "generation.image_vers_video": "video"})  # pas le LLM : son GGUF manque
        self.assertEqual(retenir.call_count, 3)
        self.assertEqual(self.config["generation"]["retouche"], "video")  # le pack plus bas dans le catalogue l'emporte
        self.assertEqual(emplacements.lire_choix("assistant")["packs"], ["images", "video", "llm"])
        self.assertEqual(r["regles"], [{"usage": "Créer une image", "nom": "Image"}, {"usage": "Retoucher", "nom": "Video"},
                                       {"usage": "Animer une image", "nom": "Video"}])
        self.assertEqual(r["manquants"], [{"usage": "Réécrire tes idées", "nom": "petit.gguf"}])
        self.assertTrue(r["essai"])

    def test_un_pack_plus_bas_pas_installe_garde_le_favori_d_avant(self):
        for f in (COMMUN, IMAGE):
            self.poser(f)
        with mock.patch.object(emplacements, "retenir_favori"):
            r = assistant.terminer(["images", "video"])
        self.assertEqual(r["favoris"]["generation.retouche"], "image")
        self.assertIn({"usage": "Retoucher", "nom": "Video"}, r["manquants"])

    def test_pas_d_essai_sans_image(self):
        with mock.patch.object(emplacements, "retenir_favori"):
            self.assertFalse(assistant.terminer(["llm"])["essai"])
        with self.assertRaisesRegex(assistant.ErreurAssistant, "Aucun modèle d'image"):
            assistant.essayer(mock.Mock())

    def test_s_ouvre_seulement_s_il_manque_de_quoi_generer(self):
        self.assertTrue(assistant.etat()["ouvrir"])  # ComfyUI absent, aucun workflow prêt
        self.etat_moteurs[0]["statut"] = "a_jour"
        self.assertTrue(assistant.etat()["ouvrir"])  # aucun workflow prêt
        for f in (COMMUN, IMAGE):
            self.poser(f)
        self.assertFalse(assistant.etat()["ouvrir"])
        self.etat_moteurs[0]["statut"] = "absent"
        with mock.patch.object(emplacements, "retenir_favori"):
            assistant.terminer(["images"])
        self.assertFalse(assistant.etat()["ouvrir"])  # terminé : plus jamais tout seul

    def test_essai_avec_ou_sans_llm(self):
        for f in (COMMUN, IMAGE):
            self.poser(f)
        self.config["generation"]["texte_vers_image"] = "image"
        self.config["llama"]["modele"] = "petit.gguf"
        plateau = mock.Mock()
        with mock.patch.object(assistant.modeles, "llm_disponibles", lambda: []):
            assistant.essayer(plateau)
        args, options = plateau.commander.call_args
        self.assertEqual((args[1], args[3], options["imposees"]), (assistant.ESSAI["prompt"], "image", {"steps": 10, "megapixels": 0.5}))
        with mock.patch.object(assistant.modeles, "llm_disponibles", lambda: ["petit.gguf"]):
            assistant.essayer(plateau)
        self.assertIsNone(plateau.commander.call_args.args[1])  # le LLM réécrit l'idée : il est essayé aussi


if __name__ == "__main__":
    unittest.main()
