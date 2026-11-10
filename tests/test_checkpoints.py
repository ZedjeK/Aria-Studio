"""Tests du changement de checkpoint : défaut par workflow (Réglages), réglages par checkpoint (LoRA du workflow, steps),
choix dans le Montage et les séquences, message clair pour un checkpoint incompatible.

data/reglages.yaml, les séquences et la galerie vivent dans un dossier temporaire ; le dossier des checkpoints est
simulé (un checkpoint déposé, « Dasiwa »), sans dépendre de ce qui est réellement dans modeles/.
"""

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from app import modeles, plateau, reglages, rushes, sequences, workflows
from app.plateau import Commande, ErreurPlateau, Plateau
from app.reglages import ErreurReglages
from tests.test_sequences import png
from tests import steps_connus

H3 = "minimax_h3_i2v"
ORIGINE_H3 = "minimax_h3_fl2va_pruned_int8_convrot.safetensors"
DASIWA = "DasiwaMinimaxH3_dasiwaHybridV4Turbo_int8.safetensors"


def deposes(dossier, extensions):
    """modeles.fichiers simulé (trié, comme le vrai) : l'origine de H3 et un checkpoint déposé, rien d'autre."""
    return sorted([ORIGINE_H3, DASIWA]) if dossier == "video/MiniMax-H3/modeles" else []


def sans_origine(dossier, extensions):
    """L'origine de H3 supprimée : il ne reste que le checkpoint déposé."""
    return [DASIWA] if dossier == "video/MiniMax-H3/modeles" else []


def presents_sauf_checkpoints(dossier, nom):
    """modeles.destination simulé : les fichiers communs (encodeurs, VAE, LoRA du workflow) sont là."""
    return SimpleNamespace(is_file=lambda: dossier != "video/MiniMax-H3/modeles")



setUpModule, tearDownModule = steps_connus.module()  # ces tests ne portent pas sur la validation des steps

