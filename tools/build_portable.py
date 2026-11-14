"""Construit la version portable d'Aria - Studio et l'installe dans %USERPROFILE%\\.Aria-Studio\\programme

    .venv\\Scripts\\python.exe tools\\build_portable.py [--dans DOSSIER]

Le programme se pose dans le sous-dossier programme\\ du dossier d'Aria, là où le lanceur l'installe lui-même
depuis un zip (tools/launcher/Launcher.cs) ; une mise à jour ne touche jamais à data\\, modeles\\, moteurs\\,
services\\ ni aria.yaml :
- python\\           CPython 3.11 portable (python-build-standalone), vérifié par SHA-256, avec les dépendances
                     exactes du .venv (celles qu'on a testées), sans rien télécharger d'autre ;
- app\\, web\\, …     le code suivi par Git (ni tests, ni outils, ni fichiers non suivis) ;
- Aria-Studio.exe    le lanceur (tools/launcher/Launcher.cs, compilé avec le csc.exe fourni par Windows).

Mise en place : la version est construite dans .build_cache\\staging, puis devient programme\\ ; la version d'avant
devient programme.precedent\\ (celle qui l'était déjà est remplacée). Ancienne disposition : les versions d'avant
posaient le programme à la racine du dossier d'Aria, avec un fichier .programme qui listait ses entrées. Ces
entrées, et elles seules, sont rangées dans programme.precedent\\ avant la mise en place (rien n'est supprimé).
Le zip de la release se fait à partir de .build_cache\\staging (--sans-installer).
"""

import argparse
import hashlib
import importlib.metadata
import json
import os
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / ".build_cache"

PYTHON_RELEASE = "20261013"
PYTHON_ASSET = f"cpython-3.11.17+{PYTHON_RELEASE}-x86_64-pc-windows-msvc-install_only.tar.gz"
PYTHON_URL = f"https://github.com/astral-sh/python-build-standalone/releases/download/{PYTHON_RELEASE}"
CSC = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Microsoft.NET" / "Framework64" / "v4.0.30319" / "csc.exe"

EXE = "Aria-Studio.exe"
SANS_LIVRER = ("tests/", "tools/", "build.ps1", ".gitignore", ".gitattributes")  # suivis par Git, inutiles à l'utilisateur
AFFAIRES = ("data", "modeles", "moteurs", "services", "aria.yaml", "reglages.json", "sauvegardes", "cache",
            "journal.json")  # à l'utilisateur : jamais touchées
MANIFESTE = ".programme"  # liste des entrées posées à la racine par l'ancienne disposition
PROGRAMME, PRECEDENT = "programme", "programme.precedent"

sys.path.insert(0, str(ROOT))
from app import VERSION  # noqa: E402
from app.emplacements import DEFAUT  # noqa: E402


def etape(texte):
    print(f"\n=== {texte}")


def recuperer(url, cible):
    if cible.exists():
        print(f"  en cache : {cible.name}")
        return cible
    cible.parent.mkdir(parents=True, exist_ok=True)
    tmp = cible.with_suffix(cible.suffix + ".part")
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "aria-studio"}), timeout=120) as r, \
            open(tmp, "wb") as f:
        shutil.copyfileobj(r, f)
    tmp.replace(cible)
    print(f"  téléchargé : {cible.name} ({cible.stat().st_size / 1024 ** 2:.1f} Mo)")
    return cible


# --- construction ------------------------------------------------------------------

def installer_python(dest):
    etape("Python portable")
    archive = recuperer(f"{PYTHON_URL}/{PYTHON_ASSET}", CACHE / PYTHON_ASSET)
    sommes = recuperer(f"{PYTHON_URL}/SHA256SUMS", CACHE / f"SHA256SUMS-{PYTHON_RELEASE}")
    attendu = next((l.split()[0] for l in sommes.read_text().splitlines() if l.endswith(PYTHON_ASSET)), None)
    obtenu = hashlib.sha256(archive.read_bytes()).hexdigest()
    if obtenu != attendu:
        archive.unlink()
        sys.exit(f"SHA-256 du Python portable invalide ({obtenu} au lieu de {attendu}) : archive supprimée.")
    print("  SHA-256 vérifié")
    with tarfile.open(archive) as tar:
        tar.extractall(dest, filter="data")  # l'archive contient un dossier « python/ »
    return dest / "python"


