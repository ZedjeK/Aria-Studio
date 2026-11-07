"""Tests de l'onglet Modèles : familles (bases Civitai et HF des fiches), Rechercher (app/decouvrir.py), Ma collection,
empreintes et « Retrouver les sources », favoris des onglets d'usage (app/collection.py).

Rien ne part sur le réseau : Civitai est simulé (httpx.MockTransport), les fichiers vivent dans un dossier temporaire.
"""

import asyncio
import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import httpx

from app import collection, config, decouvrir, fiches, modeles, workflows
from app.sources import civitai

FAMILLES = {
    "Qwen-Image2.1": {"nature": "image", "bases": ["Qwen 2.1"], "nom_contient": None, "hf": ["Qwen/Qwen-Image-2.1"], "workflows": []},
    "Qwen-Edit2511": {"nature": "image", "bases": ["Qwen"], "nom_contient": "edit", "hf": ["Qwen/Qwen-Image-Edit-2511"], "workflows": []},
    "MiniMax-H3": {"nature": "video", "bases": ["MiniMax H3"], "nom_contient": None, "hf": ["MiniMaxAI/MiniMax-H3"], "workflows": []},
}


class Familles(unittest.TestCase):
    def test_base_civitai_vers_famille(self):
        f = lambda base, nom="": workflows.famille_civitai(base, nom, FAMILLES)  # noqa: E731
        self.assertEqual(f("Qwen 2.1"), "Qwen-Image2.1")
        self.assertEqual(f("Qwen", "Consistence Edit Lora"), "Qwen-Edit2511")
        self.assertIsNone(f("Qwen", "Jib Mix Qwen"))  # « Qwen » sans « edit » : génération v1, pas pour Aria
        self.assertEqual(f("MiniMax H3", "DaSiWa"), "MiniMax-H3")
        self.assertIsNone(f("SDXL 1.0"))
        self.assertIsNone(f(None))

    def test_dossier_selon_le_type_civitai(self):
        self.assertEqual(civitai._dossier_famille("LORA", "MiniMax H3", "x", FAMILLES), "MiniMax-H3/loras")
        self.assertEqual(civitai._dossier_famille("Checkpoint", "Qwen 2.1", "x", FAMILLES), "Qwen-Image2.1/modeles")
        self.assertIsNone(civitai._dossier_famille("Workflows", "Qwen 2.1", "x", FAMILLES))

    def test_fiches_livrees_et_validation(self):
        self.assertEqual(workflows.erreurs(), [])
        reelles = workflows.familles()
        self.assertEqual(reelles["MiniMax-H3"]["nature"], "video")
        self.assertEqual(reelles["Qwen-Image2.1"]["nature"], "image")  # génération et retouche : image
        self.assertTrue(all(f["bases"] and f["hf"] for f in reelles.values()))
        self.assertEqual(fiches.problemes_civitai({"bases": []}), ["civitai.bases : la liste des « baseModel » de Civitai de cette famille"])
        self.assertTrue(fiches.problemes_civitai({"bases": ["Qwen"], "nom_contient": ""}))
        self.assertTrue(fiches.problemes_hugging_face({"bases": ["sans-organisation"]}))
        self.assertEqual(fiches.problemes_hugging_face(None), [])


