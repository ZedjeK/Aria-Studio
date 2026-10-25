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

import copy
import json
import math
import random
import re
import time
import uuid
from datetime import datetime

from app import plateau, prompt, reglages, rushes, securite, voix, workflows
from app.config import CONFIG, ROOT

DOSSIER = ROOT / "data" / "sequences"
PLANS_MAX = 12  # environ 2 min de film
GUIDE = 39  # images du plan précédent ancrées en « prolonger » (grille 17k+5 du modèle)
IPS = 24
DEPARTS = ("rush", "texte")  # premier plan
ENCHAINEMENTS = ("prolonger", "couper", "transition")  # plans suivants
LONGUEUR_IDEE = 1000
LONGUEUR_AMBIANCE = 300
LONGUEUR_CONTRAINTE = 300
# Ajouts au casting (2026-10-24) : une séquence plus ancienne les reçoit par défaut, sans être « à retourner ».
CASTING_DEFAUT = {"contrainte": "", "reference_partout": True}
_ID = re.compile(r"^\d{8}-\d{6}-[0-9a-f]{6}$")
_REPLIQUE = re.compile(r"[«\"“]")
_REPLIQUES = re.compile(r"«\s*([^»]+?)\s*»|\"([^\"]+)\"|“([^”]+)”")
# Mesuré le 2026-10-21 (4 plans Rapide de 5 s : 8 min 14) : guides, références et premier chargement
# coûtent environ 40 % de plus qu'un clip seul du Montage.
SURCOUT_SEQUENCE = 1.4
# « Audio needs 12+ steps » (doc H3) : à l'écoute (2026-10-21, même prompt et seed), Jérémy trouve la voix plus naturelle
# en 12 qu'en 8 steps. Les plans avec une réplique montent à 12 steps ; mesuré +17 % de temps sur un plan.
STEPS_PAROLE = 12
SURCOUT_PAROLE = 1.17
# Ajouté à l'idée de chaque plan avant la réécriture : à l'essai, Llama inventait un âge différent à chaque plan
# (« 30s », « middle-aged », « mid-50s ») et H3 suit le texte plutôt que l'image de référence.
CONSIGNE_SEQUENCE = ("(This is one shot of a sequence: the character's face, age, body and outfit come from the "
                     "reference picture, so never describe them; call her or him simply \"the character\". "
                     "The shot is ONE continuous take: never write \"cut to\" nor switch to another angle or place "
                     "(the next shot does that). "
                     "Keep every spoken line word for word, in quotes, and never add one: if the idea has no line in "
                     "quotes, nobody speaks.)")
# Ce qui, s'il change, oblige à retourner le plan (et ceux qui en dépendent).
CONTENU_PLAN = ("idee", "prompt", "duree", "enchainement", "rush", "seed", "ambiance")
CONTENU_SEQUENCE = ("qualite", "format", "casting")


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
    """Secondes de calcul sur la carte de Jérémy (mesures du Montage), assemblage compris."""
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
    return json.loads(_chemin(id_sequence).read_text(encoding="utf-8"))


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

    s = {"titre": titre, "qualite": qualite, "format": format_, "casting": casting, "plans": plans,
         "llm": donnees.get("llm") or None}
    # AS-18 : tout le texte de la séquence, avec les textes d'origine des rushes, clips et voix qu'elle utilise.
    securite.verifier(ambiance, contrainte, *(p["idee"] for p in plans), *(p["prompt"] or "" for p in plans),
                      *_textes(s))
    return s


def creer(donnees: dict) -> dict:
    s = _valider(donnees)
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
    s = _valider(donnees)
    tout_change = any(s[c] != ancienne[c] for c in CONTENU_SEQUENCE)
    if ancienne.get("contrainte_en", {}).get("fr") == s["casting"]["contrainte"]:
        s["contrainte_en"] = ancienne["contrainte_en"]  # traduction encore valable
    avant = {p["id"]: p for p in ancienne["plans"]}
    for p in s["plans"]:
        a = avant.get(p["id"])
        if a is None or tout_change or any(p[c] != a.get(c, True if c == "ambiance" else None) for c in CONTENU_PLAN):
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
               "steps": steps_plan(s, p), "prefixe": "aria_sequence"}
    if p["enchainement"] == "rush":
        valeurs["image"] = fichiers["rush"]
    api = wf.preparer(valeurs)  # AS-18 sur le prompt, réglages de Jérémy, qualité
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