def _dependances() -> list:
    """Les distributions de requirements.txt et toutes leurs dépendances, telles qu'installées dans le .venv."""
    from pip._vendor.packaging.requirements import Requirement  # pip est dans le .venv

    a_voir = [Requirement(l).name for l in (ROOT / "requirements.txt").read_text(encoding="utf-8").splitlines()
              if l.strip() and not l.startswith("#")]
    vues = {}
    while a_voir:
        nom = a_voir.pop().lower().replace("_", "-")
        if nom in vues:
            continue
        vues[nom] = importlib.metadata.distribution(nom)
        for texte in vues[nom].requires or []:
            req = Requirement(texte)
            if req.marker is None or req.marker.evaluate({"extra": ""}):
                a_voir.append(req.name)
    return list(vues.values())


def installer_dependances(python):
    etape("Dépendances (celles du .venv)")
    if sys.version_info[:2] != (3, 11):
        sys.exit(f"Lance ce script avec le .venv d'Aria (Python 3.11), pas Python {sys.version.split()[0]}.")
    site = python / "Lib" / "site-packages"
    for dist in _dependances():
        copies = 0
        for f in dist.files or []:
            source = Path(dist.locate_file(f))
            relatif = Path(f)
            if relatif.parts[0] == ".." or "__pycache__" in relatif.parts or not source.is_file():
                continue  # Scripts\*.exe (chemins absolus du .venv) et caches
            (site / relatif).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, site / relatif)
            copies += 1
        print(f"  {dist.metadata['Name']} {dist.version} ({copies} fichiers)")


def copier_code(dest):
    etape("Code (fichiers suivis par Git)")
    suivis = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout.decode()
    modifies = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=ROOT,
                              capture_output=True, text=True, check=True).stdout.strip()
    if modifies:
        print("  ⚠ modifications non commitées incluses :\n    " + modifies.replace("\n", "\n    "))
    n = 0
    for nom in filter(None, suivis.split("\0")):
        if nom.startswith(SANS_LIVRER):
            continue
        (dest / nom).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / nom, dest / nom)
        n += 1
    print(f"  {n} fichiers")


# Icône du lanceur : le logo de la famille Aria (web/logo.svg, clap orange), rendu avec Chromium aux tailles de
# Windows. Le .ico est versionné : la construction n'a rien à dessiner.
ICONE = ROOT / "tools" / "launcher" / "aria-studio.ico"
# Le lanceur et son icône de notification (tools/launcher/Icone.cs), compilés ensemble.
SOURCES_LANCEUR = (ROOT / "tools" / "launcher" / "Launcher.cs", ROOT / "tools" / "launcher" / "Icone.cs")


def compiler_lanceur(dest):
    etape(f"Lanceur {EXE}")
    if not CSC.exists():
        sys.exit(f"Compilateur C# introuvable : {CSC}")
    ico = ICONE
    subprocess.run([str(CSC), "/nologo", "/target:winexe", "/optimize+", f"/out:{dest / EXE}", f"/win32icon:{ico}",
                    "/codepage:65001", "/reference:System.Windows.Forms.dll", "/reference:System.Drawing.dll",
                    "/reference:System.Web.Extensions.dll", f"/resource:{ico},aria-studio.ico",  # le logo de l'icône
                    *(str(f) for f in SOURCES_LANCEUR)],
                   check=True)


# --- mise en place -----------------------------------------------------------------

class ArretMiseEnPlace(SystemExit):
    """Rien n'a été touché : le message dit pourquoi."""


