"""Scénario, onglet Image de référence : POST /api/prompt/ecrire (app/main.py) choisit la bonne écriture selon le rôle
des images, et le filtre AS-18 tient compte de leur origine. Le LLM est remplacé par un faux : rien n'est lancé."""

import asyncio
import unittest
from unittest import mock

from fastapi import HTTPException

from app import main, plateau


class Ecrire(unittest.TestCase):
    def appeler(self, role, images, prompt_="A ginger cat wakes up.", origines=()):
        appels = []

        async def reecrire(orchestrateur, idee, llm, references=0, **options):
            appels.append({"idee": idee, "references": references, **options})
            return {"idee": idee, "prompt": "The cat from the picture stretches.", "modele": "m.gguf", "duree_s": 1.0,
                    "tokens_sortie": 10, "ecriture_tps": 50.0}

        demande = main.DemandeEcriture(prompt=prompt_, role=role, images=images, workflow="minimax_h3_t2v", duree=8)
        with mock.patch.object(main.prompt, "reecrire", reecrire), \
                mock.patch.object(plateau, "vue", lambda i: f"vue:{i}"), \
                mock.patch.object(plateau, "textes_origine", lambda ids: list(origines)):
            r = asyncio.run(main.ecrire_avec_images(demande))
        return r, appels[0] if appels else None

    def test_animer_un_rush(self):
        r, appel = self.appeler("animer", ["rush1"])
        self.assertEqual((appel["video"], appel["duree"], appel["references"], appel["vues"]), ("animer", 8, 0, ["vue:rush1"]))
        self.assertEqual(r["mots"], 6)

    def test_s_inspirer_numerote_les_images(self):
        _, appel = self.appeler("inspirer", ["a", "b", "c"])
        self.assertEqual((appel["references"], appel["video"], appel["vues"]), (3, None, ["vue:a", "vue:b", "vue:c"]))

    def test_variation(self):
        _, appel = self.appeler("variation", ["a"])
        self.assertEqual((appel["references"], appel["video"]), (0, None))

    def test_nombre_d_images_controle(self):
        for role, images in (("variation", ["a", "b"]), ("animer", []), ("inspirer", ["a", "b", "c", "d"]),
                             ("inconnu", ["a"])):
            with self.assertRaises(HTTPException, msg=role) as e:
                self.appeler(role, images)
            self.assertEqual(e.exception.status_code, 400)

    def test_mineur_de_l_image_d_origine(self):
        """AS-18 ne voit pas les images : un rush né d'un personnage mineur le reste dans le prompt qui l'utilise."""
        with self.assertRaises(HTTPException) as e:
            self.appeler("animer", ["rush1"], "She slowly undresses.", origines=["A teenage girl, 15 years old."])
        self.assertEqual(e.exception.status_code, 422)


if __name__ == "__main__":
    unittest.main()
