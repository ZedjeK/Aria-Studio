"""Lot 4 (ergonomie), côté serveur : la barre d'état sait qu'une génération tourne, et la fiche d'une image ou d'un
clip garde le LLM qui a écrit son prompt au Scénario (sans rien réécrire au Plateau ni au Montage)."""

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from app import main, plateau, rushes, workflows
from app.plateau import Plateau


class CommandeEnCours(unittest.TestCase):
    def test_rien_puis_la_commande_qui_tourne(self):
        with mock.patch.object(main.plateau_, "commandes", {}):
            self.assertIsNone(main.commande_en_cours())
        commandes = {"a": SimpleNamespace(id="a", type="clip", etat="attente", etape="", progression=0.0),
                     "b": SimpleNamespace(id="b", type="creation", etat="en_cours", etape="Génération", progression=0.456),
                     "c": SimpleNamespace(id="c", type="creation", etat="fini", etape="", progression=1.0)}
        with mock.patch.object(main.plateau_, "commandes", commandes):
            # l'id : « Annuler la génération en cours » du menu de l'icône (tools/launcher/Icone.cs)
            self.assertEqual(main.commande_en_cours(), {"id": "b", "type": "creation", "etat": "en_cours", "etape": "Génération",
                                                        "progression": 0.46, "file": 2})


class EcritPar(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        d = Path(self.temp.name)
        self.patchs = [mock.patch.object(plateau, "DOSSIER", d / "creations"),
                       mock.patch.object(rushes, "FICHIER_DOSSIERS", d / "dossiers.json"),
                       mock.patch.object(workflows.Workflow, "manquants", lambda self: [])]
        for p in self.patchs:
            p.start()
        self.plateau = Plateau(orchestrateur=None)

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.temp.cleanup()

    def test_le_llm_du_scenario_est_note(self):
        c = self.plateau.commander_clip("un chat", "A slow push-in.", "texte", None, "rapide", 5, "16:9 (Widescreen)",
                                        None, 1, ecrit_par="bonsai.gguf")
        self.assertEqual((c.llm, c.prompt), ("bonsai.gguf", "A slow push-in."))  # noté, le prompt reste tel quel
        c = self.plateau.commander("un chat", "A cat.", "bonsai.gguf", "krea2_turbo", "1:1 (Square)", None, 1)
        self.assertEqual((c.llm, c.prompt), ("bonsai.gguf", "A cat."))
        self.assertIsNone(self.plateau.commander_clip("x", "A cat.", "texte", None, "rapide", 5, "16:9 (Widescreen)",
                                                      None, 1).llm)
