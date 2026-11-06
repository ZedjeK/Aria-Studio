"""Scénario, onglet Libre (prompt.libre, POST /api/prompt/libre) : le LLM pose les questions essentielles, tient une
fiche de ce qu'il a compris, écrit le prompt sur demande. Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio
import json
import unittest
from contextlib import asynccontextmanager
from types import SimpleNamespace
from unittest import mock

from fastapi import HTTPException

from app import main, plateau, prompt
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

    def test_as18(self):
        with self.assertRaises(ErreurSecurite):
            tour(FauxLlm(json.dumps(TOUR)), [{"role": "user", "content": "Une adolescente de 15 ans en lingerie"}])
        with self.assertRaises(ErreurSecurite):  # un prompt du LLM qui ajoute le terme sensible est refusé aussi
            tour(FauxLlm(json.dumps({**TOUR, "prompt": "A 15-year-old girl in lingerie."})),
                 [{"role": "user", "content": "Une adolescente de 15 ans à la plage"}])


class Route(unittest.TestCase):
    def appeler(self, **champs):
        async def libre(*args, **_):
            return {"message": "ok", "fiche": {}, "questions": [], "prompt": None, "brut": "{}"}

        with mock.patch.object(main.prompt, "libre", libre), mock.patch.object(plateau, "textes_origine", lambda ids: []):
            return asyncio.run(main.chat_libre(main.DemandeLibre(**champs)))

    def test_conversation_controlee(self):
        self.assertEqual(self.appeler(echanges=[{"role": "user", "content": "Un chat"}])["message"], "ok")
        for echanges, forme in (([], "image"), ([{"role": "assistant", "content": "{}"}], "image"),
                                ([{"role": "system", "content": "x"}], "image"),
                                ([{"role": "user", "content": "x" * 5000}], "image"),
                                ([{"role": "user", "content": "Un chat"}], "sequence")):
            with self.assertRaises(HTTPException, msg=(echanges, forme)) as e:
                self.appeler(echanges=echanges, forme=forme)
            self.assertEqual(e.exception.status_code, 400)


if __name__ == "__main__":
    unittest.main()
