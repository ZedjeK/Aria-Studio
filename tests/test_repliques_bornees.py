"""Répliques (app/repliques.py) : lues sans geler, sur un texte qui ouvre sans jamais fermer.

Sans borne, un « <d> » ou un « « » sans fermeture relisait tout le reste du texte : le temps était quadratique (32 000
caractères de « <d>[ » : 1,8 s ; 48 000 caractères de « says « » dans vers_balises : 5,7 s ; des blancs derrière un
« <d> » : bien pire encore). Chaque ouverture ne relit plus qu'au plus MAX_REPLIQUE caractères.
"""

import time
import unittest

from app import repliques


def duree(fonction, texte) -> float:
    debut = time.perf_counter()
    fonction(texte)
    return time.perf_counter() - debut


class LectureInchangee(unittest.TestCase):
    def test_les_quatre_ecritures(self):
        texte = 'Elle dit <d>[French] Bonjour.</d> puis « Merci » et "Salut" et “Au revoir”.'
        self.assertEqual(repliques.contenus(texte), ["Bonjour.", "Merci", "Salut", "Au revoir"])

    def test_la_langue_et_les_blancs_de_la_balise(self):
        self.assertEqual(repliques.contenus("<d>  [Korean]   안녕하세요.  </d>"), ["안녕하세요."])
        self.assertEqual(repliques.contenus("« \n Salut \n »"), ["Salut"])

    def test_une_replique_sur_plusieurs_lignes(self):
        self.assertEqual(repliques.contenus("<d>[French] Un.\nDeux.</d>"), ["Un.\nDeux."])

    def test_passer_en_balise_un_guillemet_derriere_un_verbe_de_parole(self):
        self.assertEqual(repliques.vers_balises('She says "Salut" to him.'),
                         "She says: <d>[French] Salut</d> to him.")
        self.assertEqual(repliques.vers_balises('A neon sign says "OPEN" above the door.'),
                         'A neon sign says "OPEN" above the door.')

    def test_la_borne_est_celle_d_une_idee_de_plan(self):
        longue = "a" * repliques.MAX_REPLIQUE
        self.assertEqual(repliques.contenus(f'"{longue}"'), [longue])
        self.assertEqual(repliques.contenus(f'"{longue}a"'), [])  # au-delà : ce n'est plus une réplique
        self.assertEqual(repliques.contenus(f"<d>{longue}</d>"), [longue])


class SansGel(unittest.TestCase):
    LIMITE_S = 8.0  # l'ancien code met plus de vingt secondes ; le nouveau, une seconde ou deux

    def test_des_guillemets_ouvrants_sans_fermeture(self):
        texte = "«" * 60_000
        for fonction in (repliques.contenus, repliques.vers_balises):
            self.assertLess(duree(fonction, texte), self.LIMITE_S, fonction.__name__)

    def test_des_balises_ouvrantes_sans_fermeture(self):
        for texte in ("<d>[" * 15_000, "<d>" * 20_000, "<d> a " * 10_000):
            for fonction in (repliques.contenus, repliques.vers_balises):
                with self.subTest(texte=texte[:8], fonction=fonction.__name__):
                    self.assertLess(duree(fonction, texte), self.LIMITE_S)

    def test_un_verbe_de_parole_puis_un_guillemet_sans_fermeture(self):
        texte = "says «" * 10_000
        self.assertLess(duree(repliques.vers_balises, texte), self.LIMITE_S)

    def test_des_blancs_derriere_une_ouverture(self):
        for texte in ("<d>" + " " * 20_000, "«" + " " * 20_000, "<d>[" + " " * 20_000):
            for fonction in (repliques.contenus, repliques.vers_balises):
                with self.subTest(texte=texte[:4], fonction=fonction.__name__):
                    self.assertLess(duree(fonction, texte), self.LIMITE_S)


if __name__ == "__main__":
    unittest.main()
