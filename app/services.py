"""Ce que les routes d'Aria partagent (app/main.py, app/routes_*.py) : le pilote des moteurs, la file du Plateau, le
téléchargeur, le banc du Casting en cours, et deux contrôles communs à plusieurs routes. Les routes y accèdent par
`services.xxx` : un test qui remplace l'orchestrateur ici le remplace partout."""

import asyncio

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


ANNULEE = 499  # la page a coupé la connexion (✕ Annuler, onglet fermé) : personne ne lit plus la réponse


async def tant_que_connecte(requete, travail):
    """Attend `travail` (un appel au LLM), et l'annule si la page coupe la connexion : en flux, llama-server arrête
    aussitôt sa génération (app/llm.py) au lieu d'écrire dans le vide. Sans requête (appel direct, tests) : attend."""
    tache = asyncio.ensure_future(travail)
    if requete is None:
        return await tache
    while True:
        fini, _ = await asyncio.wait({tache}, timeout=0.5)
        if fini:
            return tache.result()
        if await requete.is_disconnected():
            tache.cancel()
            await asyncio.gather(tache, return_exceptions=True)
            raise HTTPException(ANNULEE, "Demande annulée")


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
