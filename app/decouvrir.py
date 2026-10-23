"""Découvrir (onglet Modèles), « à la LM Studio » : recherche de modèles, LoRAs et LLM sur Civitai et Hugging Face.

Seuls les fichiers .safetensors et .gguf sont proposés au téléchargement (les autres formats
peuvent exécuter du code au chargement). Le téléchargement lui-même passe par le gestionnaire.
"""

import re
from pathlib import PurePosixPath

import httpx

from app import mesures
from app.config import CONFIG
from app.sources import EXTENSIONS_AUTORISEES, civitai, huggingface

CIVITAI = "https://civitai.com/api/v1"
HF = "https://huggingface.co"
GO = 2**30
TYPES_CIVITAI = {"lora": "LORA", "modele": "Checkpoint"}
TRIS_CIVITAI = {"telechargements": "Most Downloaded", "recents": "Newest", "notes": "Highest Rated"}
TRIS_HF = {"telechargements": "downloads", "recents": "lastModified", "notes": "likes", "tendances": "trendingScore"}
SURCOUT_LLM = 1.5 * GO  # contexte 4096 + tampons de calcul, au-delà du poids du fichier
_MORCEAU = re.compile(r"-\d{5}-of-\d{5}\.gguf$")  # GGUF découpé en plusieurs fichiers


class ErreurDecouvrir(ValueError):
    pass


def _bases_civitai(famille: str | None) -> list[str]:
    """Famille de l'appli -> valeurs « baseModel » de Civitai."""
    if not famille:
        return []
    correspondances = CONFIG.get("modeles", {}).get("familles_civitai", {})
    bases = [base for base, f in correspondances.items() if f == famille]
    return ["Qwen" if b == "Qwen (édition)" else b for b in bases]


def _apercu(version: dict) -> dict | None:
    images = [i for i in version.get("images") or [] if i.get("type", "image") == "image"]
    if not images:
        return None
    image = images[0]
    # Le CDN de Civitai sert l'original en pleine résolution : on demande une vignette.
    url = image["url"].replace("/original=true/", "/width=450/")
    return {"url": url, "nsfw": (image.get("nsfwLevel") or 1) >= 4}


async def chercher_civitai(texte: str, type_: str, famille: str | None, tri: str, curseur: str | None) -> dict:
    if type_ not in TYPES_CIVITAI:
        raise ErreurDecouvrir(f"Type inconnu pour Civitai : {type_}")
    params = [("types", TYPES_CIVITAI[type_]), ("sort", TRIS_CIVITAI.get(tri, "Most Downloaded")), ("limit", "20")]
    params += [("baseModels", b) for b in _bases_civitai(famille)]
    if texte.strip():
        params.append(("query", texte.strip()))
    if curseur:
        params.append(("cursor", curseur))
    async with httpx.AsyncClient(timeout=30, headers=civitai.entetes()) as client:
        r = await client.get(f"{CIVITAI}/models", params=params)
    if r.status_code != 200:
        raise ErreurDecouvrir(f"Civitai a répondu {r.status_code}")
    donnees = r.json()

    resultats = []
    for m in donnees.get("items", []):
        versions = []
        for v in m.get("modelVersions") or []:
            fichiers = [
                {"nom": f["name"], "taille": round(f["sizeKB"] * 1024), "principal": bool(f.get("primary"))}
                for f in v.get("files") or []
                if PurePosixPath(f["name"]).suffix.lower() in EXTENSIONS_AUTORISEES
            ]
            if not fichiers:
                continue
            versions.append({
                "id": v["id"],
                "nom": v.get("name"),
                "base": v.get("baseModel"),
                "mots_declencheurs": v.get("trainedWords") or [],
                "fichiers": fichiers,
                "lien": f"https://civitai.com/api/download/models/{v['id']}",
                "dossier": civitai._dossier_famille(m.get("type"), v.get("baseModel"), m.get("name", "")),
                "apercu": _apercu(v),
            })
        if not versions:
            continue  # rien de téléchargeable sans risque
        resultats.append({
            "source": "civitai",
            "id": m["id"],
            "nom": m["name"],
            "auteur": (m.get("creator") or {}).get("username"),
            "type": m.get("type"),
            "telechargements": (m.get("stats") or {}).get("downloadCount", 0),
            "appreciations": (m.get("stats") or {}).get("thumbsUpCount", 0),
            "page": f"https://civitai.com/models/{m['id']}",
            "apercu": versions[0]["apercu"],
            "versions": versions,
        })
    return {"resultats": resultats, "curseur": (donnees.get("metadata") or {}).get("nextCursor")}


