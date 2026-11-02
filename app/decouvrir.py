"""Rechercher (onglet Modèles), « à la LM Studio » : modèles image, modèles vidéo, LoRA et LLM sur Civitai et
Hugging Face.

Les types suivent les familles des fiches de workflow (app/workflows.familles) : un modèle image ou vidéo de
Civitai n'est proposé que si sa base va dans une famille d'Aria (bloc « civitai » des fiches), et son dossier est
déduit de cette famille. Un résultat déjà installé (même SHA-256, ou même nom de fichier sur Hugging Face) porte
un badge. Seuls les fichiers .safetensors et .gguf sont proposés au téléchargement (les autres formats peuvent
exécuter du code au chargement). Le téléchargement lui-même passe par le gestionnaire.
"""

import re
from pathlib import PurePosixPath

import httpx

from app import mesures, modeles, workflows
from app.config import CONFIG
from app.sources import EXTENSIONS_AUTORISEES, civitai, huggingface

CIVITAI = "https://civitai.com/api/v1"
HF = "https://huggingface.co"
GO = 2**30
TYPES = {"image": "Modèles image", "video": "Modèles vidéo", "lora": "LoRA", "llm": "LLM (GGUF)"}
TRIS_CIVITAI = {"telechargements": "Most Downloaded", "recents": "Newest", "notes": "Highest Rated"}
TRIS_HF = {"telechargements": "downloads", "recents": "lastModified", "notes": "likes", "tendances": "trendingScore"}
SURCOUT_LLM = 1.5 * GO  # contexte 4096 + tampons de calcul, au-delà du poids du fichier
_MORCEAU = re.compile(r"-\d{5}-of-\d{5}\.gguf$")  # GGUF découpé en plusieurs fichiers


class ErreurDecouvrir(ValueError):
    pass


def _type(type_: str) -> str:
    if type_ not in TYPES:
        raise ErreurDecouvrir(f"Type inconnu : {type_}")
    return type_


def familles(type_: str) -> list[dict]:
    """Les familles proposées pour un type : celles de sa nature (image, vidéo), toutes pour les LoRA."""
    _type(type_)
    if type_ == "llm":
        return []
    return [{"famille": nom, "nature": f["nature"], "bases": f["bases"]}
            for nom, f in workflows.familles().items() if type_ == "lora" or f["nature"] == type_]


def _bases(type_: str, famille: str | None, infos: dict) -> list[str]:
    if famille:
        if famille not in infos:
            raise ErreurDecouvrir(f"Famille inconnue : {famille}")
        return infos[famille]["bases"]
    return sorted({b for f in infos.values() if type_ == "lora" or f["nature"] == type_ for b in f["bases"]})


def _apercu(version: dict) -> dict | None:
    images = [i for i in version.get("images") or [] if i.get("type", "image") == "image"]
    if not images:
        return None
    image = images[0]
    # Le CDN de Civitai sert l'original en pleine résolution : on demande une vignette.
    url = image["url"].replace("/original=true/", "/width=450/")
    return {"url": url, "nsfw": (image.get("nsfwLevel") or 1) >= 4}


