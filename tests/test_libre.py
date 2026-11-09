"""Scénario, onglet Libre (prompt.libre, POST /api/prompt/libre) : le LLM pose les questions essentielles, tient une
fiche de ce qu'il a compris, écrit le prompt sur demande. Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio
import json
import unittest
from contextlib import asynccontextmanager
from types import SimpleNamespace
from unittest import mock

from fastapi import HTTPException

from app import main, plateau, prompt, routes_scenario
from app.securite import ErreurSecurite

TOUR = {"message": "Je note le chat.", "fiche": {"Sujet": "un chat roux", "Réplique": "None"},
        "questions": [{"question": "Quel style ?", "choix": ["photo", "peinture", "aquarelle", "3D", "en trop"]}],
        "prompt": None}


class FauxLlm:
    def __init__(self, *reponses):
        self.reponses, self.appels = list(reponses), []

    @asynccontextmanager
    async def llm(self, modele=None):
        async def generer(messages, **options):
            self.appels.append((messages, options))
            return SimpleNamespace(texte=self.reponses.pop(0))
        yield SimpleNamespace(generer=generer, modele=None, vision=None)


def tour(o, echanges, **options):
    return asyncio.run(prompt.libre(o, echanges, options.pop("forme", "image"), options.pop("workflow", None), **options))


class Lecture(unittest.TestCase):
    def test_tour_lu_et_nettoye(self):
        t = prompt.lire_libre("<think>je réfléchis</think>\n" + json.dumps(TOUR))
        self.assertEqual(t["fiche"], {"Sujet": "un chat roux"})  # « None » écrit par le LLM n'est pas une réponse
        self.assertEqual(t["questions"][0]["choix"], ["photo", "peinture", "aquarelle", "3D"])  # 4 au plus
        self.assertIsNone(t["prompt"])

    def test_rubriques_vides_ecartees(self):
        t = prompt.lire_libre(json.dumps({**TOUR, "fiche": {"Sujet": "une pilote", "Décor": "Aucun choix fait pour le décor",
                                                            "Réplique": "Aucune réplique donnée"}}))
        self.assertEqual(t["fiche"], {"Sujet": "une pilote"})

    def test_illisible(self):
        with self.assertRaises(prompt.ErreurLibre):
            prompt.lire_libre("Voici mes questions : quel style ?")


class Tour(unittest.TestCase):
    def test_questions_sans_reflexion(self):
        o = FauxLlm(json.dumps(TOUR))
        r = tour(o, [{"role": "user", "content": "Un chat roux endormi"}])
        self.assertEqual(r["questions"][0]["question"], "Quel style ?")
        self.assertEqual(json.loads(r["brut"])["fiche"], {"Sujet": "un chat roux"})  # ce que l'interface renverra
        messages, options = o.appels[0]
        self.assertIs(options["reflechir"], False)
        self.assertIn("nothing yet", messages[0]["content"])
        self.assertIn("never « vous »", messages[0]["content"])
        self.assertTrue(messages[0]["content"].endswith(prompt.CLAUSE_MINEURS))

    def test_ecrire_reflechit(self):
        o = FauxLlm(json.dumps({**TOUR, "questions": [], "prompt": "A ginger cat asleep, **photo**."}))
        r = tour(o, [{"role": "user", "content": "Un chat"}, {"role": "assistant", "content": json.dumps(TOUR)},
                     {"role": "user", "content": prompt.ECRIRE}], contexte="A cat, at night.")
        self.assertEqual(r["prompt"], "A ginger cat asleep, photo.")  # sans markdown
        messages, options = o.appels[0]
        self.assertIs(options["reflechir"], True)
        self.assertIn("A cat, at night.", messages[0]["content"])  # ce que les autres onglets fixent

    def test_json_redemande_une_fois(self):
        o = FauxLlm("Quel style veux-tu ?", json.dumps(TOUR))
        tour(o, [{"role": "user", "content": "Un chat"}])
        self.assertEqual(len(o.appels), 2)
        self.assertIn("ONLY with the JSON", o.appels[1][0][-1]["content"])
        with self.assertRaises(prompt.ErreurLibre):
            tour(FauxLlm("non", "toujours non"), [{"role": "user", "content": "Un chat"}])

    def test_clip_avec_le_rush(self):
        o = FauxLlm(json.dumps(TOUR))
        vue = prompt.Vue(b"PNG", "An orange cat asleep on a windowsill.")
        tour(o, [{"role": "user", "content": "Il se réveille"}], forme="clip", workflow="minimax_h3_t2v",
             vues=[vue], role="animer", duree=8)
        systeme, premier = o.appels[0][0][0]["content"], o.appels[0][0][1]["content"]
        self.assertIn("8-second video clip with sound", systeme)
        self.assertIn("Caméra, Son, Réplique", systeme)
        self.assertIn("An orange cat asleep", premier)  # sans vision : la fiche de l'image

    def test_les_images_selon_leur_role(self):
        """Le chat regarde les images de référence et s'en sert comme le modèle cible les recevra."""
        vue = prompt.Vue(b"PNG", "A woman with purple hair in a library.")
        for role, attendu in (("inspirer", "refer to them by number"), ("variation", "starting image"),
                              ("animer", "first frame of the clip")):
            o = FauxLlm(json.dumps(TOUR))
            tour(o, [{"role": "user", "content": "Mets-la sur une plage"}], vues=[vue], role=role)
            self.assertIn(attendu, o.appels[0][0][0]["content"], role)
            self.assertIn("(vu sur l'image)", o.appels[0][0][0]["content"], role)
        o = FauxLlm(json.dumps(TOUR))
        tour(o, [{"role": "user", "content": "Un chat"}], role="inspirer")  # sans image : rien à regarder
        self.assertNotIn("Picture 1", o.appels[0][0][0]["content"])

    def test_as18(self):
        with self.assertRaises(ErreurSecurite):
            tour(FauxLlm(json.dumps(TOUR)), [{"role": "user", "content": "Une adolescente de 15 ans en lingerie"}])
        with self.assertRaises(ErreurSecurite):  # un prompt du LLM qui ajoute le terme sensible est refusé aussi
            tour(FauxLlm(json.dumps({**TOUR, "prompt": "A 15-year-old girl in lingerie."})),
                 [{"role": "user", "content": "Une adolescente de 15 ans à la plage"}])


