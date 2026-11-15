"""Protection des mineurs sur les prompts : une seule règle.

    Un personnage mineur est autorisé (manga, scènes de famille...).
    Un personnage mineur + un terme sensible (nudité, sexualité, suggestif) est bloqué : ERREUR AS-18.
    Quelques termes sont bloqués en toutes circonstances (loli, shota...).

Les listes vivent dans deux fichiers, appliqués ensemble :
    securite/socle.yaml        le socle livré avec Aria. Depuis Réglages, il ne peut que grandir (un terme promu s'y
                               ajoute) ; le corriger se fait à la main, dans le fichier.
    reglages.json, « securite »  tes ajouts et tes phrases témoins (Réglages les ajoute et les retire). Comme tous les
                               réglages, ils ont leur sauvegarde avant chaque changement (app/fichier_reglages.py).

Le filtre ne se désactive pas : sans socle lisible, verifier() refuse tout. Le plancher de 18 ans et la lecture des
âges restent ici, dans le code.
"""

import json
import os
import re
import time
import unicodedata
from pathlib import Path

import yaml

from app import fichier_reglages
from app.config import ROOT

AGE_MAJORITE = 18
FICHIER_SOCLE = ROOT / "securite" / "socle.yaml"
FICHIER_TEMOINS = ROOT / "securite" / "temoins.yaml"  # phrases témoins livrées avec Aria
SECTION = "securite"  # la section de reglages.json qui garde tes ajouts et tes phrases témoins

LISTES = {
    "mineurs": "Mots qui désignent un mineur",
    "sensibles": "Termes sensibles (bloqués avec un mineur)",
    "toujours_bloques": "Termes toujours bloqués",
}
ATTENDUS = ("bloque", "passe")
LONGUEUR_MIN = 3  # un ajout plus court (« a », « de ») bloquerait presque tout

_NOMBRES = {
    "un": 1, "une": 1, "deux": 2, "trois": 3, "quatre": 4, "cinq": 5, "six": 6, "sept": 7, "huit": 8,
    "neuf": 9, "dix": 10, "onze": 11, "douze": 12, "treize": 13, "quatorze": 14, "quinze": 15, "seize": 16,
    "dix sept": 17,
    "one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "seven": 7, "eight": 8, "nine": 9, "ten": 10,
    "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14, "fifteen": 15, "sixteen": 16, "seventeen": 17,
}

# Les âges adultes écrits en plusieurs mots (« vingt-neuf », « vingt et un », « soixante-dix-neuf », « twenty-nine »...),
# lus à leur vraie valeur. Sans eux, le trait d'union laissait « neuf » (« un », « huit »...) seul devant « ans » et
# « vingt-neuf ans » se lisait « neuf ans » : un adulte bloqué à tort. Tous valent 18 ou plus, ce que tests/test_securite.py
# garde ; « dix sept » (17) reste le seul nombre à deux mots qui désigne un mineur.
_UNITES_FR = ("un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf")
_DIZAINES_FR = {"vingt": 20, "trente": 30, "quarante": 40, "cinquante": 50, "soixante": 60, "septante": 70,
                "huitante": 80, "nonante": 90}  # septante, huitante, nonante : Belgique et Suisse
_ADOS_FR = {"dix": 10, "onze": 11, "douze": 12, "treize": 13, "quatorze": 14, "quinze": 15, "seize": 16,
            "dix sept": 17, "dix huit": 18, "dix neuf": 19}
_UNITES_EN = ("one", "two", "three", "four", "five", "six", "seven", "eight", "nine")
_DIZAINES_EN = {"twenty": 20, "thirty": 30, "forty": 40, "fifty": 50, "sixty": 60, "seventy": 70, "eighty": 80, "ninety": 90}


def _nombres_composes() -> dict[str, int]:
    nombres = {"dix huit": 18, "dix neuf": 19, "quatre vingt": 80, "quatre vingts": 80, "soixante et onze": 71}
    for dizaine, valeur in _DIZAINES_FR.items():
        for i, unite in enumerate(_UNITES_FR, 1):
            nombres[f"{dizaine} {unite}"] = valeur + i
        nombres[f"{dizaine} et un"] = valeur + 1
    for ado, valeur in _ADOS_FR.items():  # soixante-dix à soixante-dix-neuf, quatre-vingt-dix à quatre-vingt-dix-neuf
        nombres[f"soixante {ado}"] = 60 + valeur
        nombres[f"quatre vingt {ado}"] = 80 + valeur
    for i, unite in enumerate(_UNITES_FR, 1):  # quatre-vingt-un à quatre-vingt-neuf
        nombres[f"quatre vingt {unite}"] = 80 + i
    for dizaine, valeur in _DIZAINES_EN.items():
        for i, unite in enumerate(_UNITES_EN, 1):
            nombres[f"{dizaine} {unite}"] = valeur + i
    return nombres


