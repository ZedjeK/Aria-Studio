"""Tests de l'⚡ Optimiser (app/optimiseur.py) : propositions selon le matériel et les recommandations sourcées.

Le matériel est simulé (VRAM, nom du GPU) et les réglages vont dans un fichier temporaire.
"""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

from app import modeles, optimiseur, reglages, workflows
from tests import steps_connus
from tests import reglages_jetables

GO = 2**30
LLM = "Autre-LLM-14B-Q4_K_M.gguf"  # un GGUF quelconque, hors catalogue
BONSAI = "Ternary-Bonsai-2-27B-PTQ1_0.gguf"



setUpModule, tearDownModule = steps_connus.module()  # ces tests ne portent pas sur la validation des steps

class Optimiseur(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [reglages_jetables.patch(d),
                       mock.patch.object(reglages, "FICHIER_SAMPLERS", d / "samplers.json"),
                       mock.patch.object(reglages, "_cache", None),
                       mock.patch.object(modeles, "llm_disponibles", return_value=[LLM, BONSAI]),
                       # Les LLM simulés n'ont pas à être sur le disque.
                       mock.patch.object(optimiseur.os.path, "getsize", return_value=6 * GO),
                       mock.patch.object(optimiseur.mesures, "nom_gpu", return_value="GPU de test")]
        for p in self.patchs:
            p.start()
        self.carte(16)

    def carte(self, vram_go: float, occupee_go: float = 1.0):
        for nom, valeur in (("vram_totale", vram_go * GO), ("vram_utilisee", occupee_go * GO)):
            p = mock.patch.object(optimiseur.mesures, nom, return_value=int(valeur))
            p.start()
            self.patchs.append(p)

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        self.dossier.cleanup()

    def propositions(self) -> dict:
        r = optimiseur.propositions()
        return {(p["cible"], p["cle"]): p for p in r["propositions"]}, r

    def test_recommandations_sourcees(self):
        props, r = self.propositions()
        self.assertNotIn((BONSAI, "temperature_reecriture"), props)  # déjà à la valeur de sa fiche (0,7)
        self.assertNotIn((LLM, "temperature_traduction"), props)  # hors catalogue : aucune recommandation
        self.assertEqual(props[(BONSAI, "presence_penalty")]["propose"], 1.5)
        # Un jeu par mode (fiche de Bonsai 2) : celui de la réflexion est proposé à part, pour ses propres réglages.
        self.assertEqual(props[(BONSAI, "temperature_reflexion")]["propose"], 1.0)
        self.assertEqual(props[(BONSAI, "top_k_reflexion")]["propose"], 20)
        self.assertIn("réflexion", props[(BONSAI, "temperature_reflexion")]["raison"])
        self.assertNotIn((BONSAI, "presence_penalty_reflexion"), props)  # 0 : déjà la valeur d'origine
        self.assertNotIn((BONSAI, "min_p_reflexion"), props)  # 0,05 : le défaut de llama.cpp
        self.assertEqual(r["materiel"]["gpu"], "GPU de test")

    def test_adapter_ne_concerne_que_les_llm(self):
        """« Adapter à ma machine » ne touche plus aux modèles d'image ni de vidéo : ils ont leur « Recommander »."""
        _, r = self.propositions()
        self.assertEqual({p["type"] for p in r["propositions"]}, {"llm"})
        self.assertFalse(any("Krea 2" in a or "MiniMax" in a or "Qwen" in a for a in r["alertes"]), r["alertes"])
        self.assertFalse(set(r["sans_recommandation"]) & {wf.fiche["nom"] for wf in workflows.catalogue()})

    def test_appliquer_vide_les_propositions(self):
        props, _ = self.propositions()
        for (cible, cle), p in props.items():
            reglages.modifier_llm(cible, {cle: p["propose"]})
        props, _ = self.propositions()
        self.assertEqual(props, {})

    def test_recommander_un_modele_donne_les_valeurs_de_sa_fiche(self):
        reglages.modifier_workflow("krea2_turbo", {"cfg": 4.0})
        r = optimiseur.recommandations_workflow("krea2_turbo")
        props = {p["cle"]: p for p in r["propositions"]}
        self.assertEqual((props["cfg"]["actuel"], props["cfg"]["propose"]), (4.0, 1.0))
        self.assertTrue(props["cfg"]["source"].startswith("https://"))
        self.assertTrue(props["cfg"]["raison"])
        self.assertNotIn("steps", props)  # les steps se valident par checkpoint (Modèles → Ma collection)
        self.assertNotIn("megapixels", props)  # la résolution dépend de la carte : une note, jamais une valeur imposée
        self.assertFalse(r["sans_recommandation"])
        reglages.modifier_workflow("krea2_turbo", {"cfg": 1.0})  # déjà en place : la valeur est quand même donnée
        props = {p["cle"]: p for p in optimiseur.recommandations_workflow("krea2_turbo")["propositions"]}
        self.assertEqual((props["cfg"]["actuel"], props["cfg"]["propose"]), (1.0, 1.0))

    def test_recommander_chaque_modele_livre(self):
        for wf in workflows.catalogue():
            r = optimiseur.recommandations_workflow(wf.id)
            for p in r["propositions"]:
                reglages.modifier_workflow(wf.id, {p["cle"]: p["propose"]})
            for p in optimiseur.recommandations_workflow(wf.id)["propositions"]:
                self.assertEqual(p["actuel"], p["propose"], (wf.id, p["cle"]))

    def test_modele_inconnu(self):
        with self.assertRaises(workflows.ErreurWorkflow):
            optimiseur.recommandations_workflow("n_existe_pas")

    def test_alertes_vram_sur_16_go(self):
        h3 = optimiseur.recommandations_workflow("minimax_h3_i2v")
        self.assertTrue(any("MiniMax H3 · image vers vidéo" in a and "RAM" in a for a in h3["alertes"]), h3["alertes"])
        krea = optimiseur.recommandations_workflow("krea2_turbo")
        self.assertTrue(any("Krea 2" in a and "1 MP" in a for a in krea["alertes"]), krea["alertes"])

    def test_vram_du_projecteur_comptee(self):
        """La vision est chargée d'office avec le LLM : son projecteur (0,6 Go pour Bonsai 2) occupe la carte aussi."""
        self.carte(12)  # 11 Go libres : 6 Go + 1,5 tiennent, 6 + 6 de projecteur simulé non
        _, r = self.propositions()
        self.assertFalse(any(a.startswith(BONSAI.removesuffix(".gguf")) for a in r["alertes"]))
        mmproj = Path(self.dossier.name) / "Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf"
        mmproj.write_bytes(b"GGUF")
        lancement = {"serveur": Path("llama-server.exe"), "contexte": 8192, "reflexion": True, "vision": mmproj}
        with mock.patch.object(optimiseur, "lancement_llm", lambda f: dict(lancement, vision=mmproj if f == BONSAI else None)):
            _, r = self.propositions()
        self.assertTrue(any("12.0 Go, vision comprise" in a for a in r["alertes"]), r["alertes"])

    def lancement(self, contexte, reflexion=True, vision=None):
        return {"serveur": Path("llama-server.exe"), "contexte": contexte, "reflexion": reflexion, "vision": vision}

    def test_surcout_llm_suit_le_contexte(self):
        self.assertEqual(optimiseur.surcout_llm_go(6, 4096), optimiseur.SURCOUT_LLM_GO)  # l'ancien calcul, inchangé
        self.assertAlmostEqual(optimiseur.surcout_llm_go(8.5, 16384), 1.5 + 12 * 8.5 * 0.02)  # ≈ 3,5 Go : 2 Go de cache
        self.assertGreater(optimiseur.surcout_llm_go(14, 16384), optimiseur.surcout_llm_go(6, 16384))
        self.assertEqual(optimiseur.surcout_llm_go(6, 1024), optimiseur.SURCOUT_LLM_GO)  # pas de remise sous la base

    def test_contexte_propose_quand_la_reflexion_le_releve(self):
        """Contexte réglé à 4096 avec la Réflexion : Aria lance déjà à 16 384 ; l'Optimiser propose d'aligner le réglage."""
        reglages.modifier_llm(LLM, {"contexte": 4096, "reflexion": True})
        props, _ = self.propositions()
        p = props[(LLM, "contexte")]
        self.assertEqual((p["actuel"], p["propose"]), (4096, 16384))
        self.assertIn("Réflexion", p["raison"])
        reglages.modifier_llm(LLM, {"contexte": 16384})  # comme Jérémy l'a réglé à la main : rien à proposer
        props, _ = self.propositions()
        self.assertNotIn((LLM, "contexte"), props)

    def test_pas_de_proposition_sans_reflexion_ni_vision(self):
        reglages.modifier_llm(LLM, {"contexte": 4096, "reflexion": False})
        with mock.patch.object(optimiseur, "lancement_llm", lambda f: self.lancement(4096, reflexion=False)):
            props, _ = self.propositions()
        self.assertNotIn((LLM, "contexte"), props)

    def test_la_vram_compte_le_contexte_releve_et_le_dit(self):
        reglages.modifier_llm(LLM, {"contexte": 4096, "reflexion": True})
        self.carte(9)  # 8 Go libres : 6 + 1,5 (contexte 4096) tiennent ; 6 + 1,5 + 1,44 (16 384) non
        with mock.patch.object(optimiseur, "lancement_llm", lambda f: self.lancement(16384)):
            _, r = self.propositions()
        alerte = next((a for a in r["alertes"] if a.startswith(LLM.removesuffix(".gguf"))), None)
        self.assertIsNotNone(alerte, r["alertes"])
        self.assertIn("contexte 16384", alerte)
        self.assertIn("Sans la Réflexion", alerte)  # couper la Réflexion le ferait tenir : on le dit
        with mock.patch.object(optimiseur, "lancement_llm", lambda f: self.lancement(4096, reflexion=False)):
            _, r = self.propositions()
        self.assertFalse(any(a.startswith(LLM.removesuffix(".gguf")) for a in r["alertes"]))

    def test_megapixels_selon_la_carte(self):
        """La résolution dépend de la carte : « Recommander » la dit en note, il ne l'impose jamais."""
        def conseil():
            r = optimiseur.recommandations_workflow("qwen_image_2_1")
            self.assertNotIn("megapixels", {p["cle"] for p in r["propositions"]})
            return " ".join(a for a in r["alertes"] if "MP" in a)

        self.carte(48)
        self.assertEqual(conseil(), "")  # assez de VRAM : le natif convient, rien à dire
        self.carte(20)  # 19 Go libres - 8,7 Go estimés : marge entre 6 et 12 Go
        self.assertIn("à 2 MP", conseil())  # marge moyenne : plafonné
        self.carte(12)  # petite marge : 1 MP
        self.assertIn("à 1 MP", conseil())

    def test_valeur_refusee_non_proposee(self):
        # Une recommandation que le réglage n'accepte pas (ici un sampler inconnu) est ignorée, sans erreur.
        wf = workflows.obtenir("krea2_turbo")
        fiche = {**wf.fiche, "recommandations": {"valeurs": [{"cle": "sampler", "valeur": "inexistant"}]}}
        with mock.patch.object(optimiseur.workflows, "obtenir", return_value=workflows.Workflow(fiche, wf.api)):
            r = optimiseur.recommandations_workflow("krea2_turbo")
        self.assertEqual(r["propositions"], [])

    def test_modele_sans_recommandation_dans_sa_fiche(self):
        wf = workflows.obtenir("krea2_turbo")
        fiche = {k: v for k, v in wf.fiche.items() if k != "recommandations"}
        with mock.patch.object(optimiseur.workflows, "obtenir", return_value=workflows.Workflow(fiche, wf.api)):
            r = optimiseur.recommandations_workflow("krea2_turbo")
        self.assertTrue(r["sans_recommandation"])
        self.assertEqual(r["propositions"], [])

    def test_un_derive_reprend_les_recommandations_de_son_origine(self):
        """Bonsai 2 Heretic n'est pas au catalogue : il reprend les valeurs de Bonsai 2, dont il porte le début de nom
        (celui du projecteur qu'ils partagent). Avant, « Adapter à ma machine » ne lui proposait rien."""
        heretic = "Ternary-Bonsai-2-27B-Uncensored-Heretic-PQ2_0.gguf"
        with mock.patch.object(modeles, "llm_disponibles", return_value=[heretic, LLM]):
            props, r = self.propositions()
        self.assertEqual(props[(heretic, "temperature_reflexion")]["propose"], 1.0)
        self.assertEqual(props[(heretic, "top_k_reflexion")]["propose"], 20)
        self.assertEqual(props[(heretic, "presence_penalty")]["propose"], 1.5)
        self.assertIn("reprise de Bonsai 2 27B, son modèle d'origine", props[(heretic, "temperature_reflexion")]["raison"])
        self.assertTrue(props[(heretic, "top_k_reflexion")]["source"].startswith("https://huggingface.co/prism-ml/"))
        self.assertNotIn(heretic.removesuffix(".gguf"), r["sans_recommandation"])
        self.assertIn(LLM.removesuffix(".gguf"), r["sans_recommandation"])  # sans famille connue : rien
        self.assertEqual(optimiseur._recommandations_de("Autre-Bonsai-2-27B.gguf", optimiseur._recommandations_llm()),
                         ({}, None))  # le nom doit commencer pareil, pas seulement le contenir

    def test_modele_sans_recommandation(self):
        with mock.patch.object(modeles, "llm_disponibles", return_value=["inconnu.gguf"]), \
                mock.patch.object(optimiseur.os.path, "getsize", return_value=4 * GO):
            _, r = self.propositions()
        self.assertIn("inconnu", r["sans_recommandation"])

    def test_fichiers_de_recommandations_valides(self):
        """Chaque recommandation rangée vise un réglage qui existe et respecte ses bornes."""
        for wf in workflows.catalogue():
            params = {**reglages.params_workflow(wf), "steps": reglages.params_variante(wf)["steps"]}  # steps : par checkpoint
            for r in (wf.fiche.get("recommandations") or {}).get("valeurs") or []:
                self.assertIn(r["cle"], params, wf.id)
                params[r["cle"]].valider(r["valeur"])
                self.assertTrue(r.get("source", "").startswith("https://"), (wf.id, r["cle"]))
        params = reglages.params_llm()
        for fichier, reco in optimiseur._recommandations_llm().items():  # celles du catalogue, livré avec Aria
            self.assertTrue(reco["source"].startswith("https://"), fichier)
            for r in reco["valeurs"]:
                params[r["cle"]].valider(r["valeur"])


if __name__ == "__main__":
    unittest.main()
