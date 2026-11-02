"""En-tête d'un GGUF (format de llama.cpp) : les types de ses tenseurs, lus sans charger les poids (0,15 s pour 27B).

Sert à choisir le moteur : un fork comme PrismML ajoute ses propres types (Bonsai 2 ternaire : 142 en PQ2_0, 143 en
PTQ1_0), que le llama.cpp officiel refuse au chargement (« invalid ggml type 142. should be in [0, 43) »).
"""

from functools import lru_cache
from pathlib import Path

# Types que le llama.cpp officiel épinglé (catalogue/moteurs.yaml, b11734) sait lire : [0, 43), lu dans son refus.
TYPES_OFFICIELS = 43

# Taille des valeurs fixes des métadonnées, par type GGUF (8 chaîne et 9 tableau ont leur propre longueur).
TAILLES = {0: 1, 1: 1, 2: 2, 3: 2, 4: 4, 5: 4, 6: 4, 7: 1, 10: 8, 11: 8, 12: 8}
CHAINE, TABLEAU = 8, 9
MAXI = 10**8  # au-delà, un compte n'a pas de sens : fichier abîmé


class ErreurGguf(ValueError):
    pass


def _entier(f, n: int = 8) -> int:
    octets = f.read(n)
    if len(octets) != n:
        raise ErreurGguf("GGUF tronqué")
    return int.from_bytes(octets, "little")


def _compte(f) -> int:
    n = _entier(f)
    if n > MAXI:
        raise ErreurGguf("GGUF illisible")
    return n


def _sauter_valeur(f, type_: int):
    if type_ in TAILLES:
        f.seek(TAILLES[type_], 1)
    elif type_ == CHAINE:
        f.seek(_compte(f), 1)
    elif type_ == TABLEAU:
        type_element, n = _entier(f, 4), _compte(f)
        if type_element in TAILLES:
            f.seek(TAILLES[type_element] * n, 1)
        else:
            for _ in range(n):
                _sauter_valeur(f, type_element)
    else:
        raise ErreurGguf(f"Type de métadonnée inconnu : {type_}")


@lru_cache(maxsize=32)
def _types(chemin: str, _taille: int, _date: int) -> frozenset[int]:
    with open(chemin, "rb", buffering=1 << 20) as f:
        if f.read(4) != b"GGUF":
            raise ErreurGguf("Ce n'est pas un GGUF")
        if _entier(f, 4) < 2:
            raise ErreurGguf("GGUF version 1 : trop ancien")
        n_tenseurs, n_cles = _compte(f), _compte(f)
        for _ in range(n_cles):
            f.seek(_compte(f), 1)  # nom de la clé
            _sauter_valeur(f, _entier(f, 4))
        types = set()
        for _ in range(n_tenseurs):
            f.seek(_compte(f), 1)  # nom du tenseur
            f.seek(8 * _entier(f, 4), 1)  # dimensions
            types.add(_entier(f, 4))
            f.seek(8, 1)  # position des données
    return frozenset(types)


def types_tenseurs(chemin: Path) -> frozenset[int]:
    """Types des tenseurs ; relu seulement si le fichier change (taille ou date)."""
    s = chemin.stat()
    return _types(str(chemin), s.st_size, s.st_mtime_ns)


def types_inconnus(chemin: Path) -> list[int]:
    """Types que le llama.cpp officiel ne sait pas lire : ceux d'un fork."""
    return sorted(t for t in types_tenseurs(chemin) if t >= TYPES_OFFICIELS)
