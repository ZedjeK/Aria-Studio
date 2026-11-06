"""Séquence : cases 🗣 Dialogue et 🎵 Musique d'ambiance du Scénario, données au découpage (sequences.decouper).
Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio

from app import sequences
from tests.test_sequences import Base, FauxLlama, blocs

PLANS = [{"enchainement": "texte", "duree": 5, "note": ""}, {"enchainement": "couper", "duree": 5, "note": ""}]


class CasesDuScenario(Base):
    def decouper(self, reponse, **options):
        llama = FauxLlama(reponse)
        r = asyncio.run(sequences.decouper(llama, "Une pilote rentre au hangar.", "", "", PLANS, None, **options))
        return r, llama.messages[0][1]["content"]

    def test_sans_dialogue_ni_musique(self):
        r, demande = self.decouper(blocs("She walks in.", 'She says "Bonsoir." and smiles.'), dialogue=False,
                                   lignes=["ignorée"])
        self.assertIn("Nobody speaks in the whole film", demande)
        self.assertIn("No background music.", demande)
        self.assertNotIn("Bonsoir", r["plans"][1]["prompt"])  # une réplique sans dialogue est inventée : retirée

    def test_repliques_et_musique(self):
        r, demande = self.decouper(blocs('She looks up and says "On décolle ce soir."', "She smiles."),
                                   dialogue=True, lignes=["« On décolle ce soir. »", "À demain."],
                                   musique="piano doux, tempo lent")
        self.assertIn('"On décolle ce soir." (French)', demande)
        self.assertIn('"À demain." (French)', demande)
        self.assertIn("Background music for the whole film, the same in every shot: piano doux, tempo lent", demande)
        self.assertIn('"On décolle ce soir."', r["plans"][0]["prompt"])  # gardée : elle vient des cases
        self.assertIn("Réplique « À demain. » absente du découpage : ajoute-la au plan voulu", r["alertes"])

    def test_une_replique_dans_l_histoire_vaut_dialogue(self):
        llama = FauxLlama(blocs("She walks in.", 'She says "Bonsoir."'))
        asyncio.run(sequences.decouper(llama, "Elle entre et dit « Bonsoir. »", "", "", PLANS, None, dialogue=False))
        self.assertNotIn("Nobody speaks", llama.messages[0][1]["content"])

    def test_longueurs(self):
        with self.assertRaisesRegex(sequences.ErreurSequence, "trop longues"):
            self.decouper(blocs("a", "b"), lignes=["x" * 601])
