"""Tests de l'installateur des moteurs (app/moteurs.py) : faux catalogue, fausses archives zip, GitHub simulé."""

import asyncio
import hashlib
import io
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import httpx
import yaml

from app import moteurs


def un_zip(fichiers: dict) -> bytes:
    tampon = io.BytesIO()
    with zipfile.ZipFile(tampon, "w") as z:
        for nom, contenu in fichiers.items():
            z.writestr(nom, contenu)
    return tampon.getvalue()


SERVEUR = un_zip({"llama-server.exe": "v2", "ggml.dll": "x"})
RUNTIME = un_zip({"cudart64_13.dll": "cuda"})
COMFY = un_zip({"ComfyUI_portable/ComfyUI/main.py": "print()", "ComfyUI_portable/python_embeded/python.exe": "py"})


def archive(nom, contenu, **extra):
    return {"depot": "exemple/depot", "tag": "t1", "fichier": nom, "taille": len(contenu),
            "sha256": hashlib.sha256(contenu).hexdigest(), **extra}


CATALOGUE = {"format": 1, "pilote_min": 580, "moteurs": {
    "llama": {"nom": "llama", "role": "LLM", "version": "b2", "dossier": "llama", "preuve": "llama-server.exe",
              "port": "llama", "archives": [archive("serveur.zip", SERVEUR), archive("runtime.zip", RUNTIME)]},
    "comfyui": {"nom": "ComfyUI", "role": "images", "version": "v1", "dossier": "comfyui", "preuve": "ComfyUI/main.py",
                "port": "comfyui", "facultatif": True,
                "archives": [archive("comfy.zip", COMFY, racine="ComfyUI_portable")]},
}}


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dans = Path(self.tmp.name) / "moteurs"
        self.archives = self.dans / "_archives"
        self.archives.mkdir(parents=True)
        fichier = Path(self.tmp.name) / "moteurs.yaml"
        fichier.write_text(yaml.safe_dump(CATALOGUE), encoding="utf-8")
        self.patchs = [mock.patch.object(moteurs, "CATALOGUE", fichier),
                       mock.patch.object(moteurs, "occupe", return_value=False)]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in self.patchs:
            p.stop()
        self.tmp.cleanup()

    def poser(self, *noms_contenus):
        for nom, contenu in noms_contenus:
            (self.archives / nom).write_bytes(contenu)

    def installer(self, id_moteur):
        return asyncio.run(moteurs.installer(id_moteur, self.dans, self.archives))

    def statut(self, id_moteur):
        return next(e for e in moteurs.etat(self.dans, self.archives) if e["id"] == id_moteur)

    def restes(self):
        return sorted(p.name for p in self.dans.iterdir() if p.suffix in (".new", ".extrait"))


class Installation(Base):
    def test_depuis_les_archives_deja_la(self):
        self.assertEqual(self.statut("llama")["statut"], "absent")
        self.assertEqual(self.statut("llama")["a_telecharger"], len(SERVEUR) + len(RUNTIME))
        self.poser(("serveur.zip", SERVEUR), ("runtime.zip", RUNTIME))
        self.assertEqual(self.statut("llama")["a_telecharger"], 0)
        with mock.patch.object(moteurs, "_telecharger") as telecharger:
            self.installer("llama")
        telecharger.assert_not_called()
        dossier = self.dans / "llama"
        self.assertEqual(sorted(p.name for p in dossier.iterdir()),
                         [moteurs.FICHE, "cudart64_13.dll", "ggml.dll", "llama-server.exe"])  # le runtime a rejoint le serveur
        self.assertEqual(json.loads((dossier / moteurs.FICHE).read_text(encoding="utf-8"))["version"], "b2")
        self.assertEqual(self.statut("llama")["statut"], "a_jour")
        self.assertEqual(self.restes(), [])

    def test_racine_de_l_archive(self):
        self.poser(("comfy.zip", COMFY))
        self.installer("comfyui")
        self.assertTrue((self.dans / "comfyui" / "ComfyUI" / "main.py").is_file())
        self.assertTrue((self.dans / "comfyui" / "python_embeded" / "python.exe").is_file())

    def test_installe_a_la_main_puis_autre_version(self):
        (self.dans / "llama").mkdir()
        (self.dans / "llama" / "llama-server.exe").write_text("à la main", encoding="utf-8")
        self.assertEqual(self.statut("llama")["statut"], "manuel")
        (self.dans / "llama" / moteurs.FICHE).write_text(json.dumps({"version": "b1"}), encoding="utf-8")
        self.assertEqual(self.statut("llama")["statut"], "different")

    def test_reinstaller_garde_la_precedente_et_revenir(self):
        (self.dans / "llama").mkdir()
        (self.dans / "llama" / "llama-server.exe").write_text("v1", encoding="utf-8")
        self.poser(("serveur.zip", SERVEUR), ("runtime.zip", RUNTIME))
        self.installer("llama")
        serveur = self.dans / "llama" / "llama-server.exe"
        self.assertEqual(serveur.read_text(encoding="utf-8"), "v2")
        self.assertTrue(self.statut("llama")["precedente"])
        self.assertIsNone(moteurs.revenir("llama", self.dans))  # la v1 était posée à la main : pas de fiche
        self.assertEqual(serveur.read_text(encoding="utf-8"), "v1")
        self.assertEqual(moteurs.revenir("llama", self.dans)["version"], "b2")  # et retour
        self.assertEqual(serveur.read_text(encoding="utf-8"), "v2")

    def test_revenir_sans_precedente(self):
        with self.assertRaisesRegex(moteurs.ErreurMoteur, "Pas de version précédente"):
            moteurs.revenir("llama", self.dans)


