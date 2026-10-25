"""Tests de la contrainte du casting et de « la référence sur tous les plans » (séquences du Montage).

Rien n'est envoyé à ComfyUI ni à Llama ; séquences, rushes et clips vivent dans un dossier temporaire.
"""

import asyncio
from contextlib import asynccontextmanager
from types import SimpleNamespace

from app import plateau, sequences
from app.plateau import Plateau
from app.securite import ErreurSecurite
from app.sequences import ErreurSequence
from tests.test_sequences import Base, FauxLlama, blocs, png

CONTRAINTE = "elle garde sa combinaison bleue et les cheveux attachés"
EN = "she keeps her blue flight suit and her hair tied back"


class Contrainte(Base):
    def test_validation_et_valeurs_par_defaut(self):
        s = self.creer()
        self.assertEqual((s["casting"]["contrainte"], s["casting"]["reference_partout"]), ("", True))
        s = self.creer(casting={"contrainte": f"  {CONTRAINTE}  ", "reference_partout": False})
        self.assertEqual((s["casting"]["contrainte"], s["casting"]["reference_partout"]), (CONTRAINTE, False))
        with self.assertRaisesRegex(ErreurSequence, "Contrainte de 300"):
            self.creer(casting={"contrainte": "x" * 301})

    def test_as18(self):
        ado = plateau.enregistrer(png(), {"type": "creation", "idee": "une adolescente de 15 ans"})
        with self.assertRaises(ErreurSecurite):
            self.creer(casting={"personnage": ado, "contrainte": "elle se déshabille dans chaque plan"})
        self.creer(casting={"personnage": ado, "contrainte": "uniforme scolaire bleu marine"})

    def test_ancienne_sequence_pas_a_retourner(self):
        """Une séquence d'avant les contraintes (casting sans les nouveaux champs) ne change pas en l'enregistrant."""
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        for cle in sequences.CASTING_DEFAUT:
            del s["casting"][cle]
        sequences._ecrire(s)
        envoi = {**s, "casting": {"personnage": None, "voix": "auto", "ambiance": s["casting"]["ambiance"]}}
        self.assertEqual(sequences.modifier(s["id"], envoi)["a_tourner"], [])

    def test_changer_la_contrainte_retourne_tout(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        s = sequences.modifier(s["id"], {**s, "casting": {**s["casting"], "contrainte": CONTRAINTE}})
        self.assertEqual(s["a_tourner"], [0, 1, 2, 3])

    def test_donnee_a_llama(self):
        s = self.creer(casting={"contrainte": CONTRAINTE})
        for k in range(4):
            self.assertIn(f"CONTRAINTE OBLIGATOIRE de toute la séquence, à respecter aussi dans ce plan : {CONTRAINTE}",
                          sequences.idee_pour_llm(s, k))
        self.assertNotIn("CONTRAINTE", sequences.idee_pour_llm(self.creer(), 0))

    def test_ajoutee_au_prompt(self):
        self.assertEqual(sequences.avec_contrainte("She smiles.", EN + "."),
                         f"She smiles. Constraint for every shot: {EN}.")
        self.assertEqual(sequences.avec_contrainte("She smiles.", None), "She smiles.")

    def test_traduite_une_seule_fois(self):
        s = sequences.lire(self.creer(casting={"contrainte": CONTRAINTE})["id"])
        llama = FauxLlama(f"**{EN}**")
        self.assertEqual(asyncio.run(sequences.traduire_contrainte(llama, s)), EN)
        self.assertEqual(asyncio.run(sequences.traduire_contrainte(llama, s)), EN)  # pas de 2e appel
        self.assertEqual(len(llama.messages), 1)
        sequences._ecrire(s)
        # Enregistrer sans changer la contrainte garde la traduction ; la changer l'oublie.
        s = sequences.modifier(s["id"], {**s, "titre": "Autre titre"})
        self.assertEqual(sequences.lire(s["id"])["contrainte_en"]["en"], EN)
        s = sequences.modifier(s["id"], {**s, "casting": {**s["casting"], "contrainte": "pluie continue"}})
        self.assertNotIn("contrainte_en", sequences.lire(s["id"]))


class ReferencePartout(Base):
    def setUp(self):
        super().setUp()
        self.s = self.tout_a_jour(sequences.lire(self.creer()["id"]))

    def noeuds(self, api, type_):
        return [(n, v) for n, v in api.items() if v["class_type"] == type_]

    def test_qui_recoit_quoi(self):
        s = self.s  # départ du rush (parle), prolonger, couper, transition
        self.assertEqual([sequences.references(s, k)[0] for k in range(4)], [self.pilote, None, self.pilote, self.pilote])
        s["casting"]["reference_partout"] = False
        self.assertEqual([sequences.references(s, k)[0] for k in range(4)], [None, None, self.pilote, None])
        s["plans"][3]["idee"] = "elle se retourne et dit « À demain. »"  # une transition qui parle reçoit la voix
        self.assertEqual(sequences.references(s, 3)[1], s["plans"][0]["clip"])
        s["plans"][1]["idee"] = "elle dit « Encore un peu. »"  # un plan prolongé : jamais (le son ancré suffit)
        self.assertEqual(sequences.references(s, 1), (None, None))

    def test_graphe_transition_avec_reference(self):
        api = sequences.graphe_plan(self.s, 3, "The camera rises.", 42,
                                    {"rush": "r.png", "precedent": "p.mp4", "personnage": "perso.png"})
        self.assertFalse(self.noeuds(api, "MiniMaxH3ImageToVideo"))
        (ref_id, ref), = self.noeuds(api, "MiniMaxH3ReferenceToVideo")
        self.assertEqual(api[ref["inputs"]["ref_images.ref_image_0"][0]]["inputs"]["image"], "perso.png")
        ancres = dict(self.noeuds(api, "MiniMaxH3AddGuide"))
        debut, fin = ancres["aria_ancre_0"]["inputs"], ancres["aria_ancre_1"]["inputs"]
        self.assertEqual((debut["frame_idx"], debut["positive"]), (0, [ref_id, 0]))
        self.assertEqual(api[debut["image"][0]]["inputs"]["batch_index"], 191)  # dernière image du plan 3 (8 s)
        self.assertEqual((fin["frame_idx"], fin["positive"]), (-1, ["aria_ancre_0", 0]))
        self.assertEqual(api[fin["image"][0]]["inputs"]["image"], "r.png")  # le rush d'arrivée
        guider = self.noeuds(api, "BasicGuider")[0][1]["inputs"]
        self.assertEqual(guider["conditioning"], ["aria_ancre_1", 0])

    def test_graphe_depart_avec_reference(self):
        api = sequences.graphe_plan(self.s, 0, "She looks up.", 42, {"rush": "r.png", "personnage": "perso.png"})
        self.assertTrue(self.noeuds(api, "MiniMaxH3ReferenceToVideo"))
        (_, ancre), = self.noeuds(api, "MiniMaxH3AddGuide")
        self.assertEqual((ancre["inputs"]["frame_idx"], api[ancre["inputs"]["image"][0]]["inputs"]["image"]),
                         (0, "r.png"))

    def test_plan_prolonge_toujours_sans_reference(self):
        api = sequences.graphe_plan(self.s, 1, "She stands up.", 42,
                                    {"precedent": "p.mp4", "personnage": "perso.png"})
        self.assertTrue(self.noeuds(api, "MiniMaxH3ImageToVideo"))
        self.assertFalse(self.noeuds(api, "MiniMaxH3ReferenceToVideo"))


class Tournage(Base):
    """Le vrai sequences.tourner, avec un faux ComfyUI et un faux Llama (seule la contrainte est traduite :
    les prompts sont imposés)."""

    def test_contrainte_et_references_dans_chaque_plan(self):
        plans = self.plans()
        for p in plans:
            p["prompt"] = f"{p['idee']} (imposed)"
        s = self.creer(plans=plans, casting={"personnage": self.pilote, "contrainte": CONTRAINTE})
        recus, televerses = [], []

        class FauxComfy:
            async def televerser(self, chemin):
                televerses.append(chemin.name)
                return chemin.name

            async def executer(self, api, progression=None):
                recus.append(api)
                return [("clip.mp4", b"mp4"), ("affiche.png", png(8, 8))]

        @asynccontextmanager
        async def comfyui():
            yield FauxComfy()

        llama = FauxLlama(EN)
        p = Plateau(orchestrateur=SimpleNamespace(comfyui=comfyui, llm=llama.llm))
        c = p.commander_sequence(s["id"])
        asyncio.run(sequences.tourner(p, c))
        s = sequences.lire(s["id"])
        self.assertTrue(all(pl["prompt_final"].endswith(f"Constraint for every shot: {EN}.") for pl in s["plans"]))
        self.assertEqual(s["contrainte_en"], {"fr": CONTRAINTE, "en": EN})
        # Plans 1 (départ), 3 (coupé) et 4 (transition) en ReferenceToVideo ; le plan 2 (prolongé) non.
        r2v = ["MiniMaxH3ReferenceToVideo" in {v["class_type"] for v in api.values()} for api in recus[:4]]
        self.assertEqual(r2v, [True, False, True, True])
        prompts = [plateau.fiche(pl["clip"])["prompt"] for pl in s["plans"]]
        self.assertEqual(["<Picture 1>" in pr for pr in prompts], [True, False, True, True])
        self.assertTrue(s["film"])


class DecoupageAvecContrainte(Base):
    PLANS = [{"enchainement": "texte", "duree": 5, "note": ""}, {"enchainement": "couper", "duree": 5, "note": ""}]

    def test_contrainte_envoyee_et_rendue(self):
        llama = FauxLlama(blocs("She walks.", "She smiles."))
        r = asyncio.run(sequences.decouper(llama, "Une pilote marche.", "", "", self.PLANS, None, None, False,
                                           f"  {CONTRAINTE} "))
        self.assertEqual(r["contrainte"], CONTRAINTE)
        self.assertIn(f"Mandatory constraint, respected in EVERY shot: {CONTRAINTE}", llama.messages[0][1]["content"])

    def test_as18_et_longueur(self):
        with self.assertRaises(ErreurSecurite):
            asyncio.run(sequences.decouper(FauxLlama(), "Elle danse.", "", "", self.PLANS, None, None, True,
                                           "elle se déshabille"))
        with self.assertRaisesRegex(ErreurSequence, "Contrainte de 300"):
            asyncio.run(sequences.decouper(FauxLlama(), "Elle danse.", "", "", self.PLANS, None, None, False,
                                           "x" * 301))