# Découpage (Scénario, mode Séquence) : Llama écrit d'un coup le prompt de chaque plan, en voyant toute l'histoire.

LONGUEUR_SYNOPSIS = 2000
LONGUEUR_NOTE = 500
TYPES_DECOUPAGE = {"rush": "start from the picture", "texte": "start", "prolonger": "continue", "couper": "cut",
                   "transition": "transition"}
_BLOC_PLAN = re.compile(r"SHOT\s+(\d+)\s*:?\s*\n\s*FR\s*:\s*(.+?)\n\s*EN\s*:\s*(.+?)(?=\n\s*SHOT\s+\d+|\Z)", re.S | re.I)


def _simple(texte: str) -> str:
    return " ".join(re.sub(r"[^\w\s]", " ", texte.casefold()).split())


def langue(replique: str) -> str:
    """Langue d'une réplique d'après son écriture ; à l'essai, Ministral faisait parler en coréen une réplique française
    parce que le personnage est coréen. Écriture latine : le français, langue de Jérémy."""
    if re.search(r"[぀-ヿ]", replique):
        return "Japanese"
    if re.search(r"[가-힯]", replique):
        return "Korean"
    if re.search(r"[一-鿿]", replique):
        return "Mandarin Chinese"
    if re.search(r"[Ѐ-ӿ]", replique):
        return "Russian"
    return "French"


def _valider_decoupage(synopsis: str, personnage_: str, decor: str, plans: list[dict],
                       reference: str | None) -> list[dict]:
    if not synopsis.strip():
        raise ErreurSequence("Écris le synopsis de la séquence")
    if max(len(synopsis), len(personnage_), len(decor)) > LONGUEUR_SYNOPSIS:
        raise ErreurSequence(f"Synopsis de {LONGUEUR_SYNOPSIS} caractères au plus")
    if not 2 <= len(plans) <= PLANS_MAX:
        raise ErreurSequence(f"De 2 à {PLANS_MAX} plans")
    propres = []
    for k, p in enumerate(plans):
        enchainement = p.get("enchainement")
        permis = DEPARTS if k == 0 else ENCHAINEMENTS
        if enchainement not in permis:
            raise ErreurSequence(f"Plan {k + 1} : enchaînement possible : {', '.join(permis)}")
        if enchainement == "rush" and not reference:
            raise ErreurSequence("Plan 1 : pour partir d'un rush, choisis l'image de référence")
        duree = p.get("duree", 5)
        if duree not in plateau.DUREES_VIDEO:
            raise ErreurSequence(f"Plan {k + 1} : durée proposée : {', '.join(map(str, plateau.DUREES_VIDEO))} s")
        note = str(p.get("note") or "").strip()
        if len(note) > LONGUEUR_NOTE:
            raise ErreurSequence(f"Plan {k + 1} : note de {LONGUEUR_NOTE} caractères au plus")
        rush = (reference if enchainement == "rush" else p.get("rush")) if enchainement in ("rush", "transition") else None
        if rush:
            if plateau.fiche(rush).get("type") == "clip":
                raise ErreurSequence(f"Plan {k + 1} : il faut une image, pas un clip")
            plateau.chemin_image(rush)
        propres.append({"enchainement": enchainement, "duree": duree, "note": note, "rush": rush})
    if reference:
        plateau.chemin_image(reference)
    return propres