async def chercher_civitai(texte: str, type_: str, famille: str | None, tri: str, curseur: str | None) -> dict:
    if _type(type_) == "llm":
        raise ErreurDecouvrir("Les LLM se cherchent sur Hugging Face")
    infos = workflows.familles()
    bases = _bases(type_, famille, infos)
    if not bases:
        return {"resultats": [], "curseur": None}  # aucune famille de ce type n'est connue de Civitai
    types_civitai = civitai.TYPES_LORA if type_ == "lora" else ("Checkpoint",)
    params = [("types", t) for t in types_civitai]
    params += [("sort", TRIS_CIVITAI.get(tri, "Most Downloaded")), ("limit", "20")]
    params += [("baseModels", b) for b in bases]
    if texte.strip():
        params.append(("query", texte.strip()))
    if curseur:
        params.append(("cursor", curseur))
    async with httpx.AsyncClient(timeout=30, headers=civitai.entetes()) as client:
        r = await client.get(f"{CIVITAI}/models", params=params)
    if r.status_code != 200:
        raise ErreurDecouvrir(f"Civitai a répondu {r.status_code}")
    donnees = r.json()
    installes = modeles.empreintes_installees()

    resultats = []
    for m in donnees.get("items", []):
        versions = []
        for v in m.get("modelVersions") or []:
            dossier = civitai._dossier_famille(m.get("type"), v.get("baseModel"), m.get("name", ""), infos)
            if dossier is None or (famille and not dossier.startswith(f"{famille}/")):
                continue  # base sans famille dans Aria (ex. « Qwen » sans « edit ») : pas utilisable ici
            fichiers = []
            for f in v.get("files") or []:
                if PurePosixPath(f["name"]).suffix.lower() not in EXTENSIONS_AUTORISEES:
                    continue
                sha = ((f.get("hashes") or {}).get("SHA256") or "").lower()
                fichiers.append({"nom": f["name"], "taille": round(f["sizeKB"] * 1024), "principal": bool(f.get("primary")),
                                 "installe": bool(sha) and sha in installes})
            if not fichiers:
                continue
            versions.append({
                "id": v["id"],
                "nom": v.get("name"),
                "base": v.get("baseModel"),
                "famille": dossier.split("/")[0],
                "mots_declencheurs": v.get("trainedWords") or [],
                "fichiers": fichiers,
                "lien": f"https://civitai.com/api/download/models/{v['id']}",
                "dossier": dossier,
                "apercu": _apercu(v),
            })
        if not versions:
            continue  # rien de téléchargeable sans risque, ou rien pour une famille d'Aria
        resultats.append({
            "source": "civitai",
            "id": m["id"],
            "nom": m["name"],
            "auteur": (m.get("creator") or {}).get("username"),
            "type": type_,
            "famille": versions[0]["famille"],
            # Le fichier principal seulement : une page Civitai liste aussi l'encodeur ou le VAE qu'on a déjà.
            "installe": any(f["installe"] for v in versions for f in v["fichiers"]
                            if f["principal"] or len(v["fichiers"]) == 1),
            "telechargements": (m.get("stats") or {}).get("downloadCount", 0),
            "appreciations": (m.get("stats") or {}).get("thumbsUpCount", 0),
            "page": f"https://civitai.com/models/{m['id']}",
            "apercu": versions[0]["apercu"],
            "versions": versions,
        })
    return {"resultats": resultats, "curseur": (donnees.get("metadata") or {}).get("nextCursor")}


def _requetes_hf(type_: str, famille: str | None) -> list[tuple[dict, str | None]]:
    """(filtres, famille) de chaque requête : les dérivés des dépôts d'origine des familles d'Aria (tag
    « base_model »), ceux de la famille choisie seulement ; les LoRA par leur tag « base_model:adapter »."""
    if type_ == "llm":
        return [({"filter": "gguf"}, None)]
    infos = workflows.familles()
    if famille and famille not in infos:
        raise ErreurDecouvrir(f"Famille inconnue : {famille}")
    choisies = [famille] if famille else [n for n, f in infos.items() if type_ == "lora" or f["nature"] == type_]
    prefixe = "base_model:adapter:" if type_ == "lora" else "base_model:"
    return [({"filter": f"{prefixe}{depot}"}, nom) for nom in choisies for depot in infos[nom]["hf"]]


def _utilisable_hf(m: dict, type_: str) -> bool:
    """Un modèle de diffusion doit être en safetensors et ne pas être une LoRA (et l'inverse pour les LoRA)."""
    if type_ == "llm":
        return True
    tags = m.get("tags") or []
    lora = "lora" in tags or any(t.startswith("base_model:adapter:") for t in tags)
    return lora if type_ == "lora" else not lora and "gguf" not in tags


