"""Sources de modèles : une fiche décrit un fichier distant avant son téléchargement."""

from dataclasses import asdict, dataclass, field
from pathlib import PurePosixPath
from urllib.parse import urlparse

# Les formats .ckpt, .pt, .pth, .bin (pickle) peuvent exécuter du code au chargement : refusés.
EXTENSIONS_AUTORISEES = {".safetensors", ".gguf"}


@dataclass
class Fiche:
    source: str  # "huggingface" ou "civitai"
    nom: str
    taille: int
    sha256: str | None
    url: str
    page: str
    dossier: str | None  # dossier deviné, modifiable avant téléchargement
    licence: str | None
    jeton_requis: bool  # accès protégé et jeton absent de l'environnement
    mots_declencheurs: list[str] = field(default_factory=list)  # LoRAs Civitai
    base: str | None = None  # modèle de base annoncé par la source (Civitai « baseModel »)

    def to_dict(self) -> dict:
        return asdict(self)


class ErreurSource(ValueError):
    pass


async def fiche_depuis_url(url: str) -> Fiche:
    from app.sources import civitai, huggingface

    hote = urlparse(url.strip()).hostname or ""
    if hote.endswith("huggingface.co"):
        fiche = await huggingface.fiche(url)
    elif hote.endswith("civitai.com"):
        fiche = await civitai.fiche(url)
    else:
        raise ErreurSource("Lien non reconnu : seuls Hugging Face et Civitai sont pris en charge")
    if PurePosixPath(fiche.nom).suffix.lower() not in EXTENSIONS_AUTORISEES:
        raise ErreurSource(f"Format refusé ({fiche.nom}) : seuls .safetensors et .gguf sont acceptés, "
                           "les autres formats peuvent exécuter du code au chargement")
    return fiche


def entetes(source: str) -> dict:
    from app.sources import civitai, huggingface

    return {"huggingface": huggingface.entetes, "civitai": civitai.entetes}[source]()
