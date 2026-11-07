"""Séquences du Montage : plusieurs plans MiniMax H3 enchaînés, tournés dans l'ordre et assemblés en un film.

Une séquence (data/sequences/<id>.json) : un titre, un casting (personnage de référence, voix, ambiance commune),
une qualité, et de 1 à PLANS_MAX plans. Le premier plan part d'un rush ou du texte ; chaque plan suivant
s'enchaîne au précédent :
    prolonger   même plan, sans coupure : les GUIDE dernières images du plan précédent et leur son
                sont ancrés au début du nouveau (MiniMaxH3AddGuide) ;
    couper      nouveau plan : le personnage (image) et la voix (audio) servent de références
                (MiniMaxH3ReferenceToVideo) ;
    transition  le plan part de la dernière image du précédent et se termine sur un rush (first / last frame).
Les trois ont été essayés en réel le 2026-10-20 (data/tests/sequence) avant d'écrire ce module.
La voix du casting (un clip, ou une voix de la bibliothèque : « voix:<id> », voir app/voix.py) sert aussi au plan
de départ s'il parle : ReferenceToVideo, avec le rush de départ ancré à l'image 0 (MiniMaxH3AddGuide) ; essayé
en réel le 2026-10-22 (data/tests/voix) : la première image est bien le rush.
« Référence sur tous les plans » (casting, coché par défaut) : départs et transitions reçoivent aussi le personnage
en <Picture 1> (ReferenceToVideo, leurs images de départ et d'arrivée deviennent des ancrages) ; pas les plans
prolongés (essais du 2026-10-24, data/tests/reference).
La contrainte du casting est impérative : donnée à Llama, puis ajoutée (en anglais) au prompt de chaque plan.

Chaque plan tourné est un clip des Bobines (rangé dans le dossier de la séquence) ; le film assemblé aussi.
AS-18 : les textes des plans, l'ambiance et les textes d'origine de tous les rushes utilisés passent par securite.
"""

import json
import math
import random
import re
import time
import uuid
from datetime import datetime

from app import modeles, plateau, reglages, rushes, securite, voix, workflows
from app.comfy import video_et_affiche
from app.config import CONFIG
from app.emplacements import DATA

DOSSIER = DATA / "sequences"
PLANS_MAX = 12  # environ 2 min de film
GUIDE = 39  # images du plan précédent ancrées en « prolonger » (grille 17k+5 du modèle)
IPS = 24
DEPARTS = ("rush", "texte")  # premier plan
ENCHAINEMENTS = ("prolonger", "couper", "transition")  # plans suivants
LONGUEUR_IDEE = 1000
LONGUEUR_AMBIANCE = 300
LONGUEUR_CONTRAINTE = 300
LONGUEUR_MUSIQUE = 200
# Ajouts au casting (2026-10-24) : une séquence plus ancienne les reçoit par défaut, sans être « à retourner ».
CASTING_DEFAUT = {"contrainte": "", "reference_partout": True}
_ID = re.compile(r"^\d{8}-\d{6}-[0-9a-f]{6}$")
_REPLIQUE = re.compile(r"[«\"“]")
_REPLIQUES = re.compile(r"«\s*([^»]+?)\s*»|\"([^\"]+)\"|“([^”]+)”")
# Mesuré le 2026-10-21 (4 plans Rapide de 5 s : 8 min 14) : guides, références et premier chargement
# coûtent environ 40 % de plus qu'un clip seul du Montage.
SURCOUT_SEQUENCE = 1.4
# « Audio needs 12+ steps » (doc H3) : à l'écoute (2026-10-21, même prompt et seed), l'auteur trouve la voix plus
# naturelle en 12 qu'en 8 steps. Les plans avec une réplique montent à 12 steps ; mesuré +17 % de temps sur un plan.
STEPS_PAROLE = 12
SURCOUT_PAROLE = 1.17
# Ce qui, s'il change, oblige à retourner le plan (et ceux qui en dépendent). Le résumé (idee) et l'ancienne case
# « suit l'ambiance » ne sont plus tournés : le plan tourne son prompt tel quel.
CONTENU_PLAN = ("prompt", "duree", "enchainement", "rush", "seed")
# Un autre checkpoint ou d'autres LoRA : des plans qui ne se ressemblent plus.
CONTENU_SEQUENCE = ("qualite", "format", "casting", "modele", "loras")


class ErreurSequence(ValueError):
    pass


# Calculs

def images(duree: int) -> int:
    """Nombre d'images d'un clip de `duree` s : arrondi à la grille 17k+5 de H3, comme le graphe du workflow."""
    n = max(5, round(duree * IPS))
    return n + (5 - n) % 17


def dimensions(ratio: float, megapixels: float) -> tuple[int, int]:
    """Largeur et hauteur multiples de 32, comme ImageScaleToTotalPixels (megapixels × 1024²)."""
    total = megapixels * 1024 * 1024
    return round(math.sqrt(total * ratio) / 32) * 32, round(math.sqrt(total / ratio) / 32) * 32


