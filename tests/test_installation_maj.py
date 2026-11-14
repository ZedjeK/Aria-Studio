"""Mise à jour intégrée (app/installation_maj.py) : GitHub simulé (httpx.MockTransport), zips fabriqués à la volée,
cache/mise-a-jour dans un dossier temporaire. Rien ne part sur le réseau, rien n'est lancé.
"""

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

from app import installation_maj as maj
from tests import reglages_jetables

VRAI_CLIENT = httpx.AsyncClient
VERSION = "9.9.9"
NOM = f"Aria-Studio-{VERSION}.zip"
DL = f"https://github.com/ZedjeK/Aria-Studio/releases/download/v{VERSION}"
STOCKAGE = "https://release-assets.githubusercontent.com/github-production-release-asset/1/"


def fabriquer_zip(version=VERSION, annonce=None, en_plus=None) -> bytes:
    racine = f"Aria-Studio-{version}/"
    sortie = io.BytesIO()
    with zipfile.ZipFile(sortie, "w") as z:
        z.writestr(f"{racine}Aria-Studio.exe", b"MZ")
        z.writestr(f"{racine}app/__init__.py", f'VERSION = "{annonce or version}"\n')
        for nom, contenu in (en_plus or {}).items():
            z.writestr(nom, contenu)
    return sortie.getvalue()


class FauxGitHub:
    def __init__(self, contenu: bytes, digest: str | None = None, somme: str | None = None, hote_zip: str = STOCKAGE,
                 reprise: bool = True):
        self.contenu, self.reprise, self.hote_zip = contenu, reprise, hote_zip
        empreinte = hashlib.sha256(contenu).hexdigest()
        self.digest = digest if digest is not None else f"sha256:{empreinte}"
        self.somme = somme if somme is not None else f"{empreinte}  {NOM}\n"
        self.vues = []

    def __call__(self, requete: httpx.Request) -> httpx.Response:
        url = str(requete.url)
        self.vues.append((url, requete.headers.get("range")))
        if url.endswith("/releases/latest"):
            return httpx.Response(200, json={"tag_name": f"v{VERSION}", "assets": [
                {"name": NOM, "size": len(self.contenu), "digest": self.digest, "browser_download_url": f"{DL}/{NOM}"},
                {"name": f"{NOM}.sha256", "size": 90, "browser_download_url": f"{DL}/{NOM}.sha256"}]})
        if url == f"{DL}/{NOM}.sha256":
            return httpx.Response(302, headers={"location": f"{STOCKAGE}somme"})
        if url == f"{DL}/{NOM}":
            return httpx.Response(302, headers={"location": f"{self.hote_zip}zip"})
        if url == f"{STOCKAGE}somme":
            return httpx.Response(200, text=self.somme)
        if url.endswith("/zip"):
            plage = requete.headers.get("range")
            if plage and self.reprise:
                debut = int(plage.split("=")[1].rstrip("-"))
                return httpx.Response(206, content=self.contenu[debut:])
            return httpx.Response(200, content=self.contenu)
        return httpx.Response(404)


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.patchs = [mock.patch.object(maj, "DOSSIER", base / "cache" / "mise-a-jour"),
                       mock.patch.object(maj, "RELAIS", base / "cache" / "mise-a-jour" / "relais.json"),
                       mock.patch.object(maj, "ROOT", base / "code"),
                       mock.patch.object(maj, "_etat", {"etape": "repos"}),
                       reglages_jetables.patch(base)]
        for p in self.patchs:
            p.start()

    def tearDown(self):
        for p in reversed(self.patchs):
            p.stop()
        self.tmp.cleanup()

    def preparer(self, github: FauxGitHub) -> Path:
        client = lambda **kw: VRAI_CLIENT(transport=httpx.MockTransport(github), **kw)  # noqa: E731
        with mock.patch.object(maj.httpx, "AsyncClient", client):
            return asyncio.run(maj.preparer())