class RechercherHF(unittest.TestCase):
    def test_dossier_d_un_fichier_hf(self):
        d = decouvrir._dossier_hf
        self.assertEqual(d("vae/qwen_image_vae.safetensors", "image", "Krea2"), "_base/vae")
        self.assertEqual(d("split_files/text_encoders/x.safetensors", "image", None), "_base/text_encoders")
        self.assertEqual(d("loras/style.safetensors", "image", "Krea2"), "Krea2/loras")
        self.assertEqual(d("diffusion_models/krea.safetensors", "image", "Krea2"), "Krea2/modeles")
        self.assertEqual(d("model.safetensors", "lora", "MiniMax-H3"), "MiniMax-H3/loras")
        self.assertIsNone(d("model.safetensors", "image", None))  # la famille est à choisir
        self.assertEqual(d("Q4_K_M.gguf", "llm", None), "llm")

    def test_modeles_ou_lora_selon_les_tags(self):
        lora = {"tags": ["lora", "base_model:adapter:Qwen/Qwen-Image-2.1"]}
        gguf = {"tags": ["gguf", "base_model:quantized:Qwen/Qwen-Image-2.1"]}
        finetune = {"tags": ["base_model:finetune:Qwen/Qwen-Image-2.1"]}
        self.assertEqual([decouvrir._utilisable_hf(m, "image") for m in (lora, gguf, finetune)], [False, False, True])
        self.assertTrue(decouvrir._utilisable_hf(lora, "lora"))

    def test_requetes_par_famille(self):
        with mock.patch.object(decouvrir.workflows, "familles", lambda: FAMILLES):
            self.assertEqual(decouvrir._requetes_hf("video", None), [({"filter": "base_model:MiniMaxAI/MiniMax-H3"}, "MiniMax-H3")])
            self.assertEqual(len(decouvrir._requetes_hf("lora", None)), 3)  # les LoRA de toutes les familles
            self.assertEqual(decouvrir._requetes_hf("lora", "Qwen-Edit2511"),
                             [({"filter": "base_model:adapter:Qwen/Qwen-Image-Edit-2511"}, "Qwen-Edit2511")])
            with self.assertRaises(decouvrir.ErreurDecouvrir):
                decouvrir._requetes_hf("image", "Inconnue")


class RechercherCivitai(unittest.TestCase):
    """La réponse de Civitai est simulée : un checkpoint H3 compatible, un « Qwen » sans « edit » (écarté)."""

    REPONSE = {"items": [
        {"id": 1, "name": "DaSiWa MiniMax H3", "type": "Checkpoint", "creator": {"username": "a"}, "stats": {},
         "modelVersions": [{"id": 11, "name": "v4", "baseModel": "MiniMax H3", "trainedWords": [], "images": [], "files": [
             {"name": "dasiwa.safetensors", "sizeKB": 1024, "primary": True, "hashes": {"SHA256": "AA" * 32}},
             {"name": "encodeur.safetensors", "sizeKB": 1024, "primary": False, "hashes": {"SHA256": "BB" * 32}},
             {"name": "vieux.ckpt", "sizeKB": 1024, "primary": False, "hashes": {}}]}]},
        {"id": 2, "name": "Jib Mix Qwen", "type": "Checkpoint", "stats": {},
         "modelVersions": [{"id": 21, "baseModel": "Qwen", "files": [{"name": "jib.safetensors", "sizeKB": 1, "primary": True}]}]},
    ], "metadata": {}}

    def chercher(self, installes, adulte=False):
        vus = []

        def repondre(requete):
            vus.append(requete.url)
            return httpx.Response(200, json=self.REPONSE)

        vrai = httpx.AsyncClient  # capturé avant le remplacement (decouvrir.httpx est le même module)
        client = lambda **kw: vrai(transport=httpx.MockTransport(repondre), **kw)  # noqa: E731
        with mock.patch.object(decouvrir.httpx, "AsyncClient", client), \
                mock.patch.object(decouvrir.workflows, "familles", lambda: FAMILLES), \
                mock.patch.object(decouvrir.modeles, "empreintes_installees", lambda: installes):
            return asyncio.run(decouvrir.chercher_civitai("", "video", None, "telechargements", None, adulte)), vus[0]

    def test_compatibles_seulement_et_dossier_deduit(self):
        r, url = self.chercher(set())
        self.assertEqual(url.params.get_list("baseModels"), ["MiniMax H3"])  # les bases des familles vidéo
        self.assertEqual([x["nom"] for x in r["resultats"]], ["DaSiWa MiniMax H3"])
        version = r["resultats"][0]["versions"][0]
        self.assertEqual((version["dossier"], version["famille"]), ("MiniMax-H3/modeles", "MiniMax-H3"))
        self.assertEqual([f["nom"] for f in version["fichiers"]], ["dasiwa.safetensors", "encodeur.safetensors"])

    def test_installe_d_apres_le_fichier_principal(self):
        self.assertFalse(self.chercher({"bb" * 32})[0]["resultats"][0]["installe"])  # l'encodeur qu'on a déjà ne compte pas
        self.assertTrue(self.chercher({"aa" * 32})[0]["resultats"][0]["installe"])

    def test_modeles_adultes_seulement_si_demandes(self):
        """L'API de Civitai cache presque tous les modèles NSFW si on ne les demande pas."""
        self.assertNotIn("nsfw", self.chercher(set())[1].params)
        self.assertEqual(self.chercher(set(), adulte=True)[1].params["nsfw"], "true")

    def test_liens_civitai_red_acceptes(self):
        from app.sources import Fiche, fiche_depuis_url

        async def fiche(url):
            return Fiche(source="civitai", nom="m.safetensors", taille=1, sha256=None, url=url, page=url,
                         dossier=None, licence=None, jeton_requis=False)

        with mock.patch.object(civitai, "fiche", fiche):
            for lien in ("https://civitai.red/models/2877206", "https://civitai.com/models/2877206"):
                self.assertEqual(asyncio.run(fiche_depuis_url(lien)).nom, "m.safetensors")

    def test_pages_adultes_sur_civitai_red(self):
        """civitai.com ne montre plus que le tout public ; le drapeau « nsfw » ne suffit pas (false pour Muse, niveau 31)."""
        self.assertEqual(civitai.page(2741166, niveau=31), "https://civitai.red/models/2741166")
        self.assertEqual(civitai.page(2741166, 3189679, 26), "https://civitai.red/models/2741166?modelVersionId=3189679")
        self.assertEqual(civitai.page(9, niveau=3), "https://civitai.com/models/9")  # PG et PG-13 seulement
        self.assertEqual(civitai.page(9, niveau=None), "https://civitai.com/models/9")
        self.assertEqual(self.chercher(set())[0]["resultats"][0]["page"], "https://civitai.com/models/1")
        self.REPONSE = {**self.REPONSE, "items": [{**self.REPONSE["items"][0], "nsfwLevel": 31}]}
        self.assertEqual(self.chercher(set())[0]["resultats"][0]["page"], "https://civitai.red/models/1")

    def test_pas_de_llm_sur_civitai(self):
        with self.assertRaises(decouvrir.ErreurDecouvrir):
            asyncio.run(decouvrir.chercher_civitai("", "llm", None, "telechargements", None))