def _ratio_format(format_: str) -> float:
    l, h = format_.split(" ")[0].split(":")
    return int(l) / int(h)


def dimensions_sequence(s: dict) -> tuple[int, int]:
    """Tous les plans ont la même taille (sinon pas d'assemblage) : celle du rush de départ, sinon du format."""
    mp = plateau.QUALITES_VIDEO[s["qualite"]]["megapixels"]
    premier = s["plans"][0]
    if premier["enchainement"] == "rush":
        l, h = plateau._dimensions_png(plateau.chemin_image(premier["rush"]).read_bytes())
        return dimensions(l / h, mp)
    return dimensions(_ratio_format(s["format"]), mp)


def estimation(s: dict, indices: list[int]) -> float:
    """Secondes de calcul sur la carte de l'auteur (mesures du Montage), assemblage compris."""
    q = plateau.QUALITES_VIDEO[s["qualite"]]

    def cout(plan):
        surcout = SURCOUT_PAROLE if steps_plan(s, plan) > q["steps"] else 1
        return q["estimation_s"] * SURCOUT_SEQUENCE * surcout * (plan["duree"] / 5) ** 1.25

    return sum(cout(s["plans"][k]) for k in indices) + (5 if indices else 0)


def parle(plan: dict) -> bool:
    """Le plan contient une réplique (dans l'idée ou le prompt imposé)."""
    return bool(_REPLIQUE.search(f"{plan['idee']} {plan.get('prompt') or ''}"))


def steps_plan(s: dict, plan: dict) -> int:
    """Steps de la qualité choisie, au moins STEPS_PAROLE pour un plan qui parle (la voix en a besoin)."""
    steps = plateau.QUALITES_VIDEO[s["qualite"]]["steps"]
    return max(steps, STEPS_PAROLE) if parle(plan) else steps


def repliques(texte: str) -> list[str]:
    """Répliques entre « », "" ou “”."""
    return [next(g for g in m.groups() if g).strip() for m in _REPLIQUES.finditer(texte)]


def garder_repliques(idee: str, prompt_plan: str) -> str:
    """Remet, mot pour mot, les répliques de l'idée que Llama aurait perdues (vu à l'essai du 2026-10-21)."""
    simple = lambda t: " ".join(re.sub(r"[^\w\s]", " ", t.casefold()).split())
    manquantes = [r for r in repliques(idee) if simple(r) not in simple(prompt_plan)]
    return " ".join([prompt_plan.rstrip()] + [f'The character says: "{r}"' for r in manquantes])


_FIN_PHRASE = re.compile(r"[.!?…][\"”»]?(?:\s|$)")  # fin de phrase, guillemet fermant compris


def sans_repliques_inventees(source: str, prompt_plan: str) -> tuple[str, list[str]]:
    """Retire les phrases où Llama fait dire une réplique absente de `source` (l'idée, et ce qui l'accompagne).
    Vu à l'essai du 2026-10-25 : « Mmm, c'est vrai, ça » inventé dans un plan prolongé malgré la consigne, puis
    recopié dans la transition suivante. Un texte entre guillemets présent dans la source (réplique, enseigne) reste.
    Renvoie le prompt et les répliques retirées."""
    permis, retirees = _simple(source), []
    while True:
        inventee = next((m for m in _REPLIQUES.finditer(prompt_plan)
                         if _simple(next(g for g in m.groups() if g)) not in permis), None)
        if inventee is None:
            return " ".join(prompt_plan.split()), retirees
        avant = [f.end() for f in _FIN_PHRASE.finditer(prompt_plan, 0, inventee.start())]
        debut = avant[-1] if avant else 0
        fin = _FIN_PHRASE.search(prompt_plan, inventee.end() - 2)  # la réplique peut finir la phrase (« Salut. »)
        retirees.append(next(g for g in inventee.groups() if g).strip())
        prompt_plan = prompt_plan[:debut] + " " + prompt_plan[fin.end() if fin else len(prompt_plan):]


def a_tourner(s: dict, demandes: list[int] | None = None) -> list[int]:
    """Plans à tourner : ceux demandés (ou pas encore à jour), plus ceux qui partent du plan d'avant."""
    plans = s["plans"]
    choisis = set(demandes if demandes is not None else [])
    choisis |= {k for k, p in enumerate(plans) if not (p.get("clip") and p.get("a_jour"))}
    for k in range(1, len(plans)):
        if k - 1 in choisis and plans[k]["enchainement"] in ("prolonger", "transition"):
            choisis.add(k)
    return sorted(choisis)


def personnage(s: dict) -> str | None:
    """Rush de référence des plans coupés : celui du casting, sinon le rush de départ."""
    premier = s["plans"][0]
    return s["casting"].get("personnage") or (premier["rush"] if premier["enchainement"] == "rush" else None)


