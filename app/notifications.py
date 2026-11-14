"""Fin des jobs (Plateau, Montage, Casting) : préférences de l'utilisateur, gardées dans la section
« notifications » de reglages.json (app/fichier_reglages.py ; avant, data/notifications.json).

L'interface prévient elle-même : un bandeau dans l'appli, un son, et une notification Windows (API Notifications du
navigateur) quand la fenêtre d'Aria n'est pas au premier plan. Le serveur ne garde que les deux choix ci-dessous.
"""

from app import fichier_reglages

DEFAUTS = {"active": True, "son": True}


def _lire() -> dict:
    return fichier_reglages.section("notifications")


def etat() -> dict:
    d = _lire()
    return {cle: bool(d.get(cle, defaut)) for cle, defaut in DEFAUTS.items()}


def regler(active: bool | None = None, son: bool | None = None) -> dict:
    """Change ce qui est donné (None : inchangé) et rend l'état complet."""
    d = etat()
    if active is not None:
        d["active"] = bool(active)
    if son is not None:
        d["son"] = bool(son)
    fichier_reglages.ecrire("notifications", d)
    return d