class Refus(Base):
    def test_moteur_en_marche(self):
        with mock.patch.object(moteurs, "occupe", return_value=True), \
                self.assertRaisesRegex(moteurs.ErreurMoteur, "tourne"):
            self.installer("llama")

    def test_place_insuffisante(self):
        with mock.patch.object(moteurs.shutil, "disk_usage", return_value=mock.Mock(free=10)), \
                self.assertRaisesRegex(moteurs.ErreurMoteur, "Place insuffisante"):
            self.installer("llama")

    def test_racine_absente_rien_ne_bouge(self):
        (self.dans / "comfyui").mkdir()
        (self.dans / "comfyui" / "ancien.txt").write_text("là", encoding="utf-8")
        mauvais = un_zip({"Autre/ComfyUI/main.py": "x"})
        catalogue = json.loads(json.dumps(CATALOGUE))
        catalogue["moteurs"]["comfyui"]["archives"] = [archive("comfy.zip", mauvais, racine="ComfyUI_portable")]
        moteurs.CATALOGUE.write_text(yaml.safe_dump(catalogue), encoding="utf-8")
        self.poser(("comfy.zip", mauvais))
        with self.assertRaisesRegex(moteurs.ErreurMoteur, "ne contient pas le dossier ComfyUI_portable"):
            self.installer("comfyui")
        self.assertEqual([p.name for p in (self.dans / "comfyui").iterdir()], ["ancien.txt"])
        self.assertEqual(self.restes(), [])

    def test_chemin_dangereux_dans_un_zip(self):
        piege = un_zip({"../../evade.txt": "x"})
        with self.assertRaisesRegex(moteurs.ErreurMoteur, "chemin dangereux"):
            moteurs._extraire(self._ecrire("piege.zip", piege), self.dans / "x")
        self.assertFalse((Path(self.tmp.name) / "evade.txt").exists())

    def _ecrire(self, nom, contenu):
        f = Path(self.tmp.name) / nom
        f.write_bytes(contenu)
        return f


VRAI_CLIENT = httpx.AsyncClient  # avant que les tests ne le remplacent par le faux


class FauxGitHub:
    """Remplace httpx.AsyncClient : sert `contenu`, en respectant l'en-tête Range."""

    def __init__(self, contenu, coupe_apres=None):
        self.contenu, self.coupe_apres, self.requetes = contenu, coupe_apres, []

    def __call__(self, **options):
        def repondre(requete):
            self.requetes.append(requete)
            debut = int(requete.headers.get("Range", "bytes=0-")[6:-1])
            corps = self.contenu[debut:]
            if self.coupe_apres is not None:
                corps, self.coupe_apres = corps[:self.coupe_apres], None
            return httpx.Response(206 if debut else 200, content=corps)
        return VRAI_CLIENT(transport=httpx.MockTransport(repondre), **options)


class Telechargement(Base):
    def obtenir(self, archive_):
        return asyncio.run(moteurs.obtenir(archive_, self.archives))

    def test_telecharge_depuis_la_release_et_reprend(self):
        a = CATALOGUE["moteurs"]["llama"]["archives"][0]
        github = FauxGitHub(SERVEUR, coupe_apres=10)
        with mock.patch.object(moteurs.httpx, "AsyncClient", github):
            with self.assertRaisesRegex(moteurs.ErreurMoteur, "invalide"):  # coupé : contenu incomplet
                self.obtenir({**a, "sha256": hashlib.sha256(SERVEUR[:10]).hexdigest() + "0"})
            (self.archives / "serveur.zip.part").write_bytes(SERVEUR[:10])  # un téléchargement interrompu
            fichier = self.obtenir(a)
        self.assertEqual(fichier.read_bytes(), SERVEUR)
        self.assertEqual(str(github.requetes[-1].url), "https://github.com/exemple/depot/releases/download/t1/serveur.zip")
        self.assertEqual(github.requetes[-1].headers["Range"], "bytes=10-")
        self.assertFalse((self.archives / "serveur.zip.part").exists())

    def test_empreinte_fausse_rien_ne_reste(self):
        a = {**CATALOGUE["moteurs"]["llama"]["archives"][0], "sha256": "0" * 64}
        with mock.patch.object(moteurs.httpx, "AsyncClient", FauxGitHub(SERVEUR)), \
                self.assertRaisesRegex(moteurs.ErreurMoteur, "SHA-256 de serveur.zip invalide"):
            self.obtenir(a)
        self.assertEqual(list(self.archives.iterdir()), [])

    def test_archive_locale_abimee_retelechargee(self):
        a = CATALOGUE["moteurs"]["llama"]["archives"][0]
        self.poser(("serveur.zip", b"\0" * len(SERVEUR)))  # bonne taille, mauvais contenu
        with mock.patch.object(moteurs.httpx, "AsyncClient", FauxGitHub(SERVEUR)):
            self.assertEqual(self.obtenir(a).read_bytes(), SERVEUR)


