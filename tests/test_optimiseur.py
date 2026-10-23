"""Tests de l'⚡ Optimiser (app/optimiseur.py) : propositions selon le matériel et les recommandations sourcées.

Le matériel est simulé (VRAM, nom du GPU) et les réglages vont dans un fichier temporaire.
"""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

from app import modeles, optimiseur, reglages, workflows

GO = 2**30
LLM = "Ministral-3-14B-Instruct-2512-Q4_K_M.gguf"
BONSAI = "Ternary-Bonsai-2-27B-PTQ1_0.gguf"


class Optimiseur(unittest.TestCase):
    def setUp(self):
        self.dossier = tempfile.TemporaryDirectory()
        d = Path(self.dossier.name)
        self.patchs = [mock.patch.object(reglages, "FICHIER", d / "reglages.yaml"),
                       mock.patch.object(reglages, "FICHIER_SAMPLERS", d / "samplers.json"),
                       mock.patch.object(reglages, "_cache", None),
                       mock.patch.object(modeles, "llm_disponibles", return_value=[LLM, BONSAI]),
                       mock.patch.object(optimiseur.mesures, "nom_gpu", return_value="GPU de test")]
        for p in self.patchs:
            p.start()
        self.carte(16)

    def carte(self, vram_go: float, occupee_go: float = 1.0):
        for nom, valeur in (("vram_totale", vram_go * GO), ("vram_utilisee", occupee_go * GO)):
            p = mock.patch.object(optimiseur.mesures, nom, return_value=int(valeur))
            p.start()
            self.patchs.append(p)

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        self.dossier.cleanup()

    def propositions(self) -> dict:
        r = optimiseur.propositions()
        return {(p["cible"], p["cle"]): p for p in r["propositions"]}, r

    def test_recommandations_sourcees(self):
        props, r = self.propositions()
        qwen = props[("qwen_image_2_1", "steps")]
        self.assertEqual((qwen["actuel"], qwen["propose"]), (25, 40))
        self.assertTrue(qwen["source"].startswith("https://"))
        self.assertEqual(props[("qwen_image_edit_2511", "shift")]["propose"], 3.0)
        self.assertEqual(props[(LLM, "temperature_peaufinage")]["propose"], 0.05)
        self.assertEqual(props[(BONSAI, "presence_penalty")]["propose"], 1.5)
        self.assertNotIn(("krea2_turbo", "steps"), props)  # déjà à la valeur recommandée : rien à proposer
        self.assertEqual(r["materiel"]["gpu"], "GPU de test")

    def test_appliquer_vide_les_propositions(self):
        props, _ = self.propositions()
        for (cible, cle), p in props.items():
            if p["type"] == "llm":
                reglages.modifier_llm(cible, {cle: p["propose"]})
            else:
                reglages.modifier_workflow(cible, {cle: p["propose"]})
        props, _ = self.propositions()
        self.assertEqual(props, {})

    def test_alertes_vram_sur_16_go(self):
        _, r = self.propositions()
        self.assertTrue(any("Qwen-Image-Edit 2511" in a and "RAM" in a for a in r["alertes"]))
        self.assertTrue(any("Krea 2 Turbo" in a and "1 MP" in a for a in r["alertes"]))

    def test_mégapixels_selon_la_carte(self):
        self.carte(48)
        props, _ = self.propositions()
        self.assertEqual(props[("qwen_image_2_1", "megapixels")]["propose"], 4.2)  # assez de VRAM : natif
        self.carte(20)  # 19 Go libres - 8,7 Go estimés : marge entre 6 et 12 Go
        props, _ = self.propositions()
        self.assertEqual(props[("qwen_image_2_1", "megapixels")]["propose"], 2.0)  # marge moyenne : plafonné
        self.carte(12)  # petite marge : reste à 1 MP, déjà en place, donc rien à proposer
        props, _ = self.propositions()
        self.assertNotIn(("qwen_image_2_1", "megapixels"), props)

    def test_valeur_refusee_non_proposee(self):
        # Une recommandation que le réglage n'accepte pas (ici un sampler inconnu) est ignorée, sans erreur.
        wf = workflows.obtenir("krea2_turbo")
        fiche = {**wf.fiche, "recommandations": {"valeurs": [{"cle": "sampler", "valeur": "inexistant"}]}}
        faux = [workflows.Workflow(fiche, wf.api)]
        with mock.patch.object(optimiseur.workflows, "catalogue", return_value=faux):
            props, _ = self.propositions()
        self.assertNotIn(("krea2_turbo", "sampler"), props)

    def test_modele_sans_recommandation(self):
        with mock.patch.object(modeles, "llm_disponibles", return_value=["inconnu.gguf"]), \
                mock.patch.object(optimiseur.os.path, "getsize", return_value=4 * GO):
            _, r = self.propositions()
        self.assertIn("inconnu", r["sans_recommandation"])

    def test_fichiers_de_recommandations_valides(self):
        """Chaque recommandation rangée vise un réglage qui existe et respecte ses bornes."""
        for wf in workflows.catalogue():
            params = reglages.params_workflow(wf)
            for r in (wf.fiche.get("recommandations") or {}).get("valeurs") or []:
                self.assertIn(r["cle"], params, wf.id)
                params[r["cle"]].valider(r["valeur"])
                self.assertTrue(r.get("source", "").startswith("https://"), (wf.id, r["cle"]))
        params = reglages.params_llm()
        for fichier, reco in yaml.safe_load(optimiseur.FICHIER_LLM.read_text(encoding="utf-8")).items():
            self.assertTrue(reco["source"].startswith("https://"), fichier)
            for r in reco["valeurs"]:
                params[r["cle"]].valider(r["valeur"])


if __name__ == "__main__":
    unittest.main()
