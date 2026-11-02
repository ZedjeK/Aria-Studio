"""Tests des LLM spécialisés (catalogue/llm.yaml, app/prompt.py) : un faux catalogue, un faux Llama, des fichiers
dans un dossier temporaire. Rien n'est chargé sur la carte graphique ni téléchargé."""

import asyncio
import tempfile
import unittest
from contextlib import asynccontextmanager
from pathlib import Path
from unittest import mock

from app import collection, fiches, llm, modeles, prompt
from app.llm import ErreurLlama, Reponse

SPECIALISTE = {
    "workflows": ["qwen_image_2_1"], "usage": "creation",
    "consigne": {"nom": "pe.consigne.txt", "url": "https://huggingface.co/o/d/resolve/main/system_prompt.txt",
                 "taille": 10, "sha256": "a" * 64},
    "reflexion": True, "sortie": "json", "cle": "rewritten_prompt", "contexte": 16384, "max_tokens": 8192,
    "echantillonnage": {"temperature": 1.0, "top_p": 0.95, "top_k": 20, "presence_penalty": 1.5},
}
RETOUCHE = {**SPECIALISTE, "workflows": ["qwen_image_2_1_edit"], "usage": "retouche",
            "consigne": {**SPECIALISTE["consigne"], "nom": "pe-edit.consigne.txt"},
            "vision": {"nom": "pe-edit.mmproj.gguf", "url": "https://huggingface.co/o/e/resolve/main/m.gguf",
                       "taille": 20, "sha256": "b" * 64}}
CATALOGUE = [
    {"id": "general.gguf", "nom": "Généraliste"},
    {"id": "pe.gguf", "nom": "PE création", "page": "https://huggingface.co/o/d", "url": "https://huggingface.co/o/d/resolve/main/pe.gguf",
     "taille": 100, "sha256": "c" * 64, "licence": {"nom": "Qwen Research", "url": "u", "resume": "r"}, "specialiste": SPECIALISTE},
    {"id": "pe-edit.gguf", "nom": "PE retouche", "page": "https://huggingface.co/o/e", "url": "https://huggingface.co/o/e/resolve/main/pe-edit.gguf",
     "taille": 100, "sha256": "d" * 64, "licence": {"nom": "Qwen Research", "url": "u", "resume": "r"}, "specialiste": RETOUCHE},
]


class Validation(unittest.TestCase):
    def test_fiche_complete_acceptee_et_trous_signales(self):
        self.assertEqual(fiches.problemes_specialiste(SPECIALISTE), [])
        self.assertEqual(fiches.problemes_specialiste(RETOUCHE), [])
        mauvais = {**SPECIALISTE, "usage": "video", "sortie": "json", "cle": None, "max_tokens": 20000,
                   "consigne": {"nom": "x"}}
        p = " ".join(fiches.problemes_specialiste(mauvais))
        for attendu in ("usage", "consigne", "sortie", "max_tokens"):
            self.assertIn(attendu, p)

    def test_le_catalogue_livre_est_valide(self):
        ids = [f["id"] for f in fiches.catalogue_llm()]  # lève une erreur si une fiche est invalide
        self.assertIn("Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf", ids)
        self.assertEqual(fiches.specialiste("Qwen-Image-2.1-PE-I2I.Q5_K_M.gguf")["usage"], "retouche")
        self.assertEqual(fiches.a_venir("minimax_h3_t2v")[0]["nom"], "MiniMax H3 Prompt Rewriter (lightx2v)")


class Lecture(unittest.TestCase):
    def test_reflexion_retiree_et_json_lu(self):
        texte = '<think>Je pèse le cadre…{"brouillon": 1}</think>\n{"rewritten_prompt": "A wide photograph of a cat.", "wh_ratio": "3:2"}'
        self.assertEqual(prompt.lire_specialiste(texte, SPECIALISTE), "A wide photograph of a cat.")
        self.assertEqual(prompt.lire_specialiste('{"rewritten_prompt": "Seul"}', SPECIALISTE), "Seul")  # réflexion à part
        self.assertEqual(prompt.lire_specialiste("<think>x</think> Brut ", {**SPECIALISTE, "sortie": "texte"}), "Brut")

    def test_reponse_coupee_ou_vide(self):
        for texte in ('<think>une réflexion trop longue, coupée avant la fin', '{"rewritten_prompt": ""}', '{"autre": "x"}'):
            with self.subTest(texte=texte), self.assertRaisesRegex(ErreurLlama, "illisible"):
                prompt.lire_specialiste(texte, SPECIALISTE)


