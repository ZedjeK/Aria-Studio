"""Ce que les routes d'Aria partagent (app/main.py, app/routes_*.py) : le pilote des moteurs, la file du Plateau, le
téléchargeur, le banc du Casting en cours, et deux contrôles communs à plusieurs routes. Les routes y accèdent par
`services.xxx` : un test qui remplace l'orchestrateur ici le remplace partout."""

from fastapi import HTTPException

from app import casting, modeles, plateau, retouche, rushes, sequences, voix, workflows
from app.orchestrateur import Orchestrateur
from app.securite import ErreurSecurite
from app.telechargement import Telechargeur

telechargeur = Telechargeur()
orchestrateur = Orchestrateur()
plateau_ = plateau.Plateau(orchestrateur)
banc_actif: casting.Banc | None = None  # le banc du Casting en cours (lancé par main.lancer_banc)


def banc_en_cours() -> bool:
    return bool(banc_actif and banc_actif.etat in ("attente", "en_cours"))


def verifier_llm(modele: str | None):
    """Une demande au LLM : pas pendant un banc, et un GGUF présent dans modeles/llm."""
    if banc_en_cours():
        raise HTTPException(409, "Un banc est en cours")
    if modele and modele not in modeles.llm_disponibles():
        raise HTTPException(400, f"GGUF absent de modeles/llm : {modele}")


def commander(action):
    """Met une commande dans la file du Plateau (image, retouche, clip…) ; ses refus deviennent des réponses HTTP."""
    if banc_en_cours():
        raise HTTPException(409, "Un banc est en cours")
    try:
        return action().to_dict()
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (plateau.ErreurPlateau, retouche.ErreurRetouche, workflows.ErreurWorkflow, rushes.ErreurRushes,
            sequences.ErreurSequence, voix.ErreurVoix) as e:
        raise HTTPException(400, str(e))