async def chercher_hf(texte: str, type_: str, tri: str, famille: str | None = None) -> dict:
    _type(type_)
    base = {"sort": TRIS_HF.get(tri, "downloads"), "direction": "-1", "limit": "30"}
    if texte.strip():
        base["search"] = texte.strip()
    vus = {}
    async with httpx.AsyncClient(timeout=30, headers=huggingface.entetes()) as client:
        for filtres, nom_famille in _requetes_hf(type_, famille):
            r = await client.get(f"{HF}/api/models", params={**base, **filtres})
            if r.status_code != 200:
                raise ErreurDecouvrir(f"Hugging Face a répondu {r.status_code}")
            for m in r.json():
                if _utilisable_hf(m, type_):
                    vus.setdefault(m["id"], {**m, "famille": nom_famille})
    cle_tri = {"telechargements": "downloads", "notes": "likes"}.get(tri)
    trouves = sorted(vus.values(), key=lambda m: m.get(cle_tri) or 0, reverse=True) if cle_tri else list(vus.values())
    return {"resultats": [{
        "source": "huggingface",
        "id": m["id"],
        "nom": m["id"],
        "auteur": m["id"].split("/")[0],
        "type": type_,
        "famille": m["famille"],
        "telechargements": m.get("downloads", 0),
        "appreciations": m.get("likes", 0),
        "date": m.get("lastModified"),
        "page": f"{HF}/{m['id']}",
        "protege": bool(m.get("gated")),
    } for m in trouves], "curseur": None}


def _verdict_vram(taille: int, libre: int) -> str:
    besoin = taille + SURCOUT_LLM
    if besoin <= libre * 0.9:
        return "tient"
    if besoin <= libre:
        return "juste"
    return "deborde"


_SOUS_MODELES = {"diffusion_models", "checkpoints", "unet", "transformer"}


def _dossier_hf(chemin: str, type_: str, famille: str | None) -> str | None:
    """Le dossier d'Aria d'un fichier HF : son sous-dossier dans le dépôt d'abord (vae/, loras/… des dépôts
    « repackaged » pour ComfyUI), sinon le type cherché et la famille."""
    if type_ == "llm":
        return "llm"
    parties = [p.lower() for p in PurePosixPath(chemin).parts[:-1]]
    for p in reversed(parties):
        if p in ("loras", "lora"):
            return f"{famille}/loras" if famille else None
        if p in _SOUS_MODELES:  # avant _base : diffusion_models et checkpoints vont dans la famille
            return f"{famille}/modeles" if famille else None
        if p in modeles.TYPES_BASE:
            return f"_base/{p}"
    if famille:
        return f"{famille}/{'loras' if type_ == 'lora' else 'modeles'}"
    return None  # à choisir : Hugging Face ne dit pas à quelle famille un fichier appartient


async def fichiers_hf(depot: str, type_: str, famille: str | None = None) -> dict:
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
    installes = {m["nom"] for m in modeles.inventaire()}
    fichiers = []
    for f in r.json():
        chemin = f.get("path", "")
        suffixe = PurePosixPath(chemin).suffix.lower()
        if f.get("type") != "file" or suffixe != (".gguf" if type_ == "llm" else ".safetensors"):
            continue  # un LLM pour llama.cpp (GGUF) ; un modèle de diffusion ou une LoRA pour ComfyUI (safetensors)
        nom_bas = chemin.lower()
        if suffixe == ".gguf" and ("mmproj" in nom_bas or _MORCEAU.search(chemin)
                                   or "mtp" in PurePosixPath(nom_bas).parts[0] or PurePosixPath(nom_bas).name.startswith("mtp")):
            continue  # projecteurs vision, têtes MTP, morceaux de GGUF découpés : pas des modèles autonomes
        taille = f.get("size") or (f.get("lfs") or {}).get("size", 0)
        fichiers.append({
            "nom": chemin,
            "taille": taille,
            "lien": f"{HF}/{depot}/blob/main/{chemin}",
            "dossier": _dossier_hf(chemin, type_, famille),
            "vram": _verdict_vram(taille, libre) if suffixe == ".gguf" else None,
            "installe": PurePosixPath(chemin).name in installes,
        })
    fichiers.sort(key=lambda x: x["taille"])
    return {"fichiers": fichiers, "vram_libre_go": round(libre / GO, 1), "vram_totale_go": round(vram_totale / GO, 1)}