def source_voix(s: dict, k: int) -> str | None:
    """Clip (ou « voix:<id> ») dont le son sert de voix de référence au plan k (coupé)."""
    voix_ = s["casting"].get("voix")
    if voix_ == "auto":  # le premier plan déjà tourné qui contient une réplique
        return next((p["clip"] for p in s["plans"][:k] if p.get("clip") and parle(p)), None)
    return voix_ or None


def voix_du_plan(s: dict, k: int) -> str | None:
    """Voix de référence du plan k : celle des plans coupés et des transitions qui parlent, et celle du casting pour
    un plan de départ qui parle (en Auto, c'est lui qui donne la voix : il n'en reçoit pas). Un plan prolongé garde
    la voix par le son des images ancrées."""
    plan, voix_ = s["plans"][k], s["casting"].get("voix")
    if plan["enchainement"] == "couper" or (plan["enchainement"] == "transition" and parle(plan)):
        return source_voix(s, k)
    if plan["enchainement"] in DEPARTS and parle(plan) and voix_ not in (None, "auto"):
        return voix_
    return None


def references(s: dict, k: int) -> tuple[str | None, str | None]:
    """(personnage, voix) de référence du plan k ; s'il y en a une, le plan passe en ReferenceToVideo.
    « Référence sur tous les plans » (casting) : départs et transitions reçoivent aussi le personnage. Pas les plans
    prolongés : à l'essai du 2026-10-24 (data/tests/reference), la référence y agrandissait les sauts aux raccords
    et recopiait la composition de l'image, alors que les 39 images ancrées tiennent déjà le personnage."""
    plan = s["plans"][k]
    if plan["enchainement"] == "prolonger":
        return None, None
    partout = s["casting"].get("reference_partout", True)
    return (personnage(s) if plan["enchainement"] == "couper" or partout else None), voix_du_plan(s, k)


def _id_voix(valeur: str | None) -> str | None:
    """L'identifiant d'une voix de la bibliothèque (« voix:<id> »), None pour Auto, aucune ou un clip."""
    return valeur[len(voix.PREFIXE):] if valeur and valeur.startswith(voix.PREFIXE) else None


def _rushes_utilises(s: dict) -> list[str]:
    ids = [p["rush"] for p in s["plans"] if p.get("rush")] + [s["casting"].get("personnage")]
    voix_ = s["casting"].get("voix")
    return [i for i in ids if i] + ([voix_] if voix_ not in (None, "auto") and not _id_voix(voix_) else [])


def _textes(s: dict) -> list[str]:
    """AS-18 : textes d'origine des rushes et clips utilisés, et ce qui a fait la voix de la bibliothèque."""
    id_voix = _id_voix(s["casting"].get("voix"))
    return plateau.textes_origine(_rushes_utilises(s)) + (voix.textes(id_voix) if id_voix else [])


# Stockage

def _chemin(id_sequence: str):
    if not _ID.match(id_sequence or "") or not (DOSSIER / f"{id_sequence}.json").is_file():
        raise ErreurSequence(f"Séquence inconnue : {id_sequence}")
    return DOSSIER / f"{id_sequence}.json"


def lire(id_sequence: str) -> dict:
    s = json.loads(_chemin(id_sequence).read_text(encoding="utf-8"))
    s.setdefault("loras", [])  # séquence d'avant les LoRA vidéo
    return s


def _ecrire(s: dict):
    DOSSIER.mkdir(parents=True, exist_ok=True)
    temporaire = DOSSIER / f"{s['id']}.tmp"
    temporaire.write_text(json.dumps(s, indent=4, ensure_ascii=False), encoding="utf-8")
    temporaire.replace(DOSSIER / f"{s['id']}.json")


def liste() -> list[dict]:
    if not DOSSIER.is_dir():
        return []
    sequences = [json.loads(f.read_text(encoding="utf-8")) for f in DOSSIER.glob("*.json")]
    return sorted((public(s) for s in sequences), key=lambda s: s["date"], reverse=True)


def public(s: dict) -> dict:
    indices = a_tourner(s)
    return {**s, "a_tourner": indices, "estimation_s": round(estimation(s, indices)),
            "duree_totale": sum(p["duree"] for p in s["plans"])}