class Base(unittest.TestCase):
    """Un dossier des modèles, un registre et un cache d'empreintes temporaires."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.patchs = [mock.patch.object(modeles, "DOSSIER", base / "modeles"),
                       mock.patch.object(modeles, "REGISTRE", base / "modeles.json"),
                       mock.patch.object(modeles, "EMPREINTES", base / "empreintes.json"),
                       mock.patch.object(modeles, "familles", lambda: ["Krea2", "MiniMax-H3"])]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.tmp.cleanup()

    def poser(self, dossier, nom, contenu=b"x"):
        chemin = modeles.DOSSIER / dossier / nom
        chemin.parent.mkdir(parents=True, exist_ok=True)
        chemin.write_bytes(contenu)
        return chemin


class Empreintes(Base):
    def test_calculee_une_fois_puis_recalculee_si_le_fichier_change(self):
        f = self.poser("Krea2/modeles", "moody.safetensors", b"abc")
        attendu = hashlib.sha256(b"abc").hexdigest()
        self.assertEqual(modeles.empreinte(f), attendu)
        with mock.patch.object(modeles.hashlib, "sha256", side_effect=AssertionError("recalcul")):
            self.assertEqual(modeles.empreinte(f), attendu)  # le cache sert
        f.write_bytes(b"abcd")
        self.assertEqual(modeles.empreinte(f), hashlib.sha256(b"abcd").hexdigest())
        self.assertIn(hashlib.sha256(b"abcd").hexdigest(), modeles.empreintes_installees())

    def test_doublon_refuse_meme_sous_un_autre_nom(self):
        """Civitai nomme un même fichier « …_int8 » ou « …_3281858 » selon son API : l'empreinte tranche."""
        from app.sources import Fiche
        from app.telechargement import ErreurTelechargement, Telechargeur
        f = self.poser("MiniMax-H3/modeles", "dasiwa_int8.safetensors", b"dasiwa")
        sha = modeles.empreinte(f)
        self.assertEqual(modeles.cle_par_empreinte(sha.upper()), "MiniMax-H3/modeles/dasiwa_int8.safetensors")
        fiche = Fiche(source="civitai", nom="dasiwa_3281858.safetensors", taille=6, sha256=sha,
                      url="https://civitai.com/api/download/models/1", page="p", dossier="MiniMax-H3/modeles",
                      licence=None, jeton_requis=False)
        with self.assertRaisesRegex(ErreurTelechargement, "Déjà installé sous un autre nom : MiniMax-H3/modeles/dasiwa_int8"):
            Telechargeur().ajouter(fiche, "MiniMax-H3/modeles")
        fiche.sha256, fiche.jeton_requis = "0" * 64, True
        with self.assertRaisesRegex(ErreurTelechargement, "Clés d.accès"):  # dit quoi faire, sans parler de licence
            Telechargeur().ajouter(fiche, "MiniMax-H3/modeles")
        f.unlink()
        self.assertIsNone(modeles.cle_par_empreinte(sha))  # fichier supprimé depuis : plus un doublon

    def test_l_inventaire_ignore_ce_qui_n_est_pas_un_modele(self):
        self.poser("llm", "petit.gguf")
        self.poser("llm", "recommandations.yaml")
        self.poser("Krea2/modeles", "x.safetensors.part")
        self.assertEqual([m["cle"] for m in modeles.inventaire()], ["llm/petit.gguf"])