class Ateliers(unittest.TestCase):
    def test_retouche_et_sequence(self):
        """Retouche : une consigne d'édition en anglais ; Séquence : l'histoire, en français."""
        o = FauxLlm(json.dumps(TOUR))
        tour(o, [{"role": "user", "content": "Rends le ciel orange"}], forme="retouche", workflow="qwen_image_edit_2511",
             vues=[prompt.Vue(b"PNG", "A cat on a windowsill at night.")], role="retouche")
        systeme = o.appels[0][0][0]["content"]
        self.assertIn("instruction-based image editing model", systeme)
        self.assertIn("the image to edit", systeme)
        self.assertIn('Write "prompt" (in English)', systeme)
        o = FauxLlm(json.dumps(TOUR))
        tour(o, [{"role": "user", "content": "Une pilote rentre au hangar"}], forme="sequence",
             workflow="minimax_h3_t2v")
        systeme = o.appels[0][0][0]["content"]
        self.assertIn("the STORY of a short video sequence", systeme)
        self.assertIn('Write "prompt" (in French: the whole story', systeme)
        self.assertNotIn("At most", systeme)
        self.assertIn("no shots, no camera directions", systeme)
        self.assertNotIn(prompt.ENTETE_REGLES, systeme)  # les règles des prompts de H3 ne font pas l'histoire