class Base(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [mock.patch.object(reglages, "FICHIER", d / "reglages.yaml"),
                       mock.patch.object(reglages, "_cache", None),
                       mock.patch.object(modeles, "fichiers", side_effect=deposes),
                       mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(sequences, "DOSSIER", d / "sequences"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json")]
        for p in self.patchs:
            p.start()
        self.wf = workflows.obtenir(H3)

    def tearDown(self):
        for p in reversed(self.patchs):  # à l'envers : un remplacement empilé rend bien celui d'en dessous
            p.stop()
        self.dossier.cleanup()

    def noeuds(self, api, type_):
        return [v for v in api.values() if v["class_type"] == type_]

    def unet(self, api) -> str:
        return self.noeuds(api, "UNETLoader")[0]["inputs"]["unet_name"]

    def preparer(self, **valeurs):
        return self.wf.preparer({"prompt": "A pilot smiles.", "seed": 1, "image": "a.png", **valeurs})


class ModeleParDefaut(Base):
    def test_propose_l_origine_et_les_checkpoints_deposes(self):
        p = reglages.params_workflow(self.wf)["modele"]
        self.assertEqual((p.defaut, p.choix), (ORIGINE_H3, (ORIGINE_H3, DASIWA)))
        self.assertIn("modeles/video/MiniMax-H3/modeles", p.aide)

    def test_le_defaut_s_applique_quand_rien_n_est_choisi(self):
        self.assertEqual(self.unet(self.preparer()), ORIGINE_H3)
        reglages.modifier_workflow(H3, {"modele": DASIWA})
        self.assertEqual(self.unet(self.preparer()), DASIWA)
        self.assertEqual(self.unet(self.preparer(modele=None)), DASIWA)  # None = rien de choisi, pas l'origine
        self.assertEqual(self.unet(self.preparer(modele=ORIGINE_H3)), ORIGINE_H3)  # un choix explicite l'emporte
        self.assertEqual(self.wf.choisi(None), DASIWA)

    def test_checkpoint_supprime_retour_a_l_origine(self):
        reglages.modifier_workflow(H3, {"modele": DASIWA})
        with mock.patch.object(modeles, "fichiers", return_value=[ORIGINE_H3]):
            self.assertEqual(reglages.valeurs_workflow(self.wf)["modele"], ORIGINE_H3)
            self.assertEqual(self.unet(self.preparer()), ORIGINE_H3)

    def test_refus(self):
        with self.assertRaisesRegex(ErreurReglages, "n'est pas proposé"):
            reglages.modifier_workflow(H3, {"modele": "inconnu.safetensors"})
        with self.assertRaisesRegex(workflows.ErreurWorkflow, "Modèle absent"):
            self.preparer(modele="inconnu.safetensors")


class FamilleSansOrigine(Base):
    """Le checkpoint livré avec le workflow supprimé : la famille suffit, ses autres checkpoints servent toujours."""

    def setUp(self):
        super().setUp()
        for p in (mock.patch.object(modeles, "fichiers", side_effect=sans_origine),
                  mock.patch.object(modeles, "destination", side_effect=presents_sauf_checkpoints)):
            p.start()
            self.patchs.append(p)

    def test_pret_avec_un_autre_checkpoint(self):
        self.assertEqual(self.wf.variantes(), [DASIWA])
        self.assertEqual(self.wf.manquants(), [])
        self.assertTrue(self.wf.resume()["pret"])
        with mock.patch.object(modeles, "fichiers", return_value=[]):  # plus aucun checkpoint : l'origine manque
            self.assertEqual([f["nom"] for f in self.wf.manquants()], [ORIGINE_H3])

    def test_le_defaut_passe_au_checkpoint_present(self):
        p = reglages.params_workflow(self.wf)["modele"]
        self.assertEqual((p.defaut, p.choix), (DASIWA, (DASIWA,)))
        self.assertEqual(self.wf.choisi(None), DASIWA)
        self.assertEqual(self.unet(self.preparer()), DASIWA)  # plus le fichier d'origine écrit dans api.json

    def test_defaut_regle_sur_l_origine_supprimee(self):
        with mock.patch.object(modeles, "fichiers", side_effect=deposes):
            reglages.modifier_workflow(H3, {"modele": ORIGINE_H3})
        self.assertEqual(reglages.valeurs_workflow(self.wf)["modele"], DASIWA)
        with self.assertRaisesRegex(workflows.ErreurWorkflow, "Modèle absent"):
            self.preparer(modele=ORIGINE_H3)

    def test_options_du_plateau(self):
        with mock.patch.object(modeles, "llm_disponibles", return_value=[]):
            t = Plateau(orchestrateur=None).options()["workflows"]["image_vers_video"]
        w = next(x for x in t["choix"] if x["id"] == H3)
        self.assertEqual((w["modeles"], w["modele_defaut"]), ([DASIWA], DASIWA))

    def test_conseil_sans_l_origine(self):
        c = Commande("idée", None, None, H3, "16:9", None, 1, modele=DASIWA, type="clip")
        message = plateau.expliquer_erreur(c, "UNETLoader : Error(s) in loading state_dict: size mismatch")
        self.assertIn("autre checkpoint de la famille MiniMax-H3", message)
        self.assertNotIn(ORIGINE_H3, message)


class ReglagesParCheckpoint(Base):
    def test_lora_du_workflow_retiree(self):
        self.assertEqual(len(self.noeuds(self.preparer(modele=DASIWA), "LoraLoaderModelOnly")), 1)  # le Turbo
        reglages.modifier_variante(H3, DASIWA, {"lora_workflow": False})
        api = self.preparer(modele=DASIWA)
        self.assertFalse(self.noeuds(api, "LoraLoaderModelOnly"))
        chargeur = next(n for n, v in api.items() if v["class_type"] == "UNETLoader")
        self.assertEqual(self.noeuds(api, "BasicGuider")[0]["inputs"]["model"], [chargeur, 0])  # rebranché
        self.assertTrue(self.noeuds(self.preparer(modele=ORIGINE_H3), "LoraLoaderModelOnly"))  # l'origine la garde

    def test_lora_choisie_par_l_utilisateur_gardee(self):
        reglages.modifier_variante(H3, DASIWA, {"lora_workflow": False})
        with mock.patch.object(modeles, "fichiers", side_effect=lambda d, e: ["style.safetensors"]
                               if d == "video/MiniMax-H3/loras" else deposes(d, e)):
            api = self.preparer(modele=DASIWA, loras=[{"fichier": "style.safetensors", "force": 0.8}])
        self.assertEqual([l["inputs"]["lora_name"] for l in self.noeuds(api, "LoraLoaderModelOnly")], ["style.safetensors"])

    def test_steps_du_checkpoint(self):
        reglages.modifier_variante(H3, DASIWA, {"steps": 6})
        sched = lambda api: self.noeuds(api, "BasicScheduler")[0]["inputs"]["steps"]
        self.assertEqual(sched(self.preparer(modele=DASIWA)), 6)  # ses steps, quelle que soit la résolution
        self.assertEqual(sched(self.preparer(modele=ORIGINE_H3)), 8)  # l'origine : les siens (ici le défaut de sa fiche)
        self.assertEqual(sched(self.preparer(modele=DASIWA, steps=12)), 12)  # un plan qui parle impose 12

    def test_enregistrement_et_retour(self):
        self.assertEqual(reglages.modifier_variante(H3, DASIWA, {"lora_workflow": False, "steps": 6}),
                         {"lora_workflow": False, "steps": 6})
        self.assertEqual(reglages.lire()["workflows"][H3]["variantes"], {DASIWA: {"lora_workflow": False, "steps": 6}})
        reglages.modifier_variante(H3, DASIWA, {"lora_workflow": None, "steps": None})
        self.assertNotIn("variantes", reglages.lire()["workflows"].get(H3, {}))  # rien d'autre que l'origine
        with self.assertRaisesRegex(ErreurReglages, "Checkpoint absent"):
            reglages.modifier_variante(H3, "inconnu.safetensors", {"steps": 6})

    def test_etat_pour_l_onglet(self):
        with mock.patch.object(modeles, "llm_disponibles", return_value=[]):
            etat = next(w for w in reglages.etat()["workflows"] if w["id"] == H3)
        self.assertEqual([(v["fichier"], v["origine"], v["connue"]) for v in etat["variantes"]],
                         [(ORIGINE_H3, True, True), (DASIWA, False, False)])  # déposé à la main : source inconnue
        self.assertEqual({p["cle"] for p in etat["variantes"][1]["params"]}, {"lora_workflow", "steps"})


class Montage(Base):
    def test_clip_avec_un_checkpoint(self):
        clip = plateau.enregistrer(png(), {"type": "creation", "idee": "une pilote"})
        c = Plateau(orchestrateur=None).commander_clip("elle sourit", "She smiles.", "animer", clip,
                                                        "rapide", 5, "16:9 (Widescreen)", None, 1, modele=DASIWA)
        self.assertEqual(c.modele, DASIWA)
        with self.assertRaisesRegex(ErreurPlateau, "Modèle absent"):
            Plateau(orchestrateur=None).commander_clip("x", "x", "animer", clip, "rapide", 5,
                                                        "16:9 (Widescreen)", None, 1, modele="inconnu.safetensors")

    def test_options(self):
        reglages.modifier_workflow(H3, {"modele": DASIWA})
        with mock.patch.object(modeles, "llm_disponibles", return_value=[]):
            t = Plateau(orchestrateur=None).options()["workflows"]["image_vers_video"]
        w = next(x for x in t["choix"] if x["id"] == H3)
        self.assertEqual((w["modeles"], w["modele_defaut"]), ([ORIGINE_H3, DASIWA], DASIWA))

    def test_checkpoint_incompatible_explique(self):
        c = Commande("idée", None, None, H3, "16:9", None, 1, modele=DASIWA, type="clip")
        message = plateau.expliquer_erreur(c, "UNETLoader : Error(s) in loading state_dict: size mismatch")
        self.assertIn("ne se charge pas", message)
        self.assertIn(ORIGINE_H3, message)
        c.modele = ORIGINE_H3  # l'origine qui échoue : autre chose, l'erreur brute reste
        self.assertEqual(plateau.expliquer_erreur(c, "UNETLoader : boom"), "UNETLoader : boom")


class Sequences(Base):
    def setUp(self):
        super().setUp()
        self.pilote = plateau.enregistrer(png(), {"type": "creation", "idee": "une pilote"})

    def creer(self, **autres):
        return sequences.creer({"titre": "Essai", "qualite": "rapide", **autres, "plans": [
            {"idee": "elle sourit", "enchainement": "rush", "rush": self.pilote},
            {"idee": "elle se lève", "enchainement": "prolonger"}]})

    def tout_a_jour(self, s):
        s = sequences.lire(s["id"])
        for p in s["plans"]:
            p.update(clip=plateau.enregistrer_clip(b"mp4", png(8, 8), {"type": "clip"}), a_jour=True)
        sequences._ecrire(s)
        return sequences.public(s)

    def test_la_sequence_garde_son_checkpoint(self):
        reglages.modifier_workflow(H3, {"modele": DASIWA})
        s = self.creer()
        self.assertEqual(s["modele"], DASIWA)  # le défaut au moment de la création
        reglages.modifier_workflow(H3, {"modele": None})  # le défaut change ensuite…
        s = self.tout_a_jour(s)
        self.assertEqual(sequences.modifier(s["id"], {**s, "modele": None})["modele"], DASIWA)  # … pas la séquence
        api = sequences.graphe_plan(sequences.lire(s["id"]), 0, "She smiles.", 1, {"rush": "r.png"})
        self.assertEqual(self.unet(api), DASIWA)

    def test_changer_de_checkpoint_retourne_tout(self):
        s = self.tout_a_jour(self.creer(modele=DASIWA))
        self.assertEqual(sequences.modifier(s["id"], {**s, "modele": ORIGINE_H3})["a_tourner"], [0, 1])

    def test_ancienne_sequence(self):
        s = self.tout_a_jour(self.creer())
        brute = sequences.lire(s["id"])
        del brute["modele"]  # tournée avant le choix du checkpoint : avec l'origine
        sequences._ecrire(brute)
        s = sequences.modifier(s["id"], {**s, "modele": None})
        self.assertEqual((s["modele"], s["a_tourner"]), (ORIGINE_H3, []))

    def test_refus(self):
        with self.assertRaisesRegex(sequences.ErreurSequence, "Checkpoint absent"):
            self.creer(modele="inconnu.safetensors")


if __name__ == "__main__":
    unittest.main()
