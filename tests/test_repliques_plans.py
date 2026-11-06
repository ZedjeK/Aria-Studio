"""Séquence : la réplique de chaque plan (mot pour mot, à son moment, voix off), ✨ d'un seul plan et 💬 Aide d'un
plan. Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio
import json
from unittest import mock

from fastapi import HTTPException

from app import main, plateau, prompt, sequences
from app.sequences import ErreurSequence
from tests.test_libre import FauxLlm
from tests.test_sequences import Base, FauxLlama, blocs

PLANS = [{"enchainement": "texte", "duree": 5, "note": "elle entre dans le hangar"},
         {"enchainement": "couper", "duree": 5, "note": "elle retire son casque", "replique": "« Tu m'as attendue ? »",
          "moment": "fin"},
         {"enchainement": "prolonger", "duree": 5, "note": "elle sourit"}]
REPLIQUE = "Tu m'as attendue ?"


class Repliques(Base):
    def decouper(self, reponse, plans=PLANS, **options):
        llama = FauxLlama(reponse)
        r = asyncio.run(sequences.decouper(llama, "", "", "", plans, None, **options))
        return r, llama.messages[0][1]["content"]

    def test_la_replique_du_plan_avec_son_moment(self):
        r, demande = self.decouper(blocs("She walks in.", f'She takes off her helmet and says "{REPLIQUE}"',
                                         "She smiles."))
        self.assertIn("Story: told by the shots below, in order.", demande)  # sans fil
        self.assertIn(f'2. cut, 5 s, note: elle retire son casque, line (French, word for word): "{REPLIQUE}", '
                      "spoken at the end of the shot, after the action, as a reaction", demande)
        self.assertNotIn("Nobody speaks", demande)
        self.assertEqual(r["plans"][1]["prompt"], f'She takes off her helmet and says "{REPLIQUE}"')
        self.assertEqual(r["alertes"], [])

    def test_replique_perdue_remise_a_son_moment(self):
        texte = "Medium shot of the hangar. She takes off her helmet. She smiles."
        dit = 'The character says: "Salut."'
        for moment, attendu in (
                ("debut", f"Medium shot of the hangar. {dit} She takes off her helmet. She smiles."),
                ("pendant", f"Medium shot of the hangar. She takes off her helmet. {dit} She smiles."),
                ("fin", f"Medium shot of the hangar. She takes off her helmet. She smiles. {dit}")):
            self.assertEqual(sequences.placer_replique(texte, "Salut.", moment, False), attendu, moment)
        self.assertEqual(sequences.placer_replique("She walks.", "Salut.", "debut", True),
                         'She walks. In an off-screen voiceover, the character says: "Salut." '
                         "The character's lips stay closed.")
        self.assertEqual(sequences.placer_replique('She says "Salut."', "Salut.", "debut", False), 'She says "Salut."')

    def test_voix_off_et_longueur(self):
        plans = [dict(p) for p in PLANS]
        plans[1] = {**plans[1], "off": True, "replique": " ".join(["mot"] * 14)}
        r, demande = self.decouper(blocs("A.", "B.", "C."), plans)
        self.assertIn("as an off-screen voiceover (the character's lips stay closed)", demande)
        self.assertIn("in an off-screen voiceover", r["plans"][1]["prompt"].lower())
        self.assertIn("Plan 2 : réplique de 14 mots pour 5 s (environ 12 au plus) : elle risque d'être pressée ou coupée",
                      r["alertes"])

    def test_inventee_retiree_celle_du_plan_gardee(self):
        r, _ = self.decouper(blocs('She walks in and says "Bonjour."', f'She says "{REPLIQUE}"', "She smiles."))
        self.assertNotIn("Bonjour", r["plans"][0]["prompt"])
        self.assertIn(REPLIQUE, r["plans"][1]["prompt"])

    def test_personne_ne_parle(self):
        _, demande = self.decouper(blocs("A.", "B.", "C."), [{**p, "replique": ""} for p in PLANS])
        self.assertIn("Nobody speaks in the whole film", demande)

    def test_validation(self):
        with self.assertRaisesRegex(ErreurSequence, "début, pendant l'action ou à la fin"):
            self.decouper(blocs("A.", "B.", "C."), [PLANS[0], {**PLANS[1], "moment": "apres"}, PLANS[2]])
        with self.assertRaisesRegex(ErreurSequence, "réplique de 300 caractères"):
            self.decouper(blocs("A.", "B.", "C."), [PLANS[0], {**PLANS[1], "replique": "x" * 301}, PLANS[2]])


class UnSeulPlan(Base):
    def test_ecrire_le_plan_2_seul(self):
        plans = [{**PLANS[0], "prompt": "Wide shot, a pilot walks into the hangar."}, PLANS[1],
                 {**PLANS[2], "prompt": "Continuous shot, same scene. She smiles."}]
        llama = FauxLlama("SHOT 2\nFR: elle retire son casque\nEN: Close-up, she takes off her helmet.")
        r = asyncio.run(sequences.decouper(llama, "Une pilote rentre.", "", "", plans, None, cible=1,
                                           contrainte="cheveux attachés",
                                           contrainte_en={"fr": "cheveux attachés", "en": "hair tied up"}))
        demande = llama.messages[0][1]["content"]
        self.assertIn("Write ONLY shot 2", demande)
        self.assertIn("Shot 1 as already written: Wide shot, a pilot walks into the hangar.", demande)
        self.assertIn("Shot 3 as already written: Continuous shot", demande)
        self.assertEqual((len(r["plans"]), r["cible"]), (1, 1))
        self.assertEqual(r["plans"][0]["prompt"], f'Close-up, she takes off her helmet. The character says: "{REPLIQUE}"')
        self.assertEqual(r["contrainte_en"], {"fr": "cheveux attachés", "en": "hair tied up"})  # pas retraduite
        self.assertEqual(len(llama.messages), 1)

    def test_bloc_mal_numerote_pris_quand_meme(self):
        self.assertEqual(sequences.lire_blocs("SHOT 1\nFR: a\nEN: b", [3], exact=False), [("a", "b")])
        self.assertEqual(sequences.lire_blocs("SHOT 1\nFR: a\nEN: b\nSHOT 3\nFR: c\nEN: d", [3], exact=False),
                         [("c", "d")])

    def test_plan_vide_sans_fil(self):
        with self.assertRaisesRegex(ErreurSequence, "Plan 3 : écris ce qui s'y passe"):
            asyncio.run(sequences.decouper(FauxLlama(), "", "", "", [PLANS[0], PLANS[1], {**PLANS[2], "note": ""}],
                                           None, cible=2))


class AideDUnPlan(Base):
    def appeler(self, reponse, **sequence):
        o = FauxLlm(json.dumps(reponse))
        with mock.patch.object(main, "orchestrateur", o), mock.patch.object(plateau, "textes_origine", lambda ids: []):
            r = asyncio.run(main.chat_libre(main.DemandeLibre(
                echanges=[{"role": "user", "content": "Elle est fatiguée"}], forme="plan", workflow="minimax_h3_t2v",
                sequence={"synopsis": "Une pilote rentre.", "plans": PLANS, "plan": 1, **sequence})))
        return r, o.appels[0][0][0]["content"]

    def test_le_chat_connait_la_sequence_et_garde_la_replique(self):
        tour = {"message": "ok", "fiche": {"Action": "elle retire son casque"}, "questions": [],
                "prompt": "Close-up, she takes off her helmet, tired."}
        r, systeme = self.appeler(tour)
        self.assertIn("ONE shot of a video sequence", systeme)
        self.assertIn("Write ONLY shot 2", systeme)
        self.assertIn(f'"{REPLIQUE}", spoken at the end of the shot', systeme)
        self.assertIn(prompt.REGLE_SANS_REFERENCE[:40], systeme)
        self.assertIn(prompt.ENTETE_REGLES, systeme)  # les règles de H3 écrivent ce prompt-là
        self.assertTrue(r["prompt"].endswith(f'The character says: "{REPLIQUE}"'))  # remise à la fin
        self.assertEqual(json.loads(r["brut"])["prompt"], r["prompt"])

    def test_sans_question_ni_prompt(self):
        r, _ = self.appeler({"message": "ok", "fiche": {}, "questions": [], "prompt": None})
        self.assertIsNone(r["prompt"])

    def test_sequence_obligatoire(self):
        with self.assertRaises(HTTPException):
            asyncio.run(main.chat_libre(main.DemandeLibre(echanges=[{"role": "user", "content": "x"}], forme="plan")))
