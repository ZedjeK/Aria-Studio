"""Tests du filtre AS-18 : mineur seul autorisé, mineur + terme sensible bloqué ; socle et ajouts (Réglages)."""

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import json

import yaml

from app import fichier_reglages, securite
from app.securite import ErreurListes, ErreurSecurite, ErreurTemoins
from tests import reglages_jetables


class Deshabillage(unittest.TestCase):
    MINEUR = "A teenage girl, 15 years old"

    def test_variantes_bloquees_avec_un_mineur(self):
        for texte in ("She undresses slowly.", "Undress her.", "He disrobes.", "She is disrobing by the bed.",
                      "She stands unclothed.", "She takes off her clothes.", "She pulls her clothes off.",
                      "Elle se déshabiller lentement.", "Ils se déshabillent.", "Elle enlève ses vêtements.",
                      "Elle retire ses vêtements.", "Elle est dévêtue."):
            with self.assertRaises(ErreurSecurite, msg=texte):
                securite.verifier(self.MINEUR, texte)

    def test_autorise_sans_mineur(self):
        securite.verifier("A woman in her thirties", "She undresses slowly.")

    def test_mots_ambigus_toleres(self):
        securite.verifier(self.MINEUR, "She reads a comic strip, lit by strips of neon light.")


class Temoins(unittest.TestCase):
    """Les phrases témoins livrées (securite/temoins.yaml) : le fichier de cas du filtre."""

    def test_toutes_conformes(self):
        etat = securite.etat()
        self.assertIsNone(etat["erreur"])
        self.assertGreaterEqual(len(etat["temoins"]), 20)
        for t in etat["temoins"]:
            self.assertTrue(t["ok"], f"« {t['texte']} » : attendu {t['attendu']}, obtenu {t['obtenu']} {t['termes']}")

    def test_les_deux_attentes_sont_couvertes(self):
        attendus = {t["attendu"] for t in securite.etat()["temoins"]}
        self.assertEqual(attendus, {"bloque", "passe"})


class Ages(unittest.TestCase):
    def test_formes_reconnues(self):
        for texte in ("a girl of 14", "une fille de 14", "a boy, 12 yrs old", "a student, 16 years of age",
                      "age of 15", "aged 9", "12 ans", "seventeen-year-old"):
            with self.assertRaises(ErreurSecurite, msg=texte):
                securite.verifier(texte, "nude")

    def test_adultes_et_nombres_sans_age(self):
        for texte in ("a woman of 30", "a 25 year old woman", "one of 3 dancers", "a group of 5 people"):
            securite.verifier(texte, "nude")


