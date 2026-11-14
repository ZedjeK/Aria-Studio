"""Pastilles de la barre d'onglets (GET /api/etat) : « actif » = le moteur qui tient la carte, pas un simple processus en vie.

ComfyUI reste lancé quand un LLM prend la main (sa VRAM est vidée par /free, son processus n'est pas arrêté) : il doit
alors s'afficher en veille, sinon les deux pastilles s'allument ensemble. Rien n'est lancé : les processus sont simulés.
"""

import unittest
from unittest import mock

from fastapi.testclient import TestClient

from app import jeton, main
from app.config import CONFIG

LOCAL = f"http://127.0.0.1:{CONFIG['app']['port']}"


def vivant():
    processus = mock.Mock()
    processus.poll.return_value = None
    return processus


class EtatMoteurs(unittest.TestCase):
    def setUp(self):
        self.client = TestClient(main.app, base_url=LOCAL, headers={jeton.EN_TETE: jeton.JETON})

    def etat(self, actif, comfy, llama, installe=True):
        sante = {"comfyui": {"installe": installe}, "llama": {"installe": installe}}
        with mock.patch.object(main, "sante", return_value=sante), \
                mock.patch.object(main.orchestrateur, "actif", actif), \
                mock.patch.object(main.orchestrateur.comfy, "process", comfy), \
                mock.patch.object(main.orchestrateur.llama, "process", llama), \
                mock.patch.object(main.orchestrateur.llama, "modele", None), \
                mock.patch.object(main.mesures, "vram_utilisee", return_value=2**30), \
                mock.patch.object(main.mesures, "vram_totale", return_value=16 * 2**30):
            donnees = self.client.get("/api/etat").json()
        return donnees["comfyui"]["etat"], donnees["llama"]["etat"]

    def test_le_llm_tient_la_carte_comfyui_encore_lance_est_en_veille(self):
        self.assertEqual(self.etat("llama", vivant(), vivant()), ("veille", "actif"))

    def test_comfyui_tient_la_carte(self):
        self.assertEqual(self.etat("comfyui", vivant(), None), ("actif", "veille"))

    def test_carte_liberee_les_deux_en_veille(self):
        self.assertEqual(self.etat(None, vivant(), None), ("veille", "veille"))

    def test_moteur_introuvable(self):
        self.assertEqual(self.etat(None, None, None, installe=False), ("absent", "absent"))


if __name__ == "__main__":
    unittest.main()