def _valider(donnees: dict, pour_tourner: bool = False) -> dict:
    """Séquence propre à partir de ce qu'envoie l'interface. Brouillon accepté (transition sans rush d'arrivée,
    venue du découpage du Scénario) ; `pour_tourner` refuse en plus tout ce qui ne se tournerait pas."""
    titre = str(donnees.get("titre") or "").strip()
    if not titre or len(titre) > 40:
        raise ErreurSequence("Titre de 1 à 40 caractères")
    qualite = donnees.get("qualite", "compromis")
    if qualite not in plateau.QUALITES_VIDEO:
        raise ErreurSequence(f"Qualité inconnue : {qualite}")
    format_ = donnees.get("format", "16:9 (Widescreen)")
    if format_ not in workflows.FORMATS:
        raise ErreurSequence(f"Format inconnu : {format_}")
    casting = donnees.get("casting") or {}
    ambiance = str(casting.get("ambiance") or "").strip()
    if len(ambiance) > LONGUEUR_AMBIANCE:
        raise ErreurSequence(f"Ambiance de {LONGUEUR_AMBIANCE} caractères au plus")
    contrainte = " ".join(str(casting.get("contrainte") or "").split())
    if len(contrainte) > LONGUEUR_CONTRAINTE:
        raise ErreurSequence(f"Contrainte de {LONGUEUR_CONTRAINTE} caractères au plus")
    casting = {"personnage": casting.get("personnage") or None, "voix": casting.get("voix", "auto") or None,
               "ambiance": ambiance, "contrainte": contrainte,
               "reference_partout": bool(casting.get("reference_partout", True))}
    if casting["personnage"]:
        plateau.chemin_image(casting["personnage"])  # existe ?
    if _id_voix(casting["voix"]):
        try:
            voix.gardee(_id_voix(casting["voix"]))
        except voix.ErreurVoix as e:
            raise ErreurSequence(str(e))
    elif casting["voix"] not in (None, "auto"):
        plateau.chemin_video(casting["voix"])

    plans_bruts = donnees.get("plans") or []
    if not 1 <= len(plans_bruts) <= PLANS_MAX:
        raise ErreurSequence(f"De 1 à {PLANS_MAX} plans")
    plans = []
    for k, p in enumerate(plans_bruts):
        nom = f"Plan {k + 1}"
        enchainement = p.get("enchainement")
        permis = DEPARTS if k == 0 else ENCHAINEMENTS
        if enchainement not in permis:
            raise ErreurSequence(f"{nom} : enchaînement possible : {', '.join(permis)}")
        idee = str(p.get("idee") or "").strip()
        prompt_force = (str(p["prompt"]).strip() or None) if p.get("prompt") else None
        if not idee and not prompt_force:
            raise ErreurSequence(f"{nom} : écris l'idée du plan")
        if len(idee) > LONGUEUR_IDEE:
            raise ErreurSequence(f"{nom} : idée de {LONGUEUR_IDEE} caractères au plus")
        duree = p.get("duree", 5)
        if duree not in plateau.DUREES_VIDEO:
            raise ErreurSequence(f"{nom} : durée proposée : {', '.join(map(str, plateau.DUREES_VIDEO))} s")
        rush = p.get("rush") or None
        if enchainement in ("rush", "transition"):
            if not rush and (enchainement == "rush" or pour_tourner):
                raise ErreurSequence(f"{nom} : choisis le rush " + ("de départ" if k == 0 else "d'arrivée"))
            if rush and plateau.fiche(rush).get("type") == "clip":
                raise ErreurSequence(f"{nom} : il faut une image, pas un clip")
            if rush:
                plateau.chemin_image(rush)
        else:
            rush = None
        seed = p.get("seed")
        plans.append({"id": p.get("id") or uuid.uuid4().hex[:8], "idee": idee, "prompt": prompt_force,
                      "duree": duree, "enchainement": enchainement, "rush": rush,
                      "seed": int(seed) if seed is not None else None,
                      "ambiance": bool(p.get("ambiance", True)),  # décochée : un plan qui se passe ailleurs
                      "clip": p.get("clip"), "prompt_final": p.get("prompt_final"), "a_jour": p.get("a_jour", False)})

    modele = donnees.get("modele") or None  # None : creer() prend le défaut des réglages, modifier() garde l'ancien
    if modele and modele not in _workflow().variantes():
        raise ErreurSequence(f"Checkpoint absent de modeles/{_workflow().famille}/modeles : {modele}")
    try:  # les LoRA de la famille vidéo, les mêmes sur tous les plans ; leurs réglages retenus pour la prochaine fois
        loras = plateau.valider_loras(_workflow(), donnees.get("loras"), titre)
    except plateau.ErreurPlateau as e:
        raise ErreurSequence(str(e))
    s = {"titre": titre, "qualite": qualite, "format": format_, "casting": casting, "plans": plans,
         "llm": donnees.get("llm") or None, "modele": modele, "loras": loras}
    # AS-18 : tout le texte de la séquence, avec les textes d'origine des rushes, clips et voix qu'elle utilise.
    securite.verifier(ambiance, contrainte, *(p["idee"] for p in plans), *(p["prompt"] or "" for p in plans),
                      *(l["declencheurs"] for l in loras), *_textes(s))
    return s


def _workflow() -> "workflows.Workflow":
    """Le workflow de tous les plans (image vers vidéo) : sa famille donne les checkpoints proposés."""
    return workflows.obtenir(CONFIG.get("generation", {}).get("image_vers_video"))