def demande_decoupage(synopsis: str, personnage_: str, decor: str, plans: list[dict], reference: str | None,
                      contrainte: str = "") -> str:
    """Message envoyé à Llama : l'histoire, le personnage (sauf avec une image de référence), le décor, la contrainte,
    la langue de chaque réplique, puis la liste des plans avec leur type, durée, note et arrivée."""
    lignes = [f"Story: {synopsis.strip()}",
              "Character: " + ("shown by the reference picture" if reference else (personnage_.strip() or "free")),
              f"Setting and look: {decor.strip() or 'free'}"]
    if contrainte:
        lignes.append(f"Mandatory constraint, respected in EVERY shot: {contrainte}")
    toutes = list(dict.fromkeys(repliques(synopsis) + [r for p in plans for r in repliques(p["note"])]))
    if toutes:
        lignes.append("Spoken lines, each in this language: "
                      + "; ".join(f'"{r}" ({langue(r)})' for r in toutes))
    lignes.append("Shots:")
    for k, p in enumerate(plans, 1):
        morceaux = [f"{k}. {TYPES_DECOUPAGE[p['enchainement']]}, {p['duree']} s"]
        if p["note"]:
            morceaux.append(f"note: {p['note']}")
        if p["enchainement"] == "transition":
            arrivee = ""
            if p["rush"]:
                f = plateau.fiche(p["rush"])
                arrivee = f.get("prompt") or f.get("idee") or ""
            morceaux.append(f"arrival: {_extrait(arrivee) or 'the next moment of the story'}")
        lignes.append(", ".join(morceaux))
    return "\n".join(lignes)


def lire_decoupage(texte: str, nombre: int) -> list[tuple[str, str]]:
    """Les blocs SHOT n / FR / EN de la réponse, dans l'ordre ; refuse une réponse incomplète.
    Ministral habille souvent les étiquettes (**FR:**, ### SHOT 1) et sépare les plans par --- : on l'enlève."""
    texte = re.sub(r"\*\*|^\s*#+\s*|^\s*-{3,}\s*$", "", texte, flags=re.M)
    blocs = {int(n): (fr.strip(), en.strip()) for n, fr, en in _BLOC_PLAN.findall(texte)}
    if sorted(blocs) != list(range(1, nombre + 1)):
        raise ErreurSequence(f"Llama a rendu {len(blocs)} plan(s) au lieu de {nombre}")
    return [blocs[k] for k in range(1, nombre + 1)]


_ETIQUETTE = re.compile(r"(?i)^\s*(?:start from the picture|start|cut|transition)\s*[:—-]\s*")
_ETIQUETTE_INTERNE = re.compile(r"(?i),\s*start from the picture\s*:\s*")
_ETIQUETTE_PLAN = re.compile(r"(?i)\b(?:a|an)\s+(?:start|cut|transition|continue)\s+shot\s*[,:—-]\s*(\w)")


def sans_etiquette(texte: str) -> str:
    """Retire les types de plan recopiés par Llama (« Cut: », « start from the picture: », « A cut shot, ») :
    H3 n'a pas à les lire."""
    texte = _ETIQUETTE_INTERNE.sub(", ", _ETIQUETTE.sub("", texte))
    texte = _ETIQUETTE_PLAN.sub(lambda m: m.group(1).upper(), texte).strip()  # la phrase repart en majuscule
    return texte[:1].upper() + texte[1:]


def avec_personnage(personnage_: str, texte: str) -> str:
    """Sans image de référence, l'identité tient au texte : la description du personnage, si Llama l'a omise,
    est remise en tête du plan (à l'essai, il ne la répétait pas dans les plans coupés)."""
    description = personnage_.strip().rstrip(".")
    if not description or _simple(description)[:60] in _simple(texte):
        return texte
    return f"The character is {description[:1].lower()}{description[1:]}. {texte}"


