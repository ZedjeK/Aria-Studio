"""✕ Annuler du Scénario (✨ Écrire le prompt, 💬 M'aider à préciser, ✨ Tout écrire) : la page coupe la connexion, et
le serveur annule l'appel au LLM (services.tant_que_connecte) au lieu de le laisser écrire dans le vide. Le LLM et la
requête sont simulés : rien n'est lancé."""

import asyncio
import unittest
from unittest import mock

from fastapi import HTTPException

from app import plateau, prompt, routes_scenario, services


class Requete:
    """Une requête dont la page se déconnecte au bout de `apres` vérifications (None : jamais)."""

    def __init__(self, apres=None):
        self.apres, self.verifications = apres, 0

    async def is_disconnected(self):
        self.verifications += 1
        return self.apres is not None and self.verifications >= self.apres


class Lent:
    """Un travail qui dure, et qui note s'il a été annulé."""

    def __init__(self, duree=5.0, resultat="fini"):
        self.duree, self.resultat, self.annule = duree, resultat, False

    async def __call__(self, *args, **kwargs):
        try:
            await asyncio.sleep(self.duree)
            return self.resultat
        except asyncio.CancelledError:
            self.annule = True
            raise


class TantQueConnecte(unittest.TestCase):
    def test_sans_requete_attend_simplement(self):
        self.assertEqual(asyncio.run(services.tant_que_connecte(None, Lent(0.01)())), "fini")

    def test_connecte_jusqu_au_bout(self):
        self.assertEqual(asyncio.run(services.tant_que_connecte(Requete(), Lent(0.6)())), "fini")

    def test_page_coupee_annule_le_travail(self):
        travail = Lent()
        with self.assertRaises(HTTPException) as e:
            asyncio.run(services.tant_que_connecte(Requete(apres=1), travail()))
        self.assertEqual(e.exception.status_code, services.ANNULEE)
        self.assertTrue(travail.annule)  # en flux, llama-server s'arrête avec lui

    def test_les_erreurs_du_travail_remontent(self):
        async def rate():
            raise ValueError("boom")

        with self.assertRaisesRegex(ValueError, "boom"):
            asyncio.run(services.tant_que_connecte(Requete(), rate()))


class Routes(unittest.TestCase):
    def test_chat_annule(self):
        libre = Lent()
        with mock.patch.object(prompt, "libre", libre), mock.patch.object(plateau, "textes_origine", lambda ids: []):
            with self.assertRaises(HTTPException) as e:
                asyncio.run(routes_scenario.chat_libre(routes_scenario.DemandeLibre(
                    echanges=[{"role": "user", "content": "Un chat roux"}]), Requete(apres=1)))
        self.assertEqual(e.exception.status_code, services.ANNULEE)
        self.assertTrue(libre.annule)

    def test_ecrire_le_prompt_annule(self):
        reecrire = Lent()
        with mock.patch.object(prompt, "reecrire", reecrire), mock.patch.object(plateau, "textes_origine", lambda ids: []):
            with self.assertRaises(HTTPException) as e:
                asyncio.run(routes_scenario.optimiser_prompt(routes_scenario.DemandeOptimisation(
                    atelier="image", idee="un chat roux"), Requete(apres=1)))
        self.assertEqual(e.exception.status_code, services.ANNULEE)
        self.assertTrue(reecrire.annule)

    def test_ecrire_le_prompt_avec_ce_que_le_chat_a_compris(self):
        """✍️ Écris-le du chat passe par le même rédacteur que ✨ : l'idée et la fiche arrivent telles quelles."""
        recu = {}

        async def reecrire(orchestrateur, idee, llm, references=0, **options):
            recu["idee"] = idee
            return {"prompt": "A ginger cat asleep on a windowsill, photo.", "duree_s": 1.0}

        idee = "Un chat roux\n\nPrécisé ensemble :\n- Style : photo réaliste\n- Lieu : rebord de fenêtre"
        with mock.patch.object(prompt, "reecrire", reecrire), mock.patch.object(plateau, "textes_origine", lambda ids: []):
            r = asyncio.run(routes_scenario.optimiser_prompt(routes_scenario.DemandeOptimisation(
                atelier="image", idee=idee), Requete()))
        self.assertEqual(recu["idee"], idee)
        self.assertEqual(r["mots"], 8)


if __name__ == "__main__":
    unittest.main()
