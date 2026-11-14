"""Gabarit des consignes (app/prompt.py) : les mêmes sections dans le même ordre, les règles du modèle cible rangées
dedans (profil_prompt.consigne de sa fiche), chaque règle dite une seule fois.

Origine : les règles de la fiche s'ajoutaient à plat après la consigne. Le dialogue de MiniMax H3 était décrit deux
fois, sa caméra de deux façons contraires, et « plain prose » précédait un bloc qui annonçait le remplacer. Rien
n'est lancé : on lit seulement le texte envoyé au LLM.
"""

import importlib.util
import unittest
from pathlib import Path
from unittest import mock

from app import prompt, reglages, workflows

ORDRE = ["ROLE:", "USER PRIORITY:", "PROMPT OUTLINE", "CONTENT AND WORDING:", "DIALOGUE:", "VISIBLE TEXT:", "NEVER:",
         "OUTPUT FORMAT:"]


def _outil():
    """tools/consignes.py, chargé comme un module (tools/ n'est pas un paquet)."""
    chemin = Path(__file__).resolve().parent.parent / "tools" / "consignes.py"
    spec = importlib.util.spec_from_file_location("outil_consignes", chemin)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _titres(texte: str) -> list[str]:
    return [t for t in ORDRE if f"\n\n{t}" in f"\n\n{texte}"]


