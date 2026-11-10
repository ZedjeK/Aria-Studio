"""Routes du Montage (onglet 🎞️) : clips, séquences et voix de référence. Ce qu'elles partagent
avec les autres routes est dans app/services.py."""


from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse
from pydantic import BaseModel

from app import casting, plateau, prompt, retouche, rushes, sequences, services, voix
from app.securite import ErreurSecurite

routeur = APIRouter()

class DemandeClip(BaseModel):
    idee: str = ""
    prompt: str = ""  # tourné tel quel : le LLM n'écrit qu'au Scénario
    mode: str = "animer"  # animer (un rush) ou texte
    source: str | None = None  # rush à animer
    qualite: str = "compromis"  # rapide, compromis, qualite
    duree: int = 5
    format: str = "16:9 (Widescreen)"  # texte → vidéo ; animer : un format (recadrage au centre) ou « image source »
    seed: int | None = None
    nombre: int = 1
    dossier: str | None = None
    modele: str | None = None  # checkpoint de la famille ; absent = le modèle par défaut des réglages
    loras: list[dict] = []  # [{fichier, force, declencheurs}] de modeles/video/MiniMax-H3/loras
    ecrit_par: str | None = None  # le LLM qui a écrit le prompt au Scénario (noté sur la fiche)


@routeur.post("/api/clips")
def tourner_clip(d: DemandeClip):
    if not d.prompt.strip():
        raise HTTPException(400, "Le prompt est vide : écris-le ici, ou au ✍️ Scénario")
    return services.commander(lambda: services.plateau_.commander_clip(
        d.idee or d.prompt, d.prompt, d.mode, d.source, d.qualite, d.duree, d.format, d.seed, d.nombre,
        rushes.valider_dossier(d.dossier, "clips"), d.modele, d.loras, d.ecrit_par))


# Montage, mode Séquence

class DemandeSequence(BaseModel):
    titre: str
    qualite: str = "compromis"
    format: str = "16:9 (Widescreen)"  # si le premier plan ne part pas d'un rush
    # {personnage: rush, voix: "auto" | clip | "voix:<id>" | None, ambiance: texte, contrainte: texte,
    #  reference_partout: bool}
    casting: dict = {}
    plans: list[dict] = []  # [{id, idee, prompt, duree, enchainement, rush, seed, ambiance}]
    llm: str | None = None
    modele: str | None = None  # checkpoint ; absent = défaut des réglages (création) ou celui déjà choisi
    contrainte_en: dict | None = None  # {fr, en} : traduction faite au découpage du Scénario
    loras: list[dict] = []  # [{fichier, force, declencheurs}] : les mêmes sur tous les plans


class DemandeTournageSequence(BaseModel):
    plans: list[int] | None = None  # None : les plans pas encore à jour


def _sequence(action):
    try:
        return action()
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (sequences.ErreurSequence, plateau.ErreurPlateau, rushes.ErreurRushes) as e:
        raise HTTPException(404 if "inconnue" in str(e) else 400, str(e))


@routeur.get("/api/sequences")
def lister_sequences():
    return sequences.liste()


@routeur.post("/api/sequences")
def creer_sequence(d: DemandeSequence):
    return _sequence(lambda: sequences.creer(d.model_dump()))


@routeur.get("/api/sequences/{id_sequence}")
def lire_sequence(id_sequence: str):
    return _sequence(lambda: sequences.public(sequences.lire(id_sequence)))


@routeur.put("/api/sequences/{id_sequence}")
def modifier_sequence(id_sequence: str, d: DemandeSequence):
    return _sequence(lambda: sequences.modifier(id_sequence, d.model_dump()))


@routeur.delete("/api/sequences/{id_sequence}")
def supprimer_sequence(id_sequence: str):
    _sequence(lambda: sequences.supprimer(id_sequence))
    return {"ok": True}


@routeur.post("/api/sequences/{id_sequence}/tourner")
def tourner_sequence(id_sequence: str, d: DemandeTournageSequence):
    return services.commander(lambda: services.plateau_.commander_sequence(id_sequence, d.plans))


# Voix de référence des séquences (bibliothèque data/voix)

class DemandeEssaiVoix(BaseModel):
    phrase: str = ""  # vide : la phrase par défaut
    description: str = ""  # qui parle et comment (obligatoire sans personnage)
    personnage: str | None = None  # rush du personnage (le casting de la séquence)
    seed: int | None = None


class DemandeImportVoix(BaseModel):
    audio: str  # data URL
    nom: str
    nom_origine: str = ""


class DemandeVoix(BaseModel):
    nom: str


def _voix(action):
    try:
        return action()
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (voix.ErreurVoix, plateau.ErreurPlateau, sequences.ErreurSequence) as e:
        raise HTTPException(404 if "inconnue" in str(e) else 400, str(e))


@routeur.get("/api/voix")
def lister_voix():
    return voix.liste()


@routeur.post("/api/voix/essais")
def essayer_voix(d: DemandeEssaiVoix):
    """Un essai de voix dans la file du Plateau ; la commande terminée donne le brouillon (champ voix)."""
    return services.commander(lambda: services.plateau_.commander_voix(d.phrase, d.description, d.personnage, d.seed))


@routeur.post("/api/voix/import")
def importer_voix(d: DemandeImportVoix):
    def action():
        try:
            son = retouche.depuis_data_url(d.audio)
        except retouche.ErreurRetouche:
            raise voix.ErreurVoix("Données audio invalides")
        return voix.importer(son, d.nom, d.nom_origine)
    return _voix(action)


@routeur.get("/api/voix/{id_voix}")
def lire_voix(id_voix: str):
    return _voix(lambda: voix.lire(id_voix))


@routeur.put("/api/voix/{id_voix}")
def garder_voix(id_voix: str, d: DemandeVoix):
    """Garde un essai sous ce nom, ou renomme une voix."""
    return _voix(lambda: voix.garder(id_voix, d.nom))


@routeur.delete("/api/voix/{id_voix}")
def supprimer_voix(id_voix: str):
    """À la corbeille ; renvoie les séquences repassées en Auto."""
    return {"sequences": _voix(lambda: voix.supprimer(id_voix))}


@routeur.get("/api/voix/{id_voix}/son")
def son_voix(id_voix: str):
    return _voix(lambda: FileResponse(voix.chemin_son(id_voix), media_type=voix.type_mime(id_voix)))
