"""Construit la version portable d'Aria - Studio et l'installe dans %USERPROFILE%\\.Aria-Studio

    .venv\\Scripts\\python.exe tools\\build_portable.py [--dans DOSSIER]

Le programme se pose à côté des affaires de l'utilisateur (app/emplacements.py), comme Aria-Pilote dans son
.Aria-Pilote ; une mise à jour ne touche jamais à data\\, modeles\\, moteurs\\, services\\ ni aria.yaml :
- python\\           CPython 3.11 portable (python-build-standalone), vérifié par SHA-256, avec les dépendances
                     exactes du .venv (celles qu'on a testées), sans rien télécharger d'autre ;
- app\\, web\\, …     le code suivi par Git (ni tests, ni outils, ni fichiers non suivis) ;
- Aria-Studio.exe    le lanceur (tools/launcher/Launcher.cs, compilé avec le csc.exe fourni par Windows).

Mise en place : la version est construite dans .build_cache\\staging. Le fichier .programme du dossier liste ce
que la version précédente y avait posé : seules ces entrées sont remplacées. Une entrée inconnue qui porte le
nom d'un fichier du programme arrête tout (rien n'est supprimé qu'Aria n'ait elle-même installé).
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
AFFAIRES = ("data", "modeles", "moteurs", "services", "aria.yaml")  # à l'utilisateur : jamais touchées
MANIFESTE = ".programme"

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


def compiler_lanceur(dest):
    etape(f"Lanceur {EXE}")
    if not CSC.exists():
        sys.exit(f"Compilateur C# introuvable : {CSC}")
    ico = ICONE
    subprocess.run([str(CSC), "/nologo", "/target:winexe", "/optimize+", f"/out:{dest / EXE}", f"/win32icon:{ico}",
                    "/codepage:65001", "/reference:System.Windows.Forms.dll", str(ROOT / "tools" / "launcher" / "Launcher.cs")],
                   check=True)


# --- mise en place -----------------------------------------------------------------

def mettre_en_place(staging, dossier):
    """Remplace le programme posé dans `dossier` par celui de `staging`, sans toucher aux affaires de l'utilisateur."""
    etape(f"Mise en place dans {dossier}")
    nouvelles = sorted(p.name for p in staging.iterdir())
    if conflit := [n for n in nouvelles if n in AFFAIRES or n == MANIFESTE]:
        sys.exit(f"Arrêt : le programme contient {', '.join(conflit)}, réservé à l'utilisateur.")
    manifeste = dossier / MANIFESTE
    try:
        anciennes = json.loads(manifeste.read_text(encoding="utf-8"))["entrees"]
    except FileNotFoundError:
        anciennes = []
    if inconnues := [n for n in nouvelles if (dossier / n).exists() and n not in anciennes]:
        sys.exit(f"Arrêt : {', '.join(inconnues)} existe déjà dans {dossier} sans venir d'Aria, rien n'est touché.")
    python = dossier / "python"
    if python.exists():
        essai = python.with_name("python.verif")  # un dossier dont un fichier est ouvert ne se renomme pas
        try:
            python.rename(essai)
            essai.rename(python)
        except OSError:
            sys.exit("Arrêt : Aria - Studio tourne encore. Quitte-la puis relance.")
    dossier.mkdir(parents=True, exist_ok=True)
    for nom in anciennes:
        chemin = dossier / nom
        if nom in AFFAIRES or not chemin.exists():
            continue
        shutil.rmtree(chemin) if chemin.is_dir() else chemin.unlink()
    for nom in nouvelles:
        shutil.move(str(staging / nom), str(dossier / nom))
    manifeste.write_text(json.dumps({"version": VERSION, "entrees": nouvelles}, indent=4), encoding="utf-8")
    staging.rmdir()
    print(f"  {len(nouvelles)} entrées posées, {len(anciennes)} remplacées ; data, modeles, moteurs, aria.yaml intacts")
    return dossier


def taille(dossier):
    return sum(f.stat().st_size for f in dossier.rglob("*") if f.is_file())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dans", default=str(DEFAUT), help="où installer le programme (défaut : %%USERPROFILE%%\\.Aria-Studio)")
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
