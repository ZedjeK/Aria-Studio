"""Tests du format des fiches de modèles (format 2, app/fiches.py) : fiches livrées, validation, règles du profil.

Les fiches livrées sont lues telles quelles ; les cas invalides vivent dans un dossier temporaire.
"""

import copy
import datetime
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

from app import fiches, optimiseur, prompt, workflows
from app.config import CONFIG


def fiche_valide() -> dict:
    return copy.deepcopy(workflows.obtenir("qwen_image_2_1").fiche)


class FichesLivrees(unittest.TestCase):
    def test_toutes_les_fiches_de_workflow_sont_valides(self):
        self.assertEqual(workflows.erreurs(), [])
        self.assertEqual(len(workflows.catalogue()), len(list(workflows.DOSSIER.glob("*/fiche.yaml"))))

    def test_composants_et_licences(self):
        for wf in workflows.catalogue():
            noms = {f["nom"] for f in wf.fiche["fichiers"]}
            for c in wf.licence.get("composants", []):
                self.assertIn(c["fichier"], noms, wf.id)  # un composant cité est bien un fichier de la fiche
        h3 = workflows.obtenir("minimax_h3_t2v").licence
        self.assertIn("Union européenne", h3["territoires_exclus"])
        self.assertIs(workflows.obtenir("qwen_image_2_1").licence["commercial"], False)
        krea = workflows.obtenir("krea2_turbo").licence  # accès restreint : rien d'affirmé qu'on n'a pas lu
        self.assertIsNone(krea["commercial"])
        self.assertIsNone(krea["territoires_exclus"])

    def test_aucun_minimum_sans_mesure(self):
        for wf in workflows.catalogue():
            self.assertIsNone(wf.materiel["vram_min_go"], wf.id)  # le banc de palier n'a pas encore tourné
            self.assertIsNone(wf.materiel["mesure"], wf.id)

    def test_catalogue_llm_coherent_avec_la_config(self):
        llm = {f["id"]: f for f in fiches.catalogue_llm()}
        for m in CONFIG["llama"]["modeles_au_choix"]:
            self.assertIn(m["fichier"], llm)
        for fichier in CONFIG["llama"].get("serveurs_specifiques", {}):
            self.assertEqual(llm[fichier]["serveur"], "llama-prism")
        self.assertEqual(llm[CONFIG["llama"]["modele"]]["serveur"], "llama")
        self.assertEqual(fiches.fiche_llm("Ternary-Bonsai-2-27B-PTQ1_0.gguf")["nom"], "Bonsai 2 27B")
        self.assertIsNone(fiches.fiche_llm("inconnu.gguf"))

    def test_resume_pour_l_interface(self):
        r = workflows.obtenir("minimax_h3_i2v").resume()
        self.assertEqual(r["licence"], "MiniMax H3 Community License Agreement")
        self.assertEqual(r["licence_fiche"]["verifiee"], "2026-10-26")
        self.assertEqual((r["vram_estimee_go"], r["vram_min_go"], r["style_prompt"]), (19.5, None, "brief_video"))
        self.assertGreater(r["taille"], 39 * 2**30)


class Validation(unittest.TestCase):
    def test_ancien_format_refuse(self):
        f = fiche_valide()
        del f["format"]
        self.assertEqual(fiches.problemes_workflow(f), ["format : 2 attendu (fiche d'un ancien format)"])

    def test_licence(self):
        f = fiche_valide()
        f["licence"]["commercial"] = "oui"
        del f["licence"]["url"]
        f["licence"]["verifiee"] = "hier"
        p = fiches.problemes_workflow(f)
        self.assertIn("licence.url manquant", p)
        self.assertIn("licence.commercial : true, false ou null (à lire)", p)
        self.assertIn("licence.verifiee : la date de lecture de la licence (AAAA-MM-JJ)", p)
        f = fiche_valide()
        f["licence"].update(commercial=None, territoires_exclus=None)  # « à lire » : permis
        self.assertEqual(fiches.problemes_workflow(f), [])
        f["licence"]["composants"] = [{"fichier": "x.safetensors"}]
        self.assertTrue(any("composants" in x for x in fiches.problemes_workflow(f)))

    def test_materiel(self):
        f = fiche_valide()
        f["materiel"]["vram_min_go"] = 8  # un minimum sans dire comment il a été mesuré
        self.assertIn("materiel : une VRAM minimale ou de confort doit dire comment elle a été mesurée (mesure)",
                      fiches.problemes_workflow(f))
        f["materiel"]["mesure"] = {"carte": "RTX 4080 16 Go", "date": datetime.date(2026, 10, 27),
                                   "methode": "banc de palier, --reserve-vram 8"}
        self.assertEqual(fiches.problemes_workflow(f), [])
        f["materiel"]["vram_estimee_go"] = 0
        self.assertIn("materiel.vram_estimee_go : un nombre de Go", fiches.problemes_workflow(f))

    def test_profil(self):
        f = fiche_valide()
        f["profil_prompt"].update(style="mots-clés", mots_max=5, regles="une règle")
        p = fiches.problemes_workflow(f)
        self.assertIn("profil_prompt.style : prose, instruction, brief_video", p)
        self.assertIn("profil_prompt.mots_max : un entier de 20 à 1000", p)
        self.assertIn("profil_prompt.regles : une liste", p)

    def test_fiche_llm(self):
        f = copy.deepcopy(fiches.catalogue_llm()[0])
        f.update(id="modele.bin", serveur="ollama", taille=0)
        p = fiches.problemes_llm(f)
        self.assertIn("id : le nom du fichier GGUF", p)
        self.assertIn("serveur : llama, llama-prism", p)
        self.assertIn("taille : en octets", p)


