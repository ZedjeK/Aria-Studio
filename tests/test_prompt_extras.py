"""Tests de la sous-étape 3 de l'onglet Prompt : Peaufiner, traduction des précisions, personnages enregistrés.

Llama est remplacé par un faux : aucun modèle n'est chargé.
"""

import asyncio
import json
import tempfile
import unittest
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from app import listes, personnages, prompt
from app.listes import ErreurListes
from app.securite import ErreurSecurite


class FauxOrchestrateur:
    def __init__(self, reponse: str):
        self.reponse = reponse
        self.messages = None

    @asynccontextmanager
    async def llm(self, modele=None):
        async def generer(messages, **_):
            self.messages = messages
            return SimpleNamespace(texte=self.reponse)
        yield SimpleNamespace(generer=generer, modele=None)


def lancer(coroutine):
    return asyncio.run(coroutine)


class Peaufiner(unittest.TestCase):
    def test_renvoie_la_version_du_llm(self):
        o = FauxOrchestrateur("A cinematic film still of a young woman standing in the rain.")
        r = lancer(prompt.peaufiner(o, "Cinematic film still. A young woman in her early twenties, standing.", None, 120))
        self.assertEqual(r["prompt"], o.reponse)
        self.assertIn("At most 120 words", o.messages[0]["content"])

    def test_cible_video_consigne_video(self):
        o = FauxOrchestrateur('Medium shot. She whispers in French: "On décolle ce soir." Sounds: rain.')
        lancer(prompt.peaufiner(o, 'A woman. She says in French: "On décolle ce soir."', None, 110, duree_video=8))
        self.assertIn("Never translate the lines", o.messages[0]["content"])
        self.assertIn("(Clip: 8 s, from text only)", o.messages[1]["content"])

    def test_mineur_et_terme_sensible_ajoute_par_le_llm(self):
        o = FauxOrchestrateur("A cheerful young person in a swimsuit on the beach.")  # le LLM a « perdu » l'âge
        with self.assertRaises(ErreurSecurite):
            lancer(prompt.peaufiner(o, "A teenage girl, on a beach.", None))

    def test_entree_bloquee_avant_llama(self):
        o = FauxOrchestrateur("ignoré")
        with self.assertRaises(ErreurSecurite):
            lancer(prompt.peaufiner(o, "A teenage girl wearing a bikini.", None))
        self.assertIsNone(o.messages)  # Llama n'a même pas été appelé


class Traduire(unittest.TestCase):
    def test_traduction(self):
        o = FauxOrchestrateur("holding a red umbrella")
        self.assertEqual(lancer(prompt.traduire_precisions(o, "tient un parapluie rouge")), "holding a red umbrella")

    def test_as18_sur_le_couple(self):
        o = FauxOrchestrateur("a 12-year-old girl wearing lingerie")
        with self.assertRaises(ErreurSecurite):
            lancer(prompt.traduire_precisions(o, "une fille de 12 ans en tenue légère"))


