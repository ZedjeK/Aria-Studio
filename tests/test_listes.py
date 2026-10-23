"""Tests du moteur de l'onglet Prompt (app/listes.py), sur les vraies listes de prompt/listes.

Lancer depuis la racine du projet :  .venv\\Scripts\\python.exe -m unittest discover -s tests -v
"""

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from app import listes, securite
from app.listes import ErreurListes, assembler
from app.securite import ErreurSecurite

P, M, C = "personnage", "mise-en-scene", "commun"


def femme(age_tranche="Jeune adulte (18-25 ans)", **autres):
    return {P: {"Genre": "Femme", "Âge": age_tranche, **autres}}


class Lecture(unittest.TestCase):
    def test_les_vraies_listes_se_lisent(self):
        blocs = listes.blocs()
        for id_bloc in (P, M, C, "creature", "objet", "lieu"):
            self.assertIn(id_bloc, blocs)
        self.assertEqual(len(blocs[P].sous_listes), 33)
        self.assertTrue(blocs[P].incompatibilites)

    def test_structure_de_chaque_sujet(self):
        for sujet in listes.SUJETS:
            s = listes.structure(sujet)
            self.assertTrue(s["blocs"], sujet)
            noms = [sl["nom"] for b in s["blocs"] for g in b["groupes"] for sl in g["sous_listes"]]
            self.assertEqual(len(noms), len(set((b["id"], sl["nom"]) for b in s["blocs"]
                                                for g in b["groupes"] for sl in g["sous_listes"])))
        groupes = [g["nom"] for g in listes.structure(P)["blocs"][0]["groupes"]]
        self.assertEqual(groupes, ["Identité", "Corps", "Visage", "Cheveux", "Tenue", "Attitude"])

    def test_pas_de_sous_liste_lieu_pour_un_sujet_lieu(self):
        scene = next(b for b in listes.structure("lieu")["blocs"] if b["id"] == M)
        self.assertNotIn("Lieu", [sl["nom"] for g in scene["groupes"] for sl in g["sous_listes"]])

    def test_tenue_intime_marquee_entierement_adulte(self):
        tenue = next(g for g in listes.structure(P)["blocs"][0]["groupes"] if g["nom"] == "Tenue")
        self.assertTrue(next(sl for sl in tenue["sous_listes"] if sl["nom"] == "Tenue intime")["adulte"])

    def test_sujet_inconnu(self):
        with self.assertRaises(ErreurListes):
            listes.structure("vaisseau")


class ErreursDeFichier(unittest.TestCase):
    def lire(self, contenu: str):
        with tempfile.TemporaryDirectory() as dossier:
            chemin = Path(dossier) / "essai.md"
            chemin.write_text(contenu, encoding="utf-8")
            return listes.lire(chemin)

    TABLE = "## A\n\n| Valeur | Prompt image |\n| --- | --- |\n| Un | one |\n| Deux | two |\n"

    def test_fichier_correct(self):
        bloc = self.lire("---\ntype: liste\n---\n# Essai (test)\n\n" + self.TABLE)
        self.assertEqual(bloc.titre, "Essai")
        self.assertEqual(list(bloc.sous_listes["A"].valeurs), ["Un", "Deux"])

    def test_colonne_inconnue(self):
        with self.assertRaisesRegex(ErreurListes, "colonne inconnue"):
            self.lire("## A\n| Valeur | Couleur |\n| - | - |\n| Un | rouge |\n")

    def test_adulte_mal_rempli(self):
        with self.assertRaisesRegex(ErreurListes, "essai.md:3"):
            self.lire("## A\n| Valeur | Prompt image | Adulte |\n| Un | one | Oui |\n")

    def test_famille_inconnue(self):
        with self.assertRaisesRegex(ErreurListes, "famille « asie » inconnue"):
            self.lire("## A\n| Valeur | Prompt image | Origines |\n| - | - | - |\n| Un | one | europe, asie |\n")

    def test_colonnes_en_trop(self):
        with self.assertRaisesRegex(ErreurListes, "colonnes"):
            self.lire("## A\n| Valeur | Prompt image |\n| - | - |\n| Un | one | x |\n")

    def test_valeur_en_double(self):
        with self.assertRaisesRegex(ErreurListes, "en double"):
            self.lire(self.TABLE + "| Un | uno |\n")

    def test_remplace_une_sous_liste_inconnue(self):
        with self.assertRaisesRegex(ErreurListes, "sous-liste inconnue"):
            self.lire("## A\n| Valeur | Prompt image | Remplace |\n| - | - | - |\n| Un | one | Z |\n")

    def test_incompatibilite_inconnue(self):
        with self.assertRaisesRegex(ErreurListes, "valeur inconnue"):
            self.lire(self.TABLE + "\n## Incompatibilités\n\n- Un ↔ Trois\n")

    def test_incompatibilite_ambigue(self):
        b = "## B\n\n| Valeur | Prompt image |\n| --- | --- |\n| Un | one |\n"
        with self.assertRaisesRegex(ErreurListes, "précise"):
            self.lire(self.TABLE + b + "\n## Incompatibilités\n\n- Un ↔ Deux\n")
        bloc = self.lire(self.TABLE + b + "\n## Incompatibilités\n\n- B : Un ↔ Deux\n")
        self.assertEqual(bloc.incompatibilites, [(("B", "Un"), ("A", "Deux"))])