_NOMBRES.update(_nombres_composes())
_COMPOSES_EN = {f"{dizaine} {unite}" for dizaine in _DIZAINES_EN for unite in _UNITES_EN}


def _forme(cle: str) -> str:
    """Comment un nombre se cherche dans un texte. Un nombre anglais à deux mots n'est un âge adulte que lié par un trait
    d'union (« twenty-two ») ou devant « years » au pluriel (« twenty two years old ») : « twenty two-year-old girls » dit
    vingt fillettes de deux ans, et garde sa lecture d'avant (« two-year-old », bloqué). En français rien de tel : un nombre
    d'enfants s'écrit avec « de » (« vingt enfants de neuf ans »)."""
    mots = [re.escape(mot) for mot in cle.split()]
    if cle in _COMPOSES_EN:
        return f"{mots[0]}(?:-{mots[1]}|\\s+{mots[1]}(?=\\s+years\\b))"
    return r"[\s-]+".join(mots)


_NOMBRE = r"(\d{1,2}|" + "|".join(sorted((_forme(n) for n in _NOMBRES), key=len, reverse=True)) + ")"
_PERSONNE = (r"(?:girl|boy|kid|child|daughter|son|student|pupil|teen|lad|lass|female|male|person"
             r"|fille|garcon|enfant|gamin|gamine|eleve)")

# « 12 ans », « douze ans », « 12 yo », « 12 yrs », « 12-year-old », « twelve years old », « 12 years of age »,
# « aged 12 », « âge : 12 », « age of 12 », « a girl of 14 », « une fille de 14 ».
_AGES = [
    re.compile(rf"\b{_NOMBRE}\s*-?\s*(?:yo|y/o|y\.o\.?|yrs?(?:[\s-]*old)?|years?[\s-]*old|years?\s+of\s+age|ans?)\b"),
    re.compile(rf"\bage[ds]?\s*(?:of\s+)?:?\s*{_NOMBRE}\b"),
    re.compile(rf"\b{_PERSONNE}\s+(?:of|de)\s+{_NOMBRE}\b"),
]

_cache: dict = {"cle": None, "listes": None}


class ErreurSecurite(ValueError):
    code = "AS-18"

    def __str__(self):
        return f"ERREUR {self.code} · Génération bloquée : {self.args[0]}"


class ErreurListes(ValueError):
    """Une modification des listes refusée (terme trop court, déjà présent, liste inconnue…)."""


class ErreurTemoins(ErreurListes):
    """L'ajout ferait bloquer des phrases témoins qui doivent passer : à confirmer (forcer=True)."""

    def __init__(self, terme: str, temoins: list[str]):
        super().__init__(f"« {terme} » ferait bloquer {len(temoins)} phrase{'s' if len(temoins) > 1 else ''} "
                         f"témoin{'s' if len(temoins) > 1 else ''} qui doi{'ven' if len(temoins) > 1 else ''}t passer : "
                         + " ; ".join(f"« {t} »" for t in temoins))
        self.temoins = temoins


def _normaliser(texte: str) -> str:
    sans_accents = unicodedata.normalize("NFKD", texte).encode("ascii", "ignore").decode()
    return sans_accents.lower()


def _cle(terme: str) -> str:
    """Un terme tel qu'il est comparé : sans accents, en minuscules, espaces et tirets confondus."""
    return re.sub(r"[\s-]+", " ", _normaliser(terme)).strip()


def _motif(termes) -> re.Pattern:
    termes = sorted(set(termes), key=len, reverse=True)
    return re.compile(r"\b(" + "|".join(re.escape(t).replace(r"\ ", r"[\s-]+") for t in termes) + r")\b")


