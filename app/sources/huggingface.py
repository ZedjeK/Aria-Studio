"""Fiches Hugging Face : https://huggingface.co/<owner>/<repo>/(blob|resolve)/<revision>/<chemin>."""

import os
from urllib.parse import quote, urlparse

import httpx

from app import modeles
from app.sources import ErreurSource, Fiche

HF = "https://huggingface.co"

# Noms de dossiers rencontrés dans les dépôts, ramenés aux dossiers ComfyUI.
ALIAS = {"unet": "diffusion_models", "clip": "text_encoders", "lora": "loras"}
# Ces types dépendent d'une famille (dossier <Famille>/modeles ou <Famille>/loras) : c'est à toi de la choisir.
PAR_FAMILLE = {"diffusion_models", "loras", "checkpoints"}


def entetes() -> dict:
    jeton = os.environ.get("HF_TOKEN")
    return {"Authorization": f"Bearer {jeton}"} if jeton else {}


def _analyser(url: str) -> tuple[str, str, str]:
    morceaux = [m for m in urlparse(url.strip()).path.split("/") if m]
    if len(morceaux) < 5 or morceaux[2] not in ("blob", "resolve"):
        raise ErreurSource("Lien Hugging Face attendu vers un fichier : .../<owner>/<repo>/blob/<branche>/<fichier>")
    return f"{morceaux[0]}/{morceaux[1]}", morceaux[3], "/".join(morceaux[4:])


def _deviner_dossier(chemin_fichier: str) -> str | None:
    if chemin_fichier.lower().endswith(".gguf"):
        return "llm"
    for partie in reversed(chemin_fichier.split("/")[:-1]):
        partie = ALIAS.get(partie.lower(), partie.lower())
        if partie in modeles.TYPES_BASE and partie not in PAR_FAMILLE:
            return f"_base/{partie}"
    return None


async def fiche(url: str) -> Fiche:
    depot, revision, chemin_fichier = _analyser(url)
    async with httpx.AsyncClient(timeout=20, headers=entetes()) as client:
        r = await client.get(f"{HF}/api/models/{depot}")
        if r.status_code in (401, 404):
            raise ErreurSource(f"Dépôt introuvable ou privé : {depot}")
        r.raise_for_status()
        infos = r.json()
        r = await client.post(f"{HF}/api/models/{depot}/paths-info/{quote(revision, safe='')}", json={"paths": [chemin_fichier]})
        r.raise_for_status()
        fichiers = [f for f in r.json() if f["type"] == "file"]
    if not fichiers:
        raise ErreurSource(f"Fichier introuvable dans {depot} : {chemin_fichier}")
    f = fichiers[0]

    licence = (infos.get("cardData") or {}).get("license")
    if not licence:
        licence = next((t.removeprefix("license:") for t in infos.get("tags", []) if t.startswith("license:")), None)

    return Fiche(
        source="huggingface",
        nom=chemin_fichier.split("/")[-1],
        taille=f["size"],
        sha256=(f.get("lfs") or {}).get("oid"),  # absent pour les petits fichiers hors LFS
        url=f"{HF}/{depot}/resolve/{quote(revision, safe='')}/{quote(chemin_fichier)}",
        page=f"{HF}/{depot}",
        dossier=_deviner_dossier(chemin_fichier),
        licence=licence,
        jeton_requis=bool(infos.get("gated")) and not os.environ.get("HF_TOKEN"),
    )