class Assemblage(unittest.TestCase):
    def test_personnage_complet(self):
        r = assembler(P, {
            P: {"Genre": "Femme", "Âge": "Jeune adulte (18-25 ans)", "Origine": "Japonaise",
                "Couleur des cheveux": "Noir corbeau", "Haut": "Veste en cuir", "Bas": "Jean",
                "Chaussures": "Bottines", "Accessoires": "Katana", "Pose": "Debout"},
            M: {"Lieu": "Une ruelle cyberpunk", "Moment": "La nuit"},
            C: {"Style": "Photo de cinéma", "Lumière": "Néons roses et cyan", "Cadrage": "Plan en pied"},
        })
        self.assertEqual(r["prompt"],
                         "Cinematic film still. A young woman in her early twenties, Japanese, raven black hair, "
                         "wearing leather jacket, jeans, ankle boots, with katana, standing, "
                         "in a rain-soaked cyberpunk alley, at night. "
                         "Pink and cyan neon lighting, full-body shot.")
        self.assertFalse(r["mineur"])
        self.assertEqual(r["alertes"], [])

    def test_sans_genre_ni_age(self):
        self.assertTrue(assembler(P, {})["prompt"].startswith("A person."))

    def test_article_devant_une_voyelle(self):
        self.assertTrue(assembler(P, femme("Âgé (65 ans et plus)"))["prompt"].startswith("An elderly woman"))
        r = assembler("creature", {"creature": {"Type": "Dragon oriental"}})
        self.assertTrue(r["prompt"].startswith("An eastern serpentine dragon"))

    def test_sujets_sans_type(self):
        self.assertTrue(assembler("objet", {})["prompt"].startswith("An object"))
        self.assertTrue(assembler("lieu", {})["prompt"].startswith("A landscape"))

    def test_robe_remplace_le_bas(self):
        r = assembler(P, femme(Haut="Robe d'été", Bas="Jean"))
        self.assertIn("summer dress", r["prompt"])
        self.assertNotIn("jeans", r["prompt"])
        self.assertIn("Bas (« Jean ») remplacé par « Robe d'été »", r["alertes"])

    def test_tenue_intime_remplace_haut_et_bas(self):
        r = assembler(P, femme(**{"Tenue intime": "Nuisette en soie", "Haut": "T-shirt", "Bas": "Jean"}))
        self.assertIn("wearing silk nightgown.", r["prompt"])
        self.assertEqual(len(r["alertes"]), 2)

    def test_trait_marquant_remplace_le_nez(self):
        r = assembler(P, femme(Nez="Aquilin", **{"Trait marquant": "Nez retroussé"}))
        self.assertIn("small upturned nose", r["prompt"])
        self.assertNotIn("aquiline", r["prompt"])

    def test_motif_de_tatouage(self):
        r = assembler(P, femme(**{"Signe particulier": "Tatouage dans le dos", "Motif de tatouage": "Dragon japonais"}))
        self.assertIn("tattoo on the back featuring a Japanese dragon design", r["prompt"])
        r = assembler(P, femme(**{"Signe particulier": "Lunettes", "Motif de tatouage": "Dragon japonais"}))
        self.assertNotIn("dragon", r["prompt"])
        self.assertTrue(any("Motif de tatouage ignoré" in a for a in r["alertes"]))

    def test_valeurs_vides_ignorees(self):
        r = assembler(P, femme(Taille="Moyenne", **{"Expression du visage": "Neutre"}))
        self.assertEqual(r["prompt"], "A young woman in her early twenties.")

    def test_texte_et_precisions(self):
        r = assembler("objet", {"objet": {"Catégorie": "Livre ancien"}}, texte='Aria "Artiste"',
                      precisions="avec un ruban rouge")
        self.assertIn("The text \"Aria 'Artiste'\" appears in the image.", r["prompt"])
        self.assertTrue(r["prompt"].endswith(" Avec un ruban rouge."))
        self.assertTrue(r["a_traduire"])

    def test_longueurs(self):
        with self.assertRaises(ErreurListes):
            assembler("objet", {}, texte="x" * 201)
        with self.assertRaises(ErreurListes):
            assembler("objet", {}, precisions="x" * 501)