def creer(donnees: dict) -> dict:
    s = _valider(donnees)
    if s["loras"]:
        modeles.memoriser_loras(_workflow().famille, s["loras"])
    s["modele"] = s["modele"] or _workflow().choisi(None)  # la séquence garde son checkpoint, même si le défaut change
    traduction = donnees.get("contrainte_en") or {}  # faite au découpage du Scénario
    if s["casting"]["contrainte"] and traduction.get("fr") == s["casting"]["contrainte"] and traduction.get("en"):
        s["contrainte_en"] = {"fr": traduction["fr"], "en": " ".join(str(traduction["en"]).split())[:LONGUEUR_CONTRAINTE * 2]}
        securite.verifier(s["casting"]["contrainte"], s["contrainte_en"]["en"])
    for p in s["plans"]:
        p.update(clip=None, prompt_final=None, a_jour=False)
    s.update(id=f"{datetime.now():%Y%m%d-%H%M%S}-{uuid.uuid4().hex[:6]}", date=datetime.now().isoformat(timespec="seconds"),
             film=None, dossier=_dossier(s["titre"]))
    _ecrire(s)
    return public(s)


def modifier(id_sequence: str, donnees: dict) -> dict:
    """Enregistre les changements ; un plan modifié (ou toute la séquence si la qualité, le format ou le casting
    change) n'est plus « à jour » : il sera retourné, ses anciens clips restent dans les Bobines."""
    ancienne = lire(id_sequence)
    ancienne["casting"] = {**CASTING_DEFAUT, **ancienne["casting"]}
    ancienne["modele"] = ancienne.get("modele") or _workflow().modele_origine  # tournée avant le choix du checkpoint
    ancienne["loras"] = ancienne.get("loras") or []  # tournée avant les LoRA vidéo
    s = _valider(donnees)
    if s["loras"]:
        modeles.memoriser_loras(_workflow().famille, s["loras"])
    s["modele"] = s["modele"] or ancienne["modele"]
    tout_change = any(s[c] != ancienne[c] for c in CONTENU_SEQUENCE)
    if ancienne.get("contrainte_en", {}).get("fr") == s["casting"]["contrainte"]:
        s["contrainte_en"] = ancienne["contrainte_en"]  # traduction encore valable
    avant = {p["id"]: p for p in ancienne["plans"]}
    for p in s["plans"]:
        a = avant.get(p["id"])
        if a is None or tout_change or any(p[c] != a.get(c) for c in CONTENU_PLAN):
            p.update(clip=a["clip"] if a else None, prompt_final=None, a_jour=False)
        else:
            p.update(clip=a["clip"], prompt_final=a["prompt_final"], a_jour=a["a_jour"])
    s.update(id=ancienne["id"], date=ancienne["date"], film=ancienne.get("film"), dossier=ancienne["dossier"])
    _ecrire(s)
    return public(s)


def supprimer(id_sequence: str):
    """La séquence seulement : ses clips et son film restent dans les Bobines."""
    _chemin(id_sequence).unlink()


def oublier_voix(valeur: str, nom: str) -> list[str]:
    """Une voix de la bibliothèque est supprimée : les séquences qui l'utilisaient repassent en Auto (tous leurs plans
    sont à retourner, comme pour tout changement de casting) et le disent. Renvoie leurs identifiants."""
    if not DOSSIER.is_dir():
        return []
    touchees = []
    for f in DOSSIER.glob("*.json"):
        s = json.loads(f.read_text(encoding="utf-8"))
        if s["casting"].get("voix") != valeur:
            continue
        s["casting"]["voix"] = "auto"
        for p in s["plans"]:
            p.update(prompt_final=None, a_jour=False)
        s["alerte"] = f"La voix « {nom} » a été supprimée : la séquence repasse en Auto, tous ses plans sont à retourner"
        _ecrire(s)
        touchees.append(s["id"])
    return touchees


def _dossier(titre: str) -> str:
    """Dossier des Bobines de la séquence (créé au besoin, partagé avec les Rushes)."""
    nom = re.sub(r"[^\w .'’-]", "", f"Séquence {titre}")[:40].strip()
    if nom not in rushes.dossiers():
        rushes.creer_dossier(nom)
    return nom


# Graphes ComfyUI

def _elaguer(api: dict) -> dict:
    """Ne garde que les nœuds utiles aux sorties (les branches débranchées ne doivent pas être validées)."""
    utiles, a_voir = set(), [n for n, v in api.items() if v["class_type"] in ("SaveVideo", "SaveImage", "SaveAudio")]
    while a_voir:
        n = a_voir.pop()
        if n in utiles:
            continue
        utiles.add(n)
        a_voir += [v[0] for v in api[n]["inputs"].values() if isinstance(v, list) and len(v) == 2 and v[0] in api]
    return {n: v for n, v in api.items() if n in utiles}


def _vaes(api: dict) -> tuple[str, str]:
    """(VAE vidéo, VAE audio) du graphe H3."""
    vaes = [n for n, v in api.items() if v["class_type"] == "VAELoader"]
    audio = next(n for n in vaes if "audio" in api[n]["inputs"]["vae_name"])
    return next(n for n in vaes if n != audio), audio


