"""Journal des opérations (app/journal.py), aria.log borné (app/log_borne.py), et ce qui est noté par les modules :
corbeilles, réglages. Tout vit dans un dossier temporaire."""

import json
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

from app import fichier_reglages, journal, modeles, sequences
from app.log_borne import LogBorne
from tests import reglages_jetables


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.racine = Path(self.tmp.name)
        self.patch = reglages_jetables.patch(self.racine)
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.tmp.cleanup()

    def actions(self) -> list[str]:
        return [e["action"] for e in journal.lire(journal.MAX)]


class Journal(Base):
    def test_le_plus_recent_en_premier(self):
        journal.noter("Un", "a")
        journal.noter("Deux", "b", "erreur", "parce que")
        e = journal.lire()
        self.assertEqual([x["action"] for x in e], ["Deux", "Un"])
        self.assertEqual((e[0]["resultat"], e[0]["detail"]), ("erreur", "parce que"))
        self.assertTrue((self.racine / "journal.json").is_file())

    def test_cinq_cents_au_plus(self):
        for i in range(journal.MAX + 20):
            journal.noter("Op", str(i))
        e = json.loads((self.racine / "journal.json").read_text(encoding="utf-8"))
        self.assertEqual(len(e), journal.MAX)
        self.assertEqual(e[0]["objet"], str(journal.MAX + 19))
        self.assertEqual(len(journal.lire()), journal.VISIBLES)

    def test_illisible_mis_de_cote(self):
        (self.racine / "journal.json").write_text("{ cassé", encoding="utf-8")
        journal.noter("Après", "x")
        self.assertEqual(self.actions(), ["Après"])
        cote = list(self.racine.glob("journal-illisible-*.json"))
        self.assertEqual([c.read_text(encoding="utf-8") for c in cote], ["{ cassé"])

    def test_jamais_d_exception(self):
        with mock.patch.object(Path, "replace", side_effect=PermissionError(13, "refusé")):
            journal.noter("Op", "x")  # ne lève pas

    def test_ecritures_simultanees(self):
        fils = [threading.Thread(target=journal.noter, args=("Op", str(i))) for i in range(30)]
        for f in fils:
            f.start()
        for f in fils:
            f.join()
        self.assertEqual(len(journal.lire(journal.MAX)), 30)


class CeQuiEstNote(Base):
    def test_un_changement_de_reglages_dit_la_section(self):
        fichier_reglages.ecrire("fond", {"vitesse": "lente"})
        fichier_reglages.ecrire("fond", {"vitesse": "lente"})  # rien ne change : rien de noté
        self.assertEqual([(e["action"], e["objet"]) for e in journal.lire()], [("Réglages modifiés", "fond")])

    def test_la_restauration_et_la_migration(self):
        (self.racine / "data").mkdir()
        (self.racine / "data" / "fond.json").write_text('{"vitesse": "lente"}', encoding="utf-8")
        fichier_reglages.section("fond")
        fichier_reglages.ecrire("fond", {"vitesse": "rapide"})
        fichier_reglages.restaurer(fichier_reglages.sauvegardes()[0]["nom"])
        self.assertEqual(self.actions(), ["Réglages restaurés", "Réglages modifiés", "Migration des réglages"])

    def test_la_corbeille_des_modeles(self):
        dossier = self.racine / "modeles"
        cle = "image/K/modeles/x.safetensors"
        (dossier / cle).parent.mkdir(parents=True)
        (dossier / cle).write_bytes(b"x")
        with mock.patch.object(modeles, "DOSSIER", dossier), \
                mock.patch.object(modeles, "REGISTRE", self.racine / "data" / "modeles.json"), \
                mock.patch.object(modeles, "CORBEILLE_REGISTRE", self.racine / "data" / "corbeille.json"):
            modeles._sauver_registre({cle: {"source": "civitai"}})
            modeles.mettre_a_la_corbeille(cle)
            modeles.restaurer(cle)
            modeles.mettre_a_la_corbeille(cle)
            modeles.vider_corbeille()
        self.assertEqual(self.actions(), ["Corbeille des modèles vidée", "Modèle mis à la corbeille", "Modèle restauré",
                                          "Modèle mis à la corbeille"])

    def test_la_corbeille_des_sequences(self):
        with mock.patch.object(sequences, "DOSSIER", self.racine / "data" / "sequences"):
            sequences.vider_corbeille()  # rien à vider : pas de dossier, rien de noté
        self.assertEqual(self.actions(), [])


class AriaLog(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.chemin = Path(self.tmp.name) / "logs" / "aria.log"

    def tearDown(self):
        self.tmp.cleanup()

    def test_tourne_au_dela_de_la_limite(self):
        log = LogBorne(self.chemin, limite=100)
        for i in range(30):
            log.write(f"ligne {i:02d}\n")
        log.close()
        self.assertLessEqual(self.chemin.stat().st_size, 100)
        precedent = self.chemin.with_name("aria.log.1")
        self.assertTrue(precedent.is_file())
        self.assertIn("ligne 29", self.chemin.read_text(encoding="utf-8") + precedent.read_text(encoding="utf-8"))
        self.assertEqual(sorted(p.name for p in self.chemin.parent.iterdir()), ["aria.log", "aria.log.1"])

    def test_un_gros_fichier_d_avant_tourne_a_l_ouverture(self):
        self.chemin.parent.mkdir(parents=True)
        self.chemin.write_text("x" * 500, encoding="utf-8")
        log = LogBorne(self.chemin, limite=100)
        log.write("nouveau\n")
        log.close()
        self.assertEqual(self.chemin.read_text(encoding="utf-8"), "nouveau\n")
        self.assertEqual(self.chemin.with_name("aria.log.1").read_text(encoding="utf-8"), "x" * 500)

    def test_print_et_les_traces_y_vont(self):
        log = LogBorne(self.chemin)
        print("bonjour", file=log)
        log.flush()
        log.close()
        self.assertEqual(self.chemin.read_text(encoding="utf-8"), "bonjour\n")


if __name__ == "__main__":
    unittest.main()
