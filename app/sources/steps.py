"""Suggestion du nombre de steps d'un checkpoint, lue dans le texte de sa page source.

Aucune source n'a de champ « steps » (vérifié sur Civitai : jamais dans les métadonnées des images d'exemple ; Hugging
Face : texte libre). On cherche donc un nombre à côté du mot « step » dans le nom du fichier, de la version et la
description. C'est une SUGGESTION : elle n'est jamais appliquée sans validation, et elle se déclare peu fiable dès
qu'elle n'est pas univoque."""

import re

STEPS_MIN, STEPS_MAX = 1, 150  # bornes du réglage « Steps imposés »
_N_STEP = re.compile(r"(?<![\d.])(\d{1,3})\s*[-_ ]?steps?", re.I)  # « 8-step », « 8 steps », « 8stepTURBO »
_STEPS_N = re.compile(r"\bsteps?\s*[:=]\s*(\d{1,3})(?!\d)", re.I)  # « steps: 8 » (« Step 1 » d'un tutoriel ne passe pas)
_BALISES = re.compile(r"<[^>]+>")
RAYON_EXTRAIT = 45


def _nettoyer(texte: str) -> str:
    return re.sub(r"\s+", " ", _BALISES.sub(" ", texte or "")).strip()


def suggerer(textes: dict[str, str]) -> dict:
    """textes : {origine: texte}. Retourne {statut: unique|ambigu|aucun, valeurs: [int], extraits: [{origine, texte}]}.
    unique = un seul nombre distinct dans tous les textes ; ambigu = plusieurs (« 4 et 8 », « 10 ou 20 ») ;
    aucun = rien de trouvé."""
    valeurs, extraits = [], []
    for origine, brut in textes.items():
        texte = _nettoyer(brut)
        for motif in (_N_STEP, _STEPS_N):
            for m in motif.finditer(texte):
                n = int(m.group(1))
                if not STEPS_MIN <= n <= STEPS_MAX:
                    continue
                if n not in valeurs:
                    valeurs.append(n)
                debut, fin = max(0, m.start() - RAYON_EXTRAIT), min(len(texte), m.end() + RAYON_EXTRAIT)
                extrait = {"origine": origine, "texte": ("…" if debut else "") + texte[debut:fin] + ("…" if fin < len(texte) else "")}
                if extrait not in extraits:
                    extraits.append(extrait)
    statut = "aucun" if not valeurs else "unique" if len(valeurs) == 1 else "ambigu"
    return {"statut": statut, "valeurs": sorted(valeurs), "extraits": extraits[:6]}
