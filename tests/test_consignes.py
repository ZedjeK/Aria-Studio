"""Consignes v2 (app/prompt.py) : fidélité d'abord, contenu adulte gardé, longueur de la fiche du modèle cible.

Origine : le LLM déformait le brouillon (médium, sons et caméra inventés, idée adoucie, consignes de l'utilisateur
recopiées ou ignorées). Rien n'est lancé : on lit seulement le texte envoyé au LLM.
"""

import unittest

from app import prompt


class Consignes(unittest.TestCase):
    def test_fidelite_dans_les_reecritures(self):
        for nom in ("reecriture", "video", "references"):
            texte = prompt.consigne(nom)
            self.assertIn("FIXED", texte, nom)
            self.assertIn("never copy them into the prompt", texte, nom)  # ses consignes s'appliquent, sans écho
            self.assertIn(prompt.ADULTES, texte, nom)

    def test_contenu_adulte_jamais_adouci(self):
        for nom in ("reecriture", "decoupage"):
            self.assertIn(prompt.ADULTES, prompt.consigne(nom), nom)
        self.assertIn("never make a character younger", prompt.CLAUSE_MINEURS)  # la clause mineurs reste la dernière
        self.assertTrue(prompt.consigne("reecriture").endswith(prompt.CLAUSE_MINEURS))

    def test_medium_jamais_change(self):
        self.assertIn("never switch to painting or illustration on your own", prompt.consigne("reecriture"))

    def test_pas_de_musique_inventee(self):
        self.assertIn("No background music unless the user asks for it", prompt.consigne("video"))
        self.assertIn("No background music unless the user asks for it", prompt.consigne("decoupage"))

    def test_longueur_de_la_fiche(self):
        """Le réécrivain officiel de Qwen-Image 2.1 vise 400 à 500 mots : la consigne en imposait 40 à 120 à tous."""
        self.assertEqual(prompt.mots_max_de("qwen_image_2_1"), 450)
        self.assertIn("40 to 450 words", prompt.messages("une pomme", None, None, 450)[0]["content"])
        self.assertIn("40 to 120 words", prompt.consigne("reecriture"))  # sans modèle cible
        self.assertIsNone(prompt.mots_max_de("inconnu"))

    def test_animer_ancre_la_premiere_image(self):
        """Guide d'écriture de MiniMax H3 : la première image est d'abord ancrée, puis l'action."""
        self.assertIn("anchoring it as it really is", prompt.NOTES_VUES["animer"])

    def test_longueur_d_un_clip_selon_sa_duree(self):
        self.assertEqual(prompt.mots_pour_duree(110, 5), 110)
        self.assertEqual(prompt.mots_pour_duree(110, 3), 110)
        self.assertEqual(prompt.mots_pour_duree(110, 10), 150)
        self.assertEqual(prompt.mots_pour_duree(110, 15), 190)
        self.assertIsNone(prompt.mots_pour_duree(None, 10))

    def test_h3_en_trois_champs(self):
        self.assertTrue(prompt.workflows.champs_video("minimax_h3_t2v"))
        self.assertTrue(prompt.workflows.champs_video("minimax_h3_i2v"))
        self.assertFalse(prompt.workflows.champs_video("krea2_turbo"))
        self.assertFalse(prompt.workflows.champs_video(None))
        avec = prompt.consigne("video", None, 110, champs=True)
        for champ in ("integrated_multimodal_description", "overall_soundscape", "non_diegetic_music"):
            self.assertIn(champ, avec)
        self.assertNotIn("overall_soundscape", prompt.consigne("video", None, 110))
        self.assertTrue(avec.endswith(prompt.CLAUSE_MINEURS))

    def test_mise_en_champs(self):
        brut = ("A woman walks. overall_soundscape: Footsteps. non_diegetic_music: N/A")
        t2v = prompt.mettre_en_champs(brut)
        self.assertEqual(t2v.split("\n\n"), ["integrated_multimodal_description: [Shot 1] A woman walks.",
                                              "overall_soundscape: Footsteps.", "non_diegetic_music: N/A"])
        i2v = prompt.mettre_en_champs(brut, animer=True)
        self.assertTrue(i2v.startswith(prompt.ALIGNEMENT_I2V + "\n\n" + "integrated_multimodal_description:"))
        deja = "integrated_multimodal_description: [Shot 1] Walk.\n\noverall_soundscape: Steps."
        self.assertEqual(prompt.mettre_en_champs(deja), deja)  # idempotent, un champ omis reste omis

    def test_champs_separes_et_inseres(self):
        t = prompt.mettre_en_champs("Walk. overall_soundscape: Steps. non_diegetic_music: N/A", animer=True)
        self.assertEqual(prompt.separer_champs(t[len(prompt.ALIGNEMENT_I2V):]), ("Walk.", "Steps.", "N/A"))
        avec = prompt.dans_la_description(t, "Constraint: no rain.")
        self.assertTrue(avec.startswith(prompt.ALIGNEMENT_I2V))  # la ligne d'alignement reste en tête
        self.assertIn("[Shot 1] Walk. Constraint: no rain.\n\noverall_soundscape: Steps.", avec)
        debut = prompt.dans_la_description("integrated_multimodal_description: [Shot 1] Walk.", "lora_x,", debut=True)
        self.assertEqual(debut, "integrated_multimodal_description: [Shot 1] lora_x, Walk.")
        self.assertIsNone(prompt.dans_la_description("Walk in prose.", "x"))  # pas en champs : l'appelant ajoute en fin

    def test_decoupage_en_champs(self):
        from app import decoupage
        systeme = prompt.consigne("decoupage", None, 110, champs=True)
        self.assertIn("overall_soundscape", systeme)
        self.assertNotIn("overall_soundscape", prompt.consigne("decoupage", None, 110))
        plan = {"enchainement": "prolonger", "note": "", "replique": "", "moment": "fin", "off": False, "duree": 5}
        texte, _ = decoupage.finir_plan(
            2, plan, "integrated_multimodal_description: [Shot 1] She walks on.\n\noverall_soundscape: Steps.\n\n"
                     "non_diegetic_music: N/A", "", "", champs=True)
        self.assertEqual(texte, "integrated_multimodal_description: [Shot 1] Continuous shot, same scene. She walks on.\n\n"
                                "overall_soundscape: Steps.\n\nnon_diegetic_music: N/A")

    def test_tournage_garde_les_champs(self):
        from app import plateau, sequences
        p = "integrated_multimodal_description: [Shot 1] She walks.\n\noverall_soundscape: Steps.\n\nnon_diegetic_music: N/A"
        final = plateau.avec_declencheurs(sequences.avec_contrainte(p, "no rain"), [{"declencheurs": "lora_x"}])
        self.assertIn("[Shot 1] lora_x, She walks. Constraint for every shot: no rain.\n\noverall_soundscape: Steps.", final)
        self.assertTrue(final.endswith("non_diegetic_music: N/A"))
        ref = sequences._reference(final, True, True)
        self.assertIn("<Audio 1>.\n\noverall_soundscape", ref)
        self.assertEqual(sequences.avec_contrainte("She walks.", "no rain"), "She walks. Constraint for every shot: no rain.")

    def test_fiches_apres_audit_des_guides(self):
        from app import workflows
        for w in ("minimax_h3_t2v", "minimax_h3_i2v"):
            f = workflows.obtenir(w).fiche
            self.assertEqual(f["video"]["duree_max"], 15)
            self.assertNotIn("atlascloud", " ".join(f["profil_prompt"].get("sources", [])))
        self.assertIn("Restore and colorize the photo.", " ".join(workflows.regles_prompt("qwen_image_edit_2511")))


if __name__ == "__main__":
    unittest.main()
