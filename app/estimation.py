"""Temps estimé d'un tournage du Montage : calculé d'après le checkpoint, la qualité, la durée, le nombre de clips et
la VRAM libre, sans tenir compte des clips déjà tournés.

Point de départ : le tableau de plateau.QUALITES_VIDEO, mesuré sur la RTX 4080 de l'auteur (16 Go) pour 5 s, à chaud, avec
le checkpoint d'origine. Il reste exact sur cette machine de référence ; on le corrige de ce qui change :
  - les steps du checkpoint choisi (une variante peut imposer les siens) : le temps suit les steps ;
  - la durée du clip : (D / 5)^plateau.EXPOSANT_DUREE_VIDEO, ajusté sur des clips de 15 s ;
  - la VRAM : quand le poids du checkpoint et le travail du clip dépassent la VRAM libre, ComfyUI décharge une partie en
    RAM et chaque step ralentit. Le facteur compare ta machine à la machine de référence ;
  - le nombre de clips : autant de clips, un après l'autre.
Hypothèses non mesurées : la forme du ralentissement (VRAM_COEF), la VRAM de travail d'un clip (TRAVAIL_GO_PAR_MP) et
la vitesse brute de la carte (une 4090 sera plus rapide qu'une 4080 : ce n'est pas modélisé).
"""

from app import optimiseur, plateau, workflows
from app.config import CONFIG
from app.emplacements import MODELES

EXPOSANT_DUREE = plateau.EXPOSANT_DUREE_VIDEO
GO = 2**30
LIBRE_REF_GO = 15.0  # RTX 4080 de 16 Go, bureau compris
POIDS_REF_GO = 19.5  # minimax_h3_fl2va_pruned_int8_convrot.safetensors, le checkpoint d'origine
TRAVAIL_GO_PAR_MP = 6.0  # VRAM de travail (activations) d'un clip de 5 s, par mégapixel : une hypothèse
VRAM_COEF = 1.0  # ralentissement quand toute la charge est déchargée en RAM : une hypothèse


def _decharge(besoin_go: float, libre_go: float) -> float:
    """Part de la charge qui ne tient pas en VRAM (0 : tout y tient)."""
    return max(0.0, besoin_go - libre_go) / besoin_go


def facteur_vram(poids_go: float, libre_go: float, megapixels: float, duree: float) -> float:
    """Ralentissement de cette machine, divisé par celui de la machine de référence pour le même clip : 1 sur la
    machine de référence, moins de 1 avec plus de VRAM libre ou un checkpoint plus léger, plus de 1 avec moins."""
    travail = TRAVAIL_GO_PAR_MP * megapixels * (duree / 5) ** 0.5
    ici = 1 + VRAM_COEF * _decharge(poids_go + travail, libre_go)
    ref = 1 + VRAM_COEF * _decharge(POIDS_REF_GO + travail, LIBRE_REF_GO)
    return ici / ref


def _poids_go(wf: workflows.Workflow, checkpoint: str) -> float:
    """Taille du checkpoint sur le disque, sinon l'estimation de sa fiche."""
    fichier = MODELES / wf.famille / "modeles" / checkpoint
    if fichier.is_file():
        return fichier.stat().st_size / GO
    return float(wf.materiel["vram_estimee_go"])


def _vram_libre_go() -> float | None:
    try:
        return float(optimiseur.profil_materiel()["vram_libre_go"])
    except Exception:  # pas de carte NVIDIA, NVML injoignable : on le dit, on ne devine pas
        return None


def estimer(mode: str, modele: str | None, qualite: str, duree: int, nombre: int = 1,
            libre_go: float | None = None) -> dict:
    """Secondes d'un clip et de la commande entière, avec ce qui a servi au calcul.
    `libre_go` : la VRAM libre (mesurée ici si absente). Sans mesure, le tableau d'origine seul."""
    if qualite not in plateau.QUALITES_VIDEO:
        raise ValueError(f"Qualité inconnue : {qualite}")
    if mode not in plateau.MODES_VIDEO:
        raise ValueError(f"Mode inconnu : {mode}")
    q = plateau.QUALITES_VIDEO[qualite]
    wf = workflows.obtenir(CONFIG.get("generation", {}).get(plateau.MODES_VIDEO[mode]))
    checkpoint = wf.choisi(modele)
    steps = int(reglages_variante(wf, checkpoint).get("steps") or q["steps"])
    poids = _poids_go(wf, checkpoint)
    libre = _vram_libre_go() if libre_go is None else libre_go
    clip = q["estimation_s"] * (steps / q["steps"]) * (duree / 5) ** EXPOSANT_DUREE
    facteur = 1.0
    if libre is not None:
        facteur = facteur_vram(poids, libre, q["megapixels"], duree)
        clip *= facteur
    return {"source": "calcul" if libre is not None else "tableau_d_origine",
            "clip_s": round(clip), "total_s": round(clip * nombre), "nombre": nombre,
            "checkpoint": checkpoint, "poids_go": round(poids, 1), "steps": steps,
            "vram_libre_go": None if libre is None else round(libre, 1), "facteur_vram": round(facteur, 2)}


def reglages_variante(wf: workflows.Workflow, checkpoint: str) -> dict:
    from app import reglages
    return reglages.valeurs_variante(wf, checkpoint)
