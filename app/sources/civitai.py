"""Fiches Civitai : https://civitai.com/models/<id>[?modelVersionId=<version>] (ou civitai.red : la même base)."""

import statistics
from pathlib import PurePosixPath
from urllib.parse import parse_qs, urlparse

import httpx

from app import cles
from app.sources import ErreurSource, Fiche, steps

API = "https://civitai.com/api/v1"

# civitai.com ne montre plus que le tout public : les modèles adultes s'ouvrent sur civitai.red (même base, mêmes ids).
# nsfwLevel est un masque : 1 PG, 2 PG-13, 4 R, 8 X, 16 XXX (le drapeau « nsfw » du modèle reste souvent à false).
NIVEAUX_ADULTES = 4 | 8 | 16

# Les checkpoints et LoRAs dépendent d'une famille (<nature>/<Famille>/modeles ou loras) : dossier à choisir. Le
# reste va côté image : Civitai sert l'image, et le dossier se change avant de télécharger.
TYPES = {
    "VAE": "image/_base/vae",
    "Controlnet": "image/_base/controlnet",
    "Upscaler": "image/_base/upscale_models",
}


def entetes() -> dict:
    return cles.entetes("civitai")  # clé saisie dans Réglages (coffre Windows), sinon CIVITAI_TOKEN


def page(id_modele, id_version=None, niveau: int | None = 0) -> str:
    """La page du modèle, sur civitai.red s'il a du contenu au-delà de PG-13."""
    site = "https://civitai.red" if (niveau or 0) & NIVEAUX_ADULTES else "https://civitai.com"
    return f"{site}/models/{id_modele}" + (f"?modelVersionId={id_version}" if id_version else "")


TYPES_RESSOURCE_LORA = ("lora", "locon", "dora")


def usage_auteur(version: dict, nom_fichier: str | None = None) -> dict:
    """Ce que l'auteur fait de sa LoRA dans ses images d'exemple : Civitai n'a pas de champ « force conseillée »,
    mais leurs métadonnées donnent la force utilisée (on prend la médiane) et un prompt, où se lisent souvent les mots
    qui la déclenchent quand l'auteur n'en a pas déclaré."""
    souche = PurePosixPath(nom_fichier).stem.lower() if nom_fichier else ""
    forces, prompt = [], None
    for image in version.get("images") or []:
        meta = image.get("meta") or {}
        if prompt is None and str(meta.get("prompt") or "").strip():
            prompt = " ".join(str(meta["prompt"]).split())[:500]
        loras = [r for r in meta.get("additionalResources") or []
                 if isinstance(r, dict) and str(r.get("type", "")).lower() in TYPES_RESSOURCE_LORA]
        if len(loras) > 1:  # plusieurs LoRA sur l'image : celle qui porte le nom du fichier
            loras = [r for r in loras if souche and PurePosixPath(str(r.get("name", ""))).stem.lower() == souche]
        for r in loras[:1]:
            try:
                force = float(r.get("strength", r.get("weight")))
            except (TypeError, ValueError):
                continue
            if 0 < force <= 2:
                forces.append(force)
    return {"force_auteur": round(statistics.median(forces) * 20) / 20 if forces else None,
            "images_force": len(forces), "prompt_exemple": prompt}


async def lire_version(id_version) -> dict:
    """La version Civitai, telle que l'API la donne."""
    async with httpx.AsyncClient(timeout=20, headers=entetes()) as client:
        r = await client.get(f"{API}/model-versions/{id_version}")
    if r.status_code == 404:
        raise ErreurSource(f"Version Civitai introuvable : {id_version}")
    r.raise_for_status()
    return r.json()


def id_version(page_: str | None) -> str | None:
    """L'id de version d'une page Civitai notée au registre (…?modelVersionId=<id>)."""
    return parse_qs(urlparse(page_ or "").query).get("modelVersionId", [None])[0]


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
        # Beaucoup de modèles ne se téléchargent qu'avec une clé d'API : un HEAD le dit avant la mise en file
        # (401 sans clé, redirection vers le fichier sinon). Rien n'est téléchargé.
        protege = version.get("availability") == "EarlyAccess"
        if not cles.cle("civitai"):
            try:
                protege = protege or (await client.head(f["downloadUrl"])).status_code in (401, 403)
            except httpx.HTTPError:
                pass  # réseau capricieux : le téléchargement le dira

    type_civitai = version.get("model", {}).get("type")
    base = version.get("baseModel")
    return Fiche(
        source="civitai",
        nom=f["name"],
        taille=round(f["sizeKB"] * 1024),
        sha256=(f.get("hashes") or {}).get("SHA256", "").lower() or None,
        url=f["downloadUrl"],
        page=page(version["modelId"], version["id"], version.get("nsfwLevel")),
        dossier=TYPES.get(type_civitai) or _dossier_famille(type_civitai, base, version.get("model", {}).get("name", "")),
        licence=None,  # Civitai n'expose pas de licence nommée : voir la page du modèle
        jeton_requis=protege and not cles.cle("civitai"),
        mots_declencheurs=[m.strip() for m in version.get("trainedWords") or [] if m.strip()],
        base=base,
        **(usage_auteur(version, f["name"]) if type_civitai in TYPES_LORA else {}),
        steps_suggeres=None if type_civitai in TYPES_LORA else steps.suggerer(
            {"nom du fichier": f["name"], "nom de la version": version.get("name") or "",
             "description de la version": version.get("description") or ""}),
    )


TYPES_LORA = ("LORA", "LoCon", "DoRA")


def _dossier_famille(type_civitai: str | None, base: str | None, nom_modele: str,
                     familles: dict | None = None) -> str | None:
    """Propose <nature>/<Famille>/loras ou …/modeles d'après la base Civitai (bloc civitai des fiches de workflow) ;
    à confirmer avant téléchargement."""
    from app import modeles, workflows  # workflows importe modeles, qui ne doit pas dépendre des sources

    famille = workflows.famille_civitai(base, nom_modele, familles)
    if famille is None:
        return None
    if type_civitai in TYPES_LORA:
        return modeles.dossier_famille(famille, "loras")
    if type_civitai == "Checkpoint":
        return modeles.dossier_famille(famille)
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
        "page": page(version["modelId"], version["id"], version.get("nsfwLevel")),
        "url": (fichier or {}).get("downloadUrl") or f"https://civitai.com/api/download/models/{version['id']}",
        "licence": None,
        "mots_declencheurs": [m.strip() for m in version.get("trainedWords") or [] if m.strip()],
        "base": version.get("baseModel"),
        "nom_modele": modele.get("name"),
        "version": version.get("name"),
        "type": modele.get("type"),
        **(usage_auteur(version, (fichier or {}).get("name")) if modele.get("type") in TYPES_LORA else {}),
    }
