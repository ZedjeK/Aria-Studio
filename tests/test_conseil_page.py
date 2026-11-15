"""Le conseil de la page d'un checkpoint (steps et CFG) : lu dans son texte au téléchargement, gardé après la validation,
montré dans Modèles → Ma collection. Il n'est jamais appliqué seul : aucune source n'a de champ « steps » ni « cfg »."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import collection, modeles, reglages, workflows
from app.sources import steps
from tests import reglages_jetables

H3 = "minimax_h3_i2v"
NEUF = "10Eros_Max_h3_8stepTURBO_beta7.safetensors"
ORIGINE = "minimax_h3_fl2va_pruned_int8_convrot.safetensors"
CLE = f"video/MiniMax-H3/modeles/{NEUF}"
UNIQUE = {"statut": "unique", "valeurs": [8], "extraits": []}
CFG = {"statut": "unique", "valeurs": [1.5], "extraits": [{"origine": "description", "texte": "12 steps, CFG 1.5"}]}


def deposes(dossier, extensions):
    return sorted([ORIGINE, NEUF]) if dossier == "video/MiniMax-H3/modeles" else []


class LireLeCfg(unittest.TestCase):
    def valeurs(self, texte):
        s = steps.suggerer_cfg({"description": texte})
        return s["statut"], s["valeurs"]

    def test_un_cfg_unique(self):
        self.assertEqual(self.valeurs("Use 12 steps and CFG 1.5."), ("unique", [1.5]))
        self.assertEqual(self.valeurs("cfg: 1"), ("unique", [1.0]))
        self.assertEqual(self.valeurs("cfg=2"), ("unique", [2.0]))
        self.assertEqual(self.valeurs("CFG scale 4"), ("unique", [4.0]))
        self.assertEqual(self.valeurs("CFG of 3,5 works best"), ("unique", [3.5]))  # virgule française
        self.assertEqual(self.valeurs("1.5 CFG, 12 steps"), ("unique", [1.5]))
        self.assertEqual(self.valeurs("Keep CFG 1."), ("unique", [1.0]))  # le point d'une phrase n'est pas une décimale

    def test_deux_conseils_sont_ambigus(self):
        self.assertEqual(self.valeurs("8 steps, CFG 1, or 12 steps, CFG 1.5 (max CFG 2.5)"), ("ambigu", [1.0, 1.5, 2.5]))

    def test_rien_trouve(self):
        self.assertEqual(self.valeurs("Great model, enjoy")[0], "aucun")
        self.assertEqual(self.valeurs("Edit the config file and the configuration")[0], "aucun")  # « config » n'est pas « cfg »
        self.assertEqual(self.valeurs("CFG")[0], "aucun")  # le mot sans nombre

    def test_valeurs_hors_bornes_ignorees(self):
        self.assertEqual(self.valeurs("CFG 40")[0], "aucun")  # au-delà du réglage (30)

    def test_balises_html_et_extrait(self):
        s = steps.suggerer_cfg({"description": "<p>Use <b>CFG 1.5</b> with 12 steps</p>"})
        self.assertEqual(s["valeurs"], [1.5])
        self.assertIn("CFG 1.5", s["extraits"][0]["texte"])
        self.assertEqual(s["extraits"][0]["origine"], "description")

    def test_les_steps_et_le_cfg_se_lisent_dans_le_meme_texte(self):
        texte = {"description": "Use 12 steps and CFG 1.5"}
        self.assertEqual(steps.suggerer(texte)["valeurs"], [12])
        self.assertEqual(steps.suggerer_cfg(texte)["valeurs"], [1.5])


class Telechargement(unittest.TestCase):
    """Ce que le registre garde d'un checkpoint de famille téléchargé (telechargement.registre_du_checkpoint)."""

    def fiche(self, **options):
        from app.sources import Fiche
        return Fiche(source="civitai", nom=NEUF, taille=1, sha256=None, url="u", page="p",
                     dossier="video/MiniMax-H3/modeles", licence=None, jeton_requis=False, **options)

    def test_un_checkpoint_garde_ses_steps_a_valider_et_son_conseil(self):
        from app.telechargement import registre_du_checkpoint
        r = registre_du_checkpoint(self.fiche(steps_suggeres=UNIQUE, cfg_suggere=CFG), "video/MiniMax-H3/modeles")
        self.assertEqual(r["a_valider"], {"steps": UNIQUE})
        self.assertEqual(r["conseils"], {"steps": UNIQUE, "cfg": CFG})

    def test_sans_rien_lu_le_conseil_dit_aucun(self):
        from app.telechargement import registre_du_checkpoint
        r = registre_du_checkpoint(self.fiche(), "video/MiniMax-H3/modeles")
        self.assertEqual(r["conseils"]["cfg"]["statut"], "aucun")
        self.assertEqual(r["a_valider"]["steps"]["statut"], "aucun")

    def test_un_autre_fichier_n_a_rien_de_plus(self):
        from app.telechargement import registre_du_checkpoint
        for dossier in ("video/MiniMax-H3/loras", "image/_base/vae", "llm"):
            self.assertEqual(registre_du_checkpoint(self.fiche(steps_suggeres=UNIQUE), dossier), {}, dossier)


