"""Registre, inventaire et suppression des modèles du dossier modeles/.

Rangement :
    modeles/<Famille>/modeles/   variantes du modèle de diffusion d'une famille (ex. Krea2, Qwen-Image2.1)
    modeles/<Famille>/loras/     LoRAs compatibles avec cette famille
    modeles/_base/<type>/        fichiers communs : encodeurs de texte, VAE, LoRAs internes aux workflows…
    modeles/llm/                 GGUF pour llama.cpp
Les familles sont celles déclarées par les fiches de workflow ; déposer un fichier au bon endroit suffit.
"""

import json
import shutil
from datetime import datetime
from pathlib import Path

import yaml

from app.config import CONFIG, ROOT, chemin

DOSSIER = chemin(CONFIG["modeles"]["dossier"])
REGISTRE = ROOT / "data" / "modeles.json"
CHEMINS_COMFYUI = ROOT / "data" / "comfyui_chemins.yaml"
TYPES_BASE = ["checkpoints", "diffusion_models", "text_encoders", "vae", "loras",
              "clip_vision", "controlnet", "upscale_models", "model_patches", "audio_encoders",
              "latent_upscale_models"]
EXTENSIONS_IMAGE = {".safetensors"}  # jamais de .ckpt/.pt : ils peuvent exécuter du code au chargement


class ErreurModele(ValueError):
    pass


def familles() -> list[str]:
    """Familles déclarées par les fiches de workflow (lues sans importer app.workflows)."""
    noms = set()
    for f in (ROOT / "workflows").glob("*/fiche.yaml"):
        famille = (yaml.safe_load(f.read_text(encoding="utf-8")) or {}).get("famille")
        if famille:
            noms.add(famille)
    return sorted(noms)


def dossiers() -> list[str]:
    """Sous-dossiers où l'appli peut ranger un fichier."""
    return ([f"_base/{t}" for t in TYPES_BASE]
            + [f"{f}/{sous}" for f in familles() for sous in ("modeles", "loras")]
            + ["llm"])


def fichiers(dossier: str, extensions: set[str]) -> list[str]:
    """Noms des fichiers déposés dans un sous-dossier (sans les .part en cours de téléchargement)."""
    racine = DOSSIER / dossier
    if not racine.is_dir():
        return []
    return sorted(p.name for p in racine.iterdir() if p.is_file() and p.suffix.lower() in extensions)


def llm_disponibles() -> list[str]:
    return fichiers("llm", {".gguf"})


# LoRAs : mots déclencheurs et force retenus d'une utilisation à l'autre (data/loras.json),
# à défaut ceux fournis par la source au téléchargement (registre).
MEMOIRE_LORAS = ROOT / "data" / "loras.json"


def _memoire_loras() -> dict:
    return json.loads(MEMOIRE_LORAS.read_text(encoding="utf-8")) if MEMOIRE_LORAS.is_file() else {}


def loras(famille: str) -> list[dict]:
    registre, memoire = charger_registre(), _memoire_loras()
    liste = []
    for nom in fichiers(f"{famille}/loras", EXTENSIONS_IMAGE):
        cle_lora = f"{famille}/loras/{nom}"
        retenu = memoire.get(cle_lora, {})
        source = (registre.get(cle_lora) or {}).get("mots_declencheurs") or []
        liste.append({
            "fichier": nom,
            "nom": Path(nom).stem,
            "declencheurs": retenu.get("declencheurs", ", ".join(source)),
            "force": retenu.get("force", 1.0),
            "page": (registre.get(cle_lora) or {}).get("page"),
        })
    return liste


def memoriser_loras(famille: str, choix: list[dict]):
    memoire = _memoire_loras()
    for c in choix:
        memoire[f"{famille}/loras/{c['fichier']}"] = {"declencheurs": c["declencheurs"], "force": c["force"]}
    MEMOIRE_LORAS.parent.mkdir(parents=True, exist_ok=True)
    MEMOIRE_LORAS.write_text(json.dumps(memoire, indent=4, ensure_ascii=False), encoding="utf-8")


