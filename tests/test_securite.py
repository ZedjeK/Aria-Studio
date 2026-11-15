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

    def test_ages_adultes_en_lettres_passent(self):
        """« vingt-neuf ans » ne se lit pas « neuf ans » : un âge écrit en plusieurs mots vaut sa vraie valeur."""
        for texte in ("vingt-neuf ans", "vingt neuf ans", "dix-huit ans", "dix-neuf ans", "vingt et un ans", "vingt-et-un ans",
                      "trente-deux ans", "quarante-cinq ans", "soixante-neuf ans", "soixante-dix ans", "soixante-dix-neuf ans",
                      "soixante et onze ans", "quatre-vingt-neuf ans", "quatre-vingt-dix ans", "quatre-vingt-dix-neuf ans",
                      "septante-neuf ans", "nonante-neuf ans",
                      "twenty-nine year old", "twenty-nine-year-old", "twenty nine years old", "twenty-one yo",
                      "thirty-two year old", "ninety-nine years old", "aged twenty-nine", "a girl of twenty-nine",
                      "une fille de vingt-neuf ans"):
            securite.verifier(f"a woman, {texte}", "nude")

    def test_un_nombre_anglais_ambigu_reste_bloque(self):
        """« twenty two-year-old girls » dit vingt fillettes de deux ans : sans trait d'union entre la dizaine et l'unité, et
        sans « years » au pluriel, le nombre garde sa lecture d'avant (« two-year-old »), donc il bloque."""
        for texte in ("twenty one-year-old girls", "twenty two-year-old girls", "thirty five-year-old girls",
                      "twenty three year old girls", "a girl, twenty two year old", "twenty two yo girls",
                      "forty one-year-old boys", "twenty one-years-old girls", "twenty two-years-old girls"):
            with self.assertRaises(ErreurSecurite, msg=texte):
                securite.verifier(texte, "nude")

    def test_un_nombre_anglais_lie_ou_au_pluriel_est_un_adulte(self):
        for texte in ("twenty-two-year-old girls", "twenty-two year old girls", "twenty-two yo", "twenty-two years old",
                      "twenty two years old", "a woman, twenty two years of age"):
            securite.verifier(texte, "nude")

    def test_les_mineurs_restent_bloques_sous_toutes_les_formes(self):
        for texte in ("neuf ans", "dix-sept ans", "dix sept ans", "seize ans", "onze ans", "un an", "9 ans", "17 ans",
                      "nine year old", "nine-year-old", "seventeen years old", "sixteen-year-old", "twelve yo", "aged seventeen",
                      "a girl of nine", "une fille de dix-sept ans"):
            with self.assertRaises(ErreurSecurite, msg=texte):
                securite.verifier(f"a woman, {texte}", "nude")

    def test_un_mineur_cite_apres_un_adulte_bloque_toujours(self):
        """Lire « vingt-neuf ans » en entier ne doit pas faire manquer le « neuf ans » d'une autre personne."""
        for texte in ("his twenty-nine year old mother and her nine year old daughter",
                      "elle a vingt-neuf ans et sa fille a neuf ans",
                      "a woman, twenty-one years old, and a sixteen-year-old boy"):
            with self.assertRaises(ErreurSecurite, msg=texte):
                securite.verifier(texte, "nude")

    @staticmethod
    def _en_francais(n: int) -> str:
        """Un entier de 1 à 99 en lettres (graphie standard, traits d'union), écrit ici sans rien reprendre de app/securite.py."""
        unites = ("", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf")
        ados = {10: "dix", 11: "onze", 12: "douze", 13: "treize", 14: "quatorze", 15: "quinze", 16: "seize",
                17: "dix-sept", 18: "dix-huit", 19: "dix-neuf"}
        dizaines = {2: "vingt", 3: "trente", 4: "quarante", 5: "cinquante", 6: "soixante"}
        if n < 10:
            return unites[n]
        if n < 20:
            return ados[n]
        if n < 70:
            d, u = divmod(n, 10)
            return dizaines[d] + ("" if u == 0 else "-et-un" if u == 1 else f"-{unites[u]}")
        if n < 80:
            return "soixante-et-onze" if n == 71 else f"soixante-{ados[n - 60]}"
        if n == 80:
            return "quatre-vingts"
        return f"quatre-vingt-{unites[n - 80]}" if n < 90 else f"quatre-vingt-{ados[n - 80]}"

    @staticmethod
    def _en_anglais(n: int) -> str:
        unites = ("", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine")
        ados = ("ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen")
        dizaines = ("", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety")
        if n < 10:
            return unites[n]
        if n < 20:
            return ados[n - 10]
        d, u = divmod(n, 10)
        return dizaines[d] + (f"-{unites[u]}" if u else "")

    def test_tous_les_ages_de_1_a_99_bloquent_si_et_seulement_si_moins_de_18(self):
        """Le filtre lit chaque âge écrit en chiffres ou en lettres, français et anglais, et ne bloque que les mineurs."""
        for n in range(1, 100):
            formes = (f"{n} ans", f"{n} years old", f"{n}-year-old",
                      f"{self._en_francais(n)} an{'s' if n > 1 else ''}",
                      f"{self._en_anglais(n)} year{'s' if n > 1 else ''} old", f"{self._en_anglais(n)}-year-old",
                      f"{self._en_anglais(n).replace('-', ' ')} year{'s' if n > 1 else ''} old" if n > 1 else f"{n} years old")
            for forme in formes:
                texte = f"a woman, {forme}"
                if n < securite.AGE_MAJORITE:
                    with self.assertRaises(ErreurSecurite, msg=f"{n} : « {forme} » devrait bloquer"):
                        securite.verifier(texte, "nude")
                else:
                    try:
                        securite.verifier(texte, "nude")
                    except ErreurSecurite as e:
                        self.fail(f"{n} : « {forme} » ne devrait pas bloquer ({e})")

    def test_un_age_au_pluriel_est_lu_comme_au_singulier(self):
        """« 12-year-olds » et « 14 yos » disent des enfants de 12 et de 14 ans : le « s » final ne cache pas l'âge."""
        for n in range(1, 100):
            formes = [f"{n}-year-olds", f"{n} year olds", f"{n} years olds", f"{n}-years-olds", f"{n} yos", f"{n}yos",
                      f"{n} y/os", f"{n} yrs olds", f"{n}-yr-olds"]
            if n >= 10:  # l'anglais écrit « twenty-nine-year-olds » ; sous 10 le nombre est seul : « nine-year-olds »
                formes.append(f"{self._en_anglais(n)}-year-olds")
            for forme in formes:
                texte = f"a group, {forme}"
                if n < securite.AGE_MAJORITE:
                    with self.assertRaises(ErreurSecurite, msg=f"{n} : « {forme} » devrait bloquer"):
                        securite.verifier(texte, "nude")
                else:
                    try:
                        securite.verifier(texte, "nude")
                    except ErreurSecurite as e:
                        self.fail(f"{n} : « {forme} » ne devrait pas bloquer ({e})")

    def test_un_mot_qui_commence_par_old_n_est_pas_un_age(self):
        for texte in ("12 years oldsmobile", "a 25 year oldschool dancer", "40 yosemite photos"):
            securite.verifier(texte, "nude")

    def test_les_tirets_typographiques_valent_le_trait_d_union(self):
        """Un tiret insécable, demi-cadratin, cadratin, le signe moins... étaient effacés, ce qui collait les mots :
        « dix\u2011sept ans » devenait « dixsept ans » et passait."""
        tirets = ("\u2010", "\u2011", "\u2012", "\u2013", "\u2014", "\u2015", "\u2212", "﹘", "－")
        for t in tirets:
            for texte in (f"une fille de dix{t}sept ans", f"a girl, twelve{t}year{t}old", f"des filles de 10{t}12 ans",
                          f"a girl, 12{t}year{t}old", f"seventeen{t}year{t}olds", f"a girl, ten{t}year{t}old"):
                with self.assertRaises(ErreurSecurite, msg=f"U+{ord(t):04X} : « {texte} » devrait bloquer"):
                    securite.verifier(texte, "nude")
            for texte in (f"une femme de vingt{t}neuf ans", f"a twenty{t}nine{t}year{t}old woman", f"une femme de dix{t}huit ans",
                          f"des femmes de 25{t}30 ans"):
                try:
                    securite.verifier(texte, "nude")
                except ErreurSecurite as e:
                    self.fail(f"U+{ord(t):04X} : « {texte} » ne devrait pas bloquer ({e})")

    def test_aucun_tiret_unicode_n_est_oublie(self):
        """Tout caractère que Unicode classe « ponctuation, tiret » est lu comme un trait d'union."""
        import sys
        import unicodedata
        tirets = [chr(c) for c in range(sys.maxunicode + 1) if unicodedata.category(chr(c)) == "Pd"]
        self.assertGreaterEqual(len(tirets), 26)
        for t in tirets:
            self.assertEqual(securite._normaliser(f"a{t}b", tirets=True), "a-b", f"U+{ord(t):04X}")

    def test_un_tiret_dans_un_mot_du_filtre_ne_le_cache_pas(self):
        """Lire un tiret comme un trait d'union ne doit rien faire perdre : « n\u2013ude », « ch\u00adild » (trait d'union
        conditionnel, invisible), « lo\u2013li » se lisent aussi sans le tiret et bloquent, à chaque position de coupe."""
        socle = securite._lire_socle()
        coupes = ("\u00ad", "\u2013", "\u2011", "\u2212")
        for liste, debut in (("sensibles", "a child, "), ("toujours_bloques", "")):
            for terme in socle[liste]:
                for i in range(1, len(terme)):
                    if terme[i - 1] == " " or terme[i] == " ":
                        continue  # une coupe contre une espace n'est pas une coupe dans le mot
                    for c in coupes:
                        texte = debut + terme[:i] + c + terme[i:]
                        with self.assertRaises(ErreurSecurite, msg=f"{ascii(texte)} devrait bloquer"):
                            securite.verifier(texte)

    def test_un_tiret_dans_un_age_ou_son_mot_ne_le_cache_pas(self):
        for texte in ("Age \u2013 12, nude", "Age \u2014 12, nude", "Age\u201112, nude", "aged\u00ad12, nude",
                      "age\u2212twelve, nude", "12 years o\u00adld, nude", "twelve year o\u00adld nude", "12 ye\u00adars old, nude",
                      "a 12 y\u00ado girl, nude", "6\u00ad\u00adans, nude", "6\u2013\u2013ans, nude", "6 \u2013 \u2013 ans, nude",
                      "a person, u\u00adn ans, nude"):
            with self.assertRaises(ErreurSecurite, msg=f"{ascii(texte)} devrait bloquer"):
                securite.verifier(texte)

    def test_un_trait_d_union_conditionnel_dans_un_age_adulte_ne_bloque_pas(self):
        """U+00AD est invisible : « tren\u00adte deux ans » se lit « trente deux ans »."""
        for texte in ("tren\u00adte deux ans", "d\u00adix-huit ans", "3\u00ad0 ans", "eigh\u00adteen years old"):
            securite.verifier(f"a woman, {texte}", "nude")

    def test_les_nombres_ecrits_en_plusieurs_mots_valent_tous_18_ou_plus(self):
        """Garde-fou : un nombre composé ajouté plus tard ne peut pas ouvrir un trou sous le plancher de 18 ans."""
        composes = {cle: v for cle, v in securite._NOMBRES.items() if " " in cle and cle != "dix sept"}
        self.assertGreater(len(composes), 100)
        for cle, valeur in composes.items():
            self.assertGreaterEqual(valeur, securite.AGE_MAJORITE, cle)
        self.assertEqual(securite._NOMBRES["dix sept"], 17)  # le seul nombre à deux mots qui reste un mineur
        self.assertEqual(securite.AGE_MAJORITE, 18)


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
