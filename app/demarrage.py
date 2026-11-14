"""Démarrer avec Windows : une valeur « Aria Studio » sous la clé Run du compte de l'utilisateur (HKCU), jamais de
service ni de tâche planifiée. Désactivé par défaut ; l'état se relit toujours dans le registre, jamais dans un
réglage d'Aria. Le lanceur (tools/launcher/Launcher.cs) lit et écrit la même valeur depuis le menu de l'icône.

Seulement pour Aria installée (%USERPROFILE%\\.Aria-Studio\\programme) : lancée depuis un zip ou en portable, son
chemin peut changer. La commande enregistrée lance le lanceur avec --silencieux : serveur et icône, sans navigateur."""

import os
from pathlib import Path

from app import emplacements, journal

NOM = "Aria Studio"
CLE_RUN = r"Software\Microsoft\Windows\CurrentVersion\Run"
OPTION = "--silencieux"


def cle() -> str:
    """La clé Run de l'utilisateur ; une clé d'essai seulement pour les tests isolés (tools/tester.py)."""
    essai = os.environ.get("ARIA_ESSAI_CLE_RUN")
    if essai and os.environ.get("ARIA_TESTS_ISOLES"):
        return essai
    return CLE_RUN


def exe() -> Path:
    return emplacements.programme() / "Aria-Studio.exe"


def commande() -> str:
    return f'"{exe()}" {OPTION}'


def possible() -> bool:
    return exe().is_file() and not emplacements.portable()


def _lire() -> str | None:
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, cle()) as k:
            valeur, _type = winreg.QueryValueEx(k, NOM)
            return str(valeur)
    except OSError:
        return None


def etat() -> dict:
    valeur = _lire()
    return {"possible": possible(), "actif": valeur is not None, "commande": commande(),
            "ailleurs": valeur is not None and valeur != commande(), "valeur": valeur}


def regler(actif: bool) -> dict:
    """Coche : écrit la commande (et remplace une valeur qui pointerait ailleurs). Décoche : retire la valeur. Le
    journal garde l'ancienne valeur."""
    import winreg
    avant = _lire()
    if actif:
        if not possible():
            raise ValueError("Démarrer avec Windows n'existe que pour Aria installée")
        with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, cle(), 0, winreg.KEY_SET_VALUE) as k:
            winreg.SetValueEx(k, NOM, 0, winreg.REG_SZ, commande())
        journal.noter("demarrage_windows", NOM, detail=f"activé{' (avant : ' + avant + ')' if avant else ''}")
    elif avant is not None:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, cle(), 0, winreg.KEY_SET_VALUE) as k:
            winreg.DeleteValue(k, NOM)
        journal.noter("demarrage_windows", NOM, detail=f"désactivé (avant : {avant})")
    return etat()