class Regles(unittest.TestCase):
    def test_incompatibilite_simple(self):
        with self.assertRaisesRegex(ErreurListes, "incompatibles"):
            assembler(P, femme(Pose="Sprint", Action="Lit un livre"))

    def test_incompatibilite_precisee(self):
        with self.assertRaisesRegex(ErreurListes, "incompatibles"):
            assembler(P, femme(**{"Style vestimentaire": "Pyjama", "Chaussures": "Escarpins"}))

    def test_incompatibilite_du_rendu_et_de_la_scene(self):
        with self.assertRaises(ErreurListes):
            assembler("objet", {C: {"Style": "Manga noir et blanc", "Palette": "Couleurs vives"}})
        with self.assertRaises(ErreurListes):
            assembler("creature", {M: {"Ambiance": "Sous la neige", "Saison": "Été"}})

    def test_genre(self):
        with self.assertRaisesRegex(ErreurListes, "genre Homme"):
            assembler(P, {P: {"Genre": "Homme", "Poitrine": "Menue"}})
        with self.assertRaisesRegex(ErreurListes, "genre Femme"):
            assembler(P, femme(**{"Pilosité du visage": "Barbe courte"}))
        r = assembler(P, {P: {"Genre": "Androgyne", "Silhouette": "Bedonnant"}})  # androgyne : tout est proposé
        self.assertIn("androgynous person", r["prompt"])

    def test_valeur_ou_liste_inconnue(self):
        with self.assertRaisesRegex(ErreurListes, "Valeur inconnue"):
            assembler(P, femme(Coiffure="Iroquoise"))
        with self.assertRaisesRegex(ErreurListes, "Sous-liste inconnue"):
            assembler(P, {P: {"Ailes": "Grandes"}})
        with self.assertRaisesRegex(ErreurListes, "pas proposée"):
            assembler("objet", {M: {"Moment": "La nuit"}})
        with self.assertRaisesRegex(ErreurListes, "Sous-liste inconnue"):
            assembler("lieu", {M: {"Lieu": "Un parc"}})