class Preparer(Base):
    def test_telecharge_verifie_et_decompresse(self):
        github = FauxGitHub(fabriquer_zip())
        dossier = self.preparer(github)
        self.assertEqual(dossier, maj.DOSSIER / f"Aria-Studio-{VERSION}")
        self.assertTrue((dossier / "Aria-Studio.exe").is_file())
        self.assertEqual(maj.etat()["version"], VERSION)
        self.assertIn((f"{STOCKAGE}zip", None), github.vues)  # redirection vers le stockage de GitHub suivie

    def test_empreinte_de_github_fausse(self):
        with self.assertRaisesRegex(maj.ErreurMiseAJour, "ne concordent pas"):
            self.preparer(FauxGitHub(fabriquer_zip(), digest="sha256:" + "0" * 64))

    def test_fichier_sha256_faux(self):
        with self.assertRaisesRegex(maj.ErreurMiseAJour, "ne concordent pas"):
            self.preparer(FauxGitHub(fabriquer_zip(), somme="0" * 64 + f"  {NOM}\n"))

    def test_zip_altere_rejete_et_efface(self):
        bon = fabriquer_zip()
        github = FauxGitHub(bon)
        github.contenu = bon[:-1] + bytes([bon[-1] ^ 1])  # même taille, empreinte annoncée celle du bon
        with self.assertRaisesRegex(maj.ErreurMiseAJour, "Empreinte SHA-256 incorrecte"):
            self.preparer(github)
        self.assertFalse((maj.DOSSIER / NOM).exists())

    def test_sans_empreinte_de_github_rien_n_est_installe(self):
        with self.assertRaisesRegex(maj.ErreurMiseAJour, "ne donne pas l'empreinte"):
            self.preparer(FauxGitHub(fabriquer_zip(), digest=""))

    def test_redirection_vers_un_hote_etranger_refusee(self):
        with self.assertRaisesRegex(maj.ErreurMiseAJour, "Adresse refusée : evil.example"):
            self.preparer(FauxGitHub(fabriquer_zip(), hote_zip="https://evil.example/"))

    def test_zip_piege_refuse(self):
        with self.assertRaisesRegex(maj.ErreurMiseAJour, "sort de son dossier"):
            self.preparer(FauxGitHub(fabriquer_zip(en_plus={f"Aria-Studio-{VERSION}/../../echappe.txt": "x"})))
        self.assertFalse((Path(self.tmp.name) / "echappe.txt").exists())

    def test_version_annoncee_differente_refusee(self):
        with self.assertRaisesRegex(maj.ErreurMiseAJour, "n'annonce pas la version"):
            self.preparer(FauxGitHub(fabriquer_zip(annonce="9.9.8")))

    def test_pas_plus_recente(self):
        with mock.patch.object(maj.mises_a_jour, "plus_recente", return_value=False), \
                self.assertRaisesRegex(maj.ErreurMiseAJour, "Pas de version plus récente"):
            self.preparer(FauxGitHub(fabriquer_zip()))

    def test_reprise_d_un_telechargement_coupe(self):
        contenu = fabriquer_zip(en_plus={f"Aria-Studio-{VERSION}/gros.bin": b"x" * 50000})
        maj.DOSSIER.mkdir(parents=True)
        (maj.DOSSIER / f"{NOM}.part").write_bytes(contenu[:20000])
        github = FauxGitHub(contenu)
        self.preparer(github)
        self.assertIn((f"{STOCKAGE}zip", "bytes=20000-"), github.vues)

    def test_reprise_refusee_par_le_serveur_repart_du_debut(self):
        maj.DOSSIER.mkdir(parents=True)
        (maj.DOSSIER / f"{NOM}.part").write_bytes(b"debris")
        self.preparer(FauxGitHub(fabriquer_zip(), reprise=False))
        self.assertTrue((maj.DOSSIER / f"Aria-Studio-{VERSION}" / "Aria-Studio.exe").is_file())


