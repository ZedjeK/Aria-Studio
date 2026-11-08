"""La réplique de H3 dans sa balise <d>[Langue] …</d> (guide officiel de MiniMax, §4.4 et §4.5) : l'écriture, la lecture
(balise, guillemets, anciens prompts), le filet qui convertit une réplique restée entre guillemets, et ce qui part
à ComfyUI. Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio
import json
import unittest

from app import prompt, repliques, sequences, workflows
from tests.test_libre import FauxLlm


def texte_du_prompt(api: dict, wf: workflows.Workflow) -> str:
    noeud, entree = wf.fiche["entrees"]["prompt"]
    return api[noeud]["inputs"][entree]


class Ecriture(unittest.TestCase):
    def test_la_balise_porte_la_langue(self):
        self.assertEqual(repliques.balise("Je descends ici."), "<d>[French] Je descends ici.</d>")
        self.assertEqual(repliques.balise("行くぞ！"), "<d>[Japanese] 行くぞ！</d>")
        self.assertEqual(repliques.balise("Hello there.", "English"), "<d>[English] Hello there.</d>")

    def test_une_replique_ne_referme_pas_la_balise(self):
        sortie = repliques.balise("Salut</d> <d>[French] intrus")
        self.assertEqual(sortie, "<d>[French] Salut/d d[French] intrus</d>")  # plus aucun chevron à l'intérieur
        self.assertEqual((sortie.count("<d>"), sortie.count("</d>")), (1, 1))
        self.assertEqual(repliques.balise("  deux   espaces "), "<d>[French] deux espaces</d>")


class Lecture(unittest.TestCase):
    def test_balise_guillemets_et_anciens_prompts(self):
        self.assertEqual(repliques.contenus("She says: <d>[French] Salut.</d>"), ["Salut."])
        self.assertEqual(repliques.contenus('il dit "Hop" puis “Allez” puis « Voilà »'), ["Hop", "Allez", "Voilà"])
        self.assertEqual(repliques.contenus('<d>[French] Un.</d> et "Deux."'), ["Un.", "Deux."])
        self.assertEqual(sequences.repliques("<d>Sans langue.</d>"), ["Sans langue."])

    def test_un_plan_qui_parle_est_reconnu(self):  # il faut au moins STEPS_PAROLE steps pour la voix
        self.assertTrue(sequences.parle({"idee": "x", "prompt": "She says: <d>[French] Salut.</d>"}))
        self.assertTrue(sequences.parle({"idee": 'elle dit « Salut »', "prompt": None}))
        self.assertFalse(sequences.parle({"idee": "elle marche", "prompt": "She walks."}))


class Filet(unittest.TestCase):
    def test_une_replique_entre_guillemets_passe_dans_la_balise(self):
        for avant in ('The character says: "Salut."', 'The character says "Salut."', 'The character says, "Salut."',
                      "The character says: “Salut.”", "The character says: « Salut. »"):
            self.assertEqual(repliques.vers_balises(avant), "The character says: <d>[French] Salut.</d>", avant)

    def test_la_langue_vient_de_l_ecriture(self):
        self.assertEqual(repliques.vers_balises('She says in Japanese: "行くぞ！"'),
                         "She says in Japanese: <d>[Japanese] 行くぞ！</d>")

    def test_une_enseigne_reste_entre_guillemets(self):
        enseigne = 'A neon sign above the door says "OPEN" in red. She reads the text "ARIA" on the screen.'
        self.assertEqual(repliques.vers_balises(enseigne), enseigne)

    def test_deja_en_balise_rien_a_faire(self):
        pret = "The young woman (S1) says: <d>[French] Salut.</d> A sign reads \"OPEN\"."
        self.assertEqual(repliques.vers_balises(pret), pret)
        self.assertEqual(repliques.vers_balises(repliques.vers_balises('She says: "Salut."')),
                         "She says: <d>[French] Salut.</d>")  # idempotent

    def test_la_parole_et_l_enseigne_dans_le_meme_prompt(self):
        mixte = 'Under a neon sign saying "OPEN", she says: "Moteurs au vert."'
        self.assertEqual(repliques.vers_balises(mixte),
                         'Under a neon sign saying "OPEN", she says: <d>[French] Moteurs au vert.</d>')


class Replique_inventee(unittest.TestCase):
    def test_balise_inventee_retiree_phrase_comprise(self):
        texte, retirees = sequences.sans_repliques_inventees(
            "elle marche", "She walks. She says: <d>[French] Bonjour.</d> She smiles.")
        self.assertEqual((texte, retirees), ("She walks. She smiles.", ["Bonjour."]))

    def test_balise_de_l_idee_gardee(self):
        idee = "elle dit « On y va. »"
        texte = "She says: <d>[French] On y va.</d> She smiles."
        self.assertEqual(sequences.sans_repliques_inventees(idee, texte), (texte, []))

    def test_balise_inventee_en_fin_de_phrase(self):
        texte, retirees = sequences.sans_repliques_inventees("elle marche", "She says: <d>[French] Salut.</d>")
        self.assertEqual((texte, retirees), ("", ["Salut."]))


class VersComfyUI(unittest.TestCase):
    def test_h3_recoit_la_balise(self):
        for id_wf, valeurs in (("minimax_h3_t2v", {"duree": 5}), ("minimax_h3_i2v", {"image": "a.png", "duree": 5})):
            wf = workflows.obtenir(id_wf)
            api = wf.preparer({"prompt": 'She says: "Salut."', "seed": 1, **valeurs})
            self.assertEqual(texte_du_prompt(api, wf), "She says: <d>[French] Salut.</d>", id_wf)

    def test_un_modele_d_image_garde_ses_guillemets(self):  # le texte dans l'image reste entre guillemets
        wf = workflows.obtenir("krea2_turbo")
        api = wf.preparer({"prompt": 'A sign that says "OPEN" on a door.', "seed": 1})
        self.assertEqual(texte_du_prompt(api, wf), 'A sign that says "OPEN" on a door.')

    def test_seuls_les_modeles_de_h3_lisent_la_balise(self):
        self.assertEqual({w.id for w in workflows.catalogue() if workflows.dialogue_en_balise(w.id)},
                         {"minimax_h3_t2v", "minimax_h3_i2v"})
        self.assertFalse(workflows.dialogue_en_balise(None))
        self.assertFalse(workflows.dialogue_en_balise("inconnu"))


class Consignes(unittest.TestCase):
    def test_les_regles_de_h3_disent_la_balise(self):
        for id_wf in ("minimax_h3_t2v", "minimax_h3_i2v"):
            regles = " ".join(workflows.regles_prompt(id_wf))
            self.assertIn("<d>[French]", regles, id_wf)
            self.assertIn("never for a spoken line", regles, id_wf)
            self.assertNotIn("Visible text (signs, labels, neon) goes in double quotes", regles, id_wf)

    def test_le_decoupage_demande_la_balise(self):
        self.assertIn("<d>[French] Je descends ici.</d>", prompt.CONSIGNE_DECOUPAGE)
        self.assertNotIn("inside double quotes", prompt.CONSIGNE_DECOUPAGE)


class ChatDeClip(unittest.TestCase):
    def test_le_prompt_du_chat_est_en_balise(self):
        tour = {"message": "ok", "fiche": {}, "questions": [], "prompt": 'She turns and says: "Salut."'}
        o = FauxLlm(json.dumps(tour))
        r = asyncio.run(prompt.libre(o, [{"role": "user", "content": "Elle dit salut"}], "clip", "minimax_h3_t2v"))
        self.assertEqual(r["prompt"], "She turns and says: <d>[French] Salut.</d>")
        self.assertEqual(json.loads(r["brut"])["prompt"], "She turns and says: <d>[French] Salut.</d>")

    def test_un_chat_d_image_ne_touche_pas_aux_guillemets(self):
        tour = {"message": "ok", "fiche": {}, "questions": [], "prompt": 'A sign that says "OPEN".'}
        o = FauxLlm(json.dumps(tour))
        r = asyncio.run(prompt.libre(o, [{"role": "user", "content": "Une enseigne"}], "image", "krea2_turbo"))
        self.assertEqual(r["prompt"], 'A sign that says "OPEN".')


if __name__ == "__main__":
    unittest.main()