class Mineurs(unittest.TestCase):
    """AS-18 : un mineur est autorisé, jamais avec une valeur adulte ni un terme sensible."""

    def test_ado_autorise(self):
        r = assembler(P, femme("Ado (12-17 ans)", **{"Style vestimentaire": "Uniforme scolaire japonais"}),
                      )
        self.assertTrue(r["mineur"])
        self.assertTrue(r["prompt"].startswith("A teenage girl"))

    def test_valeur_adulte_refusee(self):
        for choix in (femme("Ado (12-17 ans)", **{"Tenue intime": "Bikini"}),
                      femme("Enfant (6-11 ans)", Pose="Pose sensuelle"),
                      {**femme("Ado (12-17 ans)"), M: {"Lieu": "Un onsen"}}):
            with self.assertRaises(ErreurSecurite) as e:
                assembler(P, choix)
            self.assertIn("AS-18", str(e.exception))

    def test_age_precis_prime_sur_la_tranche(self):
        r = assembler(P, femme("Adulte (26-39 ans)"), age=12)
        self.assertTrue(r["mineur"])
        self.assertEqual(r["tranche"], "Ado (12-17 ans)")
        self.assertIn("12 years old", r["prompt"])
        self.assertTrue(any("ajustée" in a for a in r["alertes"]))
        with self.assertRaises(ErreurSecurite):
            assembler(P, femme("Adulte (26-39 ans)", **{"Tenue intime": "Bikini"}), age=12)

    def test_age_precis_adulte(self):
        r = assembler(P, {P: {"Genre": "Homme"}}, age=30)
        self.assertFalse(r["mineur"])
        self.assertTrue(r["prompt"].startswith("A man in his thirties, 30 years old"))

    def test_age_precis_hors_bornes(self):
        for age in (2, 121):
            with self.assertRaises(ErreurListes):
                assembler(P, {}, age=age)
        with self.assertRaises(ErreurListes):
            assembler("objet", {}, age=30)

    def test_precisions_sensibles_avec_un_mineur(self):
        with self.assertRaises(ErreurSecurite):
            assembler(P, femme("Ado (12-17 ans)"), precisions="in a swimsuit")
        assembler(P, femme(), precisions="in a swimsuit")  # adulte : autorisé

    def test_teenage_reconnu_par_as18(self):
        with self.assertRaises(ErreurSecurite):
            securite.verifier("a teenage girl in a bikini")

    def test_aucune_valeur_non_adulte_bloquee_pour_un_mineur(self):
        """Une valeur non marquée « Adulte » ne doit jamais déclencher AS-18 chez un mineur (sinon : la marquer)."""
        refusees = []
        for id_bloc in (P, M, C):
            for nom_sl, sl in listes._sous_listes(P, id_bloc).items():
                if nom_sl in ("Genre", "Âge"):
                    continue
                for v in sl.valeurs.values():
                    if v.adulte or (v.genre == "homme"):
                        continue
                    choix = femme("Ado (12-17 ans)")
                    choix.setdefault(id_bloc, {})[nom_sl] = v.nom
                    try:
                        assembler(P, choix)
                    except ErreurSecurite as e:
                        refusees.append(f"{nom_sl} : {v.nom} ({e})")
                    except ErreurListes:
                        pass  # condition non remplie (motif de tatouage) : sans objet ici
        self.assertEqual(refusees, [])

    def test_chaque_valeur_s_assemble_seule(self):
        for sujet in listes.SUJETS:
            for id_bloc in listes.SUJETS[sujet]["blocs"]:
                for nom_sl, sl in listes._sous_listes(sujet, id_bloc).items():
                    for v in sl.valeurs.values():
                        if v.mineur:
                            continue
                        r = assembler(sujet, {id_bloc: {nom_sl: v.nom}})
                        self.assertTrue(r["prompt"].endswith((".",)), (sujet, nom_sl, v.nom))


class Profil(unittest.TestCase):
    def test_alerte_de_longueur(self):
        with mock.patch.object(listes.workflows, "obtenir",
                               return_value=SimpleNamespace(id="x", fiche={"tache": "texte_vers_image"})), \
                mock.patch.object(listes.reglages, "valeurs_workflow", return_value={"mots_max": 5}):
            r = assembler(P, femme(Origine="Japonaise", Teint="Porcelaine"), workflow="x")
        self.assertEqual(r["mots_max"], 5)
        self.assertTrue(any("au-delà de 5" in a for a in r["alertes"]))

    def test_profils_des_fiches(self):
        self.assertEqual(assembler(P, {}, workflow="krea2_turbo")["mots_max"], 120)
        self.assertEqual(assembler(P, {}, workflow="qwen_image_2_1")["mots_max"], 200)
        self.assertEqual(assembler(P, {}, workflow="minimax_h3_t2v")["mots_max"], 110)