def graphe_plan(s: dict, k: int, prompt_plan: str, seed: int, fichiers: dict) -> dict:
    """Graphe d'un plan. `fichiers` : noms déjà téléversés dans ComfyUI (rush, precedent, personnage, et la voix :
    « voix » pour un clip, « voix_son » pour un son de la bibliothèque)."""
    p = s["plans"][k]
    q = plateau.QUALITES_VIDEO[s["qualite"]]
    wf = workflows.obtenir(CONFIG.get("generation", {}).get("image_vers_video"))
    valeurs = {"prompt": prompt_plan, "seed": seed, "duree": p["duree"], "megapixels": q["megapixels"],
               "modele": s.get("modele"), "loras": s.get("loras") or [],
               "steps": steps_plan(s, p), "prefixe": "aria_sequence"}
    if p["enchainement"] == "rush":
        valeurs["image"] = fichiers["rush"]
    api = wf.preparer(valeurs)  # AS-18 sur le prompt, réglages de l'utilisateur, qualité
    i2v = reglages.noeud_unique(api, "MiniMaxH3ImageToVideo")
    guider = reglages.noeud_unique(api, "BasicGuider")
    vae, vae_audio = _vaes(api)
    largeur, hauteur = dimensions_sequence(s)
    entrees = api[i2v]["inputs"]
    entrees.update(width=largeur, height=hauteur)
    premiere_image = entrees.pop("first_frame", None)
    precedent = s["plans"][k - 1] if k else None

    def video(cle: str, fichier: str) -> list:
        api[f"aria_{cle}"] = {"class_type": "LoadVideo", "inputs": {"file": fichier}}
        api[f"aria_{cle}_parts"] = {"class_type": "GetVideoComponents", "inputs": {"video": [f"aria_{cle}", 0]}}
        return [f"aria_{cle}", 0]

    if p["enchainement"] == "transition":  # de la dernière image du plan précédent au rush d'arrivée
        n = images(precedent["duree"])
        video("precedent", fichiers["precedent"])
        api["aria_derniere"] = {"class_type": "ImageFromBatch", "inputs": {
            "image": ["aria_precedent_parts", 0], "batch_index": n - 1, "length": 1}}
        api["aria_arrivee"] = {"class_type": "LoadImage", "inputs": {"image": fichiers["rush"]}}

    refs_ici = bool(fichiers.get("personnage") or fichiers.get("voix") or fichiers.get("voix_son"))
    if p["enchainement"] == "couper" or (p["enchainement"] != "prolonger" and refs_ici):
        ref = {"class_type": "MiniMaxH3ReferenceToVideo", "inputs": {
            "clip": entrees["clip"], "prompt": entrees["prompt"], "width": largeur, "height": hauteur,
            "length": entrees["length"], "ref_image_size": "match", "vae": [vae, 0], "audio_vae": [vae_audio, 0]}}
        if fichiers.get("personnage"):
            api["aria_personnage"] = {"class_type": "LoadImage", "inputs": {"image": fichiers["personnage"]}}
            ref["inputs"]["ref_images.ref_image_0"] = ["aria_personnage", 0]
        if fichiers.get("voix_son"):
            api["aria_voix_son"] = {"class_type": "LoadAudio", "inputs": {"audio": fichiers["voix_son"]}}
            api["aria_voix_coupee"] = {"class_type": "TrimAudioDuration", "inputs": {
                "audio": ["aria_voix_son", 0], "start_index": 0.0, "duration": float(voix.DUREE_MAX)}}
            ref["inputs"]["ref_audios.ref_audio_0"] = ["aria_voix_coupee", 0]
        elif fichiers.get("voix"):
            video("voix", fichiers["voix"])
            ref["inputs"]["ref_audios.ref_audio_0"] = ["aria_voix_parts", 1]
        api[i2v] = ref
        # Ce qui relie le plan devient un ancrage : le rush de départ à l'image 0 (essai D du 2026-10-22) ;
        # la transition à ses deux bouts (essai du 2026-10-24 : départ et arrivée aussi exacts qu'en first / last
        # frame, moins de saut, et le mouvement de caméra demandé mieux suivi).
        ancres = {"rush": [(0, premiere_image)],
                  "transition": [(0, ["aria_derniere", 0]), (-1, ["aria_arrivee", 0])]}.get(p["enchainement"], [])
        positif = [i2v, 0]
        for j, (indice, image) in enumerate(ancres):
            api[f"aria_ancre_{j}"] = {"class_type": "MiniMaxH3AddGuide", "inputs": {
                "positive": positif, "latent": [i2v, 1], "frame_idx": indice, "vae": [vae, 0], "image": image}}
            positif = [f"aria_ancre_{j}", 0]
        api[guider]["inputs"]["conditioning"] = positif
    elif p["enchainement"] == "rush":
        entrees["first_frame"] = premiere_image
    elif p["enchainement"] == "prolonger":
        # Les GUIDE dernières images du plan précédent (une de plus : AddGuide recadre à 17k+5) et leur son.
        n = images(precedent["duree"])
        video("precedent", fichiers["precedent"])
        api["aria_fin"] = {"class_type": "Video Slice", "inputs": {
            "video": ["aria_precedent", 0], "start_time": (n - GUIDE - 1) / IPS, "duration": (GUIDE + 1) / IPS,
            "strict_duration": False}}
        api["aria_fin_parts"] = {"class_type": "GetVideoComponents", "inputs": {"video": ["aria_fin", 0]}}
        api["aria_guide"] = {"class_type": "MiniMaxH3AddGuide", "inputs": {
            "positive": [i2v, 0], "latent": [i2v, 1], "frame_idx": 0, "vae": [vae, 0], "audio_vae": [vae_audio, 0],
            "image": ["aria_fin_parts", 0], "audio": ["aria_fin_parts", 1]}}
        api[guider]["inputs"]["conditioning"] = ["aria_guide", 0]
    elif p["enchainement"] == "transition":
        entrees.update(first_frame=["aria_derniere", 0], last_frame=["aria_arrivee", 0])
    wf.ajouter_affiche(api, "aria_sequence")
    return _elaguer(api)