class GarderLeConseil(unittest.TestCase):
    def setUp(self):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        dossier = Path(d.name)
        for p in (reglages_jetables.patch(dossier),
                  mock.patch.object(reglages, "_cache", None),
                  mock.patch.object(modeles, "fichiers", side_effect=deposes),
                  mock.patch.object(modeles, "REGISTRE", dossier / "registre.json")):
            p.start()
            self.addCleanup(p.stop)

    def ecrire(self, entree):
        modeles.REGISTRE.write_text(json.dumps({CLE: {"source": "civitai", **entree}}), encoding="utf-8")

    def test_le_conseil_est_lu_au_registre(self):
        self.ecrire({"a_valider": {"steps": UNIQUE}, "conseils": {"steps": UNIQUE, "cfg": CFG}})
        self.assertEqual(modeles.conseils_page(CLE), {"steps": UNIQUE, "cfg": CFG})

    def test_il_survit_a_la_validation(self):
        self.ecrire({"a_valider": {"steps": UNIQUE}, "conseils": {"steps": UNIQUE, "cfg": CFG}})
        workflows.valider_checkpoint(CLE, 12)
        self.assertIsNone(modeles.suggestion_steps(CLE))  # la suggestion à valider a servi, comme avant
        self.assertEqual(modeles.conseils_page(CLE)["cfg"]["valeurs"], [1.5])  # le conseil reste lisible

    def test_avant_cette_version_seuls_les_steps_a_valider_sont_connus(self):
        self.ecrire({"a_valider": {"steps": UNIQUE}})
        self.assertEqual(modeles.conseils_page(CLE), {"steps": UNIQUE})
        workflows.valider_checkpoint(CLE, 8)
        self.assertIsNone(modeles.conseils_page(CLE))  # validé avant : plus rien, et aucun conseil inventé

    def test_un_fichier_inconnu_n_a_pas_de_conseil(self):
        self.ecrire({})
        self.assertIsNone(modeles.conseils_page(CLE))
        self.assertIsNone(modeles.conseils_page("video/MiniMax-H3/modeles/autre.safetensors"))

    def test_ma_collection_porte_le_conseil_d_un_checkpoint(self):
        self.ecrire({"a_valider": {"steps": UNIQUE}, "conseils": {"steps": UNIQUE, "cfg": CFG}})
        workflows.valider_checkpoint(CLE, 12)
        element = {"cle": CLE, "nom": NEUF, "dossier": "video/MiniMax-H3/modeles", "taille": 1, "suivi": True,
                   "infos": {"source": "civitai"}}
        with mock.patch.object(modeles, "inventaire", return_value=[element]), \
                mock.patch.object(modeles, "espace_libre", lambda: 0):
            groupes = collection.collection()["groupes"]
        e = next(x for g in groupes for x in g["elements"] if x["cle"] == CLE)
        self.assertEqual(e["steps"], 12)
        self.assertIsNone(e["a_valider"])
        self.assertEqual(e["conseils"]["cfg"]["valeurs"], [1.5])  # à lire à côté de son réglage

    def test_un_element_hors_famille_n_a_pas_de_conseil(self):
        element = {"cle": "llm/petit.gguf", "nom": "petit.gguf", "dossier": "llm", "taille": 1, "suivi": False,
                   "infos": {}}
        with mock.patch.object(modeles, "inventaire", return_value=[element]), \
                mock.patch.object(modeles, "espace_libre", lambda: 0):
            groupes = collection.collection()["groupes"]
        e = next(x for g in groupes for x in g["elements"] if x["cle"] == "llm/petit.gguf")
        self.assertIsNone(e["conseils"])


if __name__ == "__main__":
    unittest.main()