class Optimiser(unittest.TestCase):
    def appeler(self, **champs):
        appels = []

        async def reecrire(orchestrateur, idee, llm, references=0, **options):
            appels.append(("reecrire", idee, references, options))
            return {"prompt": "A ginger cat asleep.", "duree_s": 1.0}

        async def traduire_instruction(orchestrateur, instruction, llm, regles, workflow, vue):
            appels.append(("retouche", instruction, vue))
            return "Make the sky orange; keep everything else unchanged."

        with (mock.patch.object(prompt, "reecrire", reecrire),
              mock.patch.object(prompt, "traduire_instruction", traduire_instruction),
              mock.patch.object(plateau, "vue", lambda i: f"vue:{i}"),
              mock.patch.object(plateau, "textes_origine", lambda ids: [])):
            r = asyncio.run(routes_scenario.optimiser_prompt(routes_scenario.DemandeOptimisation(**champs)))
        return r, appels[0]

    def test_image_avec_les_complements(self):
        r, (quoi, idee, refs, options) = self.appeler(atelier="image", idee="un chat roux", complements="A cat, at night.")
        self.assertEqual((quoi, refs, options["video"]), ("reecrire", 0, None))
        self.assertIn("Also fixed (keep it): A cat, at night.", idee)
        self.assertEqual(r["mots"], 4)

    def test_clip_anime_le_rush(self):
        _, (_, _, _, options) = self.appeler(atelier="clip", idee="il se réveille", images=["r1"], role="animer")
        self.assertEqual((options["video"], options["vues"]), ("animer", ["vue:r1"]))

    def test_retouche_voit_son_image(self):
        r, (quoi, instruction, vue) = self.appeler(atelier="retouche", idee="ciel orange", images=["r1"],
                                                  role="retouche")
        self.assertEqual((quoi, vue), ("retouche", "vue:r1"))
        self.assertIn("keep everything else", r["prompt"])
        with self.assertRaises(HTTPException):
            self.appeler(atelier="retouche", idee="ciel orange")  # sans image


class Route(unittest.TestCase):
    def appeler(self, **champs):
        async def libre(*args, **_):
            return {"message": "ok", "fiche": {}, "questions": [], "prompt": None, "brut": "{}"}

        with mock.patch.object(prompt, "libre", libre), mock.patch.object(plateau, "textes_origine", lambda ids: []):
            return asyncio.run(routes_scenario.chat_libre(routes_scenario.DemandeLibre(**champs)))

    def test_conversation_controlee(self):
        self.assertEqual(self.appeler(echanges=[{"role": "user", "content": "Un chat"}])["message"], "ok")
        for echanges, forme in (([], "image"), ([{"role": "assistant", "content": "{}"}], "image"),
                                ([{"role": "system", "content": "x"}], "image"),
                                ([{"role": "user", "content": "x" * 5000}], "image"),
                                ([{"role": "user", "content": "Un chat"}], "video")):
            with self.assertRaises(HTTPException, msg=(echanges, forme)) as e:
                self.appeler(echanges=echanges, forme=forme)
            self.assertEqual(e.exception.status_code, 400)


if __name__ == "__main__":
    unittest.main()


class FormatDuScenario(unittest.TestCase):
    """Le format choisi au Scénario est dit au LLM, et seulement à lui (image, clip)."""

    def test_orientation(self):
        self.assertIn("landscape", prompt.cadre_format("16:9 (Widescreen)"))
        self.assertIn("portrait", prompt.cadre_format("9:16 (Portrait Widescreen)"))
        self.assertIn("square", prompt.cadre_format("1:1 (Square)"))
        self.assertEqual(prompt.cadre_format(None), "")
        self.assertEqual(prompt.cadre_format("n'importe quoi"), "")

    def test_dans_la_consigne_image_et_clip_pas_retouche(self):
        for forme, attendu in (("image", True), ("clip", True), ("retouche", False)):
            o = FauxLlm(json.dumps(TOUR))
            tour(o, [{"role": "user", "content": "Un chat"}], forme=forme, format_="3:2 (Photo)")
            self.assertEqual("3:2 (Photo)" in o.appels[0][0][0]["content"], attendu, forme)

    def test_route_refuse_un_format_inconnu(self):
        with self.assertRaises(HTTPException) as e:
            asyncio.run(routes_scenario.chat_libre(routes_scenario.DemandeLibre(
                echanges=[routes_scenario.EchangeLibre(role="user", content="Un chat")], format="7:5")))
        self.assertEqual(e.exception.status_code, 400)
