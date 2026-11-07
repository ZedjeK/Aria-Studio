"""Séquence : musique d'ambiance du Scénario et absence de dialogue, données au découpage (sequences.decouper).
Les répliques des plans sont testées dans test_repliques_plans. Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio

from app import sequences
from tests.test_sequences import Base, FauxLlama, blocs

PLANS = [{"enchainement": "texte", "duree": 5, "note": ""}, {"enchainement": "couper", "duree": 5, "note": ""}]


class CasesDuScenario(Base):
    def decouper(self, reponse, synopsis="Une pilote rentre au hangar.", **options):
        llama = FauxLlama(reponse)
        r = asyncio.run(sequences.decouper(llama, synopsis, "", "", PLANS, None, **options))
        return r, llama.messages[0][1]["content"]

    def test_sans_replique_ni_musique(self):
        r, demande = self.decouper(blocs("She walks in.", 'She says "Bonsoir." and smiles.'))
        self.assertIn("Nobody speaks in the whole film", demande)
        self.assertIn("No background music.", demande)
        self.assertNotIn("Bonsoir", r["plans"][1]["prompt"])  # personne ne parle : une réplique est inventée

    def test_musique(self):
        _, demande = self.decouper(blocs("A.", "B."), musique="piano doux, tempo lent")
        self.assertIn("Background music for the whole film, the same in every shot: piano doux, tempo lent", demande)

    def test_une_replique_dans_l_histoire_vaut_dialogue(self):
        r, demande = self.decouper(blocs("She walks in.", 'She says "Bonsoir."'), synopsis="Elle entre et dit « Bonsoir. »")
        self.assertNotIn("Nobody speaks", demande)
        self.assertIn("Bonsoir", r["plans"][1]["prompt"])

    def test_longueur_de_la_musique(self):
        with self.assertRaisesRegex(sequences.ErreurSequence, "Musique de 200"):
            self.decouper(blocs("a", "b"), musique="x" * 201)