class _Listes:
    """Le socle et les ajouts, prêts à être cherchés ; `etage[liste][terme]` dit d'où vient chaque terme."""

    def __init__(self, socle: dict, ajouts: dict):
        self.etage = {}
        for liste in LISTES:
            self.etage[liste] = {_cle(t): "ajouts" for t in ajouts.get(liste, [])}
            self.etage[liste].update({_cle(t): "socle" for t in socle[liste]})  # le socle l'emporte
        self.motifs = {liste: _motif(self.etage[liste]) for liste in LISTES}

    def chercher(self, liste: str, texte: str) -> dict | None:
        m = self.motifs[liste].search(texte)
        if not m:
            return None
        terme = _cle(m.group(1))
        return {"terme": terme, "liste": liste, "etage": self.etage[liste].get(terme, "socle")}


def _lire_socle() -> dict:
    """Les trois listes du socle. Absent, vide ou illisible : ErreurSecurite, et plus rien ne se génère."""
    panne = f"le socle du filtre est absent, vide ou illisible ({FICHIER_SOCLE}) : répare ce fichier ou réinstalle Aria"
    try:
        brut = yaml.safe_load(FICHIER_SOCLE.read_text(encoding="utf-8"))
    except (OSError, yaml.YAMLError) as e:
        raise ErreurSecurite(panne) from e
    if not isinstance(brut, dict):
        raise ErreurSecurite(panne)
    socle = {}
    for liste in LISTES:
        termes = brut.get(liste)
        if not isinstance(termes, list) or not termes or not all(isinstance(t, str) and _cle(t) for t in termes):
            raise ErreurSecurite(panne)
        socle[liste] = termes
    return socle


def _lire_ajouts() -> dict:
    """Les ajouts et les phrases témoins de l'utilisateur (section « securite » de reglages.json). Section absente :
    rien. Section mal formée : ErreurSecurite. Un reglages.json illisible est géré par app/fichier_reglages.py : sans
    ajouts, le socle seul s'applique, le filtre n'est jamais affaibli."""
    vide = {**{liste: [] for liste in LISTES}, "temoins": []}
    brut = fichier_reglages.section(SECTION)
    if not brut:
        return vide
    panne = (f"tes ajouts au filtre sont mal formés (section « {SECTION} » de {fichier_reglages.fichier()}) : "
             "corrige-les ou retire cette section")
    for liste in LISTES:
        termes = brut.get(liste) or []
        if not isinstance(termes, list) or not all(isinstance(t, str) and _cle(t) for t in termes):
            raise ErreurSecurite(panne)
        vide[liste] = termes
    temoins = brut.get("temoins") or []
    if not isinstance(temoins, list) or not all(_temoin_valide(t) for t in temoins):
        raise ErreurSecurite(panne)
    vide["temoins"] = [{"texte": t["texte"], "attendu": t["attendu"]} for t in temoins]
    return vide


def _temoin_valide(t) -> bool:
    return isinstance(t, dict) and isinstance(t.get("texte"), str) and t.get("attendu") in ATTENDUS


def _empreinte(fichier: Path):
    try:
        etat = fichier.stat()
        return str(fichier), etat.st_mtime_ns, etat.st_size
    except OSError:
        return str(fichier), None, None


def _listes() -> _Listes:
    """Socle + ajouts, relus dès que le socle ou reglages.json change (sans redémarrage)."""
    cle = (_empreinte(FICHIER_SOCLE), _empreinte(fichier_reglages.fichier()))
    if _cache["cle"] != cle:
        _cache["listes"] = _Listes(_lire_socle(), _lire_ajouts())
        _cache["cle"] = cle
    return _cache["listes"]


def _valeur(nombre: str) -> int:
    return int(nombre) if nombre.isdigit() else _NOMBRES[re.sub(r"[\s-]+", " ", nombre)]


def _mineur(texte: str, listes: _Listes) -> dict | None:
    if trouve := listes.chercher("mineurs", texte):
        return trouve
    for motif in _AGES:
        for m in motif.finditer(texte):
            if _valeur(m.group(1)) < AGE_MAJORITE:
                return {"terme": m.group(0), "liste": "mineurs", "etage": "age"}
    return None


def _analyser(textes, listes: _Listes) -> dict | None:
    """Ce qui bloque ces textes pris ensemble ({message, termes}), ou None s'ils passent."""
    texte = _normaliser("\n".join(textes))
    if interdit := listes.chercher("toujours_bloques", texte):
        return {"message": f"terme interdit « {interdit['terme']} »", "termes": [interdit]}
    mineur = _mineur(texte, listes)
    if mineur and (sensible := listes.chercher("sensibles", texte)):
        return {"message": f"personnage mineur (« {mineur['terme']} ») associé à un terme sensible "
                           f"(« {sensible['terme']} »)", "termes": [mineur, sensible]}
    return None


