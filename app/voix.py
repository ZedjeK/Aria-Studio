"""Voix de référence des séquences : une bibliothèque de voix propres (data/voix/), réutilisables d'un film à l'autre.

Une voix vient :
    d'un essai de voix   H3 tourne un gros plan en cabine d'enregistrement capitonnée, sans musique ni ambiance,
                         où le personnage dit une phrase ; seul le son est gardé (FLAC) ;
    d'un fichier audio   un enregistrement de Jérémy (wav, mp3, flac ou ogg).
Essais réels du 2026-10-22 (data/tests/voix) : écart parole / bruit de fond de 82 dB en cabine, contre 13 dB pour
le son d'un plan tourné dans un hangar (la référence du mode Auto) ; le bruit d'une référence passe dans les plans
qui la reçoivent (14 dB, contre 48 dB avec la voix de la cabine). La référence transmet le timbre ; le ton suit
chaque plan.

Un essai reste un brouillon tant qu'il n'est pas gardé (sous un nom) ; les brouillons de plus d'un jour sont effacés.
Une voix supprimée part dans data/voix/corbeille/ ; les séquences qui l'utilisaient repassent en Auto.
AS-18 : la phrase, la description et les textes d'origine du personnage passent par securite.
"""

import json
import random
import re
import shutil
import struct
import time
import uuid
import wave
from datetime import datetime, timedelta
from io import BytesIO

from app import plateau, reglages, securite, workflows
from app.config import CONFIG, ROOT

DOSSIER = ROOT / "data" / "voix"
PREFIXE = "voix:"  # valeur du casting d'une séquence : « voix:<id> »
_ID = re.compile(r"^\d{8}-\d{6}-[0-9a-f]{6}$")

PHRASE_DEFAUT = "Bonjour. Voici ma voix, calme et posée, pour que tu l'entendes bien."
LONGUEUR_PHRASE = 200
MOTS_MAX = 25
LONGUEUR_DESCRIPTION = 300
LONGUEUR_NOM = 40
QUALITE = "rapide"  # l'image ne sert à rien : la plus petite taille
STEPS = 12  # comme les plans qui parlent (la voix a besoin de 12 steps)
# Durée de l'essai selon la phrase (environ 2,5 mots par seconde, avec de la marge).
DUREES = ((13, 5), (20, 8), (MOTS_MAX, 10))
BROUILLON_MAX = timedelta(days=1)

FORMATS_AUDIO = {"wav": "audio/wav", "mp3": "audio/mpeg", "flac": "audio/flac", "ogg": "audio/ogg"}
TAILLE_MAX = 8 * 1024 * 1024
DUREE_MIN, DUREE_MAX = 3, 15  # au-delà de 15 s, la référence est recadrée sur ses 15 premières secondes

# Repris de l'essai V2 (le plus propre).
SILENCE = ("The room is completely silent: no music, no ambient sound, no machine noise, no echo, no breathing noise. "
           "Only the character's clear voice, close to the microphone.")


class ErreurVoix(ValueError):
    pass


# Bibliothèque

def _chemin(id_voix: str):
    if not _ID.match(id_voix or "") or not (DOSSIER / f"{id_voix}.json").is_file():
        raise ErreurVoix(f"Voix inconnue : {id_voix}")
    return DOSSIER / f"{id_voix}.json"


def lire(id_voix: str) -> dict:
    return json.loads(_chemin(id_voix).read_text(encoding="utf-8"))


def _ecrire(v: dict):
    DOSSIER.mkdir(parents=True, exist_ok=True)
    temporaire = DOSSIER / f"{v['id']}.tmp"
    temporaire.write_text(json.dumps(v, indent=4, ensure_ascii=False), encoding="utf-8")
    temporaire.replace(DOSSIER / f"{v['id']}.json")


def chemin_son(id_voix: str):
    v = lire(id_voix)
    return DOSSIER / v["fichier"]


def type_mime(id_voix: str) -> str:
    return FORMATS_AUDIO[lire(id_voix)["fichier"].rsplit(".", 1)[1]]


