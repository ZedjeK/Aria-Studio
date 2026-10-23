"""Tests de la voix de référence des séquences : bibliothèque, import, essai de voix, branchement dans les séquences.

Rien n'est envoyé à ComfyUI ; voix, séquences, rushes et clips vivent dans un dossier temporaire.
"""

import asyncio
import io
import json
import struct
import tempfile
import unittest
import wave
from contextlib import asynccontextmanager
from datetime import datetime, timedelta
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from PIL import Image

from app import plateau, rushes, sequences, voix
from app.plateau import Plateau
from app.securite import ErreurSecurite
from app.sequences import ErreurSequence
from app.voix import ErreurVoix


def png(largeur=832, hauteur=1248) -> bytes:
    sortie = io.BytesIO()
    Image.new("RGB", (largeur, hauteur), (40, 30, 20)).save(sortie, "PNG")
    return sortie.getvalue()


def wav(secondes: float, frequence: int = 16000) -> bytes:
    sortie = io.BytesIO()
    with wave.open(sortie, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(frequence)
        w.writeframes(b"\0\0" * int(secondes * frequence))
    return sortie.getvalue()


def flac(secondes: float, frequence: int = 32000) -> bytes:
    """En-tête FLAC (STREAMINFO) seulement : de quoi lire la durée."""
    infos = (frequence << 44) | (1 << 41) | (15 << 36) | int(secondes * frequence)
    return b"fLaC" + bytes([0x80, 0, 0, 34]) + bytes(10) + struct.pack(">Q", infos) + bytes(16)


class Base(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(sequences, "DOSSIER", d / "sequences"),
                       mock.patch.object(voix, "DOSSIER", d / "voix"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json")]
        for p in self.patchs:
            p.start()
        self.pilote = plateau.enregistrer(png(), {"type": "creation", "idee": "une pilote", "prompt": "a pilot"})

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.dossier.cleanup()

    def voix_gardee(self, nom="Pilote"):
        return voix.importer(wav(5), nom)

    def sequence(self, voix_="auto", **autres):
        plans = [{"idee": "elle lève les yeux et dit « On décolle ce soir. »", "duree": 5, "enchainement": "rush",
                  "rush": self.pilote},
                 {"idee": "elle se lève", "duree": 5, "enchainement": "prolonger"},
                 {"idee": "gros plan, elle dit « Moteurs au vert. »", "duree": 5, "enchainement": "couper"}]
        return sequences.creer({"titre": "Décollage", "qualite": "rapide", "plans": plans,
                                "casting": {"voix": voix_, "ambiance": ""}, **autres})


class FichierAudio(Base):
    def test_formats_reconnus_par_leurs_octets(self):
        self.assertEqual(voix.format_audio(wav(1)), "wav")
        self.assertEqual(voix.format_audio(flac(1)), "flac")
        self.assertEqual(voix.format_audio(b"OggS" + bytes(30)), "ogg")
        self.assertEqual(voix.format_audio(b"ID3" + bytes(30)), "mp3")
        self.assertEqual(voix.format_audio(bytes([0xFF, 0xFB]) + bytes(30)), "mp3")
        self.assertIsNone(voix.format_audio(png(8, 8)))

    def test_duree_lue_sans_decodeur(self):
        self.assertEqual(voix.duree_audio(wav(4.5), "wav"), 4.5)
        self.assertEqual(voix.duree_audio(flac(5.17), "flac"), 5.17)
        self.assertIsNone(voix.duree_audio(b"ID3" + bytes(30), "mp3"))  # inconnue : acceptée telle quelle
        self.assertIsNone(voix.duree_audio(b"RIFF....WAVEcasse", "wav"))

    def test_import(self):
        v = voix.importer(wav(6), "  Ma   voix ", "moi.wav")
        self.assertEqual((v["nom"], v["gardee"], v["source"], v["duree_son"]), ("Ma voix", True, "fichier", 6.0))
        self.assertEqual(voix.chemin_son(v["id"]).read_bytes()[:4], b"RIFF")
        self.assertEqual(voix.type_mime(v["id"]), "audio/wav")
        self.assertEqual([x["id"] for x in voix.liste()], [v["id"]])

    def test_import_refuse(self):
        with self.assertRaisesRegex(ErreurVoix, "au moins 3 s"):
            voix.importer(wav(2), "Courte")
        with self.assertRaisesRegex(ErreurVoix, "Format audio"):
            voix.importer(png(8, 8), "Image")
        with self.assertRaisesRegex(ErreurVoix, "Nom"):
            voix.importer(wav(5), "   ")
        with mock.patch.object(voix, "TAILLE_MAX", 100):
            with self.assertRaisesRegex(ErreurVoix, "Mo au plus"):
                voix.importer(wav(5), "Lourde")


class Bibliotheque(Base):
    def brouillon(self, il_y_a=timedelta(0)):
        v = voix._nouvelle(flac(5), "flac", {"nom": None, "gardee": False, "source": "essai", "phrase": "Bonjour",
                                             "description": "", "personnage": None})
        v["date"] = (datetime.now() - il_y_a).isoformat(timespec="seconds")
        voix._ecrire(v)
        return v

    def test_brouillon_puis_garde(self):
        v = self.brouillon()
        self.assertEqual(voix.liste(), [])
        self.assertEqual(len(voix.liste(brouillons=True)), 1)
        with self.assertRaisesRegex(ErreurVoix, "Garde d'abord"):
            voix.gardee(v["id"])
        voix.garder(v["id"], "Pilote calme")
        self.assertEqual(voix.gardee(v["id"])["nom"], "Pilote calme")
        voix.garder(v["id"], "Pilote")  # renommer
        self.assertEqual(voix.liste()[0]["nom"], "Pilote")

    def test_vieux_brouillons_effaces(self):
        vieux, recent = self.brouillon(timedelta(days=2)), self.brouillon()
        garde = self.voix_gardee()
        voix.liste()
        self.assertFalse((voix.DOSSIER / vieux["fichier"]).exists())
        self.assertRaises(ErreurVoix, voix.lire, vieux["id"])
        voix.lire(recent["id"])
        voix.lire(garde["id"])

    def test_suppression_a_la_corbeille(self):
        v = self.voix_gardee()
        voix.supprimer(v["id"])
        self.assertRaisesRegex(ErreurVoix, "Voix inconnue", voix.lire, v["id"])
        self.assertTrue((voix.DOSSIER / "corbeille" / v["fichier"]).is_file())
        self.assertTrue((voix.DOSSIER / "corbeille" / f"{v['id']}.json").is_file())

    def test_identifiant_controle(self):
        for mauvais in ("../creations/x", "", "20260101-000000-abcdef"):
            self.assertRaises(ErreurVoix, voix.lire, mauvais)


class EssaiDeVoix(Base):
    def test_validation(self):
        phrase, description, duree = voix.valider_essai("", "", self.pilote)
        self.assertEqual((phrase, description, duree), (voix.PHRASE_DEFAUT, "", 5))
        self.assertEqual(voix.valider_essai(" ".join(["mot"] * 18), "", self.pilote)[2], 8)
        self.assertEqual(voix.valider_essai(" ".join(["mot"] * 24), "", self.pilote)[2], 10)
        with self.assertRaisesRegex(ErreurVoix, "25 mots"):
            voix.valider_essai(" ".join(["mot"] * 26), "", self.pilote)
        with self.assertRaisesRegex(ErreurVoix, "décris qui parle"):
            voix.valider_essai("Bonjour", "", None)
        clip = plateau.enregistrer_clip(b"mp4", png(8, 8), {"type": "clip", "idee": "un clip"})
        with self.assertRaisesRegex(ErreurVoix, "pas un clip"):
            voix.valider_essai("Bonjour", "", clip)

    def test_prompt(self):
        p = voix.prompt_essai("« Bonjour, je suis prête. »", "douce, un peu grave", True)
        self.assertIn("<Picture 1>", p)
        self.assertIn('says: "Bonjour, je suis prête."', p)
        self.assertIn("speaks French, with this voice: douce, un peu grave", p)
        self.assertIn("no music, no ambient sound", p)
        p = voix.prompt_essai("안녕하세요", "une femme de trente ans, voix grave", False)
        self.assertNotIn("<Picture 1>", p)
        self.assertIn("The character: une femme de trente ans, voix grave.", p)
        self.assertIn("speaks Korean.", p)

    def test_graphe_son_seul(self):
        api = voix.graphe_essai("A pilot says hello.", 42, 5, "perso.png")
        types = [v["class_type"] for v in api.values()]
        self.assertNotIn("SaveVideo", types)
        self.assertNotIn("SaveImage", types)
        self.assertNotIn("VAEDecode", types)  # l'image n'est même pas décodée
        self.assertNotIn("MiniMaxH3ImageToVideo", types)
        ref = next(v for v in api.values() if v["class_type"] == "MiniMaxH3ReferenceToVideo")["inputs"]
        self.assertEqual((ref["width"], ref["height"]), sequences.dimensions(1.0, 0.4))
        self.assertEqual(api[ref["ref_images.ref_image_0"][0]]["inputs"]["image"], "perso.png")
        sortie = next(v for v in api.values() if v["class_type"] == "SaveAudio")["inputs"]
        self.assertEqual(api[sortie["audio"][0]]["class_type"], "VAEDecodeAudio")
        sched = next(v for v in api.values() if v["class_type"] == "BasicScheduler")["inputs"]
        self.assertEqual(sched["steps"], 12)
        sans = voix.graphe_essai("A woman says hello.", 42, 5, None)
        ref = next(v for v in sans.values() if v["class_type"] == "MiniMaxH3ReferenceToVideo")["inputs"]
        self.assertNotIn("ref_images.ref_image_0", ref)

    def test_commande_dans_la_file(self):
        c = Plateau(orchestrateur=None).commander_voix("", "", self.pilote, 7)
        self.assertEqual((c.type, c.idee, c.parent, c.duree, c.qualite), ("voix", voix.PHRASE_DEFAUT, self.pilote,
                                                                        5, "rapide"))
        self.assertIn("voix", Plateau(orchestrateur=None).options()["video"])

    def test_as18(self):
        ado = plateau.enregistrer(png(), {"type": "creation", "idee": "une adolescente de 15 ans"})
        with self.assertRaises(ErreurSecurite):
            Plateau(orchestrateur=None).commander_voix("Je me déshabille", "", ado, None)
        Plateau(orchestrateur=None).commander_voix("Bonjour, c'est la rentrée", "", ado, None)

    def test_tournage(self):
        """La commande enregistre le son rendu par ComfyUI comme brouillon, avec ce qui l'a fait."""
        recu = {}

        class FauxComfy:
            async def televerser(self, chemin):
                return "perso.png"

            async def executer(self, api, progression=None):
                recu["api"] = api
                return [("aria_voix_00001_.flac", flac(5.17))]

        @asynccontextmanager
        async def comfyui():
            yield FauxComfy()

        p = Plateau(orchestrateur=SimpleNamespace(comfyui=comfyui))
        c = p.commander_voix("Bonjour.", "posée", self.pilote, 7)
        asyncio.run(voix.tourner_essai(p, c))
        v = voix.lire(c.voix)
        self.assertEqual((v["gardee"], v["source"], v["phrase"], v["langue"], v["seed"], v["duree_son"]),
                         (False, "essai", "Bonjour.", "French", 7, 5.17))
        self.assertIn("recording booth", v["prompt"])
        self.assertTrue(any(n["class_type"] == "SaveAudio" for n in recu["api"].values()))


class DansLesSequences(Base):
    def noeuds(self, api, type_):
        return [v for v in api.values() if v["class_type"] == type_]

    def test_casting_avec_une_voix_de_la_bibliotheque(self):
        v = self.voix_gardee()
        s = self.sequence(f"voix:{v['id']}")
        self.assertEqual(s["casting"]["voix"], f"voix:{v['id']}")
        self.assertNotIn(f"voix:{v['id']}", sequences._rushes_utilises(s))
        brouillon = voix._nouvelle(flac(5), "flac", {"nom": None, "gardee": False, "source": "essai"})
        with self.assertRaisesRegex(ErreurSequence, "Garde d'abord"):
            self.sequence(f"voix:{brouillon['id']}")
        with self.assertRaisesRegex(ErreurSequence, "Voix inconnue"):
            self.sequence("voix:20260101-000000-abcdef")

    def test_qui_recoit_la_voix(self):
        v = self.voix_gardee()
        s = self.sequence(f"voix:{v['id']}")
        self.assertEqual([sequences.voix_du_plan(s, k) for k in range(3)], [f"voix:{v['id']}", None, f"voix:{v['id']}"])
        s["plans"][0]["idee"] = "elle lève les yeux"  # ne parle pas : pas de voix, le rush en première image
        self.assertIsNone(sequences.voix_du_plan(s, 0))
        auto = self.sequence()
        self.assertIsNone(sequences.voix_du_plan(auto, 0))  # en Auto, c'est lui qui donne la voix

    def test_graphe_depart_qui_parle(self):
        s = self.sequence(f"voix:{self.voix_gardee()['id']}")
        api = sequences.graphe_plan(s, 0, "She says hello.", 42, {"rush": "r.png", "personnage": "perso.png",
                                                                   "voix_son": "v.flac"})
        self.assertFalse(self.noeuds(api, "MiniMaxH3ImageToVideo"))
        ref = self.noeuds(api, "MiniMaxH3ReferenceToVideo")[0]["inputs"]
        coupe = api[ref["ref_audios.ref_audio_0"][0]]
        self.assertEqual((coupe["class_type"], coupe["inputs"]["duration"]), ("TrimAudioDuration", 15.0))
        self.assertEqual(api[coupe["inputs"]["audio"][0]]["inputs"]["audio"], "v.flac")
        guide = self.noeuds(api, "MiniMaxH3AddGuide")[0]["inputs"]
        self.assertEqual((guide["frame_idx"], api[guide["image"][0]]["inputs"]["image"]), (0, "r.png"))
        self.assertNotIn("audio", guide)  # la voix est une référence, pas un son joué au début du plan
        guider = self.noeuds(api, "BasicGuider")[0]["inputs"]
        self.assertEqual(api[guider["conditioning"][0]]["class_type"], "MiniMaxH3AddGuide")

    def test_graphe_depart_texte_qui_parle(self):
        s = self.sequence(f"voix:{self.voix_gardee()['id']}")
        s["plans"][0].update(enchainement="texte", rush=None)
        api = sequences.graphe_plan(s, 0, "She says hello.", 42, {"voix_son": "v.flac"})
        self.assertTrue(self.noeuds(api, "MiniMaxH3ReferenceToVideo"))
        self.assertFalse(self.noeuds(api, "MiniMaxH3AddGuide"))

    def test_graphe_depart_sans_voix_inchange(self):
        s = self.sequence()
        api = sequences.graphe_plan(s, 0, "She says hello.", 42, {"rush": "r.png"})
        i2v = self.noeuds(api, "MiniMaxH3ImageToVideo")[0]["inputs"]
        self.assertEqual(api[i2v["first_frame"][0]]["inputs"]["image"], "r.png")
        self.assertFalse(self.noeuds(api, "MiniMaxH3ReferenceToVideo"))

    def test_voix_supprimee_la_sequence_repasse_en_auto(self):
        v = self.voix_gardee("Pilote")
        s = self.sequence(f"voix:{v['id']}")
        autre = self.sequence()
        lue = sequences.lire(s["id"])
        for p in lue["plans"]:
            p.update(clip=plateau.enregistrer_clip(b"mp4", png(8, 8), {"type": "clip", "idee": p["idee"]}), a_jour=True)
        sequences._ecrire(lue)
        self.assertEqual(voix.supprimer(v["id"]), [s["id"]])
        lue = sequences.public(sequences.lire(s["id"]))
        self.assertEqual(lue["casting"]["voix"], "auto")
        self.assertEqual(lue["a_tourner"], [0, 1, 2])
        self.assertIn("« Pilote » a été supprimée", lue["alerte"])
        self.assertNotIn("alerte", sequences.lire(autre["id"]))
        relue = sequences.modifier(s["id"], {**lue, "plans": lue["plans"]})
        self.assertNotIn("alerte", relue)  # l'alerte s'efface au prochain enregistrement

    def test_as18_par_la_voix(self):
        ado = plateau.enregistrer(png(), {"type": "creation", "idee": "une adolescente de 15 ans"})
        v = voix._nouvelle(flac(5), "flac", {"nom": "Ado", "gardee": True, "source": "essai", "phrase": "Bonjour",
                                             "description": "", "personnage": ado})
        with self.assertRaises(ErreurSecurite):  # le personnage de l'essai est mineur
            sequences.creer({"titre": "T", "casting": {"voix": f"voix:{v['id']}"},
                             "plans": [{"idee": "elle se déshabille", "enchainement": "texte"}]})
        sequences.creer({"titre": "T", "casting": {"voix": f"voix:{v['id']}"},
                         "plans": [{"idee": "elle range son cartable", "enchainement": "texte"}]})


if __name__ == "__main__":
    unittest.main()
