"""Fiches Civitai : https://civitai.com/models/<id>[?modelVersionId=<version>]."""

import os
from urllib.parse import parse_qs, urlparse

import httpx

from app.sources import ErreurSource, Fiche

API = "https://civitai.com/api/v1"

# Les checkpoints et LoRAs dépendent d'une famille (<Famille>/modeles, <Famille>/loras) : dossier à choisir.
TYPES = {
    "VAE": "_base/vae",
    "Controlnet": "_base/controlnet",
    "Upscaler": "_base/upscale_models",
}


def entetes() -> dict:
    jeton = os.environ.get("CIVITAI_TOKEN")
    return {"Authorization": f"Bearer {jeton}"} if jeton else {}


def _analyser(url: str) -> tuple[str | None, str | None]:
    """Renvoie (id du modèle, id de version) ; l'un des deux peut manquer."""
    u = urlparse(url.strip())
    morceaux = [m for m in u.path.split("/") if m]
    version = parse_qs(u.query).get("modelVersionId", [None])[0]
    if morceaux[:3] == ["api", "download", "models"] and len(morceaux) > 3:
        return None, morceaux[3]
    if len(morceaux) >= 2 and morceaux[0] == "models" and morceaux[1].isdigit():
        return morceaux[1], version
    raise ErreurSource("Lien Civitai attendu : https://civitai.com/models/<id>")


async def fiche(url: str) -> Fiche:
    id_modele, id_version = _analyser(url)
    async with httpx.AsyncClient(timeout=20, headers=entetes()) as client:
        if id_version is None:
            r = await client.get(f"{API}/models/{id_modele}")
            if r.status_code == 404:
                raise ErreurSource(f"Modèle Civitai introuvable : {id_modele}")
            r.raise_for_status()
            versions = r.json().get("modelVersions") or []
            if not versions:
                raise ErreurSource("Ce modèle n'a aucune version téléchargeable")
            id_version = str(versions[0]["id"])  # la plus récente
        r = await client.get(f"{API}/model-versions/{id_version}")
        if r.status_code == 404:
            raise ErreurSource(f"Version Civitai introuvable : {id_version}")
        r.raise_for_status()
        version = r.json()

    fichiers = version.get("files") or []
    f = next((f for f in fichiers if f.get("primary")), fichiers[0] if fichiers else None)
    if f is None:
        raise ErreurSource("Cette version ne contient aucun fichier")

    type_civitai = version.get("model", {}).get("type")
    base = version.get("baseModel")
    return Fiche(
        source="civitai",
        nom=f["name"],
        taille=round(f["sizeKB"] * 1024),
        sha256=(f.get("hashes") or {}).get("SHA256", "").lower() or None,
        url=f["downloadUrl"],
        page=f"https://civitai.com/models/{version['modelId']}?modelVersionId={version['id']}",
        dossier=TYPES.get(type_civitai) or _dossier_famille(type_civitai, base, version.get("model", {}).get("name", "")),
        licence=None,  # Civitai n'expose pas de licence nommée : voir la page du modèle
        jeton_requis=version.get("availability") == "EarlyAccess" and not os.environ.get("CIVITAI_TOKEN"),
        mots_declencheurs=[m.strip() for m in version.get("trainedWords") or [] if m.strip()],
        base=base,
    )


TYPES_LORA = ("LORA", "LoCon", "DoRA")


def _dossier_famille(type_civitai: str | None, base: str | None, nom_modele: str,
                     familles: dict | None = None) -> str | None:
    """Propose <Famille>/loras ou <Famille>/modeles d'après la base Civitai (bloc civitai des fiches de workflow) ;
    à confirmer avant téléchargement."""
    from app import workflows  # workflows importe modeles, qui ne doit pas dépendre des sources

    famille = workflows.famille_civitai(base, nom_modele, familles)
    if famille is None:
        return None
    if type_civitai in TYPES_LORA:
        return f"{famille}/loras"
    if type_civitai == "Checkpoint":
        return f"{famille}/modeles"
    return None


async def par_empreinte(sha256: str) -> dict | None:
    """La version Civitai d'un fichier, retrouvée par son SHA-256 (None : Civitai ne le connaît pas)."""
    async with httpx.AsyncClient(timeout=20, headers=entetes()) as client:
        r = await client.get(f"{API}/model-versions/by-hash/{sha256}")
    if r.status_code == 404:
        return None
    r.raise_for_status()
    version = r.json()
    fichier = next((f for f in version.get("files") or []
                    if (f.get("hashes") or {}).get("SHA256", "").lower() == sha256.lower()), None)
    modele = version.get("model") or {}
    return {
        "source": "civitai",
        "page": f"https://civitai.com/models/{version['modelId']}?modelVersionId={version['id']}",
        "url": (fichier or {}).get("downloadUrl") or f"https://civitai.com/api/download/models/{version['id']}",
        "licence": None,
        "mots_declencheurs": [m.strip() for m in version.get("trainedWords") or [] if m.strip()],
        "base": version.get("baseModel"),
        "nom_modele": modele.get("name"),
        "version": version.get("name"),
        "type": modele.get("type"),
    }
