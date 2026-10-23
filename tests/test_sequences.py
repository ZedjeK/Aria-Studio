"""Tests du mode Séquence du Montage : calculs, validation, AS-18, plans à retourner, graphes ComfyUI, file.

Rien n'est envoyé à ComfyUI ni à Llama ; séquences, rushes et clips vivent dans un dossier temporaire.
"""

import io
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from PIL import Image

from app import plateau, rushes, sequences
from app.plateau import ErreurPlateau, Plateau
from app.securite import ErreurSecurite
from app.sequences import ErreurSequence


def png(largeur=832, hauteur=1248) -> bytes:
    sortie = io.BytesIO()
    Image.new("RGB", (largeur, hauteur), (40, 30, 20)).save(sortie, "PNG")
    return sortie.getvalue()


class Base(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(sequences, "DOSSIER", d / "sequences"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json")]
        for p in self.patchs:
            p.start()
        self.pilote = plateau.enregistrer(png(), {"type": "creation", "idee": "une pilote", "prompt": "a pilot"})
        self.neon = plateau.enregistrer(png(1024, 1024), {"type": "creation", "idee": "une enseigne néon"})

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def plans(self):
        return [
            {"idee": "elle lève les yeux et dit « On décolle ce soir. »", "duree": 5, "enchainement": "rush",
             "rush": self.pilote},
            {"idee": "elle se lève et marche vers la porte", "duree": 5, "enchainement": "prolonger"},
            {"idee": "gros plan dans le cockpit", "duree": 8, "enchainement": "couper"},
            {"idee": "la caméra recule vers la rue", "duree": 5, "enchainement": "transition", "rush": self.neon},
        ]

    def creer(self, **autres):
        return sequences.creer({"titre": "Décollage", "qualite": "rapide", "plans": self.plans(),
                                "casting": {"ambiance": "hangar de nuit, lumière orange"}, **autres})

    @staticmethod
    def tout_a_jour(s):
        for k, p in enumerate(s["plans"]):
            p.update(clip=plateau.enregistrer_clip(b"mp4", png(8, 8), {"type": "clip", "idee": p["idee"]}), a_jour=True)
        sequences._ecrire(s)
        return s


class Calculs(Base):
    def test_images_sur_la_grille_de_h3(self):
        self.assertEqual([sequences.images(d) for d in (5, 8, 10, 15)], [124, 192, 243, 362])
        self.assertTrue(all((sequences.images(d) - 5) % 17 == 0 for d in (5, 8, 10, 15)))

    def test_dimensions_comme_comfyui(self):
        self.assertEqual(sequences.dimensions(832 / 1248, 0.4), (544, 800))  # mesuré pendant les essais
        s = self.creer()
        self.assertEqual(sequences.dimensions_sequence(s), (544, 800))  # le rush de départ décide
        s["plans"][0].update(enchainement="texte", rush=None)
        self.assertEqual(sequences.dimensions_sequence(s), sequences.dimensions(16 / 9, 0.4))

    def test_estimation(self):
        s = self.creer()
        self.assertEqual(s["a_tourner"], [0, 1, 2, 3])
        self.assertEqual(s["duree_totale"], 23)
        self.assertGreater(s["estimation_s"], 4 * 80)


class Reecriture(unittest.TestCase):
    def test_replique_perdue_remise_mot_pour_mot(self):
        idee = "elle lève les yeux et dit « On décolle ce soir. »"
        perdu = "A medium close-up of the pilot, her voice calm yet steady."
        self.assertEqual(sequences.garder_repliques(idee, perdu),
                         'A medium close-up of the pilot, her voice calm yet steady. The character says: "On décolle ce soir."')
        garde = 'She says in French: "On décolle, ce soir !"'  # ponctuation retouchée : la réplique est là
        self.assertEqual(sequences.garder_repliques(idee, garde), garde)
        self.assertEqual(sequences.repliques('il dit "Hop" puis “Allez”'), ["Hop", "Allez"])

    def test_markdown_retire(self):
        from app import prompt
        self.assertEqual(prompt.nettoyer_markdown('She says: **"Moteurs au vert."**\n\nA *click click*.\n# Audio'),
                         'She says: "Moteurs au vert." A click click. Audio')


class TexteEnvoyeALlama(Base):
    def test_ambiance_en_toile_de_fond(self):
        s = self.creer()
        texte = sequences.idee_pour_llm(s, 2)
        self.assertTrue(texte.startswith("gros plan dans le cockpit"))
        self.assertIn("l'idée gagne", texte)
        self.assertIn("hangar de nuit", texte)
        self.assertTrue(texte.endswith(sequences.CONSIGNE_SEQUENCE))
        self.assertNotIn("TRANSITION", texte)

    def test_plan_qui_ne_suit_pas_l_ambiance(self):
        plans = self.plans()
        plans[2]["ambiance"] = False
        s = self.creer(plans=plans)
        self.assertNotIn("hangar de nuit", sequences.idee_pour_llm(s, 2))
        self.assertIn("hangar de nuit", sequences.idee_pour_llm(s, 1))
        self.assertIn("ONE continuous take", sequences.idee_pour_llm(s, 2))

    def test_decocher_l_ambiance_retourne_le_plan(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        for p in s["plans"]:
            p.pop("ambiance")  # séquence enregistrée avant la case : « suit l'ambiance » par défaut
        sequences._ecrire(s)
        self.assertEqual(sequences.modifier(s["id"], s)["a_tourner"], [])
        plans = [dict(p) for p in s["plans"]]
        plans[2]["ambiance"] = False
        self.assertEqual(sequences.modifier(s["id"], {**s, "plans": plans})["a_tourner"], [2, 3])

    def test_transition_connait_son_depart_et_son_arrivee(self):
        s = self.creer()
        s["plans"][2]["prompt_final"] = "Close-up in a green-lit cockpit, the character flips two switches."
        texte = sequences.idee_pour_llm(s, 3)
        self.assertIn("starts exactly on the last image of the previous shot: Close-up in a green-lit cockpit", texte)
        self.assertIn("ends exactly on this picture: une enseigne néon", texte)  # l'idée d'origine du rush
        self.assertIn("progressively over the whole clip", texte)
        s["plans"][2]["prompt_final"] = None  # pas encore réécrit : son idée
        self.assertIn("previous shot: gros plan dans le cockpit", sequences.idee_pour_llm(s, 3))


class Validation(Base):
    def test_creation_et_dossier_des_bobines(self):
        s = self.creer()
        self.assertEqual(s["dossier"], "Séquence Décollage")
        self.assertIn("Séquence Décollage", rushes.dossiers())
        self.assertEqual(sequences.lire(s["id"])["plans"][1]["rush"], None)
        self.assertEqual(len({p["id"] for p in s["plans"]}), 4)

    def test_refus(self):
        cas = [
            ({"plans": []}, "De 1 à 12 plans"),
            ({"plans": [{"idee": "x", "enchainement": "couper"}]}, "enchaînement possible : rush, texte"),
            ({"plans": [{"idee": "x", "enchainement": "rush"}]}, "rush de départ"),
            ({"plans": [{"idee": "", "enchainement": "texte"}]}, "écris l'idée"),
            ({"plans": [{"idee": "x", "enchainement": "texte", "duree": 7}]}, "durée"),
            ({"plans": [{"idee": "x", "enchainement": "texte"}] * 13}, "De 1 à 12 plans"),
            ({"qualite": "ultra"}, "Qualité"),
            ({"titre": ""}, "Titre"),
        ]
        for autres, message in cas:
            with self.assertRaisesRegex(ErreurSequence, message, msg=autres):
                self.creer(**autres)

    def test_brouillon_sans_rush_d_arrivee(self):
        s = self.creer(plans=[{"idee": "x", "enchainement": "texte"}, {"idee": "y", "enchainement": "transition"}])
        self.assertIsNone(s["plans"][1]["rush"])
        with mock.patch.object(Plateau, "_valider_llm", return_value="x.gguf"):
            with self.assertRaisesRegex(ErreurSequence, "Plan 2 : choisis le rush d'arrivée"):
                Plateau(orchestrateur=None).commander_sequence(s["id"])

    def test_un_clip_n_est_pas_un_rush(self):
        clip = plateau.enregistrer_clip(b"mp4", png(8, 8), {"type": "clip"})
        with self.assertRaisesRegex(ErreurSequence, "une image, pas un clip"):
            self.creer(plans=[{"idee": "x", "enchainement": "rush", "rush": clip}])

    def test_as18(self):
        ado = plateau.enregistrer(png(), {"type": "creation", "idee": "une adolescente de 15 ans"})
        with self.assertRaises(ErreurSecurite):
            self.creer(plans=[{"idee": "elle se déshabille", "enchainement": "rush", "rush": ado}])
        with self.assertRaises(ErreurSecurite):  # par le personnage du casting
            self.creer(casting={"personnage": ado, "ambiance": "lingerie et bougies"})
        self.creer(casting={"personnage": ado, "ambiance": "cour d'école ensoleillée"})


class PlansATourner(Base):
    def test_modifier_un_plan(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        self.assertEqual(sequences.public(s)["a_tourner"], [])
        plans = [dict(p) for p in s["plans"]]
        plans[2]["idee"] = "gros plan, elle sourit"
        r = sequences.modifier(s["id"], {**s, "plans": plans})
        self.assertEqual(r["a_tourner"], [2, 3])  # la transition part du plan 3
        self.assertTrue(r["plans"][2]["clip"])  # l'ancien clip reste connu (corbeille au retournage)

    def test_changer_la_qualite_retourne_tout(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        self.assertEqual(sequences.modifier(s["id"], {**s, "qualite": "compromis"})["a_tourner"], [0, 1, 2, 3])

    def test_dependances(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        self.assertEqual(sequences.a_tourner(s, [0]), [0, 1])  # le plan 2 prolonge le 1
        self.assertEqual(sequences.a_tourner(s, [2]), [2, 3])

    def test_voix_automatique(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        self.assertEqual(sequences.source_voix(s, 2), s["plans"][0]["clip"])  # le seul plan avec une réplique
        s["casting"]["voix"] = None
        self.assertIsNone(sequences.source_voix(s, 2))
        self.assertEqual(sequences.personnage(s), self.pilote)  # pas de casting : le rush de départ


class StepsSelonLaParole(Base):
    def test_plans_qui_parlent_en_12_steps(self):
        s = self.creer()  # Rapide : 8 steps
        self.assertEqual([sequences.steps_plan(s, p) for p in s["plans"]], [12, 8, 8, 8])  # seul le plan 1 parle
        api = sequences.graphe_plan(s, 0, "A pilot says: \"On décolle ce soir.\"", 42, {"rush": "r.png"})
        sched = next(v for v in api.values() if v["class_type"] == "BasicScheduler")
        self.assertEqual(sched["inputs"]["steps"], 12)
        api = sequences.graphe_plan(s, 3, "The camera pulls back.", 42, {"rush": "r.png", "precedent": "p.mp4"})
        self.assertEqual(next(v for v in api.values() if v["class_type"] == "BasicScheduler")["inputs"]["steps"], 8)

    def test_qualite_deja_a_12(self):
        s = self.creer(qualite="compromis")
        self.assertEqual({sequences.steps_plan(s, p) for p in s["plans"]}, {12})

    def test_estimation_compte_le_surcout_de_la_parole(self):
        s = self.creer()
        q = plateau.QUALITES_VIDEO["rapide"]["estimation_s"] * sequences.SURCOUT_SEQUENCE
        self.assertAlmostEqual(sequences.estimation(s, [0]), q * sequences.SURCOUT_PAROLE + 5)
        self.assertAlmostEqual(sequences.estimation(s, [1]), q + 5)

    def test_prompt_impose_avec_replique(self):
        plans = self.plans()
        plans[1]["prompt"] = 'She smiles and says: "Allons-y."'
        s = self.creer(plans=plans)
        self.assertEqual(sequences.steps_plan(s, s["plans"][1]), 12)


class Graphes(Base):
    def setUp(self):
        super().setUp()
        self.s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        self.fichiers = {"rush": "r.png", "precedent": "p.mp4", "personnage": "perso.png", "voix": "v.mp4"}

    def noeuds(self, api, type_):
        return {n: v for n, v in api.items() if v["class_type"] == type_}

    def plan(self, k):
        return sequences.graphe_plan(self.s, k, "A pilot in a hangar.", 42, self.fichiers)

    def test_depart_rush(self):
        del self.fichiers["voix"]  # avec une voix, le départ passe en ReferenceToVideo (test_voix)
        api = self.plan(0)
        i2v = next(iter(self.noeuds(api, "MiniMaxH3ImageToVideo").values()))["inputs"]
        self.assertEqual((i2v["width"], i2v["height"]), (544, 800))
        self.assertEqual(api[i2v["first_frame"][0]]["inputs"]["image"], "r.png")
        self.assertTrue(self.noeuds(api, "SaveVideo") and self.noeuds(api, "SaveImage"))

    def test_prolonger(self):
        api = self.plan(1)
        guide = self.noeuds(api, "MiniMaxH3AddGuide")
        self.assertEqual(len(guide), 1)
        nid, g = next(iter(guide.items()))
        guider = next(iter(self.noeuds(api, "BasicGuider").values()))
        self.assertEqual(guider["inputs"]["conditioning"], [nid, 0])
        tranche = next(iter(self.noeuds(api, "Video Slice").values()))["inputs"]
        self.assertAlmostEqual(tranche["start_time"], (124 - 40) / 24)
        self.assertAlmostEqual(tranche["duration"], 40 / 24)
        self.assertNotIn("first_frame", next(iter(self.noeuds(api, "MiniMaxH3ImageToVideo").values()))["inputs"])
        self.assertFalse(self.noeuds(api, "LoadImage"))  # l'image du template est élaguée

    def test_couper_avec_references(self):
        api = self.plan(2)
        self.assertFalse(self.noeuds(api, "MiniMaxH3ImageToVideo"))
        ref = next(iter(self.noeuds(api, "MiniMaxH3ReferenceToVideo").values()))["inputs"]
        self.assertEqual(api[ref["ref_images.ref_image_0"][0]]["inputs"]["image"], "perso.png")
        self.assertEqual(ref["ref_audios.ref_audio_0"][1], 1)  # la piste son de GetVideoComponents
        self.assertEqual((ref["width"], ref["height"]), (544, 800))

    def test_transition(self):
        api = self.plan(3)
        i2v = next(iter(self.noeuds(api, "MiniMaxH3ImageToVideo").values()))["inputs"]
        self.assertEqual(api[i2v["last_frame"][0]]["inputs"]["image"], "r.png")
        derniere = api[i2v["first_frame"][0]]
        self.assertEqual((derniere["class_type"], derniere["inputs"]["batch_index"]), ("ImageFromBatch", 191))

    def test_assemblage(self):
        api = sequences.graphe_film(self.s, ["a.mp4", "b.mp4", "c.mp4", "d.mp4"])
        concat = next(iter(self.noeuds(api, "ConcatenateVideo").values()))["inputs"]
        self.assertEqual([k for k in concat if k.startswith("videos.")], [f"videos.video{i}" for i in range(4)])
        t = {n: v["inputs"] for n, v in self.noeuds(api, "Video Slice").items()}
        self.assertAlmostEqual(t["t0"]["duration"], 123 / 24)  # sa dernière image est refaite par le plan 2
        self.assertAlmostEqual(t["t1"]["start_time"], 39 / 24)  # sans le guide
        self.assertNotIn("t2", t)  # plan coupé : entier
        self.assertAlmostEqual(t["t3"]["start_time"], 1 / 24)  # sans l'image partagée avec le plan 3


class File(Base):
    def test_commande_dans_la_file(self):
        s = self.creer()
        with mock.patch.object(Plateau, "_valider_llm", return_value="x.gguf"):
            p = Plateau(orchestrateur=None)
            c = p.commander_sequence(s["id"])
            self.assertEqual((c.type, c.plans, c.dossier), ("sequence", [0, 1, 2, 3], "Séquence Décollage"))
            with self.assertRaisesRegex(ErreurPlateau, "déjà en tournage"):
                p.commander_sequence(s["id"])

    def test_rien_a_tourner(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        with self.assertRaisesRegex(ErreurPlateau, "Tous les plans sont à jour"):
            Plateau(orchestrateur=None).commander_sequence(s["id"])
        with self.assertRaisesRegex(ErreurSequence, "Séquence inconnue"):
            Plateau(orchestrateur=None).commander_sequence("20260101-000000-abcdef")


if __name__ == "__main__":
    unittest.main()


class FauxLlama:
    """Orchestrateur de test : rend les réponses données, dans l'ordre, et garde les messages reçus."""

    def __init__(self, *reponses):
        self.reponses, self.messages = list(reponses), []

    def llm(self, modele=None):
        from contextlib import asynccontextmanager
        from types import SimpleNamespace

        @asynccontextmanager
        async def contexte():
            async def generer(messages, **_):
                self.messages.append(messages)
                return SimpleNamespace(texte=self.reponses.pop(0))
            yield SimpleNamespace(generer=generer, modele=None)
        return contexte()


def blocs(*prompts):
    return "\n".join(f"SHOT {k}\nFR: plan {k}\nEN: {p}" for k, p in enumerate(prompts, 1))


class Decoupage(Base):
    SYNOPSIS = "Une pilote se prépare au décollage et dit « On décolle ce soir. »"
    PERSO = "A young woman in her early twenties, Korean, wearing a flight suit."
    DECOR = "In a dim orange-lit hangar."
    PLANS = [{"enchainement": "texte", "duree": 5, "note": ""},
             {"enchainement": "prolonger", "duree": 5, "note": "elle se lève"},
             {"enchainement": "couper", "duree": 8, "note": "gros plan, elle dit « Moteurs au vert. »"},
             {"enchainement": "transition", "duree": 5, "note": ""}]

    def decouper(self, llama, plans=None, reference=None, synopsis=None, mineur=False):
        import asyncio
        return asyncio.run(sequences.decouper(llama, synopsis or self.SYNOPSIS, self.PERSO, self.DECOR,
                                              plans or self.PLANS, reference, None, mineur))

    def test_demande_envoyee(self):
        plans = [dict(p) for p in self.PLANS]
        plans[3]["rush"] = self.neon
        propres = sequences._valider_decoupage(self.SYNOPSIS, self.PERSO, self.DECOR, plans, None)
        texte = sequences.demande_decoupage(self.SYNOPSIS, self.PERSO, self.DECOR, propres, None)
        self.assertIn("Story: Une pilote", texte)
        self.assertIn("Character: A young woman", texte)
        self.assertIn("Setting and look: In a dim orange-lit hangar.", texte)
        self.assertIn('Spoken lines, each in this language: "On décolle ce soir." (French); "Moteurs au vert." (French)',
                      texte)
        self.assertIn("1. start, 5 s", texte)
        self.assertIn("2. continue, 5 s, note: elle se lève", texte)
        self.assertIn("4. transition, 5 s, arrival: une enseigne néon", texte)  # idée d'origine du rush
        avec_ref = sequences.demande_decoupage(self.SYNOPSIS, self.PERSO, self.DECOR, propres, self.pilote)
        self.assertIn("Character: shown by the reference picture", avec_ref)

    def test_etiquettes_recopiees_retirees(self):  # vu avec Ministral (2026-10-21)
        self.assertEqual(sequences.sans_etiquette("Cut: medium shot, the character walks."), "Medium shot, the character walks.")
        self.assertEqual(sequences.sans_etiquette("Transition: extreme wide shot."), "Extreme wide shot.")
        self.assertEqual(sequences.sans_etiquette("Wide shot, start from the picture: the character stands."),
                         "Wide shot, the character stands.")
        self.assertEqual(sequences.sans_etiquette("A cut on the hand."), "A cut on the hand.")  # rien à retirer
        self.assertEqual(sequences.sans_etiquette("The character is a woman. A cut shot, low-angle wide of the hangar."),
                         "The character is a woman. Low-angle wide of the hangar.")
        self.assertEqual(sequences.sans_etiquette("A transition shot, craning upward."), "Craning upward.")

    def test_langue_des_repliques(self):
        self.assertEqual([sequences.langue(r) for r in ("On décolle ce soir.", "行くぞ！", "가자!", "Поехали")],
                         ["French", "Japanese", "Korean", "Russian"])

    def test_lecture_de_la_reponse(self):
        self.assertEqual(sequences.lire_decoupage("SHOT 1\nFR: a\nEN: b c\nSHOT 2:\nFR: d\nEN: e", 2),
                         [("a", "b c"), ("d", "e")])
        with self.assertRaisesRegex(ErreurSequence, "1 plan"):
            sequences.lire_decoupage("SHOT 1\nFR: a\nEN: b", 2)
        # Habillage vu avec Ministral (2026-10-21) : étiquettes en gras, séparateurs ---.
        ministral = "SHOT 1\n**FR:** a\n**EN:** b *tap*\n\n---\n\n### SHOT 2\n**FR:** c\n**EN:** **Continuous shot.** d"
        self.assertEqual(sequences.lire_decoupage(ministral, 2), [("a", "b *tap*"), ("c", "Continuous shot. d")])

    def test_decoupage_et_garde_fous(self):
        llama = FauxLlama(blocs(
            'A young woman in her early twenties, Korean, wearing a flight suit. She says in French: "On décolle ce soir."',
            "She stands up and walks to the door, the camera tracking her.",
            "**Close-up** in the cockpit, the character flips two switches.",
            "The camera pulls back slowly into the rainy street."))
        r = self.decouper(llama)
        systeme = llama.messages[0][0]["content"]
        self.assertIn("copy the character description given by the user word for word", systeme)  # sans référence
        self.assertNotIn("{regle_personnage}", systeme)
        self.assertIn("40 to 110 words", systeme)
        self.assertTrue(r["plans"][0]["prompt"].startswith("A young woman"))  # description déjà là : rien d'ajouté
        self.assertTrue(r["plans"][1]["prompt"].startswith("Continuous shot, same scene."))
        self.assertEqual(r["plans"][2]["prompt"],  # description remise, Markdown retiré, réplique de la note remise
                         "The character is a young woman in her early twenties, Korean, wearing a flight suit. "
                         'Close-up in the cockpit, the character flips two switches. The character says: "Moteurs au vert."')
        self.assertFalse(r["plans"][3]["prompt"].startswith("The character is"))  # transition : « the character »
        self.assertEqual([p["idee"] for p in r["plans"]], ["plan 1", "plan 2", "plan 3", "plan 4"])
        self.assertEqual(r["alertes"], [])

    def test_avec_reference_et_depart_du_rush(self):
        plans = [dict(p) for p in self.PLANS]
        plans[0]["enchainement"] = "rush"
        llama = FauxLlama(blocs(*["A shot."] * 4))
        r = self.decouper(llama, plans, reference=self.pilote)
        self.assertIn("never describe them", llama.messages[0][0]["content"])
        self.assertIn("1. start from the picture", llama.messages[0][1]["content"])
        self.assertNotIn("Korean", llama.messages[0][1]["content"])  # la description n'est pas envoyée
        self.assertEqual(r["plans"][0]["rush"], self.pilote)
        self.assertEqual(r["plans"][2]["prompt"], 'A shot. The character says: "Moteurs au vert."')  # rien d'imposé
        self.assertTrue(any("« On décolle ce soir. » absente" in a for a in r["alertes"]))

    def test_reponse_mal_formee_redemandee_une_fois(self):
        llama = FauxLlama("Voici le découpage :", blocs(*["A shot."] * 4))
        self.assertEqual(len(self.decouper(llama)["plans"]), 4)
        self.assertEqual(len(llama.messages), 2)
        with self.assertRaisesRegex(ErreurSequence, "au lieu de 4"):
            self.decouper(FauxLlama("rien", "toujours rien"))

    def test_validation(self):
        with self.assertRaisesRegex(ErreurSequence, "De 2 à 12 plans"):
            self.decouper(FauxLlama(), plans=self.PLANS[:1])
        plans = [dict(p) for p in self.PLANS]
        plans[0]["enchainement"] = "rush"
        with self.assertRaisesRegex(ErreurSequence, "choisis l'image de référence"):
            self.decouper(FauxLlama(), plans)
        with self.assertRaisesRegex(ErreurSequence, "synopsis"):
            self.decouper(FauxLlama(), synopsis=" ")

    def test_as18(self):
        ado = plateau.enregistrer(png(), {"type": "creation", "idee": "une adolescente de 15 ans"})
        with self.assertRaises(ErreurSecurite):  # dès l'entrée, avant Llama
            self.decouper(FauxLlama(), reference=ado, synopsis="elle se déshabille lentement")
        llama = FauxLlama(blocs("A shot.", "She stands in lingerie.", "A shot.", "A shot."))
        with self.assertRaises(ErreurSecurite):  # terme sensible ajouté par Llama, rush d'origine mineur
            self.decouper(llama, reference=ado)
        llama = FauxLlama(blocs("A shot.", "She stands in lingerie.", "A shot.", "A shot."))
        with self.assertRaises(ErreurSecurite):  # personnage mineur dans le Scénario, sans aucun mot pour le dire
            self.decouper(llama, mineur=True)

    def test_personnage_et_decor_du_scenario(self):
        from app import listes
        lumiere = next(iter(listes.blocs()["commun"].sous_listes["Lumière"].valeurs))
        d = listes.description_sequence("personnage", {
            "personnage": {"Genre": "Femme", "Âge": "Jeune adulte (18-25 ans)", "Origine": "Coréenne",
                           "Pose": "Debout", "Action": "Lit un livre"},
            "commun": {"Cadrage": "Plan taille", "Lumière": lumiere}})
        self.assertEqual(d["personnage"], "A young woman in her early twenties, Korean.")  # sans pose ni action
        self.assertNotIn("medium shot", d["decor"])  # le cadrage se choisit plan par plan
        self.assertTrue(d["decor"])
        self.assertFalse(d["mineur"])
        self.assertTrue(listes.description_sequence("personnage", {"personnage": {"Âge": "Ado (12-17 ans)"}})["mineur"])

    def test_consigne_reglable(self):
        from app import reglages
        self.assertIn("decoupage", reglages.CONSIGNES)