class Gabarit(unittest.TestCase):
    def test_sections_dans_l_ordre(self):
        for nom in prompt.GABARITS:
            texte = prompt.consigne(nom)
            positions = [texte.index(t) for t in _titres(texte)]
            self.assertEqual(positions, sorted(positions), nom)
            self.assertTrue(texte.startswith("ROLE:\n"), nom)
            self.assertLess(texte.index("OUTPUT FORMAT:"), texte.index(prompt.CLAUSE_MINEURS), nom)
            self.assertTrue(texte.endswith(prompt.CLAUSE_MINEURS), nom)  # la clause verrouillée reste la dernière
        self.assertNotIn("DIALOGUE:", prompt.consigne("reecriture"))  # une section vide n'apparaît pas
        self.assertNotIn("NEVER:", prompt.consigne("video"))

    def test_la_fiche_remplace_ou_complete(self):
        regles = {"plan": ["Open with the medium.", "End with the mood."], "contenu": ["Name colours with a modifier."],
                  "interdits": ["Quality boosters."]}
        texte = prompt.consigne("reecriture", None, 200, regles)
        self.assertIn("PROMPT OUTLINE, in this order:\n1. Open with the medium.\n2. End with the mood.", texte)
        self.assertNotIn("One flowing paragraph", texte)  # le plan de la fiche remplace
        self.assertIn("never switch to painting or illustration on your own", texte)  # le contenu s'ajoute au général
        self.assertIn("- Name colours with a modifier.", texte)
        self.assertIn("NEVER:\n- Add characters, animals or objects the idea does not imply.\n- Quality boosters.", texte)
        libre = prompt.consigne("reecriture")  # sans plan dans la fiche (Krea 2) : un paragraphe libre, en phrases
        self.assertIn("PROMPT OUTLINE:\nOne flowing paragraph of full sentences", libre)
        self.assertIn("never a comma-separated list of keywords", libre)
        # Références : le plan d'un modèle de retouche s'ajoute, on décrit d'abord ce qui est nouveau.
        refs = prompt.consigne("references", regles={"plan": ["Lead with the operation."]})
        self.assertIn("1. Describe what is new, as the user asks", refs)
        self.assertIn("2. Lead with the operation.", refs)

    def test_la_priorite_a_l_utilisateur_n_est_pas_dans_une_fiche(self):
        for nom in ("reecriture", "references", "video"):
            texte = prompt.consigne(nom, regles={"plan": ["Open with the medium."]})
            priorite = texte[texte.index("USER PRIORITY:"):texte.index("PROMPT OUTLINE")]
            for bloc in (prompt.FIDELITE, prompt.GENRE, prompt.ADULTES, "the user wins"):
                self.assertIn(bloc, priorite, nom)

    def test_format_ecrit_une_seule_fois(self):
        prose = prompt.consigne("video", None, 110, workflows.sections_prompt("minimax_h3_t2v"))
        champs = prompt.consigne("video", None, 110, workflows.sections_prompt("minimax_h3_t2v"), champs=True)
        self.assertIn("40 to 110 words, plain prose", prose)
        self.assertNotIn("plain prose", champs)  # avant : « plain prose », puis un bloc qui disait le remplacer
        self.assertNotIn("it replaces the plain-prose layout", champs)
        self.assertIn("40 to 110 words for the three fields together", champs)
        for champ in ("integrated_multimodal_description:", "overall_soundscape:", "non_diegetic_music:"):
            self.assertEqual(champs.count(f"\n{champ}"), 1, champ)

    def test_h3_chaque_regle_une_seule_fois(self):
        for id_wf in ("minimax_h3_t2v", "minimax_h3_i2v"):
            texte = prompt.consigne("video", None, 110, workflows.sections_prompt(id_wf), champs=True)
            self.assertEqual(texte.count("<d>[French]"), 1, id_wf)  # la balise des répliques, dite deux fois avant
            self.assertEqual(texte.count("(S1)"), 2, id_wf)  # l'identifiant et son exemple, dans la même section
            self.assertEqual(texte.count("never for a spoken line"), 1, id_wf)
            self.assertNotIn("with its speed", texte, id_wf)  # contredisait « speed only when they matter »
            self.assertIn("speed (slow or fast) only when they matter", texte, id_wf)
            self.assertIn("No background music unless the user asks for it", texte, id_wf)
        self.assertIn("3. Animating a picture", prompt.consigne("video", regles=workflows.sections_prompt("minimax_h3_i2v")))

    def test_aucune_ligne_en_double_dans_les_consignes_livrees(self):
        for (fiche, usage), texte in _outil().relever().items():
            lignes = [ligne for ligne in texte.split("\n") if len(ligne) > 40]
            self.assertEqual(len(lignes), len(set(lignes)), f"{fiche} · {usage}")

    def test_l_outil_releve_chaque_fiche(self):
        releve = _outil().relever()
        self.assertEqual({fiche for fiche, _ in releve}, {wf.id for wf in workflows.catalogue()})
        self.assertIn(("qwen_image_2_1_edit", "references"), releve)  # il prend des images de référence
        self.assertNotIn(("krea2_turbo", "references"), releve)
        self.assertEqual(list(_outil().relever(["krea2_turbo"])), [("krea2_turbo", "reecriture")])
        self.assertNotIn("{", releve[("minimax_h3_t2v", "decoupage")].replace("{regle_personnage}", ""))  # repères remplis

    def test_sans_gabarit_les_regles_viennent_en_bloc(self):
        """Découpage, chat Libre, consigne réécrite dans Réglages : leur texte reste d'un bloc, les règles suivent."""
        sections = workflows.sections_prompt("minimax_h3_t2v")
        bloc = prompt.bloc_modele(sections)
        self.assertTrue(bloc.startswith(prompt.ENTETE_REGLES + "\n\nPROMPT OUTLINE, in this order:\n1. Begin with"))
        self.assertIn("\n\nDIALOGUE:\n- Before a speaker's first line", bloc)
        self.assertEqual(prompt.bloc_modele(None), "")
        self.assertEqual(prompt.bloc_modele({}), "")
        self.assertEqual(prompt.bloc_modele(["Short sentences."]), f"{prompt.ENTETE_REGLES}\n- Short sentences.")
        decoupage = prompt.consigne("decoupage", None, 110, sections, champs=True)
        self.assertLess(decoupage.index(bloc), decoupage.index(prompt.FORMAT_CHAMPS))
        self.assertNotIn("ROLE:", decoupage)
        with mock.patch.object(reglages, "consigne_llm", return_value="Film it. Max {mots_max} words."):
            reecrite = prompt.consigne("video", "mon.gguf", 90, sections, champs=True)
        self.assertTrue(reecrite.startswith(f"Film it. Max 90 words.\n\n{bloc}\n\n{prompt.FORMAT_CHAMPS}"))
        self.assertTrue(reecrite.endswith(prompt.CLAUSE_MINEURS))


if __name__ == "__main__":
    unittest.main()
