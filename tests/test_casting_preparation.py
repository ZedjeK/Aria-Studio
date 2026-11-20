"""Casting : ce que ferait un casting, dit avant de le lancer (app/casting.py, preparation).

La page annonce qui serait comparé, sur quel jeu d'essai, et ce qui empêche de partir. Les règles sont celles du
lancement : `creer` refuse exactement quand `preparation` annonce un blocage, avec le même message. Rien n'est lancé :
workflows, prompts et dossiers sont factices.
"""

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from fastapi.testclient import TestClient

from app import casting, jeton, main
from app.config import CONFIG

LOCAL = f"http://127.0.0.1:{CONFIG['app']['port']}"
PROMPTS = {
    "texte_vers_image": [{"id": "portrait", "teste": "peau et regard", "prompt": "a portrait"},
                         {"id": "mains", "teste": "les mains", "prompt": "two hands"}],
    "retouche": [{"id": "veste", "teste": "un vêtement", "source": "portrait.png", "prompt": "red jacket"}],
    "reecriture": [{"id": "ramen", "teste": "l'idée ultra-courte", "prompt": "un ramen"}],
}


class Faux:
    """Un workflow du catalogue, réduit à ce que lit le Casting."""

    def __init__(self, ident: str, tache: str, nom: str, manquants: int = 0):
        self.id, self.fiche, self._manquants = ident, {"tache": tache, "nom": nom}, manquants

    def manquants(self) -> list[dict]:
        return [{"nom": f"fichier{i}"} for i in range(self._manquants)]


class Preparation(unittest.TestCase):
    def preparer(self, catalogue: list[Faux], tache: str, sources: Path | None = None) -> dict:
        with mock.patch.object(casting.workflows, "catalogue", return_value=catalogue), \
             mock.patch.object(casting, "jeu_de_prompts", side_effect=lambda t: PROMPTS[t]), \
             mock.patch.object(casting, "SOURCES", sources or Path(tempfile.gettempdir()) / "aucune-source"), \
             mock.patch.dict(CONFIG["casting"], {"seeds": [42, 1234]}):
            p = casting.preparation_de(tache)
            # Le lancement suit la même règle, avec le même message.
            if p["blocage"]:
                with self.assertRaises(casting.ErreurBanc) as refus:
                    casting.creer(tache)
                self.assertEqual(str(refus.exception), p["blocage"])
            else:
                self.assertEqual(casting.creer(tache).total, p["total"])
            return p

    def test_deux_modeles_prets(self):
        p = self.preparer([Faux("a", "texte_vers_image", "Modèle A"), Faux("b", "texte_vers_image", "Modèle B"),
                           Faux("c", "retouche", "Autre tâche")], "texte_vers_image")
        self.assertIsNone(p["blocage"])
        self.assertEqual([c["nom"] for c in p["candidats"]], ["Modèle A", "Modèle B"])
        self.assertTrue(all(c["manquants"] == 0 for c in p["candidats"]))
        self.assertEqual([e["id"] for e in p["essais"]], ["portrait", "mains"])
        self.assertEqual(p["essais"][0]["teste"], "peau et regard")
        self.assertEqual(p["seeds"], [42, 1234])
        self.assertEqual(p["total"], 2 * 2 * 2)  # modèles × prompts × seeds
        self.assertEqual(p["sources"], [])

    def test_un_seul_modele_n_a_rien_a_comparer(self):
        p = self.preparer([Faux("a", "texte_vers_image", "Modèle A")], "texte_vers_image")
        self.assertIn("au moins deux modèles", p["blocage"])
        self.assertEqual(len(p["candidats"]), 1)

    def test_un_modele_incomplet_est_nomme_par_son_nom(self):
        p = self.preparer([Faux("a", "texte_vers_image", "Modèle A"),
                           Faux("id_technique", "texte_vers_image", "Modèle B", manquants=2)], "texte_vers_image")
        self.assertIn("Modèle B n'est pas complet", p["blocage"])
        self.assertNotIn("id_technique", p["blocage"])
        self.assertEqual([c["manquants"] for c in p["candidats"]], [0, 2])

    def test_une_image_de_depart_absente_bloque(self):
        catalogue = [Faux("a", "retouche", "Retouche A"), Faux("b", "retouche", "Retouche B")]
        p = self.preparer(catalogue, "retouche")
        self.assertIn("portrait.png", p["blocage"])
        self.assertEqual(p["sources"], [{"nom": "portrait.png", "presente": False}])
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "portrait.png").write_bytes(b"png")
            p = self.preparer(catalogue, "retouche", Path(d))
        self.assertIsNone(p["blocage"])
        self.assertEqual(p["sources"], [{"nom": "portrait.png", "presente": True}])


