"""Le Plateau et le Montage tournent le prompt tel quel : le LLM n'écrit qu'au Scénario (refonte du 2026-11-06).
Origine : la réécriture libre du Plateau et du Montage déformait le brouillon de Jérémy. Rien n'est lancé."""

import unittest
from types import SimpleNamespace
from unittest import mock

from fastapi import HTTPException

from app import main


class Plateau(unittest.TestCase):
    def creer(self, **champs):
        appels = []

        def commander(*args):
            appels.append(args)
            return SimpleNamespace(to_dict=lambda: {"id": "c1"})

        with mock.patch.object(main.plateau_, "commander", commander):
            main.creer(main.DemandeCreation(**champs))
        return appels[0]

    def test_prompt_tourne_tel_quel_sans_llm(self):
        idee, prompt_force, llm = self.creer(prompt="  A ginger cat asleep on a windowsill at night.  ")[:3]
        self.assertEqual(prompt_force, "A ginger cat asleep on a windowsill at night.")
        self.assertIsNone(llm)  # jamais de LLM, même si la demande en nommait un
        self.assertEqual(idee, "  A ginger cat asleep on a windowsill at night.  ")

    def test_prompt_vide_refuse(self):
        with self.assertRaises(HTTPException) as e:
            self.creer(idee="un chat roux", prompt=" ")
        self.assertEqual(e.exception.status_code, 400)
        self.assertIn("Scénario", e.exception.detail)

    def test_plus_de_champ_llm(self):
        self.assertNotIn("llm", main.DemandeCreation.model_fields)


if __name__ == "__main__":
    unittest.main()
