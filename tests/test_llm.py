"""Génération avec llama-server (app/llm.py) : budget de réflexion, réponses vides, réponse en flux.

llama-server est simulé (httpx.MockTransport, morceaux « data: » comme le vrai) : rien n'est lancé.
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
        """Renvoie (réponse, corps envoyé) ; `contenu` : la réponse simulée de llama-server, envoyée en morceaux."""
        envoyes = []

        def repondre(requete):
            envoyes.append(json.loads(requete.content))
            deltas = [{"role": "assistant", "content": None}, {"reasoning_content": "Hmm…"}]
            deltas += [{"content": m} for m in ([contenu[:4], contenu[4:]] if contenu else [contenu])]
            morceaux = [{"choices": [{"delta": d, "finish_reason": None}]} for d in deltas]
            morceaux += [{"choices": [{"delta": {}, "finish_reason": fin}]},
                         {"choices": [], "usage": {"prompt_tokens": 700, "completion_tokens": 512},
                          "timings": {"predicted_per_second": 61.04}}]
            flux = "".join(f"data: {json.dumps(m)}\n\n" for m in morceaux) + "data: [DONE]\n\n"
            return httpx.Response(200, text=flux, headers={"content-type": "text/event-stream"})

        llama = Llama(Path("llama-server.exe"), 8095, 4096, False)
        llama.reflexion_active = reflexion
        client = lambda **kw: VRAI_CLIENT(transport=httpx.MockTransport(repondre), **kw)  # noqa: E731
        with mock.patch.object(llm.httpx, "AsyncClient", client):
            reponse = asyncio.run(llama.generer([{"role": "user", "content": "x"}], max_tokens=512, **options))
        return reponse, envoyes[0]

    def test_sans_reflexion_la_limite_reste_celle_de_la_reponse(self):
        reponse, corps = self.generer(" Un prompt. ")
        self.assertEqual(reponse.texte, "Un prompt.")  # morceaux recollés, réflexion écartée
        self.assertEqual(corps["max_tokens"], 512)
        self.assertEqual(corps["chat_template_kwargs"], {"enable_thinking": False})

    def test_la_reflexion_a_son_propre_budget(self):
        _, corps = self.generer("Un prompt.", reflexion=True)
        self.assertEqual(corps["max_tokens"], 512 + BUDGET_REFLEXION)
        self.assertNotIn("chat_template_kwargs", corps)

    def test_en_flux_pour_pouvoir_annuler(self):
        """Non streamée, une génération annulée continuait 90 s dans le vide (PrismML, mesuré le 2026-11-03)."""
        reponse, corps = self.generer("Un prompt.")
        self.assertTrue(corps["stream"])
        self.assertEqual((reponse.tokens_entree, reponse.tokens_sortie, reponse.ecriture_tps), (700, 512, 61.0))

    def test_reflexion_coupee_pour_une_demande(self):
        _, corps = self.generer("Sunset light.", reflexion=True, reflechir=False)
        self.assertEqual(corps["chat_template_kwargs"], {"enable_thinking": False})
        self.assertEqual(corps["max_tokens"], 512 + BUDGET_REFLEXION)  # au cas où le gabarit l'ignore

    def test_le_contexte_tient_la_reflexion(self):
        """Banc du 2026-11-06 : 4096 tokens de réflexion laissaient 5 réponses vides sur 10 ; il en faut 8192, et le
        contexte d'un LLM qui réfléchit doit les contenir avec la consigne, les images et la réponse."""
        from app.orchestrateur import CONTEXTE_REFLEXION
        self.assertGreaterEqual(BUDGET_REFLEXION, 8192)
        self.assertGreaterEqual(CONTEXTE_REFLEXION, BUDGET_REFLEXION + 4096)

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