class PreparationLLM(unittest.TestCase):
    def preparer(self, generation: dict, fichiers: list[str]) -> dict:
        rendu = Faux("rendu", "texte_vers_image", "Modèle de rendu")
        rendu.id = "rendu"
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "llm").mkdir()
            for nom in fichiers:
                (Path(d) / "llm" / nom).write_bytes(b"")
            with mock.patch.object(casting, "MODELES", Path(d)), \
                 mock.patch.object(casting, "jeu_de_prompts", side_effect=lambda t: PROMPTS[t]), \
                 mock.patch.object(casting.workflows, "obtenir", return_value=rendu), \
                 mock.patch.object(casting.fiches, "specialistes", return_value=[]), \
                 mock.patch.dict(CONFIG, {"generation": generation}), \
                 mock.patch.dict(CONFIG["casting"], {"candidats_llm": ["present.gguf", "absent.gguf", "temoin"],
                                                      "seed_llm": 7}):
                return casting.preparation_de("reecriture")

    def test_sans_modele_par_defaut_de_rendu(self):
        p = self.preparer({}, ["present.gguf"])
        self.assertIn("par défaut", p["blocage"])
        self.assertIsNone(p["rendu"])

    def test_un_modele_de_rendu_inconnu_bloque_sans_texte_d_exception(self):
        """Le message est écrit par le Casting : celui de l'exception ne part pas vers la page."""
        inconnu = casting.workflows.ErreurWorkflow("texte interne de l'exception")
        with mock.patch.object(casting, "jeu_de_prompts", side_effect=lambda t: PROMPTS[t]),              mock.patch.object(casting.workflows, "obtenir", side_effect=inconnu),              mock.patch.dict(CONFIG, {"generation": {"texte_vers_image": "disparu"}}):
            p = casting.preparation_de("reecriture")
            with self.assertRaises(casting.ErreurBanc) as refus:
                casting.creer("reecriture")
        self.assertEqual(p["blocage"], "Le modèle par défaut de Texte vers image est inconnu : disparu")
        self.assertEqual(str(refus.exception), p["blocage"])
        self.assertIsNone(p["rendu"])

    def test_la_preparation_ne_convertit_aucune_exception_en_texte(self):
        """Garde de l'alerte CodeQL py/stack-trace-exposure : entre `_candidats` et `preparation`, pas de str(e)."""
        source = Path(casting.__file__).read_text(encoding="utf-8")
        bloc = source[source.index("def _candidats("):source.index("class Banc:")]
        self.assertFalse("str(e)" in bloc)
        self.assertFalse("repr(e)" in bloc)

    def test_les_candidats_absents_du_disque_sont_ecartes(self):
        p = self.preparer({"texte_vers_image": "rendu"}, ["present.gguf"])
        self.assertIsNone(p["blocage"])
        self.assertEqual([c["nom"] for c in p["candidats"]], ["present", "Sans LLM (témoin)"])
        self.assertEqual(p["rendu"], {"id": "rendu", "nom": "Modèle de rendu", "manquants": 0})
        self.assertEqual(p["seeds"], [7])
        self.assertEqual(p["total"], 2 * 2 * 1)  # une réécriture puis un rendu, par candidat et par idée

    def test_le_temoin_seul_n_a_rien_a_comparer(self):
        p = self.preparer({"texte_vers_image": "rendu"}, [])
        self.assertIn("au moins deux candidats", p["blocage"])


