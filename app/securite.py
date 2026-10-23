"""Protection des mineurs sur les prompts : une seule règle.

    Un personnage mineur est autorisé (manga, scènes de famille...).
    Un personnage mineur + un terme sensible (nudité, sexualité, suggestif) est bloqué : ERREUR AS-18.
    Quelques termes sont bloqués en toutes circonstances (loli, shota...).

Le noyau des listes est figé ici : securite/termes.yaml peut seulement AJOUTER des termes,
jamais en retirer. Le plancher de 18 ans ne peut pas être abaissé.
"""

import re
import unicodedata

import yaml

from app.config import ROOT

AGE_MAJORITE = 18
FICHIER_AJOUTS = ROOT / "securite" / "termes.yaml"

# Termes comparés sans accents, en minuscules, sur des mots entiers.
_MINEURS = [
    "child", "children", "kid", "kids", "minor", "minors", "underage", "under age",
    "teen", "teens", "teenage", "teenaged", "teenager", "teenagers", "preteen", "pre teen", "tween",
    "toddler", "infant", "newborn", "schoolgirl", "schoolboy", "little girl", "little boy",
    "young girl", "young boy",
    "enfant", "enfants", "gamin", "gamine", "gamins", "gamines", "fillette", "fillettes",
    "garconnet", "mineur", "mineure", "mineurs", "mineures", "ado", "ados",
    "adolescent", "adolescente", "adolescents", "adolescentes", "preado", "preados",
    "bebe", "bebes", "ecolier", "ecoliere", "collegien", "collegienne", "lyceen", "lyceenne",
    "petite fille", "petit garcon",
]

_SENSIBLES = [
    "nude", "nudes", "naked", "nudity", "topless", "bottomless", "striptease", "stripping", "stripped naked",
    "undress", "undresses", "undressed", "undressing", "disrobe", "disrobes", "disrobed", "disrobing", "unclothed",
    "clothes off", "takes off her clothes", "takes off his clothes", "removes her clothes", "removes his clothes",
    # « strip » et « strips » sont absents : en anglais, ce sont aussi une bande dessinée ou des lanières.
    "nu", "nue", "nus", "nues", "nudite", "denude", "denudee", "seins nus",
    "deshabille", "deshabillee", "deshabilles", "deshabillees", "deshabiller", "deshabillent", "deshabillant",
    "devetu", "devetue", "devetir", "enleve ses vetements", "retire ses vetements", "ote ses vetements",
    "sex", "sexy", "sexual", "sexualized", "erotic", "erotica", "sensual", "seductive", "suggestive",
    "provocative", "lewd", "nsfw", "porn", "porno", "pornographic", "hentai", "ecchi", "fetish", "bdsm", "bondage",
    "sexe", "sexuel", "sexuelle", "erotique", "sensuel", "sensuelle", "seducteur", "seductrice", "suggestif",
    "suggestive", "provocant", "provocante", "aguicheur", "aguicheuse", "fetiche",
    "lingerie", "underwear", "panties", "thong", "bra", "bikini", "swimsuit", "bathing suit",
    # « string » est absent : en anglais, il désigne aussi les guirlandes (« string lights »).
    "sous vetements", "sous vetement", "culotte", "soutien gorge", "maillot de bain",
    "breasts", "breast", "boobs", "nipples", "cleavage", "butt", "buttocks", "crotch", "genitals",
    "seins", "poitrine", "tetons", "fesses", "entrejambe",
    "bath", "bathtub", "shower", "bain", "baignoire", "douche",
    # « embrasser » est absent : en français, c'est aussi serrer dans ses bras (scènes de famille).
    "baiser", "french kiss", "make out", "making out",
]

_TOUJOURS_BLOQUES = ["loli", "lolita", "lolicon", "shota", "shotacon", "jailbait"]

_NOMBRES = {
    "un": 1, "une": 1, "deux": 2, "trois": 3, "quatre": 4, "cinq": 5, "six": 6, "sept": 7, "huit": 8,
    "neuf": 9, "dix": 10, "onze": 11, "douze": 12, "treize": 13, "quatorze": 14, "quinze": 15, "seize": 16,
    "dix sept": 17,
    "one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "seven": 7, "eight": 8, "nine": 9, "ten": 10,
    "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14, "fifteen": 15, "sixteen": 16, "seventeen": 17,
}
_NOMBRE = r"(\d{1,2}|" + "|".join(sorted((re.escape(n).replace(r"\ ", r"[\s-]+") for n in _NOMBRES), key=len, reverse=True)) + ")"

# « 12 ans », « douze ans », « 12 yo », « 12-year-old », « twelve years old », « aged 12 », « âge : 12 ».
_AGES = [
    re.compile(rf"\b{_NOMBRE}\s*-?\s*(?:yo|y/o|y\.o\.?|years?[\s-]*old|ans?)\b"),
    re.compile(rf"\bage[ds]?\s*:?\s*{_NOMBRE}\b"),
]


class ErreurSecurite(ValueError):
    code = "AS-18"

    def __str__(self):
        return f"ERREUR {self.code} · Génération bloquée : {self.args[0]}"


def _normaliser(texte: str) -> str:
    sans_accents = unicodedata.normalize("NFKD", texte).encode("ascii", "ignore").decode()
    return sans_accents.lower()


def _motif(termes: list[str]) -> re.Pattern:
    termes = sorted({_normaliser(t) for t in termes}, key=len, reverse=True)
    return re.compile(r"\b(" + "|".join(re.escape(t).replace(r"\ ", r"[\s-]+") for t in termes) + r")\b")


def _listes() -> tuple[re.Pattern, re.Pattern, re.Pattern]:
    """Noyau figé + ajouts éventuels de securite/termes.yaml (relus à chaque appel, sans redémarrage)."""
    ajouts = {}
    if FICHIER_AJOUTS.is_file():
        ajouts = yaml.safe_load(FICHIER_AJOUTS.read_text(encoding="utf-8")) or {}
    return (
        _motif(_MINEURS + (ajouts.get("mineurs") or [])),
        _motif(_SENSIBLES + (ajouts.get("sensibles") or [])),
        _motif(_TOUJOURS_BLOQUES + (ajouts.get("toujours_bloques") or [])),
    )


def _valeur(nombre: str) -> int:
    return int(nombre) if nombre.isdigit() else _NOMBRES[re.sub(r"[\s-]+", " ", nombre)]


def _mineur(t: str, motif_mineurs: re.Pattern) -> str | None:
    if m := motif_mineurs.search(t):
        return m.group(1)
    for motif in _AGES:
        for m in motif.finditer(t):
            if _valeur(m.group(1)) < AGE_MAJORITE:
                return m.group(0)
    return None


def verifier(*textes: str):
    """Lève ErreurSecurite si les textes, pris ensemble, associent un mineur à un terme sensible."""
    t = _normaliser("\n".join(textes))
    mineurs, sensibles, toujours = _listes()
    if m := toujours.search(t):
        raise ErreurSecurite(f"terme interdit « {m.group(1)} »")
    mineur = _mineur(t, mineurs)
    if mineur and (s := sensibles.search(t)):
        raise ErreurSecurite(f"personnage mineur (« {mineur} ») associé à un terme sensible (« {s.group(1)} »)")