def graphe_film(s: dict, fichiers: list[str]) -> dict:
    """Assemblage : chaque plan, sans les images qu'il partage avec le précédent, recollés avec leur son."""
    api, entrees = {}, {}
    plans = s["plans"]
    for k, (p, fichier) in enumerate(zip(plans, fichiers)):
        debut = GUIDE / IPS if p["enchainement"] == "prolonger" else (1 / IPS if p["enchainement"] == "transition" else 0)
        suivant = plans[k + 1]["enchainement"] if k + 1 < len(plans) else None
        fin = (images(p["duree"]) - 1) / IPS if suivant == "prolonger" else None  # la dernière image est refaite
        api[f"v{k}"] = {"class_type": "LoadVideo", "inputs": {"file": fichier}}
        source = [f"v{k}", 0]
        if debut or fin:
            api[f"t{k}"] = {"class_type": "Video Slice", "inputs": {
                "video": source, "start_time": debut, "duration": (fin - debut) if fin else 0, "strict_duration": False}}
            source = [f"t{k}", 0]
        entrees[f"videos.video{k}"] = source
    api["film"] = {"class_type": "ConcatenateVideo", "inputs": {**entrees, "codec": "auto"}}
    api["film_sortie"] = {"class_type": "SaveVideo", "inputs": {"video": ["film", 0], "filename_prefix": "aria_film",
                                                               "format": "auto", "codec": "auto"}}
    api["film_parts"] = {"class_type": "GetVideoComponents", "inputs": {"video": ["film", 0]}}
    api["film_affiche_lot"] = {"class_type": "ImageFromBatch", "inputs": {
        "image": ["film_parts", 0], "batch_index": 0, "length": 1}}
    api["film_affiche"] = {"class_type": "SaveImage", "inputs": {"images": ["film_affiche_lot", 0],
                                                                 "filename_prefix": "aria_film_affiche"}}
    return api


def _simple(texte: str) -> str:
    return " ".join(re.sub(r"[^\w\s]", " ", texte.casefold()).split())


# Tournage (appelé par la file du Plateau)

def _extrait(texte: str, longueur: int = 400) -> str:
    texte = " ".join((texte or "").split())
    return texte if len(texte) <= longueur else texte[:longueur].rsplit(" ", 1)[0] + "…"


def contrainte_tournage(s: dict) -> str | None:
    """La contrainte à ajouter à chaque plan : sa traduction faite au découpage (s["contrainte_en"]) si elle vaut
    toujours, sinon le texte de l'utilisateur tel quel (H3 lit aussi le français) ; le LLM n'est plus appelé
    au tournage."""
    contrainte = s["casting"].get("contrainte")
    if not contrainte:
        return None
    cache = s.get("contrainte_en") or {}
    return cache["en"] if cache.get("fr") == contrainte and cache.get("en") else contrainte


def avec_contrainte(texte: str, contrainte_en: str | None) -> str:
    """La contrainte à la fin du prompt de chaque plan : Llama peut l'oublier, le serveur la garantit."""
    if not contrainte_en:
        return texte
    return f"{texte.rstrip()} Constraint for every shot: {contrainte_en.strip().rstrip('.')}."


def _reference(texte: str, personnage_: bool, voix: bool) -> str:
    morceaux = [texte.rstrip()]
    if personnage_:
        morceaux.append("The main character is the person from <Picture 1>.")
    if voix:
        morceaux.append("The character speaks with the same voice as in <Audio 1>.")
    return " ".join(morceaux)