class CatalogueReel(unittest.TestCase):
    def test_le_lancement_refuse_ce_que_la_preparation_bloque(self):
        """Sur le catalogue et la configuration livrés : mêmes refus, mêmes messages, pour chaque tâche."""
        taches = casting.preparation()["taches"]
        self.assertEqual(set(taches), set(casting.TACHES))
        for tache, p in taches.items():
            try:
                casting.creer(tache)
                refus = None
            except casting.ErreurBanc as e:
                refus = str(e)
            self.assertEqual(refus, p["blocage"], tache)

    def test_la_route_n_est_pas_prise_pour_un_casting(self):
        """« preparation » ne doit pas être lu comme l'identifiant d'un casting (/api/bancs/{id_banc})."""
        client = TestClient(main.app, base_url=LOCAL, headers={jeton.EN_TETE: jeton.JETON})
        r = client.get("/api/bancs/preparation")
        self.assertEqual(r.status_code, 200)
        self.assertEqual(set(r.json()["taches"]), set(casting.TACHES))
        self.assertEqual(r.json()["vram_max_occupee_go"], CONFIG["casting"]["vram_max_occupee_go"])


class ListeEtVue(unittest.TestCase):
    """La liste dit où en sont les notes ; un casting révélé dit quel modèle est par défaut aujourd'hui."""

    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.addCleanup(self.dossier.cleanup)
        racine = Path(self.dossier.name)
        (racine / "essai").mkdir()
        self.resultats = {
            "id": "essai", "tache": "texte_vers_image", "date": "2026-11-20T10:15:00", "seeds": [42],
            "prompts": PROMPTS["texte_vers_image"], "vram_base_go": 0.8,
            "workflows": {w: {"nom": w.upper(), "resume": None, "liberation": None, "generations": [
                {"prompt": p["id"], "seed": 42, "image": f"{w}/{p['id']}_42.png"} for p in PROMPTS["texte_vers_image"]]}
                for w in ("a", "b")},
            "aveugle": ["b", "a"], "revele": False, "notes": {"a/portrait_42.png": 4}, "favori": None,
        }
        self.ecrire()
        patch = mock.patch.object(casting, "DOSSIER", racine)
        patch.start()
        self.addCleanup(patch.stop)

    def ecrire(self):
        (Path(self.dossier.name) / "essai" / "resultats.json").write_text(json.dumps(self.resultats), encoding="utf-8")

    def test_la_liste_compte_les_notes(self):
        (banc,) = casting.liste()
        self.assertEqual((banc["notees"], banc["total"]), (1, 4))

    def test_le_modele_par_defaut_ne_se_dit_qu_apres_la_revelation(self):
        with mock.patch.dict(CONFIG, {"generation": {"texte_vers_image": "b"}}):
            self.assertIsNone(casting.vue("essai")["par_defaut"])  # à l'aveugle : rien ne doit trahir un modèle
            self.resultats["revele"] = True
            self.ecrire()
            self.assertEqual(casting.vue("essai")["par_defaut"], "b")
        with mock.patch.dict(CONFIG, {"generation": {"texte_vers_image": "ailleurs"}}):
            self.assertIsNone(casting.vue("essai")["par_defaut"])  # le défaut du moment n'est pas dans ce casting

    def test_le_defaut_du_moment_et_non_le_choix_fait_dans_ce_casting(self):
        self.resultats.update(revele=True, favori="a")  # choisi ici autrefois, changé depuis dans Modèles
        self.ecrire()
        with mock.patch.dict(CONFIG, {"generation": {"texte_vers_image": "b"}}):
            vue = casting.vue("essai")
        self.assertEqual((vue["favori"], vue["par_defaut"]), ("a", "b"))

    def test_le_llm_par_defaut_est_celui_que_l_appli_calcule(self):
        """Casting LLM : le défaut est `modeles.llm_defaut()` (avec son repli), pas la configuration brute. Le témoin,
        qui n'a pas de fichier, n'est jamais marqué."""
        self.resultats.update(tache="reecriture", revele=True)
        self.resultats["workflows"]["a"]["fichier"] = "a.gguf"
        self.resultats["workflows"]["b"]["fichier"] = None  # le témoin
        self.ecrire()
        with mock.patch.object(casting.modeles, "llm_defaut", return_value="a.gguf"):
            self.assertEqual(casting.vue("essai")["par_defaut"], "a")
        with mock.patch.object(casting.modeles, "llm_defaut", return_value="autre.gguf"):
            self.assertIsNone(casting.vue("essai")["par_defaut"])


if __name__ == "__main__":
    unittest.main()