async def decouper(orchestrateur, synopsis: str, personnage_: str, decor: str, plans: list[dict],
                   reference: str | None, llm: str | None = None, mineur: bool = False, contrainte: str = "") -> dict:
    """Prompts de tous les plans d'une séquence, écrits ensemble par Llama, puis contrôlés ici.
    `mineur` : le personnage du Scénario est mineur (AS-18 le sait, même si aucun mot ne le dit).
    `contrainte` : impérative sur tous les plans ; la séquence créée la garde au casting, et le tournage l'ajoute
    au prompt de chaque plan."""
    plans = _valider_decoupage(synopsis, personnage_, decor, plans, reference)
    contrainte = " ".join(str(contrainte or "").split())
    if len(contrainte) > LONGUEUR_CONTRAINTE:
        raise ErreurSequence(f"Contrainte de {LONGUEUR_CONTRAINTE} caractères au plus")
    if reference:
        personnage_ = ""  # l'image fait l'identité : la décrire ferait dériver H3 (essais du mode Séquence)
    rushes_ = [r for r in [reference, *(p["rush"] for p in plans)] if r]
    entrees = [synopsis, personnage_, decor, contrainte, *(p["note"] for p in plans),
               *plateau.textes_origine(rushes_), *(["minor"] if mineur else [])]
    securite.verifier(*entrees)

    wf = workflows.obtenir(CONFIG.get("generation", {}).get("texte_vers_video"))
    mots_max = reglages.valeurs_workflow(wf).get("mots_max") or 110
    regle_personnage = prompt.REGLE_AVEC_REFERENCE if reference else prompt.REGLE_SANS_REFERENCE
    systeme = prompt.consigne("decoupage", llm, mots_max)
    systeme = (systeme.replace("{regle_personnage}", regle_personnage) if "{regle_personnage}" in systeme
               else f"{systeme}\n\nCharacter: {regle_personnage}")  # consigne retouchée sans le repère
    echanges = [{"role": "system", "content": systeme},
                {"role": "user", "content": demande_decoupage(synopsis, personnage_, decor, plans, reference,
                                                              contrainte)}]
    regle = reglages.valeurs_llm(llm)
    debut = time.perf_counter()
    async with orchestrateur.llm(llm) as llama:
        for essai in range(2):  # une réponse mal formée est redemandée une fois
            reponse = await llama.generer(echanges, temperature=regle["temperature_reecriture"],
                                          max_tokens=max(regle["max_tokens"], 260 * len(plans) + 100),
                                          **reglages.echantillonnage(llm))
            try:
                blocs = lire_decoupage(reponse.texte, len(plans))
                break
            except ErreurSequence:
                if essai:
                    raise

    alertes, sortie = [], []
    source = " ".join([synopsis, contrainte, *(p["note"] for p in plans)])  # une réplique du synopsis va où Llama veut
    for k, (p, (fr, en)) in enumerate(zip(plans, blocs), 1):
        texte, inventees = sans_repliques_inventees(source, sans_etiquette(prompt.nettoyer_markdown(en)))
        fr, _ = sans_repliques_inventees(source, prompt.nettoyer_markdown(fr))
        alertes += [f"Plan {k} : réplique inventée par Llama, retirée : « {r} »" for r in inventees]
        if p["enchainement"] == "prolonger" and not texte.lower().startswith("continuous shot"):
            texte = f"Continuous shot, same scene. {texte}"
        if p["enchainement"] in ("texte", "couper"):
            texte = avec_personnage(personnage_, texte)
        texte = garder_repliques(p["note"], texte)  # une réplique de la note est à sa place, mot pour mot
        if len(texte.split()) > mots_max + 20:
            alertes.append(f"Plan {k} : {len(texte.split())} mots, au-delà des {mots_max} conseillés")
        sortie.append({**p, "idee": fr, "prompt": texte})
    tout = " ".join(p["prompt"] for p in sortie)
    for r in repliques(synopsis):
        if _simple(r) not in _simple(tout):
            alertes.append(f"Réplique « {r} » absente du découpage : ajoute-la au plan voulu")
    # Ensemble : un mineur des textes d'origine reste un mineur, même si Llama l'a reformulé.
    securite.verifier(*entrees, tout)
    return {"plans": sortie, "alertes": alertes, "duree_s": round(time.perf_counter() - debut, 2),
            "reference": reference, "contrainte": contrainte}


# Tournage (appelé par la file du Plateau)

def _extrait(texte: str, longueur: int = 400) -> str:
    texte = " ".join((texte or "").split())
    return texte if len(texte) <= longueur else texte[:longueur].rsplit(" ", 1)[0] + "…"


