"""Fait le zip de la release à partir de la version construite par tools/build_portable.py --sans-installer.

    python tools/zip_release.py [--sortie DOSSIER]

Lit .build_cache/staging, le range dans un dossier « Aria-Studio-X.Y.Z » (comme les zips précédents : on décompresse
et on double-clique sur Aria-Studio.exe), écrit Aria-Studio-X.Y.Z.zip et Aria-Studio-X.Y.Z.zip.sha256
(« empreinte  nom », comme l'attend le README) dans le dossier de sortie (défaut : .build_cache/release), et affiche
les deux chemins. Appelé par .github/workflows/release.yml.
"""

import argparse
import hashlib
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from app import VERSION  # noqa: E402


def faire_zip(staging: Path, sortie: Path, version: str = VERSION) -> tuple:
    if not (staging / "Aria-Studio.exe").is_file():
        sys.exit(f"Rien à zipper : {staging / 'Aria-Studio.exe'} est introuvable (lance build_portable.py --sans-installer).")
    racine = f"Aria-Studio-{version}"
    sortie.mkdir(parents=True, exist_ok=True)
    zip_ = sortie / f"{racine}.zip"
    with zipfile.ZipFile(zip_, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for f in sorted(p for p in staging.rglob("*") if p.is_file()):
            z.write(f, f"{racine}/{f.relative_to(staging).as_posix()}")
    empreinte = hashlib.sha256(zip_.read_bytes()).hexdigest()
    somme = sortie / f"{zip_.name}.sha256"
    somme.write_text(f"{empreinte}  {zip_.name}\n", encoding="ascii", newline="\n")
    return zip_, somme


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--sortie", default=str(ROOT / ".build_cache" / "release"))
    args = parser.parse_args()
    for chemin in faire_zip(ROOT / ".build_cache" / "staging", Path(args.sortie)):
        print(chemin)


if __name__ == "__main__":
    main()