class Sources(Base):
    def identifier(self, connus, civitai_repond):
        async def par_empreinte(sha):
            return civitai_repond.get(sha)

        async def lancer():
            collection.lancer_identification()
            await collection._tache_identification

        with mock.patch.object(collection, "_connus_du_catalogue", lambda: connus), \
                mock.patch.object(collection.civitai, "par_empreinte", par_empreinte):
            asyncio.run(lancer())
        return collection.identification()

    def test_catalogue_puis_civitai_puis_introuvable(self):
        aria = self.poser("Krea2/modeles", "krea.safetensors", b"aria")
        moody = self.poser("Krea2/modeles", "moody.safetensors", b"moody")
        self.poser("MiniMax-H3/modeles", "inconnu.safetensors", b"?")
        sha = lambda c: hashlib.sha256(c).hexdigest()  # noqa: E731
        connus = {sha(b"aria"): ({"url": "https://huggingface.co/Comfy-Org/Krea-2/resolve/main/k.safetensors"}, {"nom": "Krea 2"})}
        trouve = {"source": "civitai", "page": "https://civitai.com/models/9?modelVersionId=90", "url": "https://civitai.com/api/download/models/90",
                  "licence": None, "mots_declencheurs": ["moody"], "base": "Krea 2", "nom_modele": "Moody Krea 2 Mix",
                  "version": "V7", "type": "Checkpoint"}
        s = self.identifier(connus, {sha(b"moody"): trouve})
        self.assertEqual((s["etat"], s["total"], s["introuvables"], s["erreurs"]), ("termine", 3, ["inconnu.safetensors"], []))
        registre = modeles.charger_registre()
        self.assertEqual(registre[modeles.cle(aria)]["page"], "https://huggingface.co/Comfy-Org/Krea-2")
        entree = registre[modeles.cle(moody)]
        self.assertEqual((entree["retrouve"], entree["nom_modele"], entree["mots_declencheurs"]), (True, "Moody Krea 2 Mix", ["moody"]))
        self.assertEqual(entree["sha256"], sha(b"moody"))

    def test_une_seule_recherche_a_la_fois(self):
        collection._identification.update(etat="en_cours")
        try:
            with self.assertRaisesRegex(collection.ErreurCollection, "déjà en cours"):
                collection.lancer_identification()
        finally:
            collection._identification.update(etat="repos")