def liste(brouillons: bool = False) -> list[dict]:
    """Voix gardées, les plus récentes d'abord (et les brouillons si demandé)."""
    _nettoyer()
    if not DOSSIER.is_dir():
        return []
    voix = [json.loads(f.read_text(encoding="utf-8")) for f in DOSSIER.glob("*.json")]
    return sorted((v for v in voix if v["gardee"] or brouillons), key=lambda v: v["date"], reverse=True)


def gardee(id_voix: str) -> dict:
    """La voix, si elle est dans la bibliothèque (un brouillon ne sert pas de référence)."""
    v = lire(id_voix)
    if not v["gardee"]:
        raise ErreurVoix("Garde d'abord cet essai de voix sous un nom")
    return v


def _nom_valide(nom: str) -> str:
    nom = " ".join(str(nom or "").split())
    if not nom or len(nom) > LONGUEUR_NOM:
        raise ErreurVoix(f"Nom de 1 à {LONGUEUR_NOM} caractères")
    return nom


def garder(id_voix: str, nom: str) -> dict:
    """Garde un essai dans la bibliothèque sous ce nom (ou renomme une voix déjà gardée)."""
    v = lire(id_voix)
    v.update(nom=_nom_valide(nom), gardee=True)
    _ecrire(v)
    return v


def supprimer(id_voix: str) -> list[str]:
    """Met la voix à la corbeille (data/voix/corbeille) ; renvoie les séquences repassées en Auto."""
    from app import sequences  # sequences importe voix
    v = lire(id_voix)
    corbeille = DOSSIER / "corbeille"
    corbeille.mkdir(parents=True, exist_ok=True)
    for f in (DOSSIER / v["fichier"], _chemin(id_voix)):
        if f.is_file():
            shutil.move(str(f), corbeille / f.name)
    return sequences.oublier_voix(f"{PREFIXE}{id_voix}", v.get("nom") or "sans nom")


def _nettoyer():
    """Efface les essais jamais gardés de plus de BROUILLON_MAX."""
    if not DOSSIER.is_dir():
        return
    limite = datetime.now() - BROUILLON_MAX
    for f in DOSSIER.glob("*.json"):
        v = json.loads(f.read_text(encoding="utf-8"))
        if not v["gardee"] and datetime.fromisoformat(v["date"]) < limite:
            (DOSSIER / v["fichier"]).unlink(missing_ok=True)
            f.unlink()


def _nouvelle(son: bytes, extension: str, infos: dict) -> dict:
    DOSSIER.mkdir(parents=True, exist_ok=True)
    id_voix = f"{datetime.now():%Y%m%d-%H%M%S}-{uuid.uuid4().hex[:6]}"
    (DOSSIER / f"{id_voix}.{extension}").write_bytes(son)
    v = {"id": id_voix, "date": datetime.now().isoformat(timespec="seconds"), "fichier": f"{id_voix}.{extension}",
         "duree_son": duree_audio(son, extension), **infos}
    _ecrire(v)
    return v


def textes(id_voix: str) -> list[str]:
    """AS-18 : ce qui a fait la voix (phrase, description, textes d'origine du personnage de l'essai)."""
    v = lire(id_voix)
    personnage = [v["personnage"]] if v.get("personnage") else []
    return [t for t in (v.get("phrase"), v.get("description")) if t] + plateau.textes_origine(personnage)


def options() -> dict:
    return {"phrase_defaut": PHRASE_DEFAUT, "longueur_phrase": LONGUEUR_PHRASE, "mots_max": MOTS_MAX,
            "longueur_description": LONGUEUR_DESCRIPTION, "longueur_nom": LONGUEUR_NOM,
            "formats": sorted(FORMATS_AUDIO), "duree_min": DUREE_MIN, "duree_max": DUREE_MAX,
            "estimation_s": round(plateau.QUALITES_VIDEO[QUALITE]["estimation_s"] * 1.4)}


# Fichier audio de Jérémy

