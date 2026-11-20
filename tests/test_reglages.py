"""Tests du moteur de l'onglet Réglages (app/reglages.py) et de son application à Llama et aux workflows.

Le fichier data/reglages.yaml est remplacé par un fichier temporaire : les vrais réglages ne sont pas touchés.
"""

import asyncio
import json
import os
import tempfile
import unittest
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from app import fichier_reglages, listes, modeles, prompt, reglages, workflows
from app.llm import Llama
from app.reglages import ErreurReglages
from tests import steps_connus
from tests import reglages_jetables

LLM = "Autre-LLM-14B-Q4_K_M.gguf"  # un GGUF quelconque, hors catalogue



setUpModule, tearDownModule = steps_connus.module()  # ces tests ne portent pas sur la validation des steps

class AvecFichierTemporaire(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [reglages_jetables.patch(d),
                       mock.patch.object(reglages, "FICHIER_SAMPLERS", d / "samplers.json"),
                       mock.patch.object(reglages, "_cache", None),
                       mock.patch.object(modeles, "llm_disponibles", return_value=[LLM])]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()


class Cache(AvecFichierTemporaire):
    def test_deux_ecritures_a_la_meme_date_ne_se_perdent_pas(self):
        """Sous Windows l'horloge des fichiers est grossière : deux écritures rapprochées portent la même date.
        Le cache ne doit pas resservir l'ancien contenu (une modification était perdue, au hasard)."""
        reglages.modifier_workflow("minimax_h3_t2v", {"megapixels": 0.5})
        fige = fichier_reglages.fichier().stat()
        vraie_stat = Path.stat

        def stat_figee(chemin, *a, **k):
            r = vraie_stat(chemin, *a, **k)
            if chemin != fichier_reglages.fichier():
                return r
            return os.stat_result((r.st_mode, r.st_ino, r.st_dev, r.st_nlink, r.st_uid, r.st_gid, r.st_size,
                                   r.st_atime, fige.st_mtime, fige.st_ctime, r.st_atime_ns, fige.st_mtime_ns,
                                   fige.st_ctime_ns))

        with mock.patch.object(Path, "stat", stat_figee):
            reglages.modifier_workflow("minimax_h3_t2v", {"megapixels": 0.6})
            reglages.modifier_workflow("minimax_h3_t2v", {"sampler": "euler"})
            valeurs = reglages.valeurs_workflow(workflows.obtenir("minimax_h3_t2v"))
        self.assertEqual((valeurs["megapixels"], valeurs["sampler"]), (0.6, "euler"))


class Workflows(AvecFichierTemporaire):
    def test_reglages_proposes_selon_le_graphe(self):
        # « modele » (le checkpoint par défaut) partout : toutes les fiches livrées ont l'entrée modele.
        cles = {wf.id: set(reglages.params_workflow(wf)) - {"modele"} for wf in workflows.catalogue()}
        self.assertTrue(all("modele" in reglages.params_workflow(wf) for wf in workflows.catalogue()))
        # Les steps se règlent par checkpoint (params_variante) : plus de réglage « Steps » par workflow.
        self.assertEqual(cles["krea2_turbo"], {"cfg", "sampler", "scheduler", "megapixels", "ratio", "mots_max"})
        self.assertEqual(cles["qwen_image_2_1_edit"], {"cfg", "sampler", "scheduler"})
        resolutions = {"resolution_rapide", "resolution_compromis", "resolution_qualite"}  # boutons du Montage (H3)
        self.assertEqual(cles["minimax_h3_t2v"], {"sampler", "scheduler", "megapixels", "ratio", "mots_max"} | resolutions)
        self.assertEqual(cles["minimax_h3_i2v"], {"sampler", "scheduler", "megapixels", "mots_max"} | resolutions)
        self.assertTrue(all("steps" in reglages.params_variante(wf) for wf in workflows.catalogue()))

    def test_h3_sampler_et_scheduler_dans_leurs_noeuds(self):
        reglages.modifier_workflow("minimax_h3_t2v", {"sampler": "euler", "scheduler": "beta"})
        api = workflows.obtenir("minimax_h3_t2v").preparer({"prompt": "x", "seed": 1})
        self.assertEqual(api[reglages.noeud_unique(api, "KSamplerSelect")]["inputs"]["sampler_name"], "euler")
        self.assertEqual(api[reglages.noeud_unique(api, "BasicScheduler")]["inputs"]["scheduler"], "beta")
        aide = reglages.params_workflow(workflows.obtenir("minimax_h3_t2v"))["megapixels"].aide
        self.assertIn("Montage", aide)

    def test_recommander_donne_le_sampler_officiel_de_h3(self):
        from app import optimiseur
        reglages.modifier_workflow("minimax_h3_i2v", {"sampler": "euler"})
        with mock.patch.object(optimiseur, "profil_materiel", return_value={"vram_libre_go": 14}):
            i2v = {p["cle"]: p for p in optimiseur.recommandations_workflow("minimax_h3_i2v")["propositions"]}
            t2v = {p["cle"]: p for p in optimiseur.recommandations_workflow("minimax_h3_t2v")["propositions"]}
        self.assertEqual((i2v["sampler"]["actuel"], i2v["sampler"]["propose"]), ("euler", "res_multistep"))
        self.assertEqual(t2v["sampler"]["actuel"], t2v["sampler"]["propose"])  # déjà au réglage officiel

    def test_valeurs_d_origine(self):
        wf = workflows.obtenir("qwen_image_2_1_edit")
        self.assertEqual(reglages.valeurs_workflow(wf),
                         {"cfg": 1.0, "sampler": "euler", "scheduler": "simple",
                          "modele": "qwen_image_2.1_int8_convrot.safetensors"})

    def test_preparer_applique_les_reglages(self):
        reglages.modifier_workflow("qwen_image_2_1_edit", {"cfg": 2.5, "sampler": "dpmpp_2m", "scheduler": "karras"})
        wf = workflows.obtenir("qwen_image_2_1_edit")
        reglages.modifier_variante(wf.id, wf.modele_origine, {"steps": 8})  # les steps : ceux du checkpoint
        api = wf.preparer({"prompt": "make the sky red", "seed": 1, "image": "a.png"})
        ks = api[reglages.noeud_unique(api, "KSampler")]["inputs"]
        self.assertEqual((ks["steps"], ks["cfg"], ks["sampler_name"], ks["scheduler"]), (8, 2.5, "dpmpp_2m", "karras"))
        self.assertEqual(wf.api[reglages.noeud_unique(wf.api, "KSampler")]["inputs"]["cfg"], 1)  # l'original intact

    def test_la_valeur_de_l_appel_gagne(self):
        reglages.modifier_workflow("krea2_turbo", {"ratio": "16:9 (Widescreen)"})
        wf = workflows.obtenir("krea2_turbo")
        reglages.modifier_variante(wf.id, wf.modele_origine, {"steps": 12})
        api = wf.preparer({"prompt": "a cat", "seed": 1, "steps": 5})  # l'appel l'emporte sur le checkpoint (essais)
        noeud, entree = wf.fiche["entrees"]["steps"]
        self.assertEqual(api[noeud]["inputs"][entree], 5)
        noeud, entree = wf.fiche["entrees"]["ratio"]
        self.assertEqual(api[noeud]["inputs"][entree], "16:9 (Widescreen)")  # pas donné : le réglage s'applique

    def test_retour_aux_valeurs_d_origine(self):
        reglages.modifier_workflow("krea2_turbo", {"cfg": 1.5, "sampler": "euler_ancestral"})
        reglages.modifier_workflow("krea2_turbo", {"cfg": 1.0, "sampler": None})  # None, ou la valeur d'origine
        self.assertEqual(reglages.lire()["workflows"], {})
        self.assertEqual(reglages.valeurs_workflow(workflows.obtenir("krea2_turbo"))["cfg"], 1.0)

    def test_valeurs_refusees(self):
        for changement in ({"steps": 0}, {"steps": 151}, {"steps": 2.5}, {"steps": True}, {"cfg": "fort"},
                           {"sampler": "magique"}, {"ratio": "5:4"}, {"megapixels": 9}, {"inconnu": 1}):
            with self.assertRaises(ErreurReglages, msg=changement):
                reglages.modifier_workflow("krea2_turbo", changement)
        with self.assertRaises(ErreurReglages):
            reglages.modifier_workflow("krea2_turbo", {"shift": 3})  # pas de nœud de shift dans ce graphe
        with self.assertRaises(workflows.ErreurWorkflow):
            reglages.modifier_workflow("inexistant", {"steps": 3})
        self.assertFalse(fichier_reglages.fichier().exists())

    def test_une_liste_montre_toujours_sa_valeur(self):
        """Aucun checkpoint installé : la liste « Modèle par défaut » était vide, sa valeur n'était dans aucun de ses
        choix, et l'onglet Réglages comptait ce champ vide comme « 1 changement non enregistré »."""
        with mock.patch.object(workflows.Workflow, "variantes", return_value=[]):
            e = reglages.etat()
        listes_vues = 0
        for bloc in [*e["workflows"], *e["llm"]]:
            for p in bloc["params"]:
                if p["type"] == "choix":
                    listes_vues += 1
                    self.assertIn(p["valeur"], p["choix"], f"{bloc.get('id', bloc.get('fichier'))} · {p['cle']}")
        self.assertGreater(listes_vues, len(e["workflows"]))  # au moins « modele » partout, et d'autres listes

    def test_sans_checkpoint_la_liste_dit_que_l_origine_n_est_pas_installee(self):
        wf = workflows.obtenir("krea2_turbo")
        with mock.patch.object(workflows.Workflow, "variantes", return_value=[]):
            p = reglages.params_workflow(wf)["modele"]
            valeurs = reglages.valeurs_workflow(wf)
        self.assertEqual((p.choix, p.defaut, valeurs["modele"]), ((wf.modele_origine,), wf.modele_origine, wf.modele_origine))
        self.assertEqual(p.public(valeurs["modele"])["libelles"], {wf.modele_origine: f"{wf.modele_origine} (pas installé)"})

    def test_avec_des_checkpoints_la_liste_ne_propose_qu_eux(self):
        wf = workflows.obtenir("krea2_turbo")
        with mock.patch.object(workflows.Workflow, "variantes", return_value=["autre.safetensors"]):
            p = reglages.params_workflow(wf)["modele"]
        self.assertEqual((p.choix, p.defaut, p.libelles), (("autre.safetensors",), "autre.safetensors", ()))

    def test_samplers_annonces_par_comfyui(self):
        reglages.memoriser_samplers(["euler", "res_multistep", "sa_solver"], ["simple", "beta57"])
        p = reglages.params_workflow(workflows.obtenir("qwen_image_2_1"))
        self.assertEqual(p["sampler"].choix, ("euler", "res_multistep", "sa_solver"))
        self.assertIn("beta57", p["scheduler"].choix)
        reglages.memoriser_samplers(["dpmpp_2m"], ["karras"])  # la valeur d'origine reste toujours proposée
        self.assertEqual(reglages.params_workflow(workflows.obtenir("qwen_image_2_1"))["sampler"].choix,
                         ("euler", "dpmpp_2m"))

    def test_mots_max_reglable(self):
        reglages.modifier_workflow("qwen_image_2_1", {"mots_max": 60})
        self.assertEqual(listes.assembler("personnage", {}, workflow="qwen_image_2_1")["mots_max"], 60)


class FauxOrchestrateur:
    """`reflexion` : la Réflexion est active pour le GGUF chargé (None : un faux LLM qui n'en dit rien).
    `texte` : ce que le faux LLM répond."""

    def __init__(self, reflexion=None, texte="a red umbrella"):
        self.appels = []
        self.reflexion = reflexion
        self.texte = texte

    @asynccontextmanager
    async def llm(self, modele=None):
        async def generer(messages, **options):
            self.appels.append((messages, options))
            return SimpleNamespace(texte=self.texte, tokens_sortie=3, ecriture_tps=1.0)
        llama = SimpleNamespace(generer=generer, modele=None)
        if self.reflexion is not None:
            llama.reflexion_active = self.reflexion
        yield llama


class Llm(AvecFichierTemporaire):
    def test_valeurs_d_origine(self):
        v = reglages.valeurs_llm(LLM)
        self.assertEqual((v["temperature_reecriture"], v["contexte"], v["reflexion"]), (0.7, 4096, True))

    def test_temperatures_et_longueur_appliquees(self):
        reglages.modifier_llm(LLM, {"temperature_reecriture": 0.4, "temperature_traduction": 0.1, "max_tokens": 256})
        o = FauxOrchestrateur()
        asyncio.run(prompt.reecrire(o, "un chat roux", LLM))
        asyncio.run(prompt.traduire_precisions(o, "tient un parapluie", LLM))
        self.assertEqual(o.appels[0][1], {"temperature": 0.4, "max_tokens": 256,
                                          "top_p": 0.95, "top_k": 40, "min_p": 0.05, "presence_penalty": 0.0})
        self.assertEqual(o.appels[1][1]["temperature"], 0.1)
        asyncio.run(prompt.reecrire(o, "un chat roux", "autre.gguf"))  # un autre GGUF garde ses valeurs
        self.assertEqual(o.appels[2][1]["temperature"], 0.7)

    def test_echantillonnage_transmis(self):
        reglages.modifier_llm(LLM, {"top_p": 0.8, "top_k": 20, "min_p": 0.0, "presence_penalty": 1.5})
        o = FauxOrchestrateur()
        asyncio.run(prompt.traduire_precisions(o, "tient un parapluie", LLM))
        options = o.appels[0][1]
        self.assertEqual({k: options[k] for k in reglages.ECHANTILLONNAGE},
                         {"top_p": 0.8, "top_k": 20, "min_p": 0.0, "presence_penalty": 1.5})

    # Un jeu d'échantillonnage par mode : les modèles en recommandent un avec réflexion et un sans (Bonsai 2 :
    # 1,0 / 0,95 / 20 / 0,05 / 0 et 0,7 / 0,8 / 20 / 0 / 1,5). Avant, le jeu sans réflexion servait aux deux.
    SANS = {"temperature_reecriture": 0.7, "top_p": 0.8, "top_k": 20, "min_p": 0.0, "presence_penalty": 1.5}
    AVEC = {"temperature_reflexion": 1.0, "top_p_reflexion": 0.95, "top_k_reflexion": 20, "min_p_reflexion": 0.05,
            "presence_penalty_reflexion": 0.0}
    JEU_SANS = {"temperature": 0.7, "top_p": 0.8, "top_k": 20, "min_p": 0.0, "presence_penalty": 1.5}
    JEU_AVEC = {"temperature": 1.0, "top_p": 0.95, "top_k": 20, "min_p": 0.05, "presence_penalty": 0.0}

    def jeu(self, options: dict) -> dict:
        return {k: options[k] for k in ("temperature", *reglages.ECHANTILLONNAGE)}

    def test_ecriture_selon_le_mode(self):
        reglages.modifier_llm(LLM, {**self.SANS, **self.AVEC})
        self.assertEqual(reglages.ecriture(LLM, reflexion=True), self.JEU_AVEC)
        self.assertEqual(reglages.ecriture(LLM, reflexion=False), self.JEU_SANS)
        pense, coupee = FauxOrchestrateur(reflexion=True), FauxOrchestrateur(reflexion=False)
        asyncio.run(prompt.reecrire(pense, "un chat roux", LLM))
        asyncio.run(prompt.reecrire(coupee, "un chat roux", LLM))
        self.assertEqual(self.jeu(pense.appels[0][1]), self.JEU_AVEC)  # la Réflexion est active : son jeu
        self.assertEqual(self.jeu(coupee.appels[0][1]), self.JEU_SANS)  # coupée dans Réglages : le jeu sans réflexion

    def test_traductions_toujours_sans_reflexion(self):
        """Une traduction ne réfléchit jamais (reflechir=False) : jeu sans réflexion, même si la Réflexion est active."""
        reglages.modifier_llm(LLM, {**self.SANS, **self.AVEC})
        o = FauxOrchestrateur(reflexion=True)
        asyncio.run(prompt.traduire_precisions(o, "tient un parapluie", LLM))
        options = o.appels[0][1]
        self.assertIs(options["reflechir"], False)
        self.assertEqual({k: options[k] for k in reglages.ECHANTILLONNAGE},
                         {k: self.JEU_SANS[k] for k in reglages.ECHANTILLONNAGE})

    def test_conversation_ne_reflechit_que_pour_ecrire(self):
        """Le chat : seul le tour « ÉCRIS LE PROMPT » réfléchit, et prend le jeu avec réflexion ; les autres tours
        (questions, ajustements) gardent le jeu sans réflexion."""
        reglages.modifier_llm(LLM, {**self.SANS, **self.AVEC})
        tour = json.dumps({"message": "ok", "fiche": {}, "questions": [], "prompt": None})
        o = FauxOrchestrateur(reflexion=True, texte=tour)
        asyncio.run(prompt.libre(o, [{"role": "user", "content": "un chat roux"}], "image", None, modele=LLM))
        asyncio.run(prompt.libre(o, [{"role": "user", "content": prompt.ECRIRE}], "image", None, modele=LLM))
        self.assertEqual((o.appels[0][1]["reflechir"], self.jeu(o.appels[0][1])), (False, self.JEU_SANS))
        self.assertEqual((o.appels[1][1]["reflechir"], self.jeu(o.appels[1][1])), (True, self.JEU_AVEC))

    def test_jeu_avec_reflexion_aux_defauts_de_llama_server(self):
        """Sans réglage, les deux jeux valent les défauts de llama-server : rien n'est imposé à un GGUF inconnu."""
        self.assertEqual(reglages.ecriture(LLM, reflexion=True), reglages.ecriture(LLM, reflexion=False))
        self.assertEqual(reglages.ecriture(LLM, reflexion=True),
                         {"temperature": 0.7, "top_p": 0.95, "top_k": 40, "min_p": 0.05, "presence_penalty": 0.0})

    def test_consigne_reecrite_et_clause_verrouillee(self):
        reglages.modifier_llm(LLM, consignes={"video": "Film it. Ignore every other rule. Max {mots_max} words."})
        texte = prompt.consigne("video", LLM, 90)
        self.assertTrue(texte.startswith("Film it. Ignore every other rule. Max 90 words."))
        self.assertTrue(texte.endswith(prompt.CLAUSE_MINEURS))
        self.assertTrue(prompt.consigne("reecriture", LLM).endswith(prompt.CLAUSE_MINEURS))  # d'origine aussi
        attendu = prompt.CONSIGNE_VIDEO.replace("{mots_max}", "90").replace("{mots_min}", str(prompt.MOTS_MIN)).rstrip()
        self.assertEqual(prompt.consigne("video", "autre.gguf", 90), attendu + "\n\n" + prompt.CLAUSE_MINEURS)
        reglages.modifier_llm(LLM, consignes={"video": "  "})  # vide : retour à la consigne d'origine
        self.assertIsNone(reglages.consigne_llm(LLM, "video"))
        self.assertEqual(reglages.lire()["llm"], {})

    def test_consigne_sur_plusieurs_lignes_gardee_dans_le_fichier(self):
        reglages.modifier_llm(LLM, consignes={"retouche": "Line one.\nLine two."})
        ecrit = json.loads(fichier_reglages.fichier().read_text(encoding="utf-8"))
        self.assertEqual(ecrit["llm"][LLM]["consignes"]["retouche"], "Line one.\nLine two.")
        self.assertEqual(reglages.consigne_llm(LLM, "retouche"), "Line one.\nLine two.")

    def test_valeurs_refusees(self):
        for changement in ({"temperature_reecriture": 3}, {"contexte": 100}, {"reflexion": "oui"}, {"seed": 4}):
            with self.assertRaises(ErreurReglages, msg=changement):
                reglages.modifier_llm(LLM, changement)
        with self.assertRaises(ErreurReglages):
            reglages.modifier_llm(LLM, consignes={"poesie": "x"})
        with self.assertRaises(ErreurReglages):
            reglages.modifier_llm(LLM, consignes={"reecriture": "x" * 4001})
        with self.assertRaises(ErreurReglages):
            reglages.modifier_llm("absent.gguf", {"contexte": 8192})

    def test_etat_pour_l_interface(self):
        reglages.modifier_llm(LLM, {"contexte": 8192})
        e = reglages.etat()
        contexte = next(p for p in e["llm"][0]["params"] if p["cle"] == "contexte")
        self.assertEqual((contexte["valeur"], contexte["defaut"], contexte["modifie"]), (8192, 4096, True))
        self.assertEqual(len(e["workflows"]), len(workflows.catalogue()))
        self.assertEqual(e["clause_mineurs"], prompt.CLAUSE_MINEURS)


class RedemarrageLlama(unittest.TestCase):
    """llama-server ne redémarre que si le modèle, le serveur, le contexte ou la réflexion changent."""

    def lama_en_route(self):
        l = Llama(Path("serveur.exe"), 8095, 4096, False)
        l.process = SimpleNamespace(poll=lambda: None)
        l.modele, l._serveur_actif = Path("m.gguf"), Path("serveur.exe")
        return l

    def test_meme_reglage_pas_de_redemarrage(self):
        l = self.lama_en_route()
        with mock.patch.object(l, "arreter", side_effect=AssertionError("redémarrage inutile")):
            asyncio.run(l.demarrer(Path("m.gguf"), contexte=4096, reflexion=False))

    def test_contexte_change_redemarre(self):
        for options in ({"contexte": 8192}, {"reflexion": True}):
            l = self.lama_en_route()
            with mock.patch.object(l, "arreter", side_effect=RuntimeError("arrêt")):
                with self.assertRaisesRegex(RuntimeError, "arrêt", msg=options):
                    asyncio.run(l.demarrer(Path("m.gguf"), **options))


if __name__ == "__main__":
    unittest.main()
