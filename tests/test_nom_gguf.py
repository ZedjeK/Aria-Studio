"""Garde en profondeur : un GGUF ou son projecteur se désigne par un nom de fichier, jamais par un chemin.

Le nom devient l'argument -m (ou --mmproj) de llama-server, construit sous modeles/llm. Les routes valident déjà le nom
contre les fichiers présents ; ce test vérifie que le passage obligé (Orchestrateur.llm, lancement_llm) refuse seul un
nom qui sortirait du dossier, avant de lire un fichier ou de lancer quoi que ce soit.
"""

import asyncio
import unittest
from unittest import mock

from app import orchestrateur
from app.llm import ErreurLlama
from app.orchestrateur import Orchestrateur, lancement_llm, nom_de_fichier

CHEMINS = (r"..\x.gguf", "../x.gguf", r"C:\Windows\x.gguf", "C:x.gguf", "a/b.gguf", r"a\b.gguf", "..", ".",
           r"\\serveur\partage\x.gguf", "/etc/x.gguf")
NOMS = ("Bonsai-2-27B.gguf", "Qwen3.5 9B.Q4_K_M.gguf", "ternary-bonsai-2-27b-mmproj-Q8_0.gguf", "é.gguf")


class NomDeFichier(unittest.TestCase):
    def test_les_noms_de_fichier_passent(self):
        for nom in NOMS:
            self.assertEqual(nom_de_fichier(nom), nom)

    def test_les_chemins_sont_refuses(self):
        for nom in CHEMINS + ("",):
            with self.subTest(nom=nom), self.assertRaisesRegex(ErreurLlama, "Nom de GGUF invalide"):
                nom_de_fichier(nom)

    def test_sans_nom_c_est_le_llm_par_defaut(self):
        """prompt.py demande lancement_llm(None) : le LLM par défaut, pas une erreur."""
        for nom in (None, ""):
            self.assertIn("contexte", lancement_llm(nom))

    def test_lancement_llm_refuse_avant_toute_lecture(self):
        with mock.patch.object(orchestrateur.reglages, "valeurs_llm", side_effect=AssertionError("lu")):
            for nom in CHEMINS:
                with self.subTest(nom=nom), self.assertRaises(ErreurLlama):
                    lancement_llm(nom)

    def test_le_projecteur_aussi(self):
        with mock.patch.object(orchestrateur.reglages, "projecteur_llm", return_value=(r"..\x.gguf", "essai")), \
                mock.patch.object(orchestrateur.fiches, "specialiste", return_value=None), \
                mock.patch.object(orchestrateur.reglages, "moteur_llm", return_value=("llama", "essai")), \
                self.assertRaisesRegex(ErreurLlama, "Nom de GGUF invalide"):
            lancement_llm("Bonsai-2-27B.gguf")

    def test_l_orchestrateur_refuse_sans_arreter_ni_lancer_un_moteur(self):
        o = Orchestrateur()

        async def essai(nom):
            async with o.llm(nom):
                pass

        with mock.patch.object(o.llama, "demarrer", side_effect=AssertionError("llama lancé")), \
                mock.patch.object(o.llama, "arreter", side_effect=AssertionError("llama arrêté")), \
                mock.patch.object(o.comfy, "est_pret", side_effect=AssertionError("comfy sondé")):
            for nom in CHEMINS:
                with self.subTest(nom=nom), self.assertRaises(ErreurLlama):
                    asyncio.run(essai(nom))


if __name__ == "__main__":
    unittest.main()