def format_audio(son: bytes) -> str | None:
    """Format d'après les premiers octets (jamais d'après le nom)."""
    if son[:4] == b"RIFF" and son[8:12] == b"WAVE":
        return "wav"
    if son[:4] == b"fLaC":
        return "flac"
    if son[:4] == b"OggS":
        return "ogg"
    if son[:3] == b"ID3" or (len(son) > 1 and son[0] == 0xFF and son[1] & 0xE0 == 0xE0):
        return "mp3"
    return None


def duree_audio(son: bytes, format_: str) -> float | None:
    """Durée en secondes quand elle se lit sans décodeur (wav, flac) ; None sinon."""
    try:
        if format_ == "wav":
            with wave.open(BytesIO(son)) as w:
                return round(w.getnframes() / w.getframerate(), 2)
        if format_ == "flac":
            # STREAMINFO, premier bloc après « fLaC » : fréquence sur 20 bits, nombre d'échantillons sur 36 bits.
            infos = struct.unpack(">Q", son[18:26])[0]
            frequence, echantillons = infos >> 44, infos & ((1 << 36) - 1)
            return round(echantillons / frequence, 2) if frequence and echantillons else None
    except (wave.Error, EOFError, struct.error):
        pass
    return None


def importer(son: bytes, nom: str, nom_origine: str = "") -> dict:
    """Une voix enregistrée par Jérémy, gardée aussitôt dans la bibliothèque."""
    nom = _nom_valide(nom)
    if len(son) > TAILLE_MAX:
        raise ErreurVoix(f"Fichier audio de {TAILLE_MAX // (1024 * 1024)} Mo au plus")
    format_ = format_audio(son)
    if not format_:
        raise ErreurVoix(f"Format audio accepté : {', '.join(sorted(FORMATS_AUDIO))}")
    duree = duree_audio(son, format_)
    if duree is not None and duree < DUREE_MIN:
        raise ErreurVoix(f"Il faut au moins {DUREE_MIN} s de voix (ce fichier en a {duree:g})")
    return _nouvelle(son, format_, {"nom": nom, "gardee": True, "source": "fichier", "nom_origine": nom_origine[:120],
                                    "phrase": None, "description": None, "langue": None, "personnage": None,
                                    "seed": None, "duree_s": 0})


# Essai de voix (tourné par H3, dans la file du Plateau)

def valider_essai(phrase: str, description: str, personnage: str | None) -> tuple[str, str, int]:
    """(phrase, description, durée du clip en s) ; refuse ce qui ne se tournerait pas."""
    phrase = " ".join(str(phrase or "").split()) or PHRASE_DEFAUT
    description = " ".join(str(description or "").split())
    if len(phrase) > LONGUEUR_PHRASE or len(phrase.split()) > MOTS_MAX:
        raise ErreurVoix(f"Phrase d'essai de {MOTS_MAX} mots et {LONGUEUR_PHRASE} caractères au plus")
    if len(description) > LONGUEUR_DESCRIPTION:
        raise ErreurVoix(f"Description de {LONGUEUR_DESCRIPTION} caractères au plus")
    if not personnage and not description:
        raise ErreurVoix("Sans personnage, décris qui parle (par exemple « une femme d'une trentaine d'années, "
                         "voix douce et un peu grave »)")
    if personnage:
        if plateau.fiche(personnage).get("type") == "clip":
            raise ErreurVoix("Le personnage est une image, pas un clip")
        plateau.chemin_image(personnage)
    mots = len(re.sub(r"[^\w\s']", " ", phrase).split())
    duree = next(d for limite, d in DUREES if mots <= limite)
    return phrase, description, duree


