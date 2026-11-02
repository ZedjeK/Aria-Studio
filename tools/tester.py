"""Lance les tests dans un dossier utilisateur jetable : ils ne lisent ni n'écrivent jamais le vrai ~/.Aria-Studio.

Les fichiers des workflows y sont posés vides (un workflow est « prêt » quand ses fichiers sont là) et modeles/llm
reste vide : le résultat ne dépend plus de ce qui est installé sur la machine. Avant, un GGUF supprimé (Ministral)
ou ajouté (Bonsai) suffisait à faire tomber des tests, et un clone neuf en ratait 15.

    python tools/tester.py                         tous les tests
    python tools/tester.py tests.test_vision -v    arguments de unittest
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
MARQUE = "ARIA_TESTS_ISOLES"  # tests/test_isolation.py refuse de passer sans elle


def poser_fichiers_des_workflows(modeles: Path):
    for fiche in sorted((ROOT / "workflows").glob("*/fiche.yaml")):
        for f in (yaml.safe_load(fiche.read_text(encoding="utf-8")) or {}).get("fichiers", []):
            chemin = modeles / f["dossier"] / f["nom"]
            chemin.parent.mkdir(parents=True, exist_ok=True)
            chemin.touch()


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="aria-tests-") as maison:
        poser_fichiers_des_workflows(Path(maison) / ".Aria-Studio" / "modeles")
        # Path.home() suit USERPROFILE sous Windows (HOME ailleurs) : app/emplacements.py s'installe donc ici.
        env = {**os.environ, "USERPROFILE": maison, "HOME": maison, MARQUE: maison}
        arguments = sys.argv[1:] or ["discover", "-s", "tests"]
        return subprocess.run([sys.executable, "-m", "unittest", *arguments], cwd=ROOT, env=env).returncode


if __name__ == "__main__":
    sys.exit(main())
