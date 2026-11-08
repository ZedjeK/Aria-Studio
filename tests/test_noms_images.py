"""Comment chaque modèle désigne ses images de référence dans un prompt (profil_prompt.nom_image de sa fiche) : Qwen-Image
2.1 par indice, `<image1>` (doc ComfyUI et enhancer d'édition officiel) ; les autres gardent « Picture N », faute de
convention connue. Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio
import json
import unittest

from app import prompt, workflows
from tests.test_libre import FauxLlm, TOUR

QWEN_21 = ("qwen_image_2_1", "qwen_image_2_1_edit")
SANS_NOM = ("qwen_image_edit_2511", "krea2_turbo", "minimax_h3_i2v")


class NomDesImages(unittest.TestCase):
    def test_qwen_21_par_indice(self):
        for id_wf in QWEN_21:
            self.assertEqual(prompt.avec_noms_images("the woman from Picture 1 and the dragon from Picture 2", id_wf),
                             "the woman from <image1> and the dragon from <image2>", id_wf)

    def test_les_autres_gardent_picture(self):
        texte = "the woman from Picture 1"
        for id_wf in (*SANS_NOM, None, "inconnu"):
            self.assertEqual(prompt.avec_noms_images(texte, id_wf), texte, id_wf)

    def test_seules_les_fiches_qwen_21_declarent_un_nom(self):
        declares = {w.id for w in workflows.catalogue() if w.profil.get("nom_image")}
        self.assertEqual(declares, set(QWEN_21))


class Consigne(unittest.TestCase):
    def test_la_consigne_des_references_suit_le_modele(self):
        qwen = prompt.consigne("references", workflow="qwen_image_2_1_edit")
        self.assertIn("numbered <image1>, <image2>, <image3>", qwen)
        self.assertIn('"the woman from <image1>"', qwen)
        self.assertNotIn("Picture 1", qwen)
        defaut = prompt.consigne("references", workflow="qwen_image_edit_2511")
        self.assertIn("numbered Picture 1, Picture 2, Picture 3", defaut)
        self.assertEqual(prompt.consigne("references"), defaut)

    def test_une_autre_consigne_n_est_pas_touchee(self):
        self.assertEqual(prompt.consigne("reecriture", workflow="qwen_image_2_1"), prompt.consigne("reecriture"))


class VuesEtChat(unittest.TestCase):
    VUES = [prompt.Vue(b"", "A woman."), prompt.Vue(b"", "A dragon.")]

    def test_les_images_lues_dans_leur_fiche(self):
        texte = prompt.montrer("la femme sur le dragon", self.VUES, "references", voit=False,
                               workflow="qwen_image_2_1")
        self.assertIn("<image1>: A woman.\n<image2>: A dragon.", texte)
        self.assertNotIn("Picture", texte)
        sans = prompt.montrer("la femme sur le dragon", self.VUES, "references", voit=False)
        self.assertIn("Picture 1: A woman.\nPicture 2: A dragon.", sans)

    def test_le_chat_nomme_les_images_du_modele(self):
        for id_wf, attendu in (("qwen_image_2_1_edit", "<image1>"), ("qwen_image_edit_2511", "Picture 1")):
            o = FauxLlm(json.dumps(TOUR))
            asyncio.run(prompt.libre(o, [{"role": "user", "content": "Mets-la sur une plage"}], "image", id_wf,
                                     vues=[prompt.Vue(b"PNG", "A woman.")], role="inspirer"))
            self.assertIn(attendu, o.appels[0][0][0]["content"], id_wf)


class NoteDesSteps(unittest.TestCase):
    def test_la_phrase_de_la_doc_n_est_plus_citee(self):
        """« Audio needs 12+ steps » n'est dans aucune page de la doc de H3 (relu le 2026-11-08) : les fiches disent
        que le 12 est un réglage d'Aria, à l'écoute."""
        for id_wf in ("minimax_h3_t2v", "minimax_h3_i2v"):
            notes = " ".join(v["note"] for v in workflows.obtenir(id_wf).fiche["recommandations"]["valeurs"])
            self.assertNotIn("Audio needs 12+", notes, id_wf)
            self.assertIn("Réglage d'Aria", notes, id_wf)


if __name__ == "__main__":
    unittest.main()
