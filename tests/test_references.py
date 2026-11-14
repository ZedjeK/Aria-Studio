"""Tests de la zone « Références » du Plateau : branchement dans les graphes, validations, AS-18.

Les rushes de test vivent dans un dossier temporaire ; rien n'est envoyé à ComfyUI ni à Llama.
"""

import asyncio
import json
import tempfile
import unittest
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from app import fiches, plateau, prompt, reglages, workflows
from app.plateau import ErreurPlateau, Plateau
from app.securite import ErreurSecurite
from tests import steps_connus

PNG = bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010806000000")  # en-tête suffisant



setUpModule, tearDownModule = steps_connus.module()  # ces tests ne portent pas sur la validation des steps

class Graphes(unittest.TestCase):
    def test_capacites(self):
        self.assertEqual(workflows.obtenir("qwen_image_2_1_edit").accepte_references(), 10)  # le plafond d'Aria
        self.assertEqual(workflows.obtenir("qwen_image_2_1").accepte_references(), 0)  # pas d'entrée d'image
        self.assertEqual(workflows.obtenir("krea2_turbo").accepte_references(), 0)
        self.assertTrue(workflows.obtenir("krea2_turbo").accepte_variation())

    def test_inspirer_qwen_21_latent_adapte(self):
        api = workflows.obtenir("qwen_image_2_1_edit").preparer(
            {"prompt": "x", "seed": 1, "image": "a.png", "references": ["b.png"], "format_sortie": "2:3 (Portrait Photo)"})
        encodeur = next(n for n in api.values() if n["class_type"] == "TextEncodeQwenImage21")
        self.assertEqual(encodeur["inputs"]["images.image_2"], ["aria_ref_2", 0])
        self.assertEqual(api["aria_latent"]["class_type"], "EmptyLatentImage")

    def test_inspirer_qwen_21_dix_images(self):
        wf = workflows.obtenir("qwen_image_2_1_edit")
        noms = [f"r{i}.png" for i in range(2, 11)]  # l'image 1 + 9 références
        api = wf.preparer({"prompt": "x", "seed": 1, "image": "a.png", "references": noms})
        encodeur = next(n for n in api.values() if n["class_type"] == "TextEncodeQwenImage21")
        for i in range(2, 11):
            self.assertEqual(encodeur["inputs"][f"images.image_{i}"], [f"aria_ref_{i}", 0])
        with self.assertRaises(workflows.ErreurWorkflow):  # une de trop : 11 images
            wf.preparer({"prompt": "x", "seed": 1, "image": "a.png", "references": noms + ["r11.png"]})

    def test_reserve_a_la_retouche(self):
        base = workflows.obtenir("qwen_image_2_1_edit")
        self.assertFalse(base.reserve_retouche())  # aucun workflow livré n'est réservé aujourd'hui : la clé reste prête
        reserve = workflows.Workflow({**base.fiche, "reserve": "retouche"}, base.api)
        self.assertTrue(reserve.reserve_retouche())
        self.assertFalse([p for p in fiches.problemes_workflow(reserve.fiche) if p.startswith("reserve")])
        for mauvaise in ({**base.fiche, "reserve": "autre"}, {**reserve.fiche, "tache": "texte_vers_image"}):
            self.assertTrue([p for p in fiches.problemes_workflow(mauvaise) if p.startswith("reserve")])

    def test_variation(self):
        for id_wf in ("qwen_image_2_1", "krea2_turbo"):
            api = workflows.obtenir(id_wf).preparer({"prompt": "x", "seed": 1,
                                                     "variation": {"image": "a.png", "denoise": 0.6}})
            ks = api[reglages.noeud_unique(api, "KSampler")]["inputs"]
            self.assertEqual((ks["latent_image"], ks["denoise"]), (["aria_var_latent", 0], 0.6), id_wf)
            self.assertEqual(api["aria_var_latent"]["inputs"]["vae"][0], reglages.noeud_unique(api, "VAELoader"))
        with self.assertRaises(workflows.ErreurWorkflow):
            workflows.obtenir("qwen_image_2_1").preparer({"prompt": "x", "seed": 1,
                                                          "variation": {"image": "a.png", "denoise": 1.5}})

    def test_variation_recadree_a_un_autre_format(self):
        """Redessiner au format choisi : l'image de départ est recadrée au centre (jamais étirée), au nombre de
        mégapixels réglé ; sans format, elle garde le sien."""
        for id_wf in ("qwen_image_2_1", "krea2_turbo"):
            wf = workflows.obtenir(id_wf)
            sans = wf.preparer({"prompt": "x", "seed": 1, "variation": {"image": "a.png", "denoise": 0.6}})
            self.assertEqual(sans["aria_var_echelle"]["class_type"], "ImageScaleToTotalPixels", id_wf)
            self.assertNotIn("aria_var_format", sans)
            api = wf.preparer({"prompt": "x", "seed": 1,
                               "variation": {"image": "a.png", "denoise": 0.6, "format": "16:9 (Widescreen)"}})
            self.assertEqual(api["aria_var_format"]["inputs"]["aspect_ratio"], "16:9 (Widescreen)", id_wf)
            echelle = api["aria_var_echelle"]["inputs"]
            self.assertEqual((api["aria_var_echelle"]["class_type"], echelle["crop"]), ("ImageScale", "center"), id_wf)
            self.assertEqual((echelle["width"], echelle["height"]), (["aria_var_format", 0], ["aria_var_format", 1]))
            self.assertEqual(api["aria_var_latent"]["inputs"]["pixels"], ["aria_var_echelle", 0], id_wf)
        with self.assertRaisesRegex(workflows.ErreurWorkflow, "Format inconnu"):
            workflows.obtenir("krea2_turbo").preparer({"prompt": "x", "seed": 1,
                                                       "variation": {"image": "a.png", "denoise": 0.6, "format": "5:7"}})


