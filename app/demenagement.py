"""Déménagement d'une installation portable vers le dossier de l'utilisateur (%USERPROFILE%\\.Aria-Studio), et retour.

    python -m app.demenagement            essai à blanc : ce qui partirait, et ce qui bloque
    python -m app.demenagement --go       déménage
    python -m app.demenagement --retour   ramène tout à côté du code (essai à blanc, --go pour le faire)

Chaque dossier est renommé d'un bloc, sans copie : il faut donc le même disque des deux côtés. Aria doit être arrêtée
(serveur, ComfyUI, llama-server), sinon Windows refuse de renommer des fichiers ouverts. data\\ part en dernier : tant
qu'il est à côté du code, Aria reste en mode portable. Si un renommage échoue, ceux déjà faits sont défaits.
"""

import socket
import sys
from pathlib import Path

from app.config import CONFIG, ROOT
from app.emplacements import DEFAUT

# (à côté du code, dans le dossier de l'utilisateur) ; data en dernier, c'est lui qui décide du mode portable
CORRESPONDANCES = (("aria.yaml", "aria.yaml"), ("services", "moteurs"), ("modeles", "modeles"), ("data", "data"))


class ErreurDemenagement(RuntimeError):
    pass


def _ports_occupes() -> list[str]:
    occupes = []
    for nom, port in (("Aria", CONFIG["app"]["port"]), ("ComfyUI", CONFIG["comfyui"]["port"]),
                      ("llama-server", CONFIG["llama"]["port"])):
        with socket.socket() as s:
            s.settimeout(0.3)
            if s.connect_ex(("127.0.0.1", port)) == 0:
                occupes.append(f"{nom} (port {port})")
    return occupes


def _volume(chemin: Path) -> str:
    return chemin.resolve().drive.upper()


def _vide(chemin: Path) -> bool:
    return chemin.is_dir() and not any(chemin.iterdir())


def plan(retour: bool = False, code: Path = ROOT, maison: Path = DEFAUT) -> dict:
    """Les renommages à faire et ce qui les empêche (rien n'est touché)."""
    mouvements, blocages = [], []
    for local, distant in CORRESPONDANCES:
        source, cible = (maison / distant, code / local) if retour else (code / local, maison / distant)
        if not source.exists():
            continue
        if cible.exists() and not _vide(cible):
            blocages.append(f"{cible} existe déjà et n'est pas vide")
        mouvements.append((source, cible))
    if not mouvements:
        blocages.append("Rien à déménager" + (f" dans {maison}" if retour else " à côté du code"))
    elif _volume(code) != _volume(maison):
        blocages.append(f"{code} et {maison} ne sont pas sur le même disque : il faudrait tout copier")
    occupes = _ports_occupes()
    if occupes:
        blocages.append("Arrête d'abord : " + ", ".join(occupes))
    return {"mouvements": mouvements, "blocages": blocages}


def demenager(retour: bool = False, code: Path = ROOT, maison: Path = DEFAUT) -> list[tuple[Path, Path]]:
    p = plan(retour, code, maison)
    if p["blocages"]:
        raise ErreurDemenagement(" ; ".join(p["blocages"]))
    faits = []
    try:
        for source, cible in p["mouvements"]:
            cible.parent.mkdir(parents=True, exist_ok=True)
            if _vide(cible):
                cible.rmdir()
            source.rename(cible)
            faits.append((source, cible))
    except OSError as e:
        fautif = source.name
        for avant, apres in reversed(faits):  # on remet tout comme avant
            apres.rename(avant)
        raise ErreurDemenagement(f"{fautif} n'a pas pu partir ({e.strerror or e}) : rien n'a bougé. "
                                 "Un fichier est peut-être ouvert (Explorateur, ComfyUI, antivirus…)") from e
    return faits


def _taille(chemin: Path) -> str:
    if chemin.is_file():
        octets = chemin.stat().st_size
    else:
        octets = sum(f.stat().st_size for f in chemin.rglob("*") if f.is_file())
    return f"{octets / 1024 ** 3:.1f} Go" if octets >= 1024 ** 3 else f"{octets / 1024 ** 2:.0f} Mo"


def main(args: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8")  # flèches et croix dans la console Windows
    retour, go = "--retour" in args, "--go" in args
    p = plan(retour)
    print(("Retour à côté du code" if retour else f"Déménagement vers {DEFAUT}") + (" :" if go else " (essai à blanc) :"))
    for source, cible in p["mouvements"]:
        print(f"  {source}  →  {cible}  ({_taille(source)})")
    for b in p["blocages"]:
        print(f"  ✗ {b}")
    if p["blocages"]:
        return 1
    if not go:
        print("Rien n'a bougé. Ajoute --go pour le faire.")
        return 0
    try:
        demenager(retour)
    except ErreurDemenagement as e:
        print(f"  ✗ {e}")
        return 1
    print("C'est fait. Relance Aria : " + ("mode portable." if retour else f"tout est dans {DEFAUT}."))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