async def chercher_hf(texte: str, type_: str, tri: str) -> dict:
    params = {"sort": TRIS_HF.get(tri, "downloads"), "direction": "-1", "limit": "30"}
    if texte.strip():
        params["search"] = texte.strip()
    if type_ == "llm":
        params["filter"] = "gguf"
    elif type_ in ("modele", "lora"):
        params["filter"] = "safetensors"
        params["pipeline_tag"] = "text-to-image"
    else:
        raise ErreurDecouvrir(f"Type inconnu pour Hugging Face : {type_}")
    async with httpx.AsyncClient(timeout=30, headers=huggingface.entetes()) as client:
        r = await client.get(f"{HF}/api/models", params=params)
    if r.status_code != 200:
        raise ErreurDecouvrir(f"Hugging Face a répondu {r.status_code}")
    return {"resultats": [{
        "source": "huggingface",
        "id": m["id"],
        "nom": m["id"],
        "auteur": m["id"].split("/")[0],
        "type": type_,
        "telechargements": m.get("downloads", 0),
        "appreciations": m.get("likes", 0),
        "date": m.get("lastModified"),
        "page": f"{HF}/{m['id']}",
        "protege": bool(m.get("gated")),
    } for m in r.json()], "curseur": None}


def _verdict_vram(taille: int, libre: int) -> str:
    besoin = taille + SURCOUT_LLM
    if besoin <= libre * 0.9:
        return "tient"
    if besoin <= libre:
        return "juste"
    return "deborde"


async def fichiers_hf(depot: str, type_: str) -> dict:
    """Fichiers téléchargeables d'un dépôt HF ; pour les GGUF, indique s'ils tiennent dans la VRAM libre."""
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", depot):
        raise ErreurDecouvrir(f"Dépôt invalide : {depot}")
    async with httpx.AsyncClient(timeout=30, headers=huggingface.entetes()) as client:
        r = await client.get(f"{HF}/api/models/{depot}/tree/main", params={"recursive": "true"})
    if r.status_code != 200:
        raise ErreurDecouvrir(f"Dépôt inaccessible ({r.status_code}) : {depot}")
    vram_totale = mesures.vram_totale()
    # Le LLM tourne seul : seule la part prise par le bureau et les autres applis compte (plafonnée,
    # pour ne pas compter un ComfyUI qui serait chargé au moment de la recherche).
    libre = vram_totale - min(mesures.vram_utilisee(), int(CONFIG["casting"]["vram_max_occupee_go"] * GO))
    fichiers = []
    for f in r.json():
        chemin = f.get("path", "")
        suffixe = PurePosixPath(chemin).suffix.lower()
        if f.get("type") != "file" or suffixe not in EXTENSIONS_AUTORISEES:
            continue
        nom_bas = chemin.lower()
        if suffixe == ".gguf" and ("mmproj" in nom_bas or _MORCEAU.search(chemin)
                                   or "mtp" in PurePosixPath(nom_bas).parts[0] or PurePosixPath(nom_bas).name.startswith("mtp")):
            continue  # projecteurs vision, têtes MTP, morceaux de GGUF découpés : pas des modèles autonomes
        taille = f.get("size") or (f.get("lfs") or {}).get("size", 0)
        fichiers.append({
            "nom": chemin,
            "taille": taille,
            "lien": f"{HF}/{depot}/blob/main/{chemin}",
            "dossier": "llm" if suffixe == ".gguf" else None,
            "vram": _verdict_vram(taille, libre) if suffixe == ".gguf" else None,
        })
    fichiers.sort(key=lambda x: x["taille"])
    return {"fichiers": fichiers, "vram_libre_go": round(libre / GO, 1), "vram_totale_go": round(vram_totale / GO, 1)}