def idee_pour_llm(s: dict, k: int) -> str:
    """Ce que Llama reçoit pour réécrire le plan k : l'idée, l'ambiance en toile de fond, le trajet d'une transition."""
    plan = s["plans"][k]
    morceaux = [plan["idee"]]
    if s["casting"]["ambiance"] and plan.get("ambiance", True):
        # À l'essai, « hangar orange » imposé à un plan « cockpit vert » a donné un éclairage qui change en plein plan,
        # même avec cette consigne : un plan qui se passe ailleurs décoche « suit l'ambiance ».
        morceaux.append("Ambiance commune de la séquence (toile de fond, seulement là où elle convient ; "
                        f"si elle contredit l'idée du plan, l'idée gagne) : {s['casting']['ambiance']}")
    if plan["enchainement"] == "transition":
        # À l'essai, sans savoir d'où il part ni où il arrive, H3 tenait la première image puis sautait à la dernière.
        precedent = s["plans"][k - 1]
        depart = _extrait(precedent.get("prompt_final") or precedent["idee"])
        f = plateau.fiche(plan["rush"])
        arrivee = _extrait(f.get("prompt") or f.get("idee") or plan["idee"])
        morceaux.append(f"TRANSITION. The clip starts exactly on the last image of the previous shot: {depart} "
                        f"It ends exactly on this picture: {arrivee} Describe ONE continuous camera movement travelling "
                        "from the start to the end; the scene changes progressively over the whole clip, never all at "
                        "once at the end.")
    if s["casting"].get("contrainte"):
        morceaux.append("CONTRAINTE OBLIGATOIRE de toute la séquence, à respecter aussi dans ce plan : "
                        f"{s['casting']['contrainte']}")
    morceaux.append(CONSIGNE_SEQUENCE)
    return "\n\n".join(morceaux)


async def traduire_contrainte(orchestrateur, s: dict) -> str | None:
    """La contrainte en anglais, traduite une seule fois par Llama (gardée dans s["contrainte_en"])."""
    contrainte = s["casting"].get("contrainte")
    if not contrainte:
        return None
    cache = s.get("contrainte_en") or {}
    if cache.get("fr") != contrainte:
        en = prompt.nettoyer_markdown(await prompt.traduire_precisions(orchestrateur, contrainte, s["llm"]))
        s["contrainte_en"] = cache = {"fr": contrainte, "en": en}
    return cache["en"]


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

    # 1. Réécriture (Llama), avant de réserver la carte pour ComfyUI.
    if s["casting"].get("contrainte"):
        c.etape = "Traduction de la contrainte"
    contrainte_en = await traduire_contrainte(p_.orchestrateur, s)
    for rang, k in enumerate(c.plans, 1):
        plan = s["plans"][k]
        if plan["prompt"]:
            plan["prompt_final"] = plan["prompt"]
        else:
            c.etape = f"Plan {k + 1} · réécriture ({rang}/{total})"
            mode = "animer" if plan["enchainement"] == "rush" else "texte"
            reecrit = (await prompt.reecrire(p_.orchestrateur, idee_pour_llm(s, k), s["llm"], video=mode,
                                             duree=plan["duree"]))["prompt"]
            source = " ".join([plan["idee"], s["casting"]["ambiance"], s["casting"].get("contrainte", "")])
            reecrit, _ = sans_repliques_inventees(source, reecrit)
            plan["prompt_final"] = garder_repliques(plan["idee"], reecrit)
        if plan["enchainement"] == "prolonger" and not plan["prompt"]:
            plan["prompt_final"] = f"Continuous shot, same scene. {plan['prompt_final']}"
        plan["prompt_final"] = avec_contrainte(plan["prompt_final"], contrainte_en)  # prompts imposés compris
        securite.verifier(s["casting"]["ambiance"], s["casting"].get("contrainte", ""), plan["idee"],
                          plan["prompt_final"], *textes)
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
        mp4 = next((o for n, o in sorties if n.lower().endswith(".mp4")), None)
        affiche = next((o for n, o in sorties if n.lower().endswith(".png")), None)
        if not (mp4 and affiche):
            raise plateau.ErreurPlateau(f"Plan {k + 1} : ComfyUI n'a pas renvoyé la vidéo attendue")
        ancien = plan.get("clip")
        plan["clip"] = plateau.enregistrer_clip(mp4, affiche, {
            "type": "clip", "mode_video": "sequence", "sequence": s["id"], "plan": k + 1,
            "enchainement": plan["enchainement"], "idee": plan["idee"], "prompt": prompt_plan, "llm": s["llm"],
            "workflow": CONFIG.get("generation", {}).get("image_vers_video"), "qualite": s["qualite"],
            "duree": plan["duree"], "megapixels": plateau.QUALITES_VIDEO[s["qualite"]]["megapixels"],
            "steps": steps_plan(s, plan), "voix": ref_voix,
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
    mp4 = next((o for n, o in sorties if n.lower().endswith(".mp4")), None)
    affiche = next((o for n, o in sorties if n.lower().endswith(".png")), None)
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