class Personnages(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.patch = mock.patch.object(personnages, "FICHIER", Path(self.dossier.name) / "personnages.json")
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.dossier.cleanup()

    def test_enregistrer_lister_supprimer(self):
        personnages.enregistrer("Laura", {"Genre": "Femme", "Âge": "Ado (12-17 ans)", "Bas": ""}, 12)
        personnages.enregistrer("Aria", {"Genre": "Femme", "Origine": "Japonaise"}, None)
        self.assertEqual([p["nom"] for p in personnages.liste()], ["Aria", "Laura"])
        laura = personnages.liste()[1]
        self.assertEqual(laura["choix"], {"Genre": "Femme", "Âge": "Ado (12-17 ans)"})  # valeurs vides retirées
        self.assertEqual(laura["age"], 12)
        personnages.enregistrer("Aria", {"Genre": "Femme", "Origine": "Coréenne"}, None)  # écrase
        self.assertEqual(personnages.liste()[0]["choix"]["Origine"], "Coréenne")
        personnages.supprimer("Aria")
        self.assertEqual([p["nom"] for p in personnages.liste()], ["Laura"])
        with self.assertRaises(KeyError):
            personnages.supprimer("Aria")

    def test_anciennes_origines_converties(self):
        personnages.FICHIER.write_text(json.dumps({
            "Mai": {"choix": {"Genre": "Femme", "Origine": "Eurasienne franco-coréenne"}, "age": None},
            "Lin": {"choix": {"Genre": "Femme", "Origine": "Thaïlandaise"}, "age": 30},
            "Yuna": {"choix": {"Genre": "Femme", "Origine": "Coréenne"}, "age": None},
        }, ensure_ascii=False), encoding="utf-8")
        origines = {p["nom"]: p["choix"]["Origine"] for p in personnages.liste()}
        self.assertEqual(origines, {"Mai": "Eurasienne", "Lin": "Asiatique du Sud-Est", "Yuna": "Coréenne"})
        self.assertIn("Asiatique du Sud-Est", personnages.FICHIER.read_text(encoding="utf-8"))  # réécrit une fois

    def test_anciennes_silhouettes_converties(self):
        personnages.FICHIER.write_text(json.dumps({
            "Kim": {"choix": {"Genre": "Femme", "Silhouette": "Musclée de danseuse", "Origine": "Indienne"}, "age": None},
        }, ensure_ascii=False), encoding="utf-8")
        choix = personnages.liste()[0]["choix"]
        self.assertEqual((choix["Silhouette"], choix["Origine"]), ("Athlétique", "Sud-asiatique"))

    def test_anciennes_poses_et_actions_converties(self):
        personnages.FICHIER.write_text(json.dumps({
            "Ren": {"choix": {"Genre": "Femme", "Pose": "Assise en tailleur", "Action": "Aucune"}, "age": None},
            "Sora": {"choix": {"Genre": "Homme", "Action": "Écoute de la musique"}, "age": None},
        }, ensure_ascii=False), encoding="utf-8")
        choix = {p["nom"]: p["choix"] for p in personnages.liste()}
        self.assertEqual(choix["Ren"], {"Genre": "Femme", "Pose": "Assis(e) en tailleur"})  # « Aucune » retirée
        self.assertEqual(choix["Sora"]["Action"], "Écoute de la musique au casque")

    def test_toutes_les_cibles_existent(self):
        for sous_liste, correspondances in personnages.ANCIENNES_VALEURS.items():
            actuelles = set(listes.blocs()["personnage"].sous_listes[sous_liste].valeurs)
            self.assertLessEqual(set(correspondances.values()) - {None}, actuelles, sous_liste)
            self.assertFalse(set(correspondances) & actuelles, sous_liste)

    def test_regles_appliquees(self):
        with self.assertRaises(ErreurSecurite):
            personnages.enregistrer("X", {"Âge": "Ado (12-17 ans)", "Tenue intime": "Bikini"}, None)
        with self.assertRaises(ErreurSecurite):
            personnages.enregistrer("X", {"Pose": "Pose sensuelle"}, 14)
        with self.assertRaises(ErreurListes):
            personnages.enregistrer("X", {"Genre": "Homme", "Poitrine": "Menue"}, None)
        with self.assertRaises(ErreurListes):
            personnages.enregistrer("X", {"Coiffure": "Iroquoise"}, None)
        self.assertEqual(personnages.liste(), [])

    def test_noms_et_contenu(self):
        for nom in ("", "../x", "a" * 41, "<script>"):
            with self.assertRaises(ErreurListes):
                personnages.enregistrer(nom, {"Genre": "Femme"}, None)
        with self.assertRaises(ErreurListes):
            personnages.enregistrer("Vide", {}, None)
        self.assertEqual(personnages.enregistrer("Jean-Pierre O'Neil", {"Genre": "Homme"}, None)["nom"],
                         "Jean-Pierre O'Neil")


if __name__ == "__main__":
    unittest.main()