class Commandes(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        self.patch = mock.patch.object(plateau, "DOSSIER", Path(self.dossier.name))
        self.patch.start()
        self.plateau = Plateau(orchestrateur=None)

    def tearDown(self):
        self.patch.stop()
        self.dossier.cleanup()

    def rush(self, idee="", prompt_="", **autres) -> str:
        return plateau.enregistrer(PNG + bytes(16), {"type": "creation", "idee": idee, "prompt": prompt_, **autres})

    def commander(self, idee, references, mode="inspirer", **options):
        return self.plateau.commander(idee, idee, None, options.pop("workflow", None), "16:9 (Widescreen)", None, 1,
                                      references=references, mode_reference=mode, **options)

    def test_valeurs_imposees_seulement_les_entrees_du_workflow(self):
        c = self.plateau.commander("x", "x", None, "qwen_image_2_1", "1:1 (Square)", None, 1,
                                   imposees={"steps": 10, "megapixels": 0.5, "inconnue": 1})
        self.assertEqual(c.imposees, {"steps": 10, "megapixels": 0.5})

    def test_inspirer_par_defaut_qwen_21_edit(self):
        a, b = self.rush("une pilote de mecha"), self.rush("un dragon de jade")
        c = self.commander("the woman from Picture 1 on the dragon from Picture 2", [a, b])
        self.assertEqual((c.workflow, c.references, c.parent), ("qwen_image_2_1_edit", [a, b], a))
        self.assertNotIn("textes_origine", c.to_dict())

    def test_variation(self):
        a = self.rush("une pilote")
        c = self.commander("anime style", [a], "variation", liberte=0.7)
        self.assertEqual((c.workflow, c.liberte), ("qwen_image_2_1", 0.7))
        with self.assertRaisesRegex(ErreurPlateau, "une seule"):
            self.commander("x", [a, self.rush("y")], "variation")
        with self.assertRaisesRegex(ErreurPlateau, "Liberté"):
            self.commander("x", [a], "variation", liberte=2)
        with self.assertRaisesRegex(ErreurPlateau, "ne permet pas"):
            self.commander("x", [a], "variation", workflow="qwen_image_2_1_edit")

    def test_format_de_l_image_seulement_pour_redessiner(self):
        a = self.rush("une pilote")
        c = self.plateau.commander("x", "x", None, None, plateau.FORMAT_IMAGE, None, 1,
                                   references=[a], mode_reference="variation")
        self.assertEqual(c.format, plateau.FORMAT_IMAGE)
        with self.assertRaisesRegex(ErreurPlateau, "Format inconnu"):  # ni créer, ni mélanger des images
            self.plateau.commander("x", "x", None, "qwen_image_2_1", plateau.FORMAT_IMAGE, None, 1)
        with self.assertRaisesRegex(ErreurPlateau, "Format inconnu"):
            self.plateau.commander("x", "x", None, None, plateau.FORMAT_IMAGE, None, 1,
                                   references=[a], mode_reference="inspirer")
        self.assertEqual(self.plateau.options()["format_image"], plateau.FORMAT_IMAGE)

    def test_le_format_part_a_comfyui_en_redessinant(self):
        """Bout en bout : « Celui de l'image » n'envoie aucun format, un autre format recadre l'image de départ."""
        graphes = []

        class FauxComfy:
            async def televerser(self, _chemin):
                return "a.png"

            async def executer(self, api, progression=None):
                graphes.append(api)
                return [("s.png", PNG + bytes(16))]

        @asynccontextmanager
        async def comfyui():
            yield FauxComfy()

        p = Plateau(orchestrateur=SimpleNamespace(comfyui=comfyui))
        a = self.rush("une pilote")
        for format_ in (plateau.FORMAT_IMAGE, "16:9 (Widescreen)"):
            c = p.commander("x", "x", None, None, format_, 1, 1, references=[a], mode_reference="variation")
            asyncio.run(p._creer(c))
        garde, recadre = graphes
        self.assertEqual(garde["aria_var_echelle"]["class_type"], "ImageScaleToTotalPixels")
        self.assertEqual(recadre["aria_var_echelle"]["class_type"], "ImageScale")
        self.assertEqual(recadre["aria_var_format"]["inputs"]["aspect_ratio"], "16:9 (Widescreen)")

    def test_validations(self):
        a = self.rush("x")
        with self.assertRaisesRegex(ErreurPlateau, "Mode"):
            self.commander("x", [a], "collage")
        with self.assertRaisesRegex(ErreurPlateau, "inconnue"):
            self.commander("x", ["20990101-000000-abcdef"])
        with self.assertRaisesRegex(ErreurPlateau, "accepte de 1 à 10"):
            self.commander("x", [a] + [self.rush(str(i)) for i in range(10)])  # 11 images
        self.commander("x", [a] + [self.rush(str(i)) for i in range(9)])  # 10 images : accepté par défaut (2.1 Edit)
        with mock.patch.object(workflows.Workflow, "reserve_retouche", lambda self: True), \
                self.assertRaisesRegex(ErreurPlateau, "réservé à la retouche"):  # un workflow réservé ne sert pas à S'inspirer
            self.commander("x", [a], workflow="qwen_image_2_1_edit")

    def test_as18_lit_les_textes_d_origine(self):
        """Une image n'est pas lue par AS-18 : la fiche du rush (et de ses ancêtres) l'est."""
        ado = self.rush("une adolescente de 15 ans en uniforme scolaire", "A teenage girl, 15 years old")
        retouche = self.rush("ajoute une lune", "add a moon", parent=ado)
        for ref in (ado, retouche):  # directement, ou à travers une retouche
            with self.assertRaises(ErreurSecurite):
                self.commander("she wears a bikini on the beach", [ref])
            self.commander("she reads a book in a library", [ref])  # scène sans terme sensible : autorisée
        with self.assertRaises(ErreurSecurite):
            self.plateau.commander_retouche(ado, "habille-la en lingerie", False, None, None, None, 24, None)
        adulte = self.rush("une pilote de mecha", "an adult woman pilot")
        self.commander("she wears a bikini on the beach", [adulte])  # adulte : autorisé

    def test_textes_origine_sans_boucle(self):
        a = self.rush("a")
        b = self.rush("b", parent=a, references=[a])
        fiche = json.loads((plateau.DOSSIER / f"{a}.json").read_text(encoding="utf-8"))
        fiche["parent"] = b  # cycle volontaire
        (plateau.DOSSIER / f"{a}.json").write_text(json.dumps(fiche), encoding="utf-8")
        self.assertEqual(sorted(plateau.textes_origine([b])), ["a", "b"])


class Reecriture(unittest.TestCase):
    def test_consigne_dediee_aux_references(self):
        appels = []

        @asynccontextmanager
        async def llm(modele=None):
            async def generer(messages, **_):
                appels.append(messages)
                return SimpleNamespace(texte="The woman from Picture 1 rides the dragon from Picture 2.",
                                       tokens_sortie=10, ecriture_tps=1.0)
            yield SimpleNamespace(generer=generer, modele=None)

        r = asyncio.run(prompt.reecrire(SimpleNamespace(llm=llm), "la femme de l'image 1 sur le dragon de l'image 2",
                                        None, references=2))
        systeme, utilisateur = appels[0]
        self.assertTrue(systeme["content"].startswith("ROLE:\nYou write instructions for an image model that receives"))
        self.assertTrue(systeme["content"].endswith(prompt.CLAUSE_MINEURS))
        self.assertIn("(Reference pictures: 2)", utilisateur["content"])
        self.assertIn("Picture 2", r["prompt"])


if __name__ == "__main__":
    unittest.main()
