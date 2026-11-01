"""Construit la version portable d'Aria - Studio à côté du code source : ..\\Aria-Studio-X.Y.Z\\

    .venv\\Scripts\\python.exe tools\\build_portable.py [--releases-dir DOSSIER]

Le dossier ne contient que le programme ; les affaires de l'utilisateur vivent dans %USERPROFILE%\\.Aria-Studio
(app/emplacements.py), qu'une mise à jour ne touche jamais :
- python\\           CPython 3.11 portable (python-build-standalone), vérifié par SHA-256, avec les dépendances
                     exactes du .venv (celles qu'on a testées), sans rien télécharger d'autre ;
- app\\, web\\, …     le code suivi par Git (ni tests, ni outils, ni fichiers non suivis) ;
- Aria-Studio.exe    le lanceur (tools/launcher/Launcher.cs, compilé avec le csc.exe fourni par Windows).
Pas de data\\ dedans : sa présence ferait passer Aria en mode portable.

Mise en place : la version est construite dans .build_cache\\staging, puis remplace la même version ou la précédente.
Si l'utilisateur avait choisi le mode portable (data\\, modeles\\, services\\ ou aria.yaml à côté de l'exe), ses
affaires passent dans la nouvelle version avant que l'ancienne soit supprimée.
"""

import argparse
import hashlib
import importlib.metadata
import os
import re
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
SANS_LIVRER = ("tests/", "tools/", ".gitignore", ".gitattributes")  # suivis par Git, inutiles à l'utilisateur
AFFAIRES_PORTABLES = ("data", "modeles", "services", "aria.yaml")  # mode portable : jamais perdues
VERSION_RE = re.compile(r"^Aria-Studio-(\d+)\.(\d+)\.(\d+)$")

sys.path.insert(0, str(ROOT))
from app import VERSION  # noqa: E402


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


def icone(cible):
    """Icône d'Aria : un « A » rose néon sur fond nuit, aux tailles de Windows."""
    from PIL import Image, ImageDraw, ImageFont

    taille = 256
    img = Image.new("RGBA", (taille, taille), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((8, 8, taille - 8, taille - 8), radius=48, fill=(13, 13, 20, 255), outline=(255, 46, 136, 255),
                        width=10)
    try:
        police = ImageFont.truetype("segoeuib.ttf", 170)
    except OSError:
        police = ImageFont.load_default(170)
    d.text((taille / 2, taille / 2 + 6), "A", font=police, fill=(255, 46, 136, 255), anchor="mm")
    img.save(cible, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])


def compiler_lanceur(dest):
    etape(f"Lanceur {EXE}")
    if not CSC.exists():
        sys.exit(f"Compilateur C# introuvable : {CSC}")
    ico = CACHE / "aria.ico"
    icone(ico)
    subprocess.run([str(CSC), "/nologo", "/target:winexe", "/optimize+", f"/out:{dest / EXE}", f"/win32icon:{ico}",
                    "/codepage:65001", "/reference:System.Windows.Forms.dll", str(ROOT / "tools" / "launcher" / "Launcher.cs")],
                   check=True)


# --- mise en place -----------------------------------------------------------------

def versions(dossier):
    trouvees = [(tuple(int(x) for x in m.groups()), p) for p in dossier.iterdir()
                if p.is_dir() and (m := VERSION_RE.match(p.name))]
    return [p for _, p in sorted(trouvees)]


def mettre_en_place(staging, dossier):
    etape("Mise en place")
    cible = dossier / f"Aria-Studio-{VERSION}"
    anciennes = [p for p in versions(dossier) if p != cible]
    source = cible if cible.exists() else (anciennes[-1] if anciennes else None)
    if source:
        essai = source.with_name(source.name + ".verif")  # un dossier dont un fichier est ouvert ne se renomme pas
        try:
            source.rename(essai)
            essai.rename(source)
        except OSError:
            sys.exit(f"Arrêt : {source.name} est en cours d'utilisation. Quitte Aria - Studio puis relance.")
        if not (source / EXE).exists():
            sys.exit(f"Arrêt : {source} ne ressemble pas à une version d'Aria - Studio, il n'est pas touché.")
        for nom in AFFAIRES_PORTABLES:
            if (source / nom).exists():
                shutil.move(str(source / nom), str(staging / nom))
                print(f"  mode portable : {nom} repris depuis {source.name}")
        restes = [n for n in AFFAIRES_PORTABLES if (source / n).exists()]
        if restes:
            sys.exit(f"Arrêt : {', '.join(restes)} encore dans {source}, rien n'a été supprimé.")
        shutil.rmtree(source)
        print(f"  {'ancien contenu remplacé' if source == cible else 'ancienne version supprimée'} : {source.name}")
    shutil.move(str(staging), str(cible))
    return cible


def taille(dossier):
    return sum(f.stat().st_size for f in dossier.rglob("*") if f.is_file())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--releases-dir", default=str(ROOT.parent), help="où placer Aria-Studio-X.Y.Z (défaut : à côté du code)")
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
    version = mettre_en_place(staging, Path(args.releases_dir).resolve())

    etape("Terminé")
    print(f"  {version} ({taille(version) / 1024 ** 2:.0f} Mo)")
    print(f"  Lancement : double-clic sur « {EXE} » ; tes affaires restent dans %USERPROFILE%\\.Aria-Studio")


if __name__ == "__main__":
    main()