class TacheDeFond(Base):
    def setUp(self):
        super().setUp()
        self.poser(("serveur.zip", SERVEUR), ("runtime.zip", RUNTIME))
        self.journal = []

    def verrou(self):
        test = self

        class Verrou:
            async def __aenter__(self):
                test.journal.append(("verrou", (test.dans / "llama.new").is_dir()))  # tout est prêt à côté

            async def __aexit__(self, *_):
                test.journal.append(("libere", (test.dans / "llama").is_dir()))
        return Verrou()

    def test_installe_en_arriere_plan_et_echange_sous_verrou(self):
        async def scenario():
            t = moteurs.lancer("llama", verrou=self.verrou, dans=self.dans)
            self.assertEqual((t["nom"], t["fini"]), ("llama", False))
            with self.assertRaisesRegex(moteurs.ErreurMoteur, "déjà en cours"):
                moteurs.lancer("comfyui")
            await moteurs._execution
            return moteurs.tache()

        t = asyncio.run(scenario())
        self.assertEqual((t["etape"], t["erreur"], t["fini"]), ("Installé ✓", None, True))
        self.assertEqual(self.journal, [("verrou", True), ("libere", True)])
        self.assertTrue((self.dans / "llama" / "llama-server.exe").is_file())

    def test_annuler_un_telechargement_garde_le_part(self):
        (self.archives / "serveur.zip").unlink()

        class Lent(FauxGitHub):
            def __call__(self, **options):
                async def repondre(requete):
                    return httpx.Response(200, content=self.flux())
                return VRAI_CLIENT(transport=httpx.MockTransport(repondre), **options)

            async def flux(self):
                yield self.contenu[:10]
                await asyncio.sleep(30)  # le reste n'arrive jamais : on annule avant
                yield self.contenu[10:]

        async def scenario():
            with mock.patch.object(moteurs.httpx, "AsyncClient", Lent(SERVEUR)):
                moteurs.lancer("llama", dans=self.dans)
                for _ in range(300):  # 3 s au plus
                    if moteurs.tache()["etape"].startswith("Téléchargement"):
                        break
                    await asyncio.sleep(0.01)
                self.assertTrue(moteurs.annuler())
                await asyncio.gather(moteurs._execution, return_exceptions=True)
            return moteurs.tache()

        t = asyncio.run(scenario())
        self.assertIn("Annulé", t["erreur"])
        self.assertEqual((self.archives / "serveur.zip.part").read_bytes(), SERVEUR[:10])  # reprendra de là
        self.assertFalse((self.dans / "llama").exists())

    def test_revenir_efface_le_message_installe(self):
        asyncio.run(self._lancer_et_attendre())
        self.assertEqual(moteurs.tache()["etape"], "Installé ✓")
        (self.dans / "llama.prev").mkdir()
        (self.dans / "llama.prev" / "llama-server.exe").write_text("v1", encoding="utf-8")
        moteurs.revenir("llama", self.dans)
        self.assertIsNone(moteurs.tache())

    async def _lancer_et_attendre(self):
        moteurs.lancer("llama", dans=self.dans)
        await moteurs._execution

    def test_moteur_etranger_sur_le_port_rien_ne_change(self):
        async def scenario():
            with mock.patch.object(moteurs, "occupe", return_value=True):  # ni arrêté par l'orchestrateur, ni à nous
                moteurs.lancer("llama", verrou=self.verrou, dans=self.dans)
                await moteurs._execution
            return moteurs.tache()

        t = asyncio.run(scenario())
        self.assertIn("tourne encore", t["erreur"])
        self.assertFalse((self.dans / "llama").exists())
        self.assertEqual(self.restes(), [])


class Carte(Base):
    def test_sans_nvidia(self):
        with mock.patch.dict("sys.modules", {"pynvml": None}):
            c = moteurs.carte()
        self.assertEqual((c["nvidia"], c["ok"]), (False, False))

    def test_pilote_trop_ancien(self):
        faux = mock.Mock(nvmlDeviceGetName=lambda h: "RTX", nvmlSystemGetDriverVersion=lambda: "566.36")
        with mock.patch.dict("sys.modules", {"pynvml": faux}):
            c = moteurs.carte()
        self.assertEqual((c["nvidia"], c["ok"], c["pilote"]), (True, False, "566.36"))
        self.assertIn("580", c["message"])


if __name__ == "__main__":
    unittest.main()