class SelonOrigine(unittest.TestCase):
    """Le filtre est appliqué par l'interface ; on vérifie ici les données qu'elle reçoit."""

    @staticmethod
    def proposees(origine: str, sous_liste: str) -> list[str]:
        sl = {s["nom"]: s for b in listes.structure(P)["blocs"] for g in b["groupes"] for s in g["sous_listes"]}
        familles = next(v for v in sl["Origine"]["valeurs"] if v["nom"] == origine)["familles"]
        return [v["nom"] for v in sl[sous_liste]["valeurs"]
                if not v["origines"] or set(v["origines"]) & set(familles)]

    def test_chaque_origine_a_sa_famille(self):
        for v in listes.blocs()[P].sous_listes["Origine"].valeurs.values():
            self.assertTrue(v.familles, v.nom)

    def test_coreenne(self):
        self.assertEqual(self.proposees("Coréenne", "Teint"), ["Porcelaine", "Clair", "Clair rosé", "Doré", "Hâlé"])
        self.assertEqual(self.proposees("Coréenne", "Forme des yeux"),
                         ["En amande", "En amande relevés", "Paupière simple (monolide)", "Paupière double fine",
                          "Tombants", "En amande étirés (félins)"])
        self.assertEqual(self.proposees("Coréenne", "Nez"), ["Fin et droit", "Droit", "Retroussé", "Arrondi", "Large et doux"])
        yeux = self.proposees("Coréenne", "Couleur des yeux")
        self.assertIn("Noirs", yeux)
        self.assertIn("Rouge (fantasy)", yeux)  # le fantasy reste pour tout le monde
        self.assertNotIn("Bleu clair", yeux)

    def test_metisse_cumule(self):
        self.assertIn("Bleu clair", self.proposees("Eurasienne", "Couleur des yeux"))
        self.assertIn("Paupière simple (monolide)", self.proposees("Eurasienne", "Forme des yeux"))

    def test_chaque_origine_garde_un_choix_partout(self):
        for origine in listes.blocs()[P].sous_listes["Origine"].valeurs:
            for sl in ("Teint", "Forme des yeux", "Couleur des yeux", "Nez"):
                self.assertGreaterEqual(len(self.proposees(origine, sl)), 3, (origine, sl))

    def test_le_serveur_ne_bloque_pas(self):
        r = assembler(P, femme(Origine="Coréenne", **{"Couleur des yeux": "Bleu clair"}))
        self.assertIn("light blue eyes", r["prompt"])

    def test_descriptions_de_silhouette(self):
        silhouettes = listes.blocs()[P].sous_listes["Silhouette"].valeurs.values()
        self.assertEqual(len(silhouettes), 10)
        self.assertTrue(all(v.description.endswith(".") for v in silhouettes))


class PosesEtActions(unittest.TestCase):
    @staticmethod
    def categories(sous_liste: str) -> dict[str, list]:
        sortie = {}
        for v in listes.blocs()[P].sous_listes[sous_liste].valeurs.values():
            sortie.setdefault(v.categorie, []).append(v)
        return sortie

    def test_19_categories(self):
        self.assertEqual(list(self.categories("Pose")), ["Naturelle", "Dynamique", "Mode et éditorial", "Kawaii et anime",
                                                         "Martiale", "Sexy", "Intime"])
        self.assertEqual(len(self.categories("Action")), 12)
        self.assertNotIn("", self.categories("Pose"))
        self.assertNotIn("", self.categories("Action"))

    def test_sexy_et_intime_reservees_aux_adultes(self):
        poses = self.categories("Pose")
        self.assertTrue(all(v.adulte for cat in ("Sexy", "Intime") for v in poses[cat]))
        self.assertFalse(any(v.adulte for cat in ("Naturelle", "Kawaii et anime") for v in poses[cat]))

    def test_rien_d_ouvert_aux_mineurs_ne_declenche_as18(self):
        # Chaque pose ou action proposée à un mineur doit passer le filtre avec un personnage mineur.
        for sous_liste in ("Pose", "Action"):
            for v in listes.blocs()[P].sous_listes[sous_liste].valeurs.values():
                if not v.adulte:
                    securite.verifier(v.prompt, "minor")

    def test_mineur_sans_pose_sexy(self):
        with self.assertRaises(ErreurSecurite):
            assembler(P, {P: {"Âge": "Ado (12-17 ans)", "Pose": "Cambrée"}})
        r = assembler(P, {P: {"Âge": "Ado (12-17 ans)", "Pose": "Mains en cœur", "Action": "Lit un manga"}})
        self.assertIn("reading a manga", r["prompt"])

    def test_action_qui_impose_la_posture(self):
        r = assembler(P, femme(Pose="Debout", Action="Conduit une voiture"))
        self.assertNotIn("standing", r["prompt"])
        self.assertIn("driving a car", r["prompt"])
        self.assertTrue(any("remplacé par « Conduit une voiture »" in a for a in r["alertes"]))

    def test_incompatibilite(self):
        with self.assertRaisesRegex(ErreurListes, "incompatibles"):
            assembler(P, femme(Pose="Sprint", Action="Lit un livre"))

    def test_categorie_livree_a_l_interface(self):
        sl = {s["nom"]: s for b in listes.structure(P)["blocs"] for g in b["groupes"] for s in g["sous_listes"]}
        self.assertEqual(next(v for v in sl["Action"]["valeurs"] if v["nom"] == "Câble une baie de serveurs")["categorie"],
                         "Travail")


