"""Clés d'accès aux sources de modèles (Civitai, Hugging Face), saisies dans l'onglet Réglages.

Elles sont rangées dans le Gestionnaire d'identification Windows (coffre chiffré du compte, comme Git ou Edge),
jamais dans un fichier d'Aria ; une variable d'environnement (CIVITAI_TOKEN, HF_TOKEN) sert de repli. Une clé ne
repart jamais vers l'interface : on ne dit que d'où elle vient. « Tester » la présente à sa source.
"""

import ctypes
import os
import sys
from ctypes import wintypes

import httpx

SOURCES = {
    "civitai": {"nom": "Civitai", "variable": "CIVITAI_TOKEN",
                "aide": "civitai.com → ton avatar → Account settings → API Keys → Add API key"},
    "huggingface": {"nom": "Hugging Face", "variable": "HF_TOKEN",
                    "aide": "huggingface.co → Settings → Access Tokens → Create new token (type Read)"},
}
CIBLE = "Aria-Studio/{}"  # nom de l'identifiant dans le Gestionnaire d'identification Windows
LONGUEUR_MAX = 512


class ErreurCle(ValueError):
    pass


# Gestionnaire d'identification Windows (advapi32 : CredWriteW, CredReadW, CredDeleteW)

class _Credential(ctypes.Structure):
    _fields_ = [("Flags", wintypes.DWORD), ("Type", wintypes.DWORD), ("TargetName", wintypes.LPWSTR),
                ("Comment", wintypes.LPWSTR), ("LastWritten", wintypes.FILETIME),
                ("CredentialBlobSize", wintypes.DWORD), ("CredentialBlob", ctypes.POINTER(ctypes.c_ubyte)),
                ("Persist", wintypes.DWORD), ("AttributeCount", wintypes.DWORD), ("Attributes", ctypes.c_void_p),
                ("TargetAlias", wintypes.LPWSTR), ("UserName", wintypes.LPWSTR)]


_GENERIQUE, _PERSISTE_LOCAL = 1, 2  # CRED_TYPE_GENERIC, CRED_PERSIST_LOCAL_MACHINE
_INTROUVABLE = 1168  # ERROR_NOT_FOUND


def _advapi():
    if sys.platform != "win32":
        raise ErreurCle("Le coffre des clés n'existe que sous Windows : utilise la variable d'environnement")
    return ctypes.WinDLL("advapi32", use_last_error=True)


def _ecrire(cible: str, valeur: str):
    blob = valeur.encode("utf-8")
    tampon = (ctypes.c_ubyte * len(blob)).from_buffer_copy(blob)
    c = _Credential(Type=_GENERIQUE, TargetName=cible, CredentialBlobSize=len(blob),
                    CredentialBlob=ctypes.cast(tampon, ctypes.POINTER(ctypes.c_ubyte)), Persist=_PERSISTE_LOCAL,
                    UserName="Aria - Studio")
    if not _advapi().CredWriteW(ctypes.byref(c), 0):
        raise ErreurCle(f"Le coffre Windows a refusé la clé (erreur {ctypes.get_last_error()})")


def _lire(cible: str) -> str | None:
    advapi = _advapi()
    pointeur = ctypes.POINTER(_Credential)()
    if not advapi.CredReadW(cible, _GENERIQUE, 0, ctypes.byref(pointeur)):
        if ctypes.get_last_error() == _INTROUVABLE:
            return None
        raise ErreurCle(f"Le coffre Windows n'a pas rendu la clé (erreur {ctypes.get_last_error()})")
    try:
        c = pointeur.contents
        return bytes(c.CredentialBlob[:c.CredentialBlobSize]).decode("utf-8")
    finally:
        advapi.CredFree(pointeur)


def _effacer(cible: str):
    if not _advapi().CredDeleteW(cible, _GENERIQUE, 0) and ctypes.get_last_error() != _INTROUVABLE:
        raise ErreurCle(f"Le coffre Windows n'a pas effacé la clé (erreur {ctypes.get_last_error()})")


# Clés par source

def _source(source: str) -> dict:
    if source not in SOURCES:
        raise ErreurCle(f"Source inconnue : {source}")
    return SOURCES[source]


def _du_coffre(source: str) -> str | None:
    try:
        return _lire(CIBLE.format(source))
    except ErreurCle:
        return None  # pas de coffre (autre système) ou coffre indisponible : la variable prend le relais


def cle(source: str) -> str | None:
    """La clé à utiliser : celle saisie dans Aria (coffre Windows), sinon la variable d'environnement."""
    return _du_coffre(source) or os.environ.get(_source(source)["variable"]) or None


def entetes(source: str) -> dict:
    valeur = cle(source)
    return {"Authorization": f"Bearer {valeur}"} if valeur else {}


def etat() -> dict:
    """D'où vient chaque clé (aria, variable, ou aucune) : jamais la clé elle-même."""
    sortie = {}
    for source, s in SOURCES.items():
        origine = "aria" if _du_coffre(source) else "variable" if os.environ.get(s["variable"]) else None
        sortie[source] = {"nom": s["nom"], "variable": s["variable"], "aide": s["aide"], "origine": origine}
    return sortie


def enregistrer(source: str, valeur: str) -> dict:
    _source(source)
    valeur = (valeur or "").strip()
    if not valeur or len(valeur) > LONGUEUR_MAX or any(c.isspace() for c in valeur):
        raise ErreurCle("Colle la clé seule, sans espace ni retour à la ligne")
    _ecrire(CIBLE.format(source), valeur)
    return etat()


def supprimer(source: str) -> dict:
    _source(source)
    _effacer(CIBLE.format(source))
    return etat()


async def tester(source: str) -> dict:
    """Présente la clé à sa source, qui dit à quel compte elle appartient (whoami-v2, /api/v1/me) ou la refuse."""
    _source(source)
    valeur = cle(source)
    if not valeur:
        return {"ok": False, "message": "Aucune clé à tester"}
    entete = {"Authorization": f"Bearer {valeur}"}
    try:
        async with httpx.AsyncClient(timeout=20) as client:
            if source == "huggingface":
                r = await client.get("https://huggingface.co/api/whoami-v2", headers=entete)
                if r.status_code == 200:
                    return {"ok": True, "message": f"Clé acceptée : compte {r.json().get('name', '?')}"}
            else:
                r = await client.get("https://civitai.com/api/v1/me", headers=entete)
                if r.status_code == 200:
                    nom = (r.json() or {}).get("username")
                    return {"ok": True, "message": f"Clé acceptée{f' : compte {nom}' if nom else ' par Civitai'}"}
    except httpx.HTTPError as e:
        return {"ok": False, "message": f"Source injoignable : {e!r}"}
    return {"ok": False, "message": f"Clé refusée ({r.status_code}) : vérifie-la, ou recrée-la"}