def prompt_essai(phrase: str, description: str, avec_personnage: bool) -> str:
    """Le prompt de l'essai V2 : gros plan en cabine, la phrase dans sa langue, le silence autour."""
    from app import sequences  # sequences importe voix
    replique = phrase.replace("«", "").replace("»", "").strip().strip('"“”').strip()
    morceaux = []
    if avec_personnage:
        morceaux.append("The main character is the person from <Picture 1>.")
    elif description:
        morceaux.append(f"The character: {description}.")
    morceaux.append("A close-up portrait of the character in a small recording booth lined with acoustic foam, "
                    "plain dark background, soft studio light. The character looks at the camera and says: "
                    f"\"{replique}\"")
    voix = f", with this voice: {description}" if avec_personnage and description else ""
    morceaux.append(f"The character speaks {sequences.langue(replique)}{voix}.")
    morceaux.append(SILENCE)
    return " ".join(morceaux)


def graphe_essai(prompt: str, seed: int, duree: int, personnage: str | None) -> dict:
    """ReferenceToVideo (le personnage en <Picture 1>, ou aucun), son seul en sortie (SaveAudio, FLAC)."""
    from app import sequences  # sequences importe voix
    q = plateau.QUALITES_VIDEO[QUALITE]
    wf = workflows.obtenir(CONFIG.get("generation", {}).get("image_vers_video"))
    api = wf.preparer({"prompt": prompt, "seed": seed, "duree": duree, "megapixels": q["megapixels"],
                       "steps": STEPS, "prefixe": "aria_voix"})  # AS-18 sur le prompt
    i2v = reglages.noeud_unique(api, "MiniMaxH3ImageToVideo")
    cv = reglages.noeud_unique(api, "CreateVideo")
    vae, vae_audio = sequences._vaes(api)
    largeur, hauteur = sequences.dimensions(1.0, q["megapixels"])  # l'image ne compte pas : carré, le plus léger
    entrees = api[i2v]["inputs"]
    ref = {"class_type": "MiniMaxH3ReferenceToVideo", "inputs": {
        "clip": entrees["clip"], "prompt": entrees["prompt"], "width": largeur, "height": hauteur,
        "length": entrees["length"], "ref_image_size": "match", "vae": [vae, 0], "audio_vae": [vae_audio, 0]}}
    if personnage:
        api["aria_personnage"] = {"class_type": "LoadImage", "inputs": {"image": personnage}}
        ref["inputs"]["ref_images.ref_image_0"] = ["aria_personnage", 0]
    api[i2v] = ref
    api["aria_voix"] = {"class_type": "SaveAudio", "inputs": {"audio": api[cv]["inputs"]["audio"],
                                                              "filename_prefix": "aria_voix"}}
    for n in [n for n, v in api.items() if v["class_type"] in ("SaveVideo", "SaveImage")]:
        del api[n]  # ni vidéo ni vignette : seul le son compte
    return sequences._elaguer(api)


async def tourner_essai(p_: "plateau.Plateau", c: "plateau.Commande"):
    """Commande « voix » du Plateau : un essai de voix, enregistré comme brouillon (c.voix)."""
    from app import sequences  # sequences importe voix
    _nettoyer()
    securite.verifier(c.idee, c.description, *c.textes_origine)
    prompt = prompt_essai(c.idee, c.description, bool(c.parent))
    seed = c.seed if c.seed is not None else random.randrange(2**48)
    debut = time.perf_counter()
    c.etape, c.progression = "Essai de voix", 0.0
    async with p_.orchestrateur.comfyui() as comfy:
        personnage = await comfy.televerser(plateau.chemin_image(c.parent)) if c.parent else None
        sorties = await comfy.executer(graphe_essai(prompt, seed, c.duree, personnage), progression=p_._suivi(c))
    son = next((o for n, o in sorties if n.lower().endswith(".flac")), None)
    if not son:
        raise ErreurVoix("ComfyUI n'a pas renvoyé le son de l'essai")
    c.voix = _nouvelle(son, "flac", {
        "nom": None, "gardee": False, "source": "essai", "phrase": c.idee, "description": c.description,
        "langue": sequences.langue(c.idee), "personnage": c.parent, "prompt": prompt, "seed": seed,
        "duree": c.duree, "duree_s": round(time.perf_counter() - debut, 2)})["id"]