def verifier(*textes: str):
    """Lève ErreurSecurite si les textes, pris ensemble, associent un mineur à un terme sensible."""
    if refus := _analyser(textes, _listes()):
        raise ErreurSecurite(refus["message"])


# Réglages → Système → Sécurité : voir les listes, ajouter, essayer, promouvoir (app/main.py, web/reglages.js).

def essayer(texte: str) -> dict:
    """Le verdict du filtre sur une phrase, sans rien générer."""
    refus = _analyser([texte], _listes())
    return {"bloque": bool(refus), "message": refus["message"] if refus else None,
            "termes": refus["termes"] if refus else []}


def _temoins_livres() -> list[dict]:
    try:
        brut = yaml.safe_load(FICHIER_TEMOINS.read_text(encoding="utf-8")) or []
    except (OSError, yaml.YAMLError):
        return []
    return [{"texte": t["texte"], "attendu": t["attendu"]} for t in brut if _temoin_valide(t)]


def _evaluer_temoins(listes: _Listes, ajouts: dict) -> list[dict]:
    evalues = []
    for livre, temoins in ((True, _temoins_livres()), (False, ajouts["temoins"])):
        for t in temoins:
            refus = _analyser([t["texte"]], listes)
            obtenu = "bloque" if refus else "passe"
            evalues.append({**t, "livre": livre, "obtenu": obtenu, "ok": obtenu == t["attendu"],
                            "termes": refus["termes"] if refus else []})
    return evalues


def etat() -> dict:
    """Ce que montre la carte Sécurité de Réglages. Une liste illisible n'empêche pas de l'afficher : `erreur`."""
    base = {"fichier_socle": str(FICHIER_SOCLE), "fichier_ajouts": str(fichier_reglages.fichier()), "erreur": None,
            "listes": [], "temoins": [], "longueur_min": LONGUEUR_MIN}
    try:
        socle, ajouts, listes = _lire_socle(), _lire_ajouts(), _listes()
    except ErreurSecurite as e:
        return {**base, "erreur": e.args[0]}
    base["listes"] = [{"cle": cle, "nom": nom, "socle": socle[cle], "ajouts": ajouts[cle]} for cle, nom in LISTES.items()]
    base["temoins"] = _evaluer_temoins(listes, ajouts)
    return base


def _ecrire(fichier: Path, texte: str):
    """Écriture en deux temps : le fichier n'est jamais laissé à moitié écrit."""
    fichier.parent.mkdir(parents=True, exist_ok=True)
    provisoire = fichier.with_name(fichier.name + ".tmp")
    provisoire.write_text(texte, encoding="utf-8", newline="\n")
    os.replace(provisoire, fichier)


def _ecrire_ajouts(ajouts: dict):
    """Tes ajouts, dans reglages.json : une sauvegarde avant chaque changement, et une ligne au journal (contrat des
    applis Aria). Une section sans rien dedans est retirée."""
    section = {cle: ajouts[cle] for cle in (*LISTES, "temoins") if ajouts[cle]}
    fichier_reglages.ecrire(SECTION, section)


def _propre(terme: str) -> str:
    return " ".join(str(terme).split())


def _exiger_liste(liste: str):
    if liste not in LISTES:
        raise ErreurListes(f"Liste inconnue : {liste}")


def ajouter(liste: str, terme: str, forcer: bool = False) -> dict:
    """Un terme de plus dans tes ajouts. Refusé s'il est trop court ou déjà présent ; s'il ferait bloquer une phrase
    témoin qui doit passer, ErreurTemoins tant que `forcer` n'est pas donné."""
    _exiger_liste(liste)
    terme = _propre(terme)
    cle = _cle(terme)
    if len(cle.replace(" ", "")) < LONGUEUR_MIN or not re.fullmatch(r"[a-z0-9][a-z0-9 ']*[a-z0-9]", cle):
        raise ErreurListes(f"Un terme fait au moins {LONGUEUR_MIN} lettres ou chiffres, sans ponctuation")
    socle, ajouts = _lire_socle(), _lire_ajouts()
    if cle in {_cle(t) for t in socle[liste]}:
        raise ErreurListes(f"« {terme} » est déjà dans le socle")
    if cle in {_cle(t) for t in ajouts[liste]}:
        raise ErreurListes(f"« {terme} » est déjà dans tes ajouts")
    futur = {**ajouts, liste: [*ajouts[liste], terme]}
    if not forcer:
        avant = {t["texte"] for t in _evaluer_temoins(_Listes(socle, ajouts), ajouts) if t["obtenu"] == "bloque"}
        casses = [t["texte"] for t in _evaluer_temoins(_Listes(socle, futur), ajouts)
                  if t["attendu"] == "passe" and t["obtenu"] == "bloque" and t["texte"] not in avant]
        if casses:
            raise ErreurTemoins(terme, casses)
    _ecrire_ajouts(futur)
    return etat()


