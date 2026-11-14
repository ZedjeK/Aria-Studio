"""Recherche d'une nouvelle version : la dernière release du dépôt GitHub public, comparée à app.VERSION.

Une requête anonyme vers l'API GitHub, au plus une fois par jour (au démarrage, puis quand l'interface le demande),
sauf « Vérifier maintenant ». Le choix de l'utilisateur (vérifier ou non) est dans la section « mises_a_jour » de
reglages.json (app/fichier_reglages.py) ; le résultat de la dernière recherche, qui se refait, dans cache/mise_a_jour.json. Rien n'est jamais téléchargé ni installé : l'interface montre la version, ses notes et le lien de
la release.
Sans réseau, sans release publiée ou avec un dépôt inaccessible, rien ne s'affiche en erreur : l'état le dit.
"""

import json
import re
from datetime import datetime, timedelta

import httpx

from app import VERSION
from app import fichier_reglages
from app.config import CONFIG
from app.emplacements import CACHE

FICHIER = CACHE / "mise_a_jour.json"  # le résultat de la dernière recherche
DELAI = timedelta(days=1)
API = "https://api.github.com"
_VERSION = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)$")


def depot() -> str:
    return CONFIG.get("mises_a_jour", {}).get("depot", "ZedjeK/Aria-Studio")


def numero(texte: str | None) -> tuple[int, int, int] | None:
    """« v0.9.1 » ou « 0.9.1 » → (0, 9, 1) ; None pour un tag qui n'est pas une version."""
    m = _VERSION.match((texte or "").strip())
    return tuple(int(x) for x in m.groups()) if m else None


def plus_recente(derniere: str | None, installee: str = VERSION) -> bool:
    d, i = numero(derniere), numero(installee)
    return bool(d and i and d > i)


def _lire() -> dict:
    """Le résultat de la dernière recherche (cache) et le choix « active » (reglages.json), réunis."""
    try:
        d = json.loads(FICHIER.read_text(encoding="utf-8"))
        d = d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        d = {}
    d.pop("active", None)
    return {**d, "active": fichier_reglages.section("mises_a_jour").get("active", True)}


def _ecrire(donnees: dict):
    resultat = {k: v for k, v in donnees.items() if k != "active"}
    FICHIER.parent.mkdir(parents=True, exist_ok=True)
    FICHIER.write_text(json.dumps(resultat, indent=4, ensure_ascii=False), encoding="utf-8")
    if bool(donnees.get("active", True)) != fichier_reglages.section("mises_a_jour").get("active", True):
        fichier_reglages.ecrire("mises_a_jour", {"active": bool(donnees.get("active", True))})


def etat() -> dict:
    """Ce que l'interface affiche, sans interroger GitHub."""
    d = _lire()
    derniere = d.get("derniere")
    return {"version": VERSION, "depot": depot(), "active": d.get("active", True),
            "verifie_le": d.get("verifie_le"), "erreur": d.get("erreur"), "derniere": derniere,
            "disponible": plus_recente(derniere["version"]) if derniere else False}


def activer(active: bool) -> dict:
    _ecrire({**_lire(), "active": bool(active)})
    return etat()


async def verifier(force: bool = False) -> dict:
    """Interroge GitHub si la vérification est active et que la dernière réponse a plus d'un jour (ou si `force`)."""
    d = _lire()
    if not force:
        if not d.get("active", True):
            return etat()
        if d.get("verifie_le") and datetime.now() - datetime.fromisoformat(d["verifie_le"]) < DELAI:
            return etat()
    d.update(verifie_le=datetime.now().isoformat(timespec="seconds"), erreur=None)
    try:
        async with httpx.AsyncClient(timeout=10, headers={"Accept": "application/vnd.github+json",
                                                          "User-Agent": f"Aria-Studio/{VERSION}"}) as client:
            r = await client.get(f"{API}/repos/{depot()}/releases/latest")
        if r.status_code == 404:  # dépôt sans release publiée (ou inaccessible) : rien à proposer
            d.update(derniere=None, erreur="Aucune release publiée sur ce dépôt pour l'instant")
        elif r.status_code != 200:
            d["erreur"] = f"GitHub a répondu {r.status_code}"
        else:
            rel = r.json()
            version = (rel.get("tag_name") or "").lstrip("v")
            if numero(version) is None:
                d["erreur"] = f"Dernière release sans numéro de version lisible : {rel.get('tag_name')}"
            else:
                d["derniere"] = {"version": version, "nom": rel.get("name") or f"v{version}",
                                 "url": rel.get("html_url"), "notes": (rel.get("body") or "")[:4000],
                                 "date": (rel.get("published_at") or "")[:10]}
    except httpx.HTTPError as e:
        d["erreur"] = f"GitHub injoignable : {type(e).__name__}"
    _ecrire(d)
    return etat()
