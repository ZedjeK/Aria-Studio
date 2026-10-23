"""Personnages enregistrés de l'onglet Prompt (data/personnages.json).

On garde les choix (valeurs françaises du bloc « personnage » et l'âge précis), pas le texte anglais :
un personnage se rappelle dans n'importe quelle scène et suit les listes si elles évoluent.
"""

import json
import re

from app import listes
from app.config import ROOT

FICHIER = ROOT / "data" / "personnages.json"
_NOM = re.compile(r"[\w][\w .'’-]{0,39}")

# Origines d'avant la simplification à 17 (2026-10-20) : un personnage enregistré est converti à la lecture.
ANCIENNES_ORIGINES = {
    "Française aux racines bretonnes": "Européenne de l'Ouest", "Française": "Européenne de l'Ouest",
    "Britannique": "Européenne de l'Ouest", "Irlandaise": "Européenne de l'Ouest",
    "Allemande": "Européenne de l'Ouest", "Nord-américaine": "Européenne de l'Ouest",
    "Scandinave": "Nordique",
    "Méditerranéenne (italienne, espagnole, grecque)": "Méditerranéenne",
    "Slave (russe, polonaise, ukrainienne)": "Slave",
    "Mongole": "Chinoise",
    "Vietnamienne": "Asiatique du Sud-Est", "Thaïlandaise": "Asiatique du Sud-Est",
    "Philippine": "Asiatique du Sud-Est", "Indonésienne": "Asiatique du Sud-Est",
    "Eurasienne franco-japonaise": "Eurasienne", "Eurasienne franco-coréenne": "Eurasienne",
    "Eurasienne franco-vietnamienne": "Eurasienne", "Eurasienne franco-chinoise": "Eurasienne",
    "Indienne": "Sud-asiatique",
    "Maghrébine": "Moyen-orientale", "Libanaise": "Moyen-orientale", "Persane": "Moyen-orientale",
    "Turque": "Moyen-orientale",
    "Ouest-africaine": "Africaine", "Est-africaine": "Africaine",
    "Afro-américaine": "Afro-descendante", "Antillaise": "Afro-descendante",
    "Brésilienne": "Latino-américaine", "Mexicaine": "Latino-américaine",
}
# Silhouettes d'avant la simplification à 10 (2026-10-20).
ANCIENNES_SILHOUETTES = {
    "Longiligne": "Fine et élancée",
    "Sportive et tonique": "Athlétique", "Musclée de danseuse": "Athlétique", "Sec et nerveux": "Athlétique",
    "Musclé": "Musclé(e)", "Carrure large": "Musclé(e)",
    "Voluptueuse": "Pulpeuse",
    "Trapu": "Trapu(e)", "Robuste": "Trapu(e)",
    "Généreuse": "Ronde",
}
# Poses et actions d'avant le catalogue par catégories (2026-10-20) ; None = valeur retirée.
ANCIENNES_POSES = {
    "Assise": "Assis(e)", "Assise en tailleur": "Assis(e) en tailleur", "Accroupie": "Accroupi(e)",
    "Agenouillée": "Agenouillé(e)", "Allongée dans l'herbe": "Allongé(e) dans l'herbe",
    "Appuyée contre un mur": "Appuyé(e) contre un mur", "Recroquevillée": "Recroquevillé(e)",
    "Allongée sur un lit": "Allongé(e) sur un lit",
}
ANCIENNES_ACTIONS = {"Écoute de la musique": "Écoute de la musique au casque", "Aucune": None}
ANCIENNES_VALEURS = {"Origine": ANCIENNES_ORIGINES, "Silhouette": ANCIENNES_SILHOUETTES,
                     "Pose": ANCIENNES_POSES, "Action": ANCIENNES_ACTIONS}


def _lire() -> dict:
    if not FICHIER.is_file():
        return {}
    personnages = json.loads(FICHIER.read_text(encoding="utf-8"))
    converti = False
    for p in personnages.values():
        choix = p.get("choix", {})
        for sous_liste, correspondances in ANCIENNES_VALEURS.items():
            if (ancienne := choix.get(sous_liste)) in correspondances:
                if correspondances[ancienne] is None:
                    del choix[sous_liste]
                else:
                    choix[sous_liste] = correspondances[ancienne]
                converti = True
    if converti:
        _ecrire(personnages)
    return personnages


def _ecrire(personnages: dict):
    FICHIER.parent.mkdir(parents=True, exist_ok=True)
    temporaire = FICHIER.with_suffix(".tmp")
    temporaire.write_text(json.dumps(personnages, ensure_ascii=False, indent=2), encoding="utf-8")
    temporaire.replace(FICHIER)


def liste() -> list[dict]:
    return [{"nom": nom, **p} for nom, p in sorted(_lire().items(), key=lambda x: x[0].lower())]


def enregistrer(nom: str, choix: dict, age: int | None) -> dict:
    nom = nom.strip()
    if not _NOM.fullmatch(nom):
        raise listes.ErreurListes("Nom de personnage invalide (lettres, chiffres, espaces, 40 caractères au plus)")
    choix = {sl: v for sl, v in (choix or {}).items() if v}
    if not choix and age is None:
        raise listes.ErreurListes("Rien à enregistrer : choisis au moins une valeur du personnage")
    # Mêmes règles qu'à l'assemblage (genre, incompatibilités, valeurs adultes et mineurs).
    listes.assembler("personnage", {"personnage": choix}, age)
    personnages = _lire()
    personnages[nom] = {"choix": choix, "age": age}
    _ecrire(personnages)
    return {"nom": nom, **personnages[nom]}


def supprimer(nom: str):
    personnages = _lire()
    if nom not in personnages:
        raise KeyError(nom)
    del personnages[nom]
    _ecrire(personnages)