def _sans(termes: list[str], terme: str) -> list[str]:
    reste = [t for t in termes if _cle(t) != _cle(terme)]
    if len(reste) == len(termes):
        raise ErreurListes(f"« {_propre(terme)} » n'est pas dans tes ajouts")
    return reste


def retirer(liste: str, terme: str) -> dict:
    """Retire un de TES ajouts. Le socle, lui, ne se retire pas d'ici."""
    _exiger_liste(liste)
    ajouts = _lire_ajouts()
    _ecrire_ajouts({**ajouts, liste: _sans(ajouts[liste], terme)})
    return etat()


def _inserer_dans_socle(texte: str, liste: str, terme: str) -> str:
    """Le texte du socle avec `terme` en fin de `liste`, sans réécrire le reste (commentaires gardés)."""
    lignes = texte.split("\n")
    debut = next((i for i, l in enumerate(lignes) if l.startswith(f"{liste}:")), None)
    if debut is None:
        raise ErreurListes(f"{FICHIER_SOCLE.name} n'a pas de liste « {liste} » : ajoute le terme à la main")
    fin = next((i for i in range(debut + 1, len(lignes)) if re.match(r"[A-Za-z_]+:", lignes[i])), len(lignes))
    while fin - 1 > debut and (not lignes[fin - 1].strip() or lignes[fin - 1].startswith("#")):
        fin -= 1  # les lignes vides et les commentaires de tête appartiennent à la liste suivante
    lignes.insert(fin, f"  - {json.dumps(terme, ensure_ascii=False)}  # promu le {time.strftime('%Y-%m-%d')}")
    return "\n".join(lignes)


def promouvoir(liste: str, terme: str) -> dict:
    """Fait passer un de tes ajouts dans le socle. À sens unique : il ne se retire plus depuis Réglages."""
    _exiger_liste(liste)
    socle, ajouts = _lire_socle(), _lire_ajouts()
    reste = _sans(ajouts[liste], terme)
    terme = next(t for t in ajouts[liste] if _cle(t) == _cle(terme))
    nouveau = _inserer_dans_socle(FICHIER_SOCLE.read_text(encoding="utf-8"), liste, terme)
    try:
        relu = yaml.safe_load(nouveau)
        correct = (isinstance(relu, dict) and all(relu.get(l) == socle[l] for l in LISTES if l != liste)
                   and relu.get(liste) == [*socle[liste], terme])
    except yaml.YAMLError:
        correct = False
    if not correct:
        raise ErreurListes(f"{FICHIER_SOCLE.name} a une forme inattendue : ajoute « {terme} » à la main dans ce fichier")
    _ecrire(FICHIER_SOCLE, nouveau)
    _ecrire_ajouts({**ajouts, liste: reste})
    return etat()


def ajouter_temoin(texte: str, attendu: str) -> dict:
    """Une phrase témoin de plus : « bloque » ou « passe », revérifiée à chaque changement de liste."""
    texte = _propre(texte)
    if not texte:
        raise ErreurListes("Écris la phrase témoin")
    if attendu not in ATTENDUS:
        raise ErreurListes("Une phrase témoin doit bloquer ou passer")
    ajouts = _lire_ajouts()
    if texte in {t["texte"] for t in [*_temoins_livres(), *ajouts["temoins"]]}:
        raise ErreurListes("Cette phrase témoin existe déjà")
    _ecrire_ajouts({**ajouts, "temoins": [*ajouts["temoins"], {"texte": texte, "attendu": attendu}]})
    return etat()


def retirer_temoin(texte: str) -> dict:
    """Retire une de TES phrases témoins (celles livrées avec Aria restent)."""
    ajouts = _lire_ajouts()
    reste = [t for t in ajouts["temoins"] if t["texte"] != texte]
    if len(reste) == len(ajouts["temoins"]):
        raise ErreurListes("Cette phrase témoin n'est pas dans les tiennes")
    _ecrire_ajouts({**ajouts, "temoins": reste})
    return etat()