def restantes(sous_liste: str, ref: str, valeur: str) -> list[str]:
    """Valeurs de `sous_liste` encore proposées quand `ref` vaut `valeur` (règle « Avec <ref> »)."""
    return [v.nom for v in listes.blocs()[P].sous_listes[sous_liste].valeurs.values()
            if valeur in dict(v.avec).get(ref, (valeur,))]


class Silhouette(unittest.TestCase):
    def test_corps_incoherent_refuse(self):
        with self.assertRaisesRegex(ErreurListes, "ne va pas avec « Menue » \\(Silhouette\\)"):
            assembler(P, femme(Silhouette="Menue", Poitrine="Très généreuse"))
        with self.assertRaisesRegex(ErreurListes, "« Trapu\\(e\\) » \\(Silhouette\\)"):
            assembler(P, femme(Silhouette="Trapu(e)", Taille="Très grande"))
        with self.assertRaisesRegex(ErreurListes, "« Pulpeuse » \\(Silhouette\\)"):
            assembler(P, femme(Silhouette="Pulpeuse", Hanches="Étroites"))

    def test_corps_coherent_et_sans_silhouette(self):
        r = assembler(P, femme(Silhouette="En sablier", Poitrine="Très généreuse", Hanches="Très marquées"))
        self.assertIn("hourglass figure", r["prompt"])
        assembler(P, femme(Poitrine="Très généreuse", Hanches="Étroites"))  # pas de silhouette : pas de règle

    def test_chaque_silhouette_garde_des_choix(self):
        for s in listes.blocs()[P].sous_listes["Silhouette"].valeurs:
            for sl in ("Taille", "Poitrine", "Hanches"):
                self.assertGreaterEqual(len(restantes(sl, "Silhouette", s)), 2, (s, sl))

    def lire(self, colonne_avec: str, valeur: str):
        contenu = ("## Silhouette\n| Valeur | Prompt image |\n| - | - |\n| Menue | petite |\n\n"
                   f"## Taille\n| Valeur | Prompt image | {colonne_avec} |\n| - | - | - |\n| Grande | tall | {valeur} |\n")
        with tempfile.TemporaryDirectory() as dossier:
            chemin = Path(dossier) / "essai.md"
            chemin.write_text(contenu, encoding="utf-8")
            return listes.lire(chemin)

    def test_colonne_avec_mal_remplie(self):
        self.assertEqual(self.lire("Avec Silhouette", "Menue").sous_listes["Taille"].valeurs["Grande"].avec,
                         (("Silhouette", ("Menue",)),))
        with self.assertRaisesRegex(ErreurListes, "« Géante », absent de Silhouette"):
            self.lire("Avec Silhouette", "Géante")
        with self.assertRaisesRegex(ErreurListes, "« Avec Coiffure », sous-liste inconnue"):
            self.lire("Avec Coiffure", "Menue")


