"""MiniMax H3 et la durée du clip : la consigne dit la durée, et les heures de coupe sont contrôlées côté serveur.

Règles du guide officiel de base (§4.2, relu mot pour mot le 2026-11-12) : pas d'heure sur le premier plan, puis une heure
strictement croissante qui tombe dans la durée de la vidéo ; une coupe apporte une information nouvelle. Aucun nombre
d'actions ni de répliques par durée : aucune source n'en donne.
"""

import asyncio
import unittest
from contextlib import asynccontextmanager
from types import SimpleNamespace

from app import prompt

DEUX_PLANS = ("integrated_multimodal_description: [Shot 1] A woman stands at a window. [Shot 2] At 00:03.500, the "
              "camera cuts to her hands.\n\noverall_soundscape: Rain.\n\nnon_diegetic_music: N/A")


class ControleDesCoupes(unittest.TestCase):
    def test_des_coupes_dans_la_duree_ne_donnent_aucune_alerte(self):
        self.assertEqual(prompt.controler_coupes(DEUX_PLANS, 5), [])
        trois = ("[Shot 1] A. [Shot 2] At 00:03.500, the camera cuts to B. "
                 "[Shot 3] At 00:09.000, the shot cuts to C.")
        self.assertEqual(prompt.controler_coupes(trois, 10), [])

    def test_une_coupe_apres_la_fin_du_clip(self):
        alertes = prompt.controler_coupes("[Shot 1] A. [Shot 2] At 00:07.000, the camera cuts to B.", 5)
        self.assertEqual(len(alertes), 1)
        self.assertIn("coupe à 00:07.000", alertes[0])
        self.assertIn("après la fin du clip (5 s)", alertes[0])

    def test_une_coupe_a_la_seconde_exacte_de_la_fin_est_dehors(self):
        self.assertEqual(len(prompt.controler_coupes("[Shot 1] A. [Shot 2] At 00:05.000, cut to B.", 5)), 1)

    def test_des_coupes_dans_le_desordre(self):
        texte = "[Shot 1] A. [Shot 2] At 00:06.000, cut to B. [Shot 3] At 00:04.000, cut to C."
        alertes = prompt.controler_coupes(texte, 10)
        self.assertEqual(len(alertes), 1)
        self.assertIn("Plan 3", alertes[0])
        self.assertIn("pas après celle du plan précédent (00:06.000)", alertes[0])

    def test_deux_coupes_a_la_meme_heure(self):
        texte = "[Shot 1] A. [Shot 2] At 00:04.000, cut to B. [Shot 3] At 00:04.000, cut to C."
        self.assertEqual(len(prompt.controler_coupes(texte, 10)), 1)

    def test_le_premier_plan_n_a_pas_d_heure(self):
        alertes = prompt.controler_coupes("[Shot 1] At 00:00.000, A. [Shot 2] At 00:03.000, cut to B.", 5)
        self.assertEqual(len(alertes), 1)
        self.assertIn("premier plan", alertes[0])

    def test_un_plan_suivant_sans_heure(self):
        alertes = prompt.controler_coupes("[Shot 1] A. [Shot 2] The camera cuts to B.", 5)
        self.assertEqual(len(alertes), 1)
        self.assertIn("Plan 2", alertes[0])
        self.assertIn("son heure", alertes[0])

    def test_sans_coupe_ou_sans_duree_rien_a_dire(self):
        self.assertEqual(prompt.controler_coupes("A woman stands at a window.", 5), [])
        self.assertEqual(prompt.controler_coupes("integrated_multimodal_description: [Shot 1] A woman.", 5), [])
        # sans durée, l'ordre se contrôle encore, la fin du clip non
        self.assertEqual(prompt.controler_coupes("[Shot 1] A. [Shot 2] At 00:30.000, cut to B.", None), [])

    def test_une_heure_avec_minutes_et_fraction_courte(self):
        self.assertEqual(len(prompt.controler_coupes("[Shot 1] A. [Shot 2] At 01:02.5, cut to B.", 15)), 1)  # 62,5 s
        self.assertEqual(prompt.controler_coupes("[Shot 1] A. [Shot 2] At 00:02, cut to B.", 5), [])  # sans fraction


class ConsigneEtDuree(unittest.TestCase):
    def test_la_consigne_dit_la_duree_du_clip(self):
        avec = prompt.consigne("video", None, 224, duree=8)
        self.assertIn("Clip length: 8 seconds.", avec)
        self.assertNotIn("strictly increase", avec)  # sans les champs de H3, pas d'heures de coupe
        self.assertTrue(avec.endswith(prompt.CLAUSE_MINEURS))

    def test_avec_les_champs_de_h3_les_heures_de_coupe_sont_bornees(self):
        avec = prompt.consigne("video", None, 224, champs=True, duree=8)
        self.assertIn("Clip length: 8 seconds. If you write cuts, their times", avec)
        self.assertIn("all fall inside these 8 seconds; Shot 1 has no time.", avec)

    def test_sans_duree_la_consigne_ne_change_pas(self):
        self.assertNotIn("Clip length", prompt.consigne("video", None, 110))
        self.assertNotIn("Clip length", prompt.consigne("reecriture", None, 120, duree=8))  # une image n'a pas de durée

    def test_une_coupe_doit_apporter_une_information_nouvelle(self):
        """Guide officiel §4.2 : sinon, un mouvement de caméra."""
        texte = prompt.consigne("video")
        self.assertIn("new information about the subject, the space, the state, the viewpoint or the time", texte)
        self.assertIn("use a camera movement instead", texte)


class ReecritureDUnClip(unittest.TestCase):
    def reecrire(self, reponse: str, workflow: str | None, duree: int):
        appels = []

        @asynccontextmanager
        async def llm(modele=None):
            async def generer(messages, **_):
                appels.append(messages)
                return SimpleNamespace(texte=reponse, tokens_sortie=12, ecriture_tps=1.0)
            yield SimpleNamespace(generer=generer, modele=None)

        r = asyncio.run(prompt.reecrire(SimpleNamespace(llm=llm), "elle regarde la pluie", None, video="texte",
                                        duree=duree, workflow=workflow))
        return r, appels[0][0]["content"]

    def test_h3_rend_les_alertes_et_dit_la_duree_au_llm(self):
        r, systeme = self.reecrire(DEUX_PLANS, "minimax_h3_t2v", 5)
        self.assertEqual(r["alertes"], [])
        self.assertIn("Clip length: 5 seconds. If you write cuts", systeme)
        r, _ = self.reecrire(DEUX_PLANS.replace("00:03.500", "00:08.000"), "minimax_h3_t2v", 5)
        self.assertEqual(len(r["alertes"]), 1)
        self.assertIn("après la fin du clip (5 s)", r["alertes"][0])

    def test_la_duree_du_clip_n_est_pas_celle_de_l_ecriture(self):
        """`duree` sert aussi à chronométrer le LLM : l'alerte doit viser la durée du clip, pas ce temps-là."""
        r, _ = self.reecrire(DEUX_PLANS.replace("00:03.500", "00:09.000"), "minimax_h3_t2v", 10)
        self.assertEqual(r["alertes"], [])  # 9 s dans un clip de 10 s

    def test_un_autre_modele_n_a_pas_d_alertes_de_coupe(self):
        r, systeme = self.reecrire("A woman looks at the rain.", None, 8)
        self.assertNotIn("alertes", r)
        self.assertIn("Clip length: 8 seconds.", systeme)


if __name__ == "__main__":
    unittest.main()
