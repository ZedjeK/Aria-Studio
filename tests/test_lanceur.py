"""Le lanceur (tools/launcher/Launcher.cs) compilé pour de vrai, puis essayé sans fenêtre dans un dossier utilisateur
jetable (USERPROFILE redirigé) : la proposition d'installer, l'installation, la version précédente, le raccourci du
Bureau, la marque « téléchargé depuis Internet » et « Ne plus me proposer ».

Les options --essai-* du lanceur n'ouvrent aucune fenêtre et ne lancent pas Aria. Sans le compilateur C# de Windows,
ces tests sont sautés.
"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CSC = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Microsoft.NET" / "Framework64" / "v4.0.30319" / "csc.exe"
EXE = "Aria-Studio.exe"


@unittest.skipUnless(os.name == "nt" and CSC.exists(), "compilateur C# de Windows absent")
class Lanceur(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.compile = tempfile.TemporaryDirectory()
        cls.exe = Path(cls.compile.name) / EXE
        r = subprocess.run([str(CSC), "/nologo", "/target:winexe", f"/out:{cls.exe}", "/codepage:65001",
                            "/reference:System.Windows.Forms.dll", "/reference:System.Drawing.dll",
                            "/reference:System.Web.Extensions.dll",
                            f"/resource:{ROOT / 'tools' / 'launcher' / 'aria-studio.ico'},aria-studio.ico",
                            str(ROOT / "tools" / "launcher" / "Launcher.cs"),
                            str(ROOT / "tools" / "launcher" / "Icone.cs")], capture_output=True)
        if r.returncode:
            raise AssertionError(r.stdout.decode("utf-8", "replace"))

    @classmethod
    def tearDownClass(cls):
        cls.compile.cleanup()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.profil = base / "profil"
        self.maison = self.profil / ".Aria-Studio"
        self.programme = self.maison / "programme"
        self.zip = base / "Telechargements" / "Aria-Studio-2.1.0"
        (self.zip / "app").mkdir(parents=True)
        (self.zip / "python").mkdir()
        shutil.copy2(self.exe, self.zip / EXE)
        (self.zip / "app" / "__init__.py").write_text('VERSION = "2.1.0"', encoding="utf-8")
        (self.zip / "config.yaml").write_text("app:\n    port: 8075\n", encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def lancer(self, exe: Path, *options) -> int:
        env = {**os.environ, "USERPROFILE": str(self.profil)}
        return subprocess.run([str(exe), *options], env=env, capture_output=True, timeout=60).returncode

    def proposerait(self, exe: Path, *options) -> bool:
        return self.lancer(exe, "--essai-proposer", *options) == 0

    def test_propose_depuis_un_zip(self):
        self.assertTrue(self.proposerait(self.zip / EXE))

    def test_ne_propose_pas_en_portable_ni_avec_l_option(self):
        self.assertFalse(self.proposerait(self.zip / EXE, "--portable"))
        (self.zip / "data").mkdir()
        self.assertFalse(self.proposerait(self.zip / EXE))

    def reglages(self) -> dict:
        return json.loads((self.maison / "reglages.json").read_text(encoding="utf-8"))

    def test_ne_plus_me_proposer_garde_le_reste(self):
        self.maison.mkdir(parents=True)
        (self.maison / "reglages.json").write_text(json.dumps(
            {"migration": {"date": "2026-11-13"}, "fond": {"vitesse": "lente"}, "llm": {"a.gguf": {"contexte": 8192}},
             "loras": {"image/K/loras/s.safetensors": {"force": 0.8, "declencheurs": "néon"}},
             "installation": {"proposee": True}}, ensure_ascii=False), encoding="utf-8")
        self.assertTrue(self.proposerait(self.zip / EXE))
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-ne-plus-proposer"), 0)
        self.assertEqual(self.reglages(), {
            "migration": {"date": "2026-11-13"}, "fond": {"vitesse": "lente"}, "llm": {"a.gguf": {"contexte": 8192}},
            "loras": {"image/K/loras/s.safetensors": {"force": 0.8, "declencheurs": "néon"}},
            "installation": {"proposee": False}})
        self.assertFalse(self.proposerait(self.zip / EXE))

    def test_ne_plus_me_proposer_sans_reglages_json(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-ne-plus-proposer"), 0)
        self.assertEqual(self.reglages(), {"installation": {"proposee": False}})  # sans « migration » : Aria migrera

    def test_ancien_aria_yaml_tant_qu_aria_n_a_pas_migre(self):
        self.maison.mkdir(parents=True)
        (self.maison / "aria.yaml").write_text("favoris: {}\ninstallation_proposee: false\n", encoding="utf-8")
        self.assertFalse(self.proposerait(self.zip / EXE))
        (self.maison / "reglages.json").write_text('{"migration": {"date": "x"}}', encoding="utf-8")
        self.assertTrue(self.proposerait(self.zip / EXE))  # migré sans la clé : c'est reglages.json qui fait foi

    def test_reglages_illisible_jamais_ecrase(self):
        self.maison.mkdir(parents=True)
        (self.maison / "reglages.json").write_text("{ cassé", encoding="utf-8")
        self.assertTrue(self.proposerait(self.zip / EXE))
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-ne-plus-proposer"), 1)
        self.assertEqual((self.maison / "reglages.json").read_text(encoding="utf-8"), "{ cassé")

    def adresse(self, exe: Path) -> str:
        env = {**os.environ, "USERPROFILE": str(self.profil)}
        return subprocess.run([str(exe), "--essai-adresse"], env=env, capture_output=True, timeout=60).stdout.decode()

    def test_adresse_avec_le_jeton_de_la_session(self):
        self.assertEqual(self.adresse(self.zip / EXE), "http://127.0.0.1:8075/")  # pas de session : l'adresse nue
        (self.maison / "cache").mkdir(parents=True)
        (self.maison / "cache" / "session.json").write_text('{"port": 8075, "jeton": "a+b/c=="}', encoding="utf-8")
        self.assertEqual(self.adresse(self.zip / EXE), "http://127.0.0.1:8075/?jeton=a%2Bb%2Fc%3D%3D")

    def test_adresse_en_mode_portable(self):
        (self.zip / "data").mkdir()
        (self.zip / "cache").mkdir()
        (self.zip / "cache" / "session.json").write_text('{"jeton": "portable"}', encoding="utf-8")
        self.assertEqual(self.adresse(self.zip / EXE), "http://127.0.0.1:8075/?jeton=portable")

    def test_mise_a_jour_attend_l_arret_d_aria_puis_installe(self):
        import sys
        import time
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)  # la version d'avant
        nouvelle = self.maison / "cache" / "mise-a-jour" / "Aria-Studio-2.2.0"
        shutil.copytree(self.zip, nouvelle)
        (nouvelle / "app" / "__init__.py").write_text('VERSION = "2.2.0"', encoding="utf-8")
        serveur = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(2)"])  # « Aria » qui s'arrête
        debut = time.time()
        self.assertEqual(self.lancer(nouvelle / EXE, "--essai-mise-a-jour", str(serveur.pid)), 0)
        self.assertGreaterEqual(time.time() - debut, 1.0)  # il a attendu la fin du serveur
        self.assertEqual((self.programme / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.2.0"')
        self.assertEqual((self.maison / "programme.precedent" / "app" / "__init__.py").read_text(encoding="utf-8"),
                         'VERSION = "2.1.0"')

    def verrou_icone(self) -> str:
        """Le nom du verrou de l'icône pour ce dossier d'Aria (NomVerrou de Icone.cs : FNV-1a du chemin en minuscules)."""
        h = 2166136261
        for c in str(self.maison).lower():
            h = ((h ^ ord(c)) * 16777619) & 0xFFFFFFFF
        return f"Local\Aria-Studio-icone-{h:08x}"

    def test_mise_a_jour_attend_la_fermeture_de_l_icone(self):
        import ctypes
        import threading
        import time
        noyau = ctypes.WinDLL("kernel32", use_last_error=True)
        noyau.CreateMutexW.restype = ctypes.c_void_p
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        nouvelle = self.maison / "cache" / "mise-a-jour" / "Aria-Studio-2.2.0"
        shutil.copytree(self.zip, nouvelle)
        pret = threading.Event()

        def icone():  # l'icône d'une Aria qui s'arrête : elle tient son verrou encore 2 s
            verrou = noyau.CreateMutexW(None, True, self.verrou_icone())
            pret.set()
            time.sleep(2)
            noyau.ReleaseMutex(ctypes.c_void_p(verrou))
            noyau.CloseHandle(ctypes.c_void_p(verrou))

        fil = threading.Thread(target=icone)
        fil.start()
        pret.wait(5)
        debut = time.time()
        self.assertEqual(self.lancer(nouvelle / EXE, "--essai-mise-a-jour", "999999"), 0)  # serveur déjà arrêté
        fil.join()
        self.assertGreaterEqual(time.time() - debut, 1.5)  # il a attendu l'icône avant d'installer
        self.assertTrue((self.maison / "programme.precedent").is_dir())

    def test_mise_a_jour_sans_numero_refusee(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-mise-a-jour"), 1)
        self.assertFalse(self.programme.exists())

    def test_installe_dans_programme_sans_la_marque_internet(self):
        with open(str(self.zip / "config.yaml") + ":Zone.Identifier", "w", encoding="utf-8") as marque:
            marque.write("[ZoneTransfer]\nZoneId=3\n")
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        self.assertTrue((self.programme / EXE).is_file())
        self.assertEqual((self.programme / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.1.0"')
        self.assertTrue((self.programme / "python").is_dir())
        with self.assertRaises(OSError):
            open(str(self.programme / "config.yaml") + ":Zone.Identifier", encoding="utf-8").close()
        self.assertTrue((self.zip / EXE).is_file())  # le zip décompressé reste tel quel
        self.assertFalse(self.proposerait(self.programme / EXE))  # depuis programme\ : plus de proposition
        self.assertFalse((self.profil / "Desktop" / "Aria Studio.lnk").exists())  # case décochée

    def test_reinstaller_garde_la_version_d_avant(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        (self.zip / "app" / "__init__.py").write_text('VERSION = "2.2.0"', encoding="utf-8")
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        self.assertEqual((self.programme / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.2.0"')
        precedent = self.maison / "programme.precedent"
        self.assertEqual((precedent / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.1.0"')
        self.assertFalse((self.maison / "programme.nouveau").exists())

    def test_la_version_d_avant_ne_propose_rien_et_ne_s_installe_pas_sur_elle_meme(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        precedent = self.maison / "programme.precedent"
        self.assertFalse(self.proposerait(precedent / EXE))
        self.assertEqual(self.lancer(precedent / EXE, "--essai-installer"), 1)
        self.assertTrue((precedent / EXE).is_file())
        self.assertTrue((self.programme / EXE).is_file())

    def test_programme_ouvert_rien_ne_change(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
        (self.zip / "app" / "__init__.py").write_text('VERSION = "2.2.0"', encoding="utf-8")
        with open(self.programme / "config.yaml", encoding="utf-8"):  # Aria lancée depuis programme\
            self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 1)
        self.assertEqual((self.programme / "app" / "__init__.py").read_text(encoding="utf-8"), 'VERSION = "2.1.0"')
        self.assertFalse((self.maison / "programme.precedent").exists())

    def test_raccourci_sur_le_bureau_de_l_utilisateur(self):
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer", "--raccourci"), 0)
        lien = self.profil / "Desktop" / "Aria Studio.lnk"
        self.assertTrue(lien.is_file())
        self.assertIn(str(self.programme / EXE).encode("utf-16-le"), lien.read_bytes())

    def test_raccourci_deja_la_jamais_remplace_en_silence(self):
        (self.profil / "Desktop").mkdir(parents=True)
        lien = self.profil / "Desktop" / "Aria Studio.lnk"
        lien.write_bytes(b"a moi")
        self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer", "--raccourci"), 0)
        self.assertEqual(lien.read_bytes(), b"a moi")


    # --- l'icône de notification (tools/launcher/Icone.cs) ---

    def sortie(self, exe: Path, *options, env: dict | None = None) -> tuple[int, dict]:
        env = {**os.environ, "USERPROFILE": str(self.profil), **(env or {})}
        r = subprocess.run([str(exe), *options], env=env, capture_output=True, timeout=60)
        lignes = r.stdout.decode("utf-8").splitlines()
        valeurs = {}
        for ligne in lignes:
            cle, _, valeur = ligne.partition("=")
            valeurs.setdefault(cle, []).append(valeur)
        return r.returncode, {k: v[0] if len(v) == 1 else v for k, v in valeurs.items()}

    def test_demarrer_avec_windows_seulement_installee_et_dans_la_cle_d_essai(self):
        import uuid
        import winreg
        cle = r"Software\Aria-Studio-tests\lanceur-" + uuid.uuid4().hex
        env = {"ARIA_ESSAI_CLE_RUN": cle, "ARIA_TESTS_ISOLES": "1"}
        try:
            code, e = self.sortie(self.zip / EXE, "--essai-demarrage", "etat", env=env)
            self.assertEqual((code, e["possible"], e["actif"]), (0, "0", "0"))
            code, _ = self.sortie(self.zip / EXE, "--essai-demarrage", "activer", env=env)
            self.assertEqual(code, 2)  # pas installée : rien d'écrit
            self.assertEqual(self.lancer(self.zip / EXE, "--essai-installer"), 0)
            code, e = self.sortie(self.programme / EXE, "--essai-demarrage", "activer", env=env)
            self.assertEqual((code, e["possible"], e["actif"]), (0, "1", "1"))
            # la même commande que celle qu'écrit app/demarrage.py
            self.assertEqual(e["valeur"], f'"{self.programme / EXE}" --silencieux')
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, cle) as k:
                self.assertEqual(winreg.QueryValueEx(k, "Aria Studio")[0], e["valeur"])
            code, e = self.sortie(self.programme / EXE, "--essai-demarrage", "desactiver", env=env)
            self.assertEqual((code, e["actif"]), (0, "0"))
        finally:
            for chemin in (cle, r"Software\Aria-Studio-tests"):
                try:
                    winreg.DeleteKey(winreg.HKEY_CURRENT_USER, chemin)
                except OSError:
                    pass

    def icone(self, etat: dict) -> dict:
        fichier = Path(self.tmp.name) / "etat.json"
        fichier.write_text(json.dumps({"telechargements": [], "maj": {"disponible": False}, **etat}), encoding="utf-8")
        code, e = self.sortie(self.zip / EXE, "--essai-icone", str(fichier))
        self.assertEqual(code, 0)
        return e

    REPOS = {"comfyui": {"etat": "veille"}, "llama": {"etat": "absent", "modele": None}, "vram_utilisee_go": 0.5,
             "vram_totale_go": 16.0, "commande": None, "garde": {"attente": None, "alerte": None}}

    def test_icone_delavee_sans_pastille_au_repos(self):
        e = self.icone({"etat": self.REPOS})
        self.assertEqual((e["pastille"], e["pleine"]), ("", "0"))
        self.assertEqual(e["infobulle"], "Aria Studio · rien en cours")

    def test_icone_pleine_quand_un_moteur_tient_la_carte(self):
        e = self.icone({"etat": {**self.REPOS, "comfyui": {"etat": "actif"}}})
        self.assertEqual((e["pastille"], e["pleine"]), ("", "1"))
        self.assertIn("|ComfyUI chargé · 0,5 / 16 Go", e["ligne"])

    def test_pastille_bleue_pour_une_mise_a_jour(self):
        e = self.icone({"etat": self.REPOS, "maj": {"disponible": True, "derniere": {"version": "2.1.0"}}})
        self.assertEqual((e["pastille"], e["pleine"]), ("bleu", "0"))

    def test_pastille_cyan_pour_un_telechargement_avant_la_mise_a_jour(self):
        e = self.icone({"etat": self.REPOS, "maj": {"disponible": True, "derniere": {"version": "2.1.0"}},
                        "telechargements": [{"nom": "Krea", "etat": "termine", "recu": 1, "taille": 1},
                                            {"nom": "Wan", "etat": "en_cours", "recu": 25, "taille": 100}]})
        self.assertEqual(e["pastille"], "cyan")
        self.assertIn("cyan|Téléchargement : Wan · 25 %", e["ligne"])

    def test_pastille_orange_avant_tout_le_reste(self):
        commande = {"id": "c1", "type": "creation", "etat": "en_cours", "etape": "Image 1/2", "progression": 0.5, "file": 2}
        e = self.icone({"etat": {**self.REPOS, "commande": commande,
                                 "garde": {"attente": {"moteur": "ComfyUI", "vram_go": 9.0}, "alerte": None}},
                        "maj": {"disponible": True, "derniere": {"version": "2.1.0"}}})
        self.assertEqual(e["pastille"], "orange")
        self.assertEqual(e["ligne"][0], "orange|Carte graphique : une autre appli la tient · ta génération attend")
        self.assertIn("cyan|Génère : image 1/2 · 50 % · 2 en file", e["ligne"])

    def test_infobulle_tient_dans_la_limite_de_windows(self):
        commande = {"id": "c1", "type": "creation", "etat": "en_cours", "etape": "", "progression": 0.123, "file": 12345678}
        e = self.icone({"etat": {**self.REPOS, "commande": commande}})
        self.assertLessEqual(len(e["infobulle"]), 63)


if __name__ == "__main__":
    unittest.main()
