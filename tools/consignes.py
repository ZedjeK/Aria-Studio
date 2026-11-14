"""Écrit les consignes envoyées au LLM, telles qu'elles partent pour chaque modèle cible et chaque usage.

Rien n'est lancé (ni LLM, ni ComfyUI) : le texte est seulement assemblé, dans un dossier utilisateur jetable comme
tools/tester.py. Ce sont donc les consignes d'origine et les fiches livrées, sans tes réglages ni tes consignes
réécrites. Sert à relire une consigne en entier, et à comparer deux versions du code (avant, après).

    python tools/consignes.py                      tout, à l'écran
    python tools/consignes.py krea2_turbo          une seule fiche
    python tools/consignes.py --dossier sortie     un fichier .txt par fiche et par usage
"""

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path[:0] = [str(ROOT), str(ROOT / "tools")]

from tester import MARQUE, poser_fichiers_des_workflows  # noqa: E402  (tools/tester.py : le même dossier jetable)

# Les usages du LLM selon le style de prompt du modèle cible (profil_prompt.style de sa fiche).
USAGES_PAR_STYLE = {"prose": ("reecriture", "references"), "instruction": ("retouche", "references"),
                    "brief_video": ("video", "decoupage")}
DUREE_CLIP = 5  # secondes : la longueur d'un prompt vidéo en dépend (prompt.mots_pour_duree)


def usages(wf) -> list[str]:
    """Les consignes que ce modèle cible reçoit ; « references » seulement s'il prend des images de référence."""
    return [u for u in USAGES_PAR_STYLE.get(wf.profil.get("style"), ())
            if u != "references" or wf.accepte_references()]


def assembler(wf, usage: str) -> str:
    """La consigne d'origine de cet usage pour ce modèle cible, comme app/prompt.py et app/decoupage.py l'assemblent."""
    from app import prompt, workflows
    regles = workflows.sections_prompt(wf.id)
    champs = workflows.champs_video(wf.id)
    mots = prompt.mots_max_de(wf.id)
    if usage == "reecriture":
        return prompt.consigne(usage, None, mots, regles)
    if usage == "video":
        return prompt.consigne(usage, None, prompt.mots_pour_duree(mots, DUREE_CLIP), regles, champs=champs)
    if usage == "decoupage":
        texte = prompt.consigne(usage, None, mots or 110, regles, champs=champs)
        return texte.replace("{regle_personnage}", prompt.REGLE_SANS_REFERENCE)
    return prompt.consigne(usage, None, regles=regles, workflow=wf.id)  # retouche, references


def relever(ids: list[str] | None = None) -> dict[tuple[str, str], str]:
    """{(fiche, usage): consigne}, pour toutes les fiches du catalogue ou seulement celles de `ids`."""
    from app import workflows
    return {(wf.id, usage): assembler(wf, usage)
            for wf in sorted(workflows.catalogue(), key=lambda w: w.id) if not ids or wf.id in ids
            for usage in usages(wf)}


def _dans_un_dossier_jetable() -> int:
    with tempfile.TemporaryDirectory(prefix="aria-consignes-") as maison:
        poser_fichiers_des_workflows(Path(maison) / ".Aria-Studio" / "modeles")
        env = {**os.environ, "USERPROFILE": maison, "HOME": maison, MARQUE: maison}
        return subprocess.run([sys.executable, str(Path(__file__).resolve()), *sys.argv[1:]], cwd=ROOT, env=env).returncode


def main() -> int:
    analyse = argparse.ArgumentParser(description="Consignes envoyées au LLM, par modèle cible et par usage.")
    analyse.add_argument("fiches", nargs="*", help="identifiants de fiches (toutes sans argument)")
    analyse.add_argument("--dossier", type=Path, help="écrire un fichier .txt par fiche et par usage dans ce dossier")
    choix = analyse.parse_args()
    if not os.environ.get(MARQUE):  # jamais dans le vrai ~/.Aria-Studio : on se relance dans un dossier jetable
        return _dans_un_dossier_jetable()
    releve = relever(choix.fiches)
    if not releve:
        print("Aucune fiche à ce nom.", file=sys.stderr)
        return 1
    if choix.dossier:
        choix.dossier.mkdir(parents=True, exist_ok=True)
        for (fiche, usage), texte in releve.items():
            (choix.dossier / f"{fiche}.{usage}.txt").write_text(texte + "\n", encoding="utf-8")
        print(f"{len(releve)} consignes écrites dans {choix.dossier}")
        return 0
    sys.stdout.reconfigure(encoding="utf-8")
    for (fiche, usage), texte in releve.items():
        print(f"===== {fiche} · {usage} ({len(texte.split())} mots) =====\n{texte}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
