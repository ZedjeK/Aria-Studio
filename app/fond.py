"""Fond animé de l'interface (des gouttes en cercles qui allument une grille de points) : les réglages de l'utilisateur,
gardés dans data/fond.json.

L'interface dessine le fond elle-même ; le serveur ne garde que les quatre réglages ci-dessous. Ils portent les noms
du contrat des applis Aria (fond.actif, fond.vitesse, fond.minimum, fond.maximum), à un niveau de fichier près :
ils vivent dans un petit fichier à part, comme data/notifications.json, et non dans reglages.yaml, que le module
reglages contrôle clé par clé pour les LLM et les workflows.

    {"actif": true, "vitesse": "normale", "minimum": 2, "maximum": 5}
"""

import json

from app.emplacements import DATA

FICHIER = DATA / "fond.json"
VITESSES = ("lente", "normale", "rapide")
BORNE_BASSE, BORNE_HAUTE = 1, 12
DEFAUTS = {"actif": True, "vitesse": "normale", "minimum": 2, "maximum": 5}


def _lire() -> dict:
    try:
        d = json.loads(FICHIER.read_text(encoding="utf-8"))
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def _nombre(valeur, defaut: int) -> int:
    """Un entier entre 1 et 12 ; toute autre valeur (texte, booléen, décimal, absent) redonne le défaut."""
    if isinstance(valeur, bool) or not isinstance(valeur, int):
        return defaut
    return max(BORNE_BASSE, min(BORNE_HAUTE, valeur))


def _valide(d: dict) -> dict:
    """Les quatre réglages, ramenés dans leurs bornes ; un fichier modifié à la main ne peut rien casser."""
    minimum = _nombre(d.get("minimum"), DEFAUTS["minimum"])
    maximum = _nombre(d.get("maximum"), DEFAUTS["maximum"])
    actif = d.get("actif", DEFAUTS["actif"])
    vitesse = d.get("vitesse")
    return {
        "actif": actif if isinstance(actif, bool) else DEFAUTS["actif"],
        "vitesse": vitesse if vitesse in VITESSES else DEFAUTS["vitesse"],
        # Les bornes inversées dans le fichier sont remises dans l'ordre
        "minimum": min(minimum, maximum),
        "maximum": max(minimum, maximum),
    }


def etat() -> dict:
    return _valide(_lire())


def regler(actif: bool | None = None, vitesse: str | None = None,
           minimum: int | None = None, maximum: int | None = None) -> dict:
    """Change ce qui est donné (None : inchangé) et rend l'état complet."""
    d = etat()
    if actif is not None:
        d["actif"] = bool(actif)
    if vitesse is not None:
        if vitesse not in VITESSES:
            raise ValueError(f"vitesse : {', '.join(VITESSES)}")
        d["vitesse"] = vitesse
    if minimum is not None:
        d["minimum"] = _nombre(minimum, d["minimum"])
    if maximum is not None:
        d["maximum"] = _nombre(maximum, d["maximum"])
    d = _valide(d)
    FICHIER.parent.mkdir(parents=True, exist_ok=True)
    FICHIER.write_text(json.dumps(d, indent=4, ensure_ascii=False), encoding="utf-8")
    return d
