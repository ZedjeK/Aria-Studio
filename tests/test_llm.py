"""Génération avec llama-server (app/llm.py) : budget de réflexion et réponses vides.

llama-server est simulé (httpx.MockTransport) : rien n'est lancé.
"""

import asyncio
import json
import unittest
from pathlib import Path
from unittest import mock

import httpx

from app import llm
from app.llm import BUDGET_REFLEXION, ErreurLlama, Llama

VRAI_CLIENT = httpx.AsyncClient  # capturé avant les remplacements (llm.httpx est le même module)


class Generer(unittest.TestCase):
    def generer(self, contenu, fin="stop", reflexion=False, **options):
        """Renvoie (texte, corps envoyé) ; `contenu` : la réponse simulée de llama-server."""
        envoyes = []

        def repondre(requete):
            envoyes.append(json.loads(requete.content))
            return httpx.Response(200, json={
                "choices": [{"message": {"content": contenu, "reasoning_content": "Hmm…"}, "finish_reason": fin}],
                "usage": {"prompt_tokens": 700, "completion_tokens": 512}, "timings": {}})

        llama = Llama(Path("llama-server.exe"), 8095, 4096, False)
        llama.reflexion_active = reflexion
        client = lambda **kw: VRAI_CLIENT(transport=httpx.MockTransport(repondre), **kw)  # noqa: E731
        with mock.patch.object(llm.httpx, "AsyncClient", client):
            reponse = asyncio.run(llama.generer([{"role": "user", "content": "x"}], max_tokens=512, **options))
        return reponse.texte, envoyes[0]

    def test_sans_reflexion_la_limite_reste_celle_de_la_reponse(self):
        texte, corps = self.generer(" Un prompt. ")
        self.assertEqual(texte, "Un prompt.")
        self.assertEqual(corps["max_tokens"], 512)
        self.assertEqual(corps["chat_template_kwargs"], {"enable_thinking": False})

    def test_la_reflexion_a_son_propre_budget(self):
        _, corps = self.generer("Un prompt.", reflexion=True)
        self.assertEqual(corps["max_tokens"], 512 + BUDGET_REFLEXION)
        self.assertNotIn("chat_template_kwargs", corps)

    def test_un_specialiste_compte_deja_sa_reflexion(self):
        _, corps = self.generer("{}", reflexion=True, budget_reflexion=0)
        self.assertEqual(corps["max_tokens"], 512)

    def test_reponse_vide_jamais_en_silence(self):
        """Le Bonsai 2 Heretic, réflexion active, rendait "" après 512 tokens : le Montage tournait sans prompt."""
        with self.assertRaisesRegex(ErreurLlama, "réfléchir.*Réflexion"):
            self.generer("", fin="length", reflexion=True)
        with self.assertRaisesRegex(ErreurLlama, "limite de 512 tokens"):
            self.generer("  ", fin="length")
        with self.assertRaisesRegex(ErreurLlama, "vide"):
            self.generer(None)


if __name__ == "__main__":
    unittest.main()