class FicheEcartee(unittest.TestCase):
    """Une fiche ajoutée à la main mais incomplète n'empêche pas Aria de démarrer : elle est écartée, et dite."""

    def test_catalogue_et_erreurs(self):
        with tempfile.TemporaryDirectory() as d:
            racine = Path(d)
            shutil.copytree(workflows.DOSSIER / "qwen_image_2_1", racine / "qwen_image_2_1")
            shutil.copytree(workflows.DOSSIER / "krea2_turbo", racine / "maison")
            fiche = yaml.safe_load((racine / "maison" / "fiche.yaml").read_text(encoding="utf-8"))
            fiche.update(id="maison", format=1)
            (racine / "maison" / "fiche.yaml").write_text(yaml.safe_dump(fiche, allow_unicode=True), encoding="utf-8")
            with mock.patch.object(workflows, "DOSSIER", racine):
                self.assertEqual([wf.id for wf in workflows.catalogue()], ["qwen_image_2_1"])
                self.assertEqual(workflows.erreurs(), ["Fiche maison : format : 2 attendu (fiche d'un ancien format)"])
                with self.assertRaisesRegex(workflows.ErreurWorkflow, "Workflow inconnu"):
                    workflows.obtenir("maison")


class ReglesDuProfil(unittest.TestCase):
    def test_ajoutees_a_la_consigne_avant_la_clause(self):
        texte = prompt.consigne("reecriture", None, 120, ["Use tags, comma separated.", "No camera terms."])
        regles = texte.index("Rules of the target model:\n- Use tags, comma separated.\n- No camera terms.")
        self.assertLess(regles, texte.index(prompt.CLAUSE_MINEURS))  # la clause verrouillée reste la dernière
        self.assertNotIn("Rules of the target model", prompt.consigne("reecriture", None, 120))
        self.assertIn("No camera terms.", prompt.messages("une pomme", None, ["No camera terms."])[0]["content"])

    def test_lues_dans_la_fiche_du_modele_cible(self):
        self.assertEqual(workflows.regles_prompt("qwen_image_2_1"), [])  # fiches livrées : aucune règle en plus
        self.assertEqual(workflows.regles_prompt(None), [])
        self.assertEqual(workflows.regles_prompt("inconnu"), [])
        wf = workflows.obtenir("qwen_image_2_1")
        wf.fiche["profil_prompt"]["regles"] = ["Short sentences."]
        with mock.patch.object(workflows, "obtenir", return_value=wf):
            self.assertEqual(workflows.regles_prompt("qwen_image_2_1"), ["Short sentences."])


class Optimiseur(unittest.TestCase):
    def test_recommandations_llm_du_catalogue(self):
        """Un nouvel utilisateur n'a pas modeles/llm/recommandations.yaml (non versionné) : le catalogue suffit."""
        with mock.patch.object(optimiseur, "FICHIER_LLM", Path(tempfile.gettempdir()) / "absent_aria.yaml"):
            recos = optimiseur._recommandations_llm()
        self.assertEqual(recos["Ministral-3-14B-Instruct-2512-Q4_K_M.gguf"]["valeurs"][0]["cle"], "temperature_peaufinage")
        self.assertIn("Ternary-Bonsai-2-27B-PTQ1_0.gguf", recos)


if __name__ == "__main__":
    unittest.main()