def ecrire_chemins_comfyui() -> Path:
    """Déclare à ComfyUI les dossiers de modèles, familles comprises (relu à chaque démarrage)."""
    for f in familles():
        for sous in ("modeles", "loras"):
            (DOSSIER / f / sous).mkdir(parents=True, exist_ok=True)
    declaration = {
        "aria_base": {"base_path": str(DOSSIER / "_base"), "is_default": True, **{t: f"{t}/" for t in TYPES_BASE}},
        "aria_familles": {
            "base_path": str(DOSSIER),
            "diffusion_models": "\n".join(f"{f}/modeles/" for f in familles()),
            "loras": "\n".join(f"{f}/loras/" for f in familles()),
        },
    }
    CHEMINS_COMFYUI.parent.mkdir(parents=True, exist_ok=True)
    CHEMINS_COMFYUI.write_text(yaml.safe_dump(declaration, allow_unicode=True, sort_keys=False), encoding="utf-8")
    return CHEMINS_COMFYUI


def destination(dossier: str, nom: str) -> Path:
    if dossier not in dossiers():
        raise ErreurModele(f"Dossier inconnu : {dossier}")
    if nom in ("", ".", "..") or "/" in nom or "\\" in nom or ":" in nom:
        raise ErreurModele(f"Nom de fichier invalide : {nom}")
    return DOSSIER / dossier / nom


def cle(chemin_fichier: Path) -> str:
    return chemin_fichier.relative_to(DOSSIER).as_posix()


def charger_registre() -> dict[str, dict]:
    if not REGISTRE.is_file():
        return {}
    return json.loads(REGISTRE.read_text(encoding="utf-8"))


def _sauver_registre(registre: dict[str, dict]):
    REGISTRE.parent.mkdir(parents=True, exist_ok=True)
    temporaire = REGISTRE.with_suffix(".json.tmp")
    temporaire.write_text(json.dumps(registre, indent=4, ensure_ascii=False), encoding="utf-8")
    temporaire.replace(REGISTRE)


def enregistrer(fichier: Path, fiche: dict, sha256: str):
    registre = charger_registre()
    registre[cle(fichier)] = {
        "source": fiche["source"],
        "page": fiche["page"],
        "url": fiche["url"],
        "licence": fiche["licence"],
        "mots_declencheurs": fiche.get("mots_declencheurs") or [],
        "base": fiche.get("base"),
        "taille": fichier.stat().st_size,
        "sha256": sha256,
        "date": datetime.now().isoformat(timespec="seconds"),
    }
    _sauver_registre(registre)


def inventaire() -> list[dict]:
    registre = charger_registre()
    modeles = []
    for dossier in dossiers():
        racine = DOSSIER / dossier
        if not racine.is_dir():
            continue
        for fichier in sorted(racine.rglob("*")):
            if not fichier.is_file() or fichier.suffix == ".part":
                continue
            c = cle(fichier)
            modeles.append({
                "cle": c,
                "dossier": dossier,
                "nom": fichier.name,
                "taille": fichier.stat().st_size,
                "suivi": c in registre,
                "infos": registre.get(c),
            })
    return modeles


def espace_libre() -> int:
    return shutil.disk_usage(DOSSIER).free


def supprimer(cle_modele: str):
    """Suppression définitive, réservée aux modèles téléchargés par l'appli."""
    registre = charger_registre()
    if cle_modele not in registre:
        raise ErreurModele("Seuls les modèles téléchargés par l'appli peuvent être supprimés")
    fichier = (DOSSIER / cle_modele).resolve()
    if not fichier.is_relative_to(DOSSIER.resolve()):
        raise ErreurModele("Chemin hors du dossier modeles/")
    fichier.unlink(missing_ok=True)
    del registre[cle_modele]
    _sauver_registre(registre)