class LlmParWorkflow(unittest.TestCase):
    """Le LLM d'un workflow : retenu dans un aria.yaml temporaire, repris par le Plateau quand rien n'est demandé."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.installes = ["defaut.gguf", "bonsai.gguf"]
        self.patchs = [mock.patch.object(workflows.emplacements, "_fichier_choix", lambda: Path(self.tmp.name) / "aria.yaml"),
                       mock.patch.object(workflows.modeles, "llm_disponibles", lambda: self.installes),
                       mock.patch.dict(config.CONFIG["llama"], {"modele": "defaut.gguf"})]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.tmp.cleanup()

    def test_choisi_sinon_par_defaut(self):
        self.assertEqual(workflows.llm_de("minimax_h3_i2v"), "defaut.gguf")
        workflows.choisir_llm("minimax_h3_i2v", "bonsai.gguf")
        self.assertEqual((workflows.llm_de("minimax_h3_i2v"), workflows.llm_de("qwen_image_2_1")), ("bonsai.gguf", "defaut.gguf"))
        self.assertEqual(workflows.llm_de(None), "defaut.gguf")
        self.installes.remove("bonsai.gguf")  # le GGUF a disparu : retour au LLM par défaut
        self.assertEqual(workflows.llm_de("minimax_h3_i2v"), "defaut.gguf")

    def test_retour_au_defaut_et_refus(self):
        workflows.choisir_llm("minimax_h3_i2v", "bonsai.gguf")
        workflows.choisir_llm("minimax_h3_i2v", None)
        self.assertEqual(workflows.llms_choisis(), {})
        with self.assertRaisesRegex(workflows.ErreurWorkflow, "GGUF absent"):
            workflows.choisir_llm("minimax_h3_i2v", "absent.gguf")
        with self.assertRaises(workflows.ErreurWorkflow):
            workflows.choisir_llm("inconnu", "bonsai.gguf")

    def test_le_plateau_prend_le_llm_du_workflow(self):
        from app.plateau import Plateau
        workflows.choisir_llm("minimax_h3_i2v", "bonsai.gguf")
        self.assertEqual(Plateau._valider_llm(None, "minimax_h3_i2v"), "bonsai.gguf")
        self.assertEqual(Plateau._valider_llm("defaut.gguf", "minimax_h3_i2v"), "defaut.gguf")  # choix d'une création
        self.assertEqual(Plateau._valider_llm(None), "defaut.gguf")


class FauxWorkflow:
    def __init__(self, id_, tache, references=0, manquants=()):
        self.id, self._references, self._manquants = id_, references, list(manquants)
        self.fiche = {"nom": id_.capitalize(), "tache": tache}

    def accepte_references(self):
        return self._references

    def manquants(self):
        return self._manquants


class Favoris(unittest.TestCase):
    def setUp(self):
        self.wfs = {"edit": FauxWorkflow("edit", "retouche", references=3), "t2i": FauxWorkflow("t2i", "texte_vers_image"),
                    "incomplet": FauxWorkflow("incomplet", "retouche", manquants=[{"nom": "x"}])}
        self.config = {"generation": {}}

        def obtenir(i):
            if i not in self.wfs:
                raise workflows.ErreurWorkflow(f"Workflow inconnu : {i}")
            return self.wfs[i]

        self.patchs = [mock.patch.object(collection.workflows, "obtenir", obtenir),
                       mock.patch.object(collection, "CONFIG", self.config),
                       mock.patch.object(collection, "usages", lambda: {}),
                       mock.patch.object(collection.emplacements, "retenir_favori")]
        self.retenir = [p.start() for p in self.patchs][-1]

    def tearDown(self):
        for p in self.patchs:
            p.stop()

    def test_favori_retenu_dans_aria_yaml(self):
        collection.definir_favori("references", "edit")
        self.retenir.assert_called_once_with("generation", "references", "edit")
        self.assertEqual(self.config["generation"]["references"], "edit")

    def test_refus(self):
        for cle, wf, motif in (("inconnu", "edit", "Usage inconnu"), ("retouche", "t2i", "ne sert pas"),
                               ("references", "t2i", "ne sert pas"), ("retouche", "incomplet", "pas complet"),
                               ("retouche", "absent", "inconnu")):
            with self.subTest(cle=cle, wf=wf), self.assertRaisesRegex(collection.ErreurCollection, motif):
                collection.definir_favori(cle, wf)
        self.retenir.assert_not_called()


if __name__ == "__main__":
    unittest.main()