class FauxLlama:
    def __init__(self, texte):
        self.texte, self.appels = texte, []
        self.modele = None

    async def generer(self, messages, **parametres):
        self.appels.append((messages, parametres))
        return Reponse(self.texte, 10, 20, 100.0, 50.0)


class FauxOrchestrateur:
    def __init__(self, llama):
        self.llama, self.charges = llama, []

    @asynccontextmanager
    async def llm(self, modele=None):
        self.charges.append(modele)
        yield self.llama


class Chemin(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.llm_dir = Path(self.tmp.name) / "llm"
        self.llm_dir.mkdir()
        (self.llm_dir / "pe.consigne.txt").write_text("CONSIGNE OFFICIELLE", encoding="utf-8")
        (self.llm_dir / "pe-edit.consigne.txt").write_text("CONSIGNE RETOUCHE", encoding="utf-8")
        self.patchs = [mock.patch.object(fiches, "catalogue_llm", lambda: CATALOGUE),
                       mock.patch.object(prompt, "MODELES", Path(self.tmp.name)),
                       mock.patch.dict(prompt.CONFIG["llama"], {"modele": "general.gguf"})]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.tmp.cleanup()

    def test_choix_du_specialiste_sinon_generaliste(self):
        self.assertIs(prompt.specialiste_pour("pe.gguf", "creation", "qwen_image_2_1"), SPECIALISTE)
        self.assertIsNone(prompt.specialiste_pour("pe.gguf", "creation", "krea2_turbo"))  # autre modèle
        self.assertIsNone(prompt.specialiste_pour("pe.gguf", "retouche", "qwen_image_2_1"))  # autre usage
        self.assertEqual(prompt.generaliste("pe.gguf"), "general.gguf")
        self.assertEqual(prompt.generaliste("bonsai.gguf"), "bonsai.gguf")

    def test_demande_consigne_officielle_image_et_echantillonnage(self):
        messages, parametres = prompt.demande_specialiste(RETOUCHE, "mets un coucher de soleil", b"PNG")
        self.assertTrue(messages[0]["content"].startswith("CONSIGNE RETOUCHE"))
        self.assertIn(prompt.CLAUSE_MINEURS, messages[0]["content"])  # la clause verrouillée suit toujours
        self.assertTrue(messages[1]["content"][0]["image_url"]["url"].startswith("data:image/png;base64,"))
        self.assertEqual((parametres["temperature"], parametres["top_k"], parametres["max_tokens"]), (1.0, 20, 8192))
        (self.llm_dir / "pe.consigne.txt").unlink()
        with self.assertRaisesRegex(ErreurLlama, "Consigne du LLM spécialisé absente"):
            prompt.demande_specialiste(SPECIALISTE, "x")

    def test_reecriture_par_le_specialiste_ou_repli(self):
        llama = FauxLlama('<think>…</think>{"rewritten_prompt": "A vertical photograph of a ginger cat."}')
        orch = FauxOrchestrateur(llama)
        r = asyncio.run(prompt.reecrire(orch, "un chat roux", "pe.gguf", workflow="qwen_image_2_1"))
        self.assertEqual(r["prompt"], "A vertical photograph of a ginger cat.")
        self.assertEqual(llama.appels[0][0][0]["content"].split("\n")[0], "CONSIGNE OFFICIELLE")
        # Références : le spécialiste n'a pas appris ça, le LLM par défaut prend la main avec la consigne d'Aria.
        llama.texte = "A woman from Picture 1 on a beach."
        asyncio.run(prompt.reecrire(orch, "elle à la plage", "pe.gguf", references=1, workflow="qwen_image_2_1"))
        self.assertEqual(orch.charges[-1], "general.gguf")
        self.assertIn("Picture 1", llama.appels[-1][0][0]["content"])

    def test_generaliste_markdown_nettoye_et_genre_dans_la_consigne(self):
        llama = FauxLlama('A cyberpunk alley, a **"Ramen 24h"** sign above *her* katana.')
        r = asyncio.run(prompt.reecrire(FauxOrchestrateur(llama), "une samouraï androïde", "general.gguf",
                                        workflow="qwen_image_2_1"))
        self.assertEqual(r["prompt"], 'A cyberpunk alley, a "Ramen 24h" sign above her katana.')
        consigne = llama.appels[0][0][0]["content"]
        self.assertIn("« un chevalier »", consigne)  # les deux genres en exemple (banc du 2026-11-02)
        self.assertIn("« une samouraï »", consigne)
        for nom in ("reecriture", "references", "video", "decoupage"):
            self.assertIn("Never change it", prompt.consigne(nom), nom)

    def test_retouche_par_le_specialiste_qui_voit_l_image(self):
        llama = FauxLlama('{"rewritten_prompt": "Change the sky to a warm sunset; keep everything else."}')
        orch = FauxOrchestrateur(llama)
        texte = asyncio.run(prompt.traduire_instruction(orch, "coucher de soleil", "pe-edit.gguf", None,
                                                        "qwen_image_2_1_edit", prompt.Vue(b"PNG")))
        self.assertEqual(texte, "Change the sky to a warm sunset; keep everything else.")
        self.assertEqual(orch.charges, ["pe-edit.gguf"])
        asyncio.run(prompt.traduire_instruction(orch, "coucher de soleil", "pe-edit.gguf", None, "qwen_image_2_1_edit"))
        self.assertEqual(orch.charges[-1], "general.gguf")  # sans image (agrandissement) : le généraliste


class Installation(unittest.TestCase):
    def test_gguf_consigne_et_vision_en_file_sauf_ce_qui_est_la(self):
        with tempfile.TemporaryDirectory() as tmp, \
                mock.patch.object(modeles, "DOSSIER", Path(tmp)), \
                mock.patch.object(modeles, "familles", lambda: []), \
                mock.patch.object(fiches, "catalogue_llm", lambda: CATALOGUE):
            (Path(tmp) / "llm").mkdir()
            (Path(tmp) / "llm" / "pe-edit.consigne.txt").write_text("déjà là")
            telechargeur = mock.Mock()
            telechargeur.ajouter.side_effect = lambda fiche, dossier: mock.Mock(id=fiche.nom)
            r = collection.installer_llm("pe-edit.gguf", telechargeur)
            self.assertEqual(r["ajoutes"], ["pe-edit.gguf", "pe-edit.mmproj.gguf"])
            fiche = telechargeur.ajouter.call_args_list[1].args[0]
            self.assertEqual((fiche.sha256, fiche.taille, fiche.licence), ("b" * 64, 20, "Qwen Research"))
            (Path(tmp) / "llm" / "pe-edit.mmproj.gguf").write_text("x")
            (Path(tmp) / "llm" / "pe-edit.gguf").write_text("x")
            self.assertEqual(modeles.llm_disponibles(), ["pe-edit.gguf"])  # le projecteur n'est pas un LLM
            with self.assertRaisesRegex(collection.ErreurCollection, "inconnu"):
                collection.installer_llm("absent.gguf", telechargeur)


class Lancement(unittest.TestCase):
    def test_mmproj_passe_a_llama_server(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            for nom in ("llama-server.exe", "pe.gguf", "pe.mmproj.gguf"):
                (base / nom).write_bytes(b"x")
            l = llm.Llama(base / "llama-server.exe", 8999, 4096, False)
            processus = mock.Mock()
            processus.poll.return_value = None

            async def pret():
                return True

            with mock.patch.object(llm, "LOGS", base / "logs"), \
                    mock.patch.object(llm.subprocess, "Popen", return_value=processus) as popen, \
                    mock.patch.object(l, "est_pret", pret):
                asyncio.run(l.demarrer(base / "pe.gguf", contexte=16384, reflexion=True, vision=base / "pe.mmproj.gguf"))
            arguments = popen.call_args.args[0]
            self.assertEqual(arguments[arguments.index("--mmproj") + 1], str(base / "pe.mmproj.gguf"))
            self.assertNotIn("--reasoning-budget", arguments)  # réflexion gardée
            l._log.close()


if __name__ == "__main__":
    unittest.main()