class Coiffure(unittest.TestCase):
    def test_coiffures_selon_la_longueur(self):
        rases = restantes("Coiffure", "Longueur des cheveux", "Rasés")
        self.assertEqual(rases, ["Coupe militaire"])
        courts = restantes("Coiffure", "Longueur des cheveux", "Courts")
        self.assertNotIn("Queue de cheval haute", courts)
        self.assertIn("Frange droite", courts)
        longs = restantes("Coiffure", "Longueur des cheveux", "Très longs")
        self.assertIn("Chignon de samouraï", longs)
        self.assertNotIn("Coupe militaire", longs)

    def test_chaque_longueur_garde_des_coiffures(self):
        for longueur in listes.blocs()[P].sous_listes["Longueur des cheveux"].valeurs:
            self.assertGreaterEqual(len(restantes("Coiffure", "Longueur des cheveux", longueur)), 1, longueur)

    def test_regle_au_serveur(self):
        with self.assertRaisesRegex(ErreurListes, "« Tresse » \\(Coiffure\\) ne va pas avec « Carré »"):
            assembler(P, femme(**{"Longueur des cheveux": "Carré", "Coiffure": "Tresse"}))
        r = assembler(P, femme(**{"Longueur des cheveux": "Longs", "Coiffure": "Tresse"}))
        self.assertIn("single braid", r["prompt"])

    def test_plus_d_incompatibilites_de_cheveux(self):
        for (sl_g, _), (sl_d, _) in listes.blocs()[P].incompatibilites:
            self.assertNotEqual({sl_g, sl_d}, {"Longueur des cheveux", "Coiffure"})


H3 = "minimax_h3_t2v"


class Video(unittest.TestCase):
    PLAN = {"Caméra": "Avancée lente", "Langue": "Japonais", "Ton": "Murmuré", "Ambiance sonore": "Pluie",
            "Musique": "Synthwave"}

    def test_plan_livre_et_marque(self):
        blocs = {b["id"]: b for b in listes.structure(P)["blocs"]}
        self.assertTrue(blocs["video"]["video"])
        self.assertFalse(blocs["personnage"]["video"])
        self.assertEqual([g["nom"] for g in blocs["video"]["groupes"]], ["Caméra", "Voix", "Son"])

    def test_brief_dans_l_ordre(self):
        choix = {**femme(), "commun": {"Cadrage": "Plan taille"}, "video": self.PLAN}
        r = assembler(P, choix, workflow=H3, action="she slowly turns toward the camera", replique="On décolle ce soir.")
        self.assertTrue(r["video"])
        p = r["prompt"]
        self.assertTrue(p.startswith("Medium shot, waist up. A young woman"), p)
        morceaux = ["She slowly turns toward the camera.", "The camera slowly pushes in.",
                    'She says in Japanese, in a soft whisper: "On décolle ce soir."',
                    "Sounds: rain pattering on the pavement.", "Music: slow retro synthwave."]
        positions = [p.index(m) for m in morceaux]
        self.assertEqual(positions, sorted(positions))

    def test_qui_parle_et_valeurs_par_defaut(self):
        self.assertIn('They say in French: "Salut."', assembler(P, {}, workflow=H3, replique="Salut.")["prompt"])
        self.assertIn("A voice-over says", assembler("lieu", {}, workflow=H3, replique="Bienvenue.")["prompt"])
        r = assembler(P, {"video": {"Ton": "Doux", "Musique": "Aucune"}}, workflow=H3)
        self.assertTrue(r["prompt"].endswith("No music."))
        self.assertTrue(any("écris d'abord une réplique" in a for a in r["alertes"]))

    def test_texte_dans_l_image_ignore_en_video(self):
        r = assembler(P, {}, texte="ARIA", workflow=H3)
        self.assertNotIn("ARIA", r["prompt"])
        self.assertTrue(any("Texte dans l'image ignoré" in a for a in r["alertes"]))

    def test_plan_ignore_pour_une_image(self):
        r = assembler(P, {"video": self.PLAN}, workflow="krea2_turbo", action="she runs", replique="Hop")
        self.assertEqual(r["prompt"], "A person.")
        self.assertFalse(r["video"])

    def test_longueurs_et_as18(self):
        with self.assertRaisesRegex(listes.ErreurListes, "Réplique trop longue"):
            assembler(P, {}, workflow=H3, replique="x" * 201)
        with self.assertRaisesRegex(listes.ErreurListes, "Action trop longue"):
            assembler(P, {}, workflow=H3, action="x" * 501)
        with self.assertRaises(securite.ErreurSecurite):
            assembler(P, {"personnage": {"Âge": next(v.nom for v in listes.blocs()["personnage"].sous_listes["Âge"]
                                                     .valeurs.values() if v.mineur)}},
                      workflow=H3, action="she undresses")


if __name__ == "__main__":
    unittest.main()