def _libre(dossier):
    """Un dossier dont un fichier est ouvert (Aria lancée depuis là) ne se renomme pas."""
    essai = dossier.with_name(dossier.name + ".verif")
    try:
        dossier.rename(essai)
        essai.rename(dossier)
    except OSError:
        raise ArretMiseEnPlace(f"Arrêt : Aria - Studio tourne encore depuis {dossier}. Quitte-la puis relance.")


def ranger_ancienne_disposition(maison):
    """Programme posé à la racine du dossier d'Aria par les versions d'avant : ses entrées (et seulement elles, d'après
    .programme) partent dans programme.precedent\\. Rend la liste des entrées déplacées."""
    manifeste = maison / MANIFESTE
    if not manifeste.is_file():
        return []
    entrees = [n for n in json.loads(manifeste.read_text(encoding="utf-8")).get("entrees", [])
               if n not in AFFAIRES and n not in (PROGRAMME, PRECEDENT, MANIFESTE) and "/" not in n and "\\" not in n]
    precedent = maison / PRECEDENT
    if (maison / PROGRAMME).exists() or precedent.exists():
        raise ArretMiseEnPlace(f"Arrêt : {maison} contient à la fois l'ancienne disposition ({MANIFESTE}) et "
                               f"{PROGRAMME}\\ ou {PRECEDENT}\\ ; rien n'est touché.")
    if (maison / "python").exists():
        _libre(maison / "python")
    precedent.mkdir()
    deplaces = []
    for nom in entrees:
        if (maison / nom).exists():
            shutil.move(str(maison / nom), str(precedent / nom))
            deplaces.append(nom)
    shutil.move(str(manifeste), str(precedent / MANIFESTE))
    print(f"  Ancienne disposition rangée dans {precedent} : {', '.join(deplaces) or 'aucune entrée'}")
    return deplaces


def mettre_en_place(staging, maison):
    """`staging` devient maison\\programme ; la version d'avant devient programme.precedent\\. Les affaires de
    l'utilisateur (data, modeles, moteurs, services, aria.yaml) ne sont jamais touchées."""
    programme, precedent = maison / PROGRAMME, maison / PRECEDENT
    etape(f"Mise en place dans {programme}")
    if programme.is_dir() and any(programme.iterdir()) and not (programme / EXE).is_file():
        raise ArretMiseEnPlace(f"Arrêt : {programme} existe et ne contient pas {EXE} ; rien n'est touché.")
    maison.mkdir(parents=True, exist_ok=True)
    ranger_ancienne_disposition(maison)
    if programme.exists():
        _libre(programme)
        if precedent.exists():
            shutil.rmtree(precedent)
        programme.rename(precedent)
    shutil.move(str(staging), str(programme))
    print(f"  Programme posé dans {programme}" + (f", version d'avant dans {precedent}" if precedent.exists() else "")
          + " ; data, modeles, moteurs, aria.yaml intacts")
    return programme


def taille(dossier):
    return sum(f.stat().st_size for f in dossier.rglob("*") if f.is_file())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dans", default=str(DEFAUT),
                        help="dossier d'Aria, le programme va dans son programme\\ (défaut : %%USERPROFILE%%\\.Aria-Studio)")
    parser.add_argument("--sans-installer", action="store_true", help="construire seulement (.build_cache\\staging, pour le zip)")
    args = parser.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")

    staging = CACHE / "staging"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    print(f"Construction d'Aria - Studio v{VERSION}")

    python = installer_python(staging)
    installer_dependances(python)
    copier_code(staging)
    compiler_lanceur(staging)
    etape("Terminé")
    if args.sans_installer:
        print(f"  {staging} ({taille(staging) / 1024 ** 2:.0f} Mo)")
        return
    dossier = mettre_en_place(staging, Path(args.dans).resolve())
    print(f"  Lancement : double-clic sur « {dossier / EXE} »")


if __name__ == "__main__":
    main()