class Listes(unittest.TestCase):
    """Socle et ajouts sur des copies jetables : le vrai socle n'est jamais touché par ces tests."""

    def setUp(self):
        self.dossier = Path(tempfile.mkdtemp(prefix="aria-as18-"))
        self.addCleanup(shutil.rmtree, self.dossier, ignore_errors=True)
        self.socle = self.dossier / "socle.yaml"
        self.reglages = reglages_jetables.fichier(self.dossier)  # reglages.json jetable : tes ajouts y vivent
        shutil.copy(securite.FICHIER_SOCLE, self.socle)
        patchs = [mock.patch.object(securite, "FICHIER_SOCLE", self.socle), reglages_jetables.patch(self.dossier)]
        for p in patchs:
            p.start()
            self.addCleanup(p.stop)

    def section(self):
        """La section « securite » de reglages.json, relue sur le disque."""
        return json.loads(self.reglages.read_text(encoding="utf-8")).get("securite", {}) if self.reglages.is_file() else {}

    def liste(self, etat, cle):
        return next(l for l in etat["listes"] if l["cle"] == cle)

    def test_un_ajout_compte_tout_de_suite_et_se_retire(self):
        securite.verifier("a child", "wearing a zorglub")
        etat = securite.ajouter("sensibles", "Zorglub")
        self.assertEqual(self.liste(etat, "sensibles")["ajouts"], ["Zorglub"])
        with self.assertRaises(ErreurSecurite) as e:
            securite.verifier("a child", "wearing a zorglub")
        self.assertIn("zorglub", str(e.exception))
        etat = securite.retirer("sensibles", "zorglub")
        self.assertEqual(self.liste(etat, "sensibles")["ajouts"], [])
        securite.verifier("a child", "wearing a zorglub")

    def test_ajouts_refuses(self):
        for liste, terme in (("sensibles", "a"), ("sensibles", "de"), ("sensibles", "!!!"), ("sensibles", "Nude"),
                             ("mineurs", "enfant"), ("inconnue", "zorglub")):
            with self.assertRaises(ErreurListes, msg=terme):
                securite.ajouter(liste, terme)
        securite.ajouter("sensibles", "zorglub")
        with self.assertRaises(ErreurListes):
            securite.ajouter("sensibles", "ZORGLUB")
        self.assertFalse(isinstance(ErreurListes("x"), ErreurSecurite))

    def test_un_ajout_qui_casse_un_temoin_se_confirme(self):
        securite.ajouter_temoin("a child with a kite", "passe")
        with self.assertRaises(ErreurTemoins) as e:
            securite.ajouter("sensibles", "kite")
        self.assertEqual(e.exception.temoins, ["a child with a kite"])
        self.assertEqual(self.liste(securite.etat(), "sensibles")["ajouts"], [])
        etat = securite.ajouter("sensibles", "kite", forcer=True)
        temoin = next(t for t in etat["temoins"] if t["texte"] == "a child with a kite")
        self.assertFalse(temoin["ok"])
        self.assertEqual(temoin["termes"][-1], {"terme": "kite", "liste": "sensibles", "etage": "ajouts"})

    def test_le_socle_ne_se_retire_pas(self):
        with self.assertRaises(ErreurListes):
            securite.retirer("sensibles", "nude")
        with self.assertRaises(ErreurListes):
            securite.promouvoir("sensibles", "nude")
        self.assertIn("nude", self.liste(securite.etat(), "sensibles")["socle"])

    def test_promotion(self):
        avant = self.socle.read_text(encoding="utf-8")
        securite.ajouter("mineurs", "Zorgling")
        etat = securite.promouvoir("mineurs", "zorgling")
        self.assertEqual(self.liste(etat, "mineurs")["ajouts"], [])
        self.assertEqual(self.liste(etat, "mineurs")["socle"][-1], "Zorgling")
        apres = self.socle.read_text(encoding="utf-8")
        self.assertEqual(len(apres.split("\n")), len(avant.split("\n")) + 1)
        self.assertEqual([l for l in apres.split("\n") if "Zorgling" not in l], avant.split("\n"))  # le reste intact
        self.assertLess(apres.index("Zorgling"), apres.index("sensibles:"))  # en fin de sa liste, pas de la suivante
        with self.assertRaises(ErreurListes):
            securite.retirer("mineurs", "zorgling")
        with self.assertRaises(ErreurSecurite):
            securite.verifier("a zorgling", "nude")

    def test_promotion_refusee_si_le_socle_a_une_forme_inattendue(self):
        relu = yaml.safe_load(self.socle.read_text(encoding="utf-8"))
        self.socle.write_text(yaml.safe_dump(relu, default_flow_style=True, allow_unicode=True), encoding="utf-8")
        securite.ajouter("mineurs", "zorgling")
        contenu = self.socle.read_text(encoding="utf-8")
        with self.assertRaises(ErreurListes):
            securite.promouvoir("mineurs", "zorgling")
        self.assertEqual(self.socle.read_text(encoding="utf-8"), contenu)
        self.assertEqual(self.liste(securite.etat(), "mineurs")["ajouts"], ["zorgling"])

    def test_socle_absent_vide_ou_illisible_tout_est_refuse(self):
        for contenu in (None, "", "mineurs: [", "mineurs: [child]\nsensibles: []\ntoujours_bloques: [loli]\n",
                        "- child\n"):
            if contenu is None:
                self.socle.unlink()
            else:
                self.socle.write_text(contenu, encoding="utf-8")
            with self.assertRaises(ErreurSecurite, msg=repr(contenu)) as e:
                securite.verifier("a red apple on a table")
            self.assertIn("socle", str(e.exception))
            self.assertTrue(securite.etat()["erreur"])

    def test_ajouts_mal_formes_tout_est_refuse(self):
        self.reglages.write_text(json.dumps({"migration": {}, "securite": {"sensibles": "nude"}}), encoding="utf-8")
        with self.assertRaises(ErreurSecurite) as e:
            securite.verifier("a red apple on a table")
        self.assertIn("reglages.json", str(e.exception))
        self.assertTrue(securite.etat()["erreur"])
        self.reglages.write_text(json.dumps({"migration": {}, "securite": {"sensibles": ["zorglub"]}}), encoding="utf-8")
        securite.verifier("a red apple on a table")  # un reglages.json corrigé à la main est relu

    def test_reglages_illisible_le_socle_seul_s_applique(self):
        self.reglages.write_text("{ pas du json", encoding="utf-8")
        securite.verifier("a red apple on a table")
        with self.assertRaises(ErreurSecurite):
            securite.verifier("a child, nude")

    def test_les_ajouts_ne_retirent_rien_au_socle(self):
        self.reglages.write_text(json.dumps({"migration": {}, "securite": {"mineurs": [], "sensibles": []}}), encoding="utf-8")
        with self.assertRaises(ErreurSecurite):
            securite.verifier("a child, nude")

    def test_les_ajouts_vivent_dans_reglages_json_avec_leur_sauvegarde(self):
        securite.ajouter("sensibles", "zorglub")
        self.assertEqual(self.section(), {"sensibles": ["zorglub"]})
        securite.ajouter("sensibles", "klaatu")
        self.assertEqual(self.section()["sensibles"], ["zorglub", "klaatu"])
        sauvegardes = fichier_reglages.sauvegardes()
        self.assertTrue(sauvegardes)  # l'état d'avant chaque changement est gardé, comme pour tout reglages.json
        securite.retirer("sensibles", "zorglub")
        securite.retirer("sensibles", "klaatu")
        self.assertEqual(self.section(), {})  # plus rien : la section disparaît

    def test_temoins_de_l_utilisateur(self):
        livres = len(securite.etat()["temoins"])
        etat = securite.ajouter_temoin("  a child   with a kite ", "passe")
        self.assertEqual(len(etat["temoins"]), livres + 1)
        self.assertEqual(etat["temoins"][-1]["texte"], "a child with a kite")
        self.assertFalse(etat["temoins"][-1]["livre"])
        for texte, attendu in (("a child with a kite", "passe"), ("", "passe"), ("autre", "peut-etre")):
            with self.assertRaises(ErreurListes):
                securite.ajouter_temoin(texte, attendu)
        with self.assertRaises(ErreurListes):  # une phrase livrée ne se retire pas
            securite.retirer_temoin(etat["temoins"][0]["texte"])
        self.assertEqual(len(securite.retirer_temoin("a child with a kite")["temoins"]), livres)

    def test_essayer_dit_ce_qui_bloque_et_d_ou_ca_vient(self):
        r = securite.essayer("a child, nude")
        self.assertTrue(r["bloque"])
        self.assertEqual([(t["terme"], t["etage"]) for t in r["termes"]], [("child", "socle"), ("nude", "socle")])
        self.assertFalse(securite.essayer("a child in a garden")["bloque"])
        self.assertEqual(securite.essayer("a girl of 14, nude")["termes"][0]["etage"], "age")


if __name__ == "__main__":
    unittest.main()