class Installer(Base):
    def test_relais_au_nouveau_lanceur_puis_arret(self):
        lancements, arrets = [], []
        dossier = maj.DOSSIER / f"Aria-Studio-{VERSION}"

        async def preparer():
            maj._changer(version=VERSION)
            return dossier

        with mock.patch.object(maj, "preparer", preparer), mock.patch.object(maj, "portable", return_value=False), \
                mock.patch.object(maj.subprocess, "Popen", side_effect=lambda a, **k: lancements.append((a, k))):
            maj.DOSSIER.mkdir(parents=True)
            asyncio.run(maj.installer(lambda: arrets.append(True)))
        arguments, options = lancements[0]
        self.assertEqual(arguments[0], str(dossier / "Aria-Studio.exe"))
        self.assertEqual(arguments[1], "--mise-a-jour")
        self.assertEqual(arguments[3], str(maj.ROOT / "Aria-Studio.exe"))
        self.assertEqual(options["cwd"], dossier)
        self.assertEqual(arrets, [True])
        self.assertEqual(maj.etat()["etape"], "redemarrage")
        self.assertEqual(json.loads(maj.RELAIS.read_text(encoding="utf-8"))["vers"], VERSION)

    def test_un_echec_est_montre_et_rien_n_est_lance(self):
        async def preparer():
            raise maj.ErreurMiseAJour("réseau coupé")

        with mock.patch.object(maj, "preparer", preparer), \
                mock.patch.object(maj.subprocess, "Popen", side_effect=AssertionError("rien ne doit être lancé")):
            asyncio.run(maj.installer(lambda: self.fail("rien ne doit s'arrêter")))
        e = maj.etat()
        self.assertEqual((e["etape"], e["erreur"]), ("erreur", "réseau coupé"))

    def test_mode_portable_decompresse_a_cote(self):
        dossier = maj.DOSSIER / f"Aria-Studio-{VERSION}"
        (dossier / "app").mkdir(parents=True)
        (dossier / "Aria-Studio.exe").write_bytes(b"MZ")

        async def preparer():
            maj._changer(version=VERSION)
            return dossier

        with mock.patch.object(maj, "preparer", preparer), mock.patch.object(maj, "portable", return_value=True), \
                mock.patch.object(maj.os, "startfile", create=True), \
                mock.patch.object(maj.subprocess, "Popen", side_effect=AssertionError("pas de relais en portable")):
            asyncio.run(maj.installer(lambda: self.fail("Aria ne s'arrête pas en portable")))
        cible = maj.ROOT.parent / f"Aria-Studio-{VERSION}"
        self.assertTrue((cible / "Aria-Studio.exe").is_file())
        self.assertEqual(maj.etat()["etape"], "portable")


class Bilan(Base):
    def test_version_attendue(self):
        maj.DOSSIER.mkdir(parents=True)
        maj.RELAIS.write_text(json.dumps({"de": "2.0.0", "vers": maj.VERSION}), encoding="utf-8")
        self.assertEqual(maj.bilan_au_demarrage(), "installee")
        self.assertFalse(maj.RELAIS.exists())

    def test_ancienne_version_toujours_la(self):
        maj.DOSSIER.mkdir(parents=True)
        maj.RELAIS.write_text(json.dumps({"de": maj.VERSION, "vers": "99.0.0"}), encoding="utf-8")
        self.assertEqual(maj.bilan_au_demarrage(), "echec")

    def test_rien_a_dire(self):
        self.assertIsNone(maj.bilan_au_demarrage())


class Hotes(unittest.TestCase):
    def test_seulement_github_en_https(self):
        for url in ("https://github.com/x", "https://api.github.com/x", "https://release-assets.githubusercontent.com/x"):
            self.assertTrue(maj.adresse_permise(url), url)
        with mock.patch.dict("os.environ", {"ARIA_ESSAI_GITHUB": ""}):
            for url in ("http://github.com/x", "https://github.com.evil.example/x", "https://evil.example/x",
                        "http://127.0.0.1:9000/x"):
                self.assertFalse(maj.adresse_permise(url), url)

    def test_le_faux_github_d_essai_seulement_pendant_les_tests(self):
        with mock.patch.dict("os.environ", {"ARIA_ESSAI_GITHUB": "http://127.0.0.1:9000"}):
            self.assertTrue(maj.adresse_permise("http://127.0.0.1:9000/zip"))
            with mock.patch.dict("os.environ", {"ARIA_TESTS_ISOLES": ""}):
                self.assertFalse(maj.adresse_permise("http://127.0.0.1:9000/zip"))
        with mock.patch.dict("os.environ", {"ARIA_ESSAI_GITHUB": "http://evil.example:9000"}):
            self.assertFalse(maj.adresse_permise("http://evil.example:9000/zip"))


if __name__ == "__main__":
    unittest.main()