async def tourner(p_: "plateau.Plateau", c: "plateau.Commande"):
    """Tourne les plans c.plans de la séquence c.sequence, dans l'ordre, puis assemble le film."""
    s = lire(c.sequence)
    textes = _textes(s)
    total = len(c.plans)

    # 1. Prompts : écrits au Scénario (découpage) ou dans le plan. Le LLM n'écrit plus au tournage.
    contrainte_en = contrainte_tournage(s)
    for k in c.plans:
        plan = s["plans"][k]
        if not (plan.get("prompt") or "").strip():
            raise ErreurSequence(f"Plan {k + 1} sans prompt : écris-le dans le plan, ou découpe la séquence au ✍️ Scénario")
        # La contrainte, garantie ; les mots déclencheurs des LoRA en tête, comme pour une image.
        plan["prompt_final"] = plateau.avec_declencheurs(avec_contrainte(plan["prompt"].strip(), contrainte_en),
                                                         s.get("loras"))
        securite.verifier(s["casting"].get("contrainte", ""), plan["idee"], plan["prompt_final"], *textes)
    _ecrire(s)

    # 2. Tournage, plan après plan : chacun peut partir du précédent.
    for rang, k in enumerate(c.plans, 1):
        plan = s["plans"][k]
        debut = time.perf_counter()
        c.etape, c.progression = f"Plan {k + 1} · tournage ({rang}/{total})", 0.0
        seed = plan["seed"] if plan["seed"] is not None else random.randrange(2**48)
        ref_personnage, ref_voix = references(s, k)
        prompt_plan = _reference(plan["prompt_final"], bool(ref_personnage), bool(ref_voix))
        async with p_.orchestrateur.comfyui() as comfy:
            fichiers = {}
            if plan["rush"]:
                fichiers["rush"] = await comfy.televerser(plateau.chemin_image(plan["rush"]))
            if plan["enchainement"] in ("prolonger", "transition"):
                fichiers["precedent"] = await comfy.televerser(plateau.chemin_video(s["plans"][k - 1]["clip"]))
            if ref_personnage:
                fichiers["personnage"] = await comfy.televerser(plateau.chemin_image(ref_personnage))
            if _id_voix(ref_voix):
                fichiers["voix_son"] = await comfy.televerser(voix.chemin_son(_id_voix(ref_voix)))
            elif ref_voix:
                fichiers["voix"] = await comfy.televerser(plateau.chemin_video(ref_voix))
            sorties = await comfy.executer(graphe_plan(s, k, prompt_plan, seed, fichiers), progression=p_._suivi(c))
        mp4, affiche = video_et_affiche(sorties)
        if not (mp4 and affiche):
            raise plateau.ErreurPlateau(f"Plan {k + 1} : ComfyUI n'a pas renvoyé la vidéo attendue")
        ancien = plan.get("clip")
        plan["clip"] = plateau.enregistrer_clip(mp4, affiche, {
            "type": "clip", "mode_video": "sequence", "sequence": s["id"], "plan": k + 1,
            "enchainement": plan["enchainement"], "idee": plan["idee"], "prompt": prompt_plan, "llm": s["llm"],
            "workflow": CONFIG.get("generation", {}).get("image_vers_video"), "qualite": s["qualite"],
            "duree": plan["duree"], "megapixels": plateau.QUALITES_VIDEO[s["qualite"]]["megapixels"],
            "steps": steps_plan(s, plan), "voix": ref_voix, "modele": _workflow().choisi(s.get("modele")),
            "seed": seed, "parent": plan["rush"], "dossier": s["dossier"],
            "duree_s": round(time.perf_counter() - debut, 2)})
        plan["a_jour"] = True
        c.images.append(plan["clip"])
        _ecrire(s)  # un plan tourné est acquis, même si la suite échoue
        if ancien:
            rushes.agir([ancien], "corbeille")

    # 3. Assemblage du film (quelques secondes).
    c.etape, c.progression = "Assemblage du film", 0.0
    debut = time.perf_counter()
    async with p_.orchestrateur.comfyui() as comfy:
        noms = [await comfy.televerser(plateau.chemin_video(p["clip"])) for p in s["plans"]]
        sorties = await comfy.executer(graphe_film(s, noms))
    mp4, affiche = video_et_affiche(sorties)
    if not (mp4 and affiche):
        raise plateau.ErreurPlateau("Assemblage : ComfyUI n'a pas renvoyé le film")
    ancien_film = s.get("film")
    s["film"] = plateau.enregistrer_clip(mp4, affiche, {
        "type": "clip", "mode_video": "film", "sequence": s["id"], "idee": s["titre"],
        "prompt": " | ".join(p["prompt_final"] or "" for p in s["plans"]), "llm": s["llm"], "workflow": None,
        "qualite": s["qualite"], "duree": sum(p["duree"] for p in s["plans"]), "plans": [p["clip"] for p in s["plans"]],
        "seed": None, "parent": None, "dossier": s["dossier"], "duree_s": round(time.perf_counter() - debut, 2)})
    c.images.append(s["film"])
    _ecrire(s)
    if ancien_film:
        rushes.agir([ancien_film], "corbeille")
