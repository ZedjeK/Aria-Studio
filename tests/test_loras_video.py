"""LoRA par usage et LoRA vidéo : le classement de Ma collection (image, retouche, vidéo, d'après les workflows de
la famille), les LoRA d'un clip du Montage et celles d'une séquence (les mêmes sur tous les plans).
Rien n'est envoyé à ComfyUI : dossier des modèles, registre et mémoire des LoRA sont temporaires."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from app import collection, modeles, plateau, rushes, sequences, workflows
from app.plateau import ErreurPlateau, Plateau
from app.securite import ErreurSecurite
from tests.test_sequences import Base as BaseSequence
from tests import steps_connus

STYLE = "style_h3.safetensors"
LORA = {"fichier": STYLE, "force": 0.8, "declencheurs": "cinematic grain"}



setUpModule, tearDownModule = steps_connus.module()  # ces tests ne portent pas sur la validation des steps

class ModelesTemporaires:
    """Un dossier des modèles avec le checkpoint de H3, une LoRA H3 et une LoRA Qwen-Image 2.1."""

    def demarrer(self):
        self.temp = tempfile.TemporaryDirectory()
        d = Path(self.temp.name)
        for famille, nom in (("MiniMax-H3", STYLE), ("Qwen-Image2.1", "detail.safetensors")):
            (d / "modeles" / modeles.dossier_famille(famille, "loras")).mkdir(parents=True)
            (d / "modeles" / modeles.dossier_famille(famille, "loras") / nom).write_bytes(b"x")
        (d / "modeles" / "video" / "MiniMax-H3" / "modeles").mkdir()  # sans checkpoint, la famille n'a rien à charger
        (d / "modeles" / "video" / "MiniMax-H3" / "modeles" / workflows.obtenir("minimax_h3_t2v").modele_origine).touch()
        self.patchs_modeles = [mock.patch.object(modeles, "DOSSIER", d / "modeles"),
                               mock.patch.object(modeles, "REGISTRE", d / "modeles.json"),
                               mock.patch.object(modeles, "MEMOIRE_LORAS", d / "loras.json")]
        for p in self.patchs_modeles:
            p.start()

    def arreter(self):
        for p in self.patchs_modeles:
            p.stop()
        self.temp.cleanup()


class Usages(unittest.TestCase, ModelesTemporaires):
    def setUp(self):
        self.demarrer()

    def tearDown(self):
        self.arreter()

    def test_usages_des_familles(self):
        f = workflows.familles()
        self.assertEqual(f["Krea2"]["usages"], ["image"])
        self.assertEqual(f["Qwen-Image2.1"]["usages"], ["image", "retouche"])  # création et retouche
        self.assertEqual(f["MiniMax-H3"]["usages"], ["video"])
        self.assertEqual(f["Krea2"]["nature"], "image")  # le rangement des modèles ne change pas

    def test_les_lora_de_ma_collection_portent_leurs_usages(self):
        with mock.patch.object(modeles, "espace_libre", lambda: 0):
            loras = next(g for g in collection.collection()["groupes"] if g["id"] == "lora")["elements"]
        usages = {e["nom"]: e["usages"] for e in loras}
        self.assertEqual(usages, {STYLE: ["video"], "detail.safetensors": ["image", "retouche"]})


class Clip(unittest.TestCase, ModelesTemporaires):
    def setUp(self):
        self.demarrer()
        self.temp_plateau = tempfile.TemporaryDirectory()
        d = Path(self.temp_plateau.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json"),
                       # les poids de H3 ne sont pas dans le dossier temporaire : le workflow les croit là
                       mock.patch.object(workflows.Workflow, "manquants", lambda self: [])]
        for p in self.patchs:
            p.start()
        self.plateau = Plateau(orchestrateur=None)

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.temp_plateau.cleanup()
        self.arreter()

    def clip(self, **options):
        valeurs = dict(idee="x", prompt_clip="A slow push-in.", mode="texte", source=None, qualite="compromis",
                       duree=5, format="16:9 (Widescreen)", seed=None, nombre=1)
        return self.plateau.commander_clip(**{**valeurs, **options})

    def test_lora_du_clip(self):
        c = self.clip(loras=[LORA])
        self.assertEqual(c.loras, [LORA])
        self.assertEqual(Plateau._avec_declencheurs(c), "cinematic grain, A slow push-in.")
        self.assertEqual(modeles.loras("MiniMax-H3")[0]["force"], 0.8)  # retenue pour la prochaine fois

    def test_refus(self):
        with self.assertRaisesRegex(ErreurPlateau, "absente de modeles/video/MiniMax-H3/loras"):
            self.clip(loras=[{"fichier": "detail.safetensors", "force": 1}])  # une LoRA image n'entre pas en vidéo
        with self.assertRaisesRegex(ErreurPlateau, "entre 0 et 2"):
            self.clip(loras=[{**LORA, "force": 2.5}])
        with self.assertRaises(ErreurSecurite):  # les mots déclencheurs passent par AS-18
            self.clip(prompt_clip="A 12-year-old girl at the beach.", loras=[{**LORA, "declencheurs": "lingerie"}])

    def test_lora_dans_le_graphe_avant_le_turbo(self):
        wf = workflows.obtenir("minimax_h3_t2v")
        api = wf.preparer({"prompt": "x", "seed": 1, "duree": 5, "loras": [LORA]})
        self.assertEqual(api["aria_lora_1"]["inputs"]["lora_name"], STYLE)
        chargeur = wf.fiche["entrees"]["modele"][0]
        self.assertEqual(api["aria_lora_1"]["inputs"]["model"], [chargeur, 0])
        lecteurs = [n for n, v in api.items() if [chargeur, 0] in v["inputs"].values()]
        self.assertEqual(lecteurs, ["aria_lora_1"])  # tout ce qui lisait le modèle lit la LoRA


class Sequence(BaseSequence, ModelesTemporaires):
    def setUp(self):
        super().setUp()
        self.demarrer()

    def tearDown(self):
        self.arreter()
        super().tearDown()

    def test_loras_gardees_et_dans_chaque_plan(self):
        s = sequences.lire(self.creer(loras=[LORA])["id"])
        self.assertEqual(s["loras"], [LORA])
        api = sequences.graphe_plan(self.tout_a_jour(s), 1, "A pilot in a hangar.", 42, {"precedent": "p.mp4"})
        self.assertEqual(api["aria_lora_1"]["inputs"]["lora_name"], STYLE)

    def test_changer_les_loras_fait_tout_retourner(self):
        s = self.tout_a_jour(sequences.lire(self.creer()["id"]))
        self.assertEqual(sequences.public(s)["a_tourner"], [])
        donnees = {k: s[k] for k in ("titre", "qualite", "format", "casting", "plans", "modele")}
        r = sequences.modifier(s["id"], {**donnees, "loras": [LORA]})
        self.assertEqual(r["a_tourner"], [0, 1, 2, 3])

    def test_ancienne_sequence_sans_loras(self):
        s = sequences.lire(self.creer()["id"])
        chemin = sequences.DOSSIER / f"{s['id']}.json"
        brut = chemin.read_text(encoding="utf-8").replace('"loras": [],', "")
        chemin.write_text(brut, encoding="utf-8")
        self.assertEqual(sequences.lire(s["id"])["loras"], [])

    def test_refus(self):
        with self.assertRaisesRegex(sequences.ErreurSequence, "absente de modeles/video/MiniMax-H3/loras"):
            self.creer(loras=[{"fichier": "detail.safetensors", "force": 1}])
