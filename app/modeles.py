"""Registre, inventaire et suppression des modèles du dossier modeles/.

Rangement, l'image et la vidéo chacune de son côté, sans rien partager :
    modeles/<nature>/<Famille>/modeles/   variantes du modèle de diffusion d'une famille (ex. image/Krea2)
    modeles/<nature>/<Famille>/loras/     LoRAs compatibles avec cette famille
    modeles/<nature>/_base/<type>/        fichiers de la nature : encodeurs de texte, VAE, LoRAs internes aux workflows…
    modeles/llm/                          GGUF pour llama.cpp
<nature> : image ou video. Les familles et leur nature sont celles déclarées par les fiches de workflow ; déposer un
fichier au bon endroit suffit. Avant le 2026-11-10, tout était à plat (modeles/_base, modeles/<Famille>) :
app/migration_modeles.py range l'ancienne organisation au démarrage.
"""

import copy
import hashlib
import json
import logging
import shutil
from datetime import datetime
from pathlib import Path

import yaml

from app import fiches, fichier_reglages, memo
from app.config import CONFIG, ROOT
from app.emplacements import CACHE, DATA, MODELES

DOSSIER = MODELES
REGISTRE = DATA / "modeles.json"
CHEMINS_COMFYUI = CACHE / "comfyui_chemins.yaml"  # réécrit à chaque démarrage de ComfyUI
TYPES_BASE = ["checkpoints", "diffusion_models", "text_encoders", "vae", "loras",
              "clip_vision", "controlnet", "upscale_models", "model_patches", "audio_encoders",
              "latent_upscale_models"]
EXTENSIONS_IMAGE = {".safetensors"}  # jamais de .ckpt/.pt : ils peuvent exécuter du code au chargement
EXTENSIONS_MODELES = EXTENSIONS_IMAGE | {".gguf"}  # l'inventaire ignore le reste (recommandations.yaml…)
EMPREINTES = CACHE / "empreintes.json"  # SHA-256 déjà calculés (recalculables) : {clé: {taille, date, sha256}}


NATURES = ("image", "video")
# La nature d'une famille : vidéo dès qu'un de ses workflows fait de la vidéo (app.workflows s'en sert aussi).
NATURES_TACHE = {"texte_vers_image": "image", "retouche": "image", "image_vers_video": "video",
                 "texte_vers_video": "video"}
BASE = "_base"
log = logging.getLogger(__name__)


class ErreurModele(ValueError):
    pass


_familles = memo.SelonFichiers()


def _natures() -> dict[str, str]:
    """{famille: nature} d'après les fiches de workflow (lues sans importer app.workflows), relues seulement quand une
    fiche change : destination() s'en sert pour chaque fichier vérifié."""
    chemins = sorted((ROOT / "workflows").glob("*/fiche.yaml"))

    def lire() -> dict[str, str]:
        natures = {}
        for c in chemins:
            fiche = memo.lire_yaml(c) or {}
            if fiche.get("famille"):
                nature = NATURES_TACHE.get(fiche.get("tache"), "image")
                if natures.get(fiche["famille"]) != "video":
                    natures[fiche["famille"]] = nature
        return dict(sorted(natures.items()))

    return dict(_familles.lire(memo.empreinte(chemins), lire))


def familles() -> list[str]:
    return list(_natures())


def nature(famille: str) -> str:
    """image ou video : le côté du dossier modeles/ où vit la famille."""
    try:
        return _natures()[famille]
    except KeyError:
        raise ErreurModele(f"Famille inconnue : {famille}") from None


def dossier_famille(famille: str, sous: str = "modeles") -> str:
    """« image/Krea2/modeles », « video/MiniMax-H3/loras »…"""
    return f"{nature(famille)}/{famille}/{sous}"


def dossier_base(nature_: str, type_: str) -> str:
    return f"{nature_}/{BASE}/{type_}"


def decouper(dossier: str) -> tuple[str | None, str | None, str]:
    """(nature, famille ou None pour _base, sous-dossier) : « image/Krea2/loras » → (image, Krea2, loras),
    « video/_base/vae » → (video, None, vae), « llm » → (None, None, llm)."""
    morceaux = dossier.split("/")
    if len(morceaux) == 3 and morceaux[0] in NATURES:
        return morceaux[0], None if morceaux[1] == BASE else morceaux[1], morceaux[2]
    return None, None, dossier


def dossiers() -> list[str]:
    """Sous-dossiers où l'appli peut ranger un fichier."""
    return ([dossier_base(n, t) for n in NATURES for t in TYPES_BASE]
            + [dossier_famille(f, sous) for f in familles() for sous in ("modeles", "loras")]
            + ["llm"])


def fichiers(dossier: str, extensions: set[str]) -> list[str]:
    """Noms des fichiers déposés dans un sous-dossier (sans les .part en cours de téléchargement)."""
    racine = DOSSIER / dossier
    if not racine.is_dir():
        return []
    return sorted(p.name for p in racine.iterdir() if p.is_file() and p.suffix.lower() in extensions)


def llm_disponibles() -> list[str]:
    """Les GGUF de modeles/llm, sans les projecteurs d'images (mmproj) : ce ne sont pas des LLM."""
    return [f for f in fichiers("llm", {".gguf"}) if "mmproj" not in f.lower()]


def llm_defaut() -> str:
    """Le LLM par défaut : celui de config.yaml s'il est installé, sinon le premier favori installé (modeles_au_choix),
    sinon le premier GGUF installé qui n'est pas un spécialiste. Sans ce repli, supprimer le GGUF de config.yaml
    (Ministral, le 2026-11-02) faisait échouer toute réécriture sans LLM choisi. Rien d'installé : celui de config.yaml,
    et l'erreur dit qu'il manque."""
    llama, presents = CONFIG["llama"], llm_disponibles()
    candidats = [llama["modele"], *(m["fichier"] for m in llama.get("modeles_au_choix", [])),
                 *(f for f in presents if not fiches.specialiste(f))]
    return next((f for f in candidats if f in presents), llama["modele"])


# LoRAs : mots déclencheurs et force retenus d'une utilisation à l'autre (section « loras » de reglages.json,
# app/fichier_reglages.py ; avant, data/loras.json), à défaut ceux fournis par la source au téléchargement (registre).

def _memoire_loras() -> dict:
    return copy.deepcopy(fichier_reglages.section("loras"))


USAGE_LORA = ("force_auteur", "images_force", "prompt_exemple")  # lus dans les images d'exemple (Civitai)
FORCE_LORA = (0.0, 2.0)
LONGUEUR_DECLENCHEURS = 300


def reglage_lora(cle_lora: str, registre: dict | None = None, memoire: dict | None = None) -> dict:
    """Mots déclencheurs et force d'une LoRA : ceux que l'utilisateur a réglés, sinon ceux de la source (mots
    déclarés par l'auteur, force de ses images d'exemple), sinon 1,0."""
    retenu = (memoire if memoire is not None else _memoire_loras()).get(cle_lora, {})
    infos = (registre if registre is not None else charger_registre()).get(cle_lora) or {}
    source = ", ".join(infos.get("mots_declencheurs") or [])
    return {
        "declencheurs": retenu.get("declencheurs", source),
        "force": retenu.get("force", infos.get("force_auteur") or 1.0),
        "regle": bool(retenu),  # réglé par l'utilisateur (au Plateau ou dans Ma collection)
        "declencheurs_source": source,
        **{k: infos.get(k) for k in USAGE_LORA},
        "civitai": infos.get("source") == "civitai",
    }


def loras(famille: str) -> list[dict]:
    registre, memoire = charger_registre(), _memoire_loras()
    liste = []
    dossier = dossier_famille(famille, "loras")
    for nom in fichiers(dossier, EXTENSIONS_IMAGE):
        cle_lora = f"{dossier}/{nom}"
        r = reglage_lora(cle_lora, registre, memoire)
        liste.append({"fichier": nom, "nom": Path(nom).stem, "declencheurs": r["declencheurs"], "force": r["force"],
                      "page": (registre.get(cle_lora) or {}).get("page")})
    return liste


def _famille_lora(cle_lora: str) -> tuple[str, str]:
    """<nature>/<Famille>/loras/<fichier> → (famille, fichier) ; les LoRA de _base sont internes aux workflows."""
    dossier, _, fichier = cle_lora.rpartition("/")
    _, famille, sous = decouper(dossier)
    if famille is None or sous != "loras" or famille not in familles() or dossier != dossier_famille(famille, "loras") \
            or fichier not in fichiers(dossier, EXTENSIONS_IMAGE):
        raise ErreurModele(f"LoRA inconnue : {cle_lora}")
    return famille, fichier


def regler_lora(cle_lora: str, declencheurs: str, force: float) -> dict:
    """Ma collection : les mots déclencheurs et la force d'une LoRA, retenus comme ceux réglés au Plateau."""
    famille, fichier = _famille_lora(cle_lora)
    declencheurs = " ".join(str(declencheurs or "").split())
    if len(declencheurs) > LONGUEUR_DECLENCHEURS:
        raise ErreurModele(f"Mots déclencheurs : {LONGUEUR_DECLENCHEURS} caractères au plus")
    if not FORCE_LORA[0] <= force <= FORCE_LORA[1]:
        raise ErreurModele("Force de LoRA entre 0 et 2")
    memoriser_loras(famille, [{"fichier": fichier, "declencheurs": declencheurs, "force": round(force, 2)}])
    return reglage_lora(cle_lora)


def oublier_reglage_lora(cle_lora: str) -> dict:
    """Revient aux valeurs de la source (mots de l'auteur, force de ses images d'exemple)."""
    _famille_lora(cle_lora)
    memoire = _memoire_loras()
    if memoire.pop(cle_lora, None) is not None:
        fichier_reglages.ecrire("loras", memoire)
    return reglage_lora(cle_lora)


def mettre_a_jour_source(cle_lora: str, infos: dict):
    """Relire Civitai : les mots déclarés et l'usage de l'auteur remplacent ceux du registre (le reste est gardé)."""
    registre = charger_registre()
    entree = registre.get(cle_lora)
    if entree is None:
        raise ErreurModele(f"Fichier absent du registre : {cle_lora}")
    entree["mots_declencheurs"] = infos.get("mots_declencheurs") or []
    for k in USAGE_LORA:
        entree[k] = infos.get(k)
    _sauver_registre(registre)


def memoriser_loras(famille: str, choix: list[dict]):
    memoire = _memoire_loras()
    for c in choix:
        memoire[f"{dossier_famille(famille, 'loras')}/{c['fichier']}"] = {"declencheurs": c["declencheurs"], "force": c["force"]}
    fichier_reglages.ecrire("loras", memoire)


def ecrire_chemins_comfyui() -> Path:
    """Déclare à ComfyUI les dossiers de modèles, une racine par nature (relu à chaque démarrage). ComfyUI cherche un
    fichier par son nom dans toutes les racines : homonymes() signale deux fichiers du même nom de part et d'autre."""
    for f in familles():
        for sous in ("modeles", "loras"):
            (DOSSIER / dossier_famille(f, sous)).mkdir(parents=True, exist_ok=True)
    declaration = {}
    for n in NATURES:
        siennes = [f for f in familles() if nature(f) == n]
        chemins = {t: f"{BASE}/{t}/" for t in TYPES_BASE}
        chemins["diffusion_models"] = "\n".join([chemins["diffusion_models"], *(f"{f}/modeles/" for f in siennes)])
        chemins["loras"] = "\n".join([chemins["loras"], *(f"{f}/loras/" for f in siennes)])
        declaration[f"aria_{n}"] = {"base_path": str(DOSSIER / n), **({"is_default": True} if n == "image" else {}),
                                    **chemins}
    if doubles := homonymes():
        log.warning("Même nom côté image et côté vidéo, ComfyUI n'en verra qu'un : %s", ", ".join(doubles))
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


def est_checkpoint(dossier: str) -> bool:
    """<nature>/<Famille>/modeles : les checkpoints que les workflows proposent en variantes."""
    _, famille, sous = decouper(dossier)
    return famille is not None and sous == "modeles"


def homonymes() -> list[str]:
    """Fichiers du même nom côté image et côté vidéo : ComfyUI, qui cherche par nom, n'en verrait qu'un."""
    vus: dict[str, set[str]] = {}
    for n in NATURES:
        racine = DOSSIER / n
        if racine.is_dir():
            for f in racine.rglob("*"):
                if f.is_file() and f.suffix.lower() in EXTENSIONS_MODELES:
                    vus.setdefault(f.name, set()).add(n)
    return sorted(nom for nom, natures in vus.items() if len(natures) > 1)


def suggestion_steps(cle_modele: str) -> dict | None:
    """La suggestion de steps lue à son téléchargement (sources.steps), tant qu'il n'est pas validé."""
    return ((charger_registre().get(cle_modele) or {}).get("a_valider") or {}).get("steps")


def oublier_attente(cle_modele: str):
    """Le checkpoint est validé : sa suggestion de steps n'a plus lieu d'être. Ce n'est pas elle qui le met en service
    (ce sont ses steps validés, workflows.Workflow.steps_de) ; sans entrée au registre, rien à faire."""
    registre = charger_registre()
    if (registre.get(cle_modele) or {}).pop("a_valider", None) is not None:
        _sauver_registre(registre)


def connu(dossier: str, nom: str) -> bool:
    """Le fichier a-t-il été téléchargé par Aria (source et licence au registre), ou déposé à la main ?"""
    return f"{dossier}/{nom}" in charger_registre()


def charger_registre() -> dict[str, dict]:
    if not REGISTRE.is_file():
        return {}
    return json.loads(REGISTRE.read_text(encoding="utf-8"))


def _sauver_registre(registre: dict[str, dict]):
    REGISTRE.parent.mkdir(parents=True, exist_ok=True)
    temporaire = REGISTRE.with_suffix(".json.tmp")
    temporaire.write_text(json.dumps(registre, indent=4, ensure_ascii=False), encoding="utf-8")
    temporaire.replace(REGISTRE)


def enregistrer(fichier: Path, fiche: dict, sha256: str, **en_plus):
    """Inscrit un fichier au registre : téléchargé par Aria, ou retrouvé (en_plus : retrouve=True, nom_modele…)."""
    registre = charger_registre()
    registre[cle(fichier)] = {
        "source": fiche["source"],
        "page": fiche["page"],
        "url": fiche["url"],
        "licence": fiche["licence"],
        "mots_declencheurs": fiche.get("mots_declencheurs") or [],
        "base": fiche.get("base"),
        **{k: fiche[k] for k in USAGE_LORA if fiche.get(k) is not None},
        "taille": fichier.stat().st_size,
        "sha256": sha256,
        "date": datetime.now().isoformat(timespec="seconds"),
        **en_plus,
    }
    _sauver_registre(registre)


def _empreintes() -> dict:
    try:
        return json.loads(EMPREINTES.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def empreinte_connue(fichier: Path) -> str | None:
    """Le SHA-256 déjà calculé de ce fichier, s'il n'a pas changé depuis (même taille, même date)."""
    e = _empreintes().get(cle(fichier))
    s = fichier.stat()
    return e["sha256"] if e and e.get("taille") == s.st_size and e.get("date") == s.st_mtime else None


def empreinte(fichier: Path) -> str:
    """SHA-256 du fichier (long : une trentaine de secondes pour 13 Go), gardé pour la fois suivante."""
    if connue := empreinte_connue(fichier):
        return connue
    s = fichier.stat()
    h = hashlib.sha256()
    with fichier.open("rb") as f:
        while bloc := f.read(16 << 20):
            h.update(bloc)
    toutes = _empreintes()
    toutes[cle(fichier)] = {"taille": s.st_size, "date": s.st_mtime, "sha256": h.hexdigest()}
    EMPREINTES.parent.mkdir(parents=True, exist_ok=True)
    EMPREINTES.write_text(json.dumps(toutes, indent=4), encoding="utf-8")
    return h.hexdigest()


def cle_par_empreinte(sha256: str) -> str | None:
    """Le fichier installé qui a cette empreinte (registre, ou cache des empreintes calculées), s'il y en a un."""
    sha256 = sha256.lower()
    connus = {i["sha256"].lower(): c for c, i in charger_registre().items() if i.get("sha256")}
    connus.update({e["sha256"].lower(): c for c, e in _empreintes().items()})
    cle_ = connus.get(sha256)
    return cle_ if cle_ and (DOSSIER / cle_).is_file() else None


def empreintes_installees() -> set[str]:
    """Les SHA-256 connus des fichiers installés (registre et cache) : « déjà installé ✓ » dans Rechercher."""
    connues = {i["sha256"].lower() for i in charger_registre().values() if i.get("sha256")}
    return connues | {e["sha256"].lower() for e in _empreintes().values()}


def inventaire() -> list[dict]:
    registre = charger_registre()
    modeles = []
    for dossier in dossiers():
        racine = DOSSIER / dossier
        if not racine.is_dir():
            continue
        for fichier in sorted(racine.rglob("*")):
            if not fichier.is_file() or fichier.suffix.lower() not in EXTENSIONS_MODELES:
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


# --- corbeille ---------------------------------------------------------------------------------------------------
# Un modèle supprimé n'est jamais effacé d'un coup : il part dans <dossier des modèles>\.corbeille\, au même chemin
# relatif. C'est un renommage sur le même disque, instantané même pour 20 Go (dans data\, un dossier des modèles rangé
# sur un autre disque obligerait à tout recopier). Ni ComfyUI ni l'inventaire ne voient .corbeille\ : ils ne lisent
# que les sous-dossiers de dossiers(). Sa fiche quitte le registre pour CORBEILLE_REGISTRE, avec la date et le dossier
# des modèles d'alors, et y revient à la restauration. Seul vider_corbeille() efface pour de bon.

CORBEILLE = ".corbeille"
CORBEILLE_REGISTRE = DATA / "corbeille_modeles.json"


def _dans(base: Path, cle_modele: str) -> Path:
    chemin = (base / cle_modele).resolve()
    if not chemin.is_relative_to(base.resolve()) or chemin == base.resolve():
        raise ErreurModele("Chemin hors du dossier des modèles")
    return chemin


def _charger_corbeille() -> dict[str, dict]:
    try:
        d = json.loads(CORBEILLE_REGISTRE.read_text(encoding="utf-8"))
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


def _sauver_corbeille(corbeille: dict[str, dict]):
    CORBEILLE_REGISTRE.parent.mkdir(parents=True, exist_ok=True)
    temporaire = CORBEILLE_REGISTRE.with_suffix(".tmp")
    temporaire.write_text(json.dumps(corbeille, indent=2, ensure_ascii=False), encoding="utf-8")
    temporaire.replace(CORBEILLE_REGISTRE)


def _fichier_corbeille(cle_modele: str, entree: dict) -> Path:
    base = Path(entree.get("dossier_modeles") or DOSSIER)
    return _dans(base / CORBEILLE, cle_modele)


def mettre_a_la_corbeille(cle_modele: str) -> dict:
    """Réservé aux modèles téléchargés par l'appli (un fichier déposé à la main reste à l'utilisateur)."""
    registre = charger_registre()
    if cle_modele not in registre:
        raise ErreurModele("Seuls les modèles téléchargés par l'appli peuvent être supprimés")
    fichier = _dans(DOSSIER, cle_modele)
    corbeille = _charger_corbeille()
    if fichier.is_file():
        cible = _dans(DOSSIER / CORBEILLE, cle_modele)
        if cle_modele in corbeille or cible.exists():
            raise ErreurModele(f"{fichier.name} est déjà dans la corbeille : restaure-le ou vide la corbeille d'abord")
        cible.parent.mkdir(parents=True, exist_ok=True)
        try:
            fichier.rename(cible)
        except OSError as e:
            raise ErreurModele(f"{fichier.name} n'a pas pu aller à la corbeille ({e.strerror or e}) : il est peut-être "
                               "ouvert par ComfyUI. Arrête-le, puis réessaie") from e
        corbeille[cle_modele] = {**registre[cle_modele], "corbeille": datetime.now().isoformat(timespec="seconds"),
                                 "dossier_modeles": str(DOSSIER)}
        _sauver_corbeille(corbeille)
    # Sans fichier (déjà effacé à la main), il n'y a rien à garder : seule sa fiche s'en va
    del registre[cle_modele]
    _sauver_registre(registre)
    return {"corbeille": cle_modele}


def corbeille() -> dict:
    """Ce que montre Modèles → Ma collection : chaque modèle à la corbeille, sa taille, et la place qu'ils occupent."""
    elements, total = [], 0
    for cle_modele, entree in sorted(_charger_corbeille().items(), key=lambda e: e[1].get("corbeille", ""), reverse=True):
        try:
            chemin = _fichier_corbeille(cle_modele, entree)
        except ErreurModele:
            continue
        present = chemin.is_file()
        taille = chemin.stat().st_size if present else 0
        total += taille
        dossier_modeles = entree.get("dossier_modeles") or str(DOSSIER)
        elements.append({"cle": cle_modele, "nom": Path(cle_modele).name, "taille": taille, "present": present,
                         "date": entree.get("corbeille"), "dossier_modeles": dossier_modeles,
                         "ailleurs": Path(dossier_modeles).resolve() != DOSSIER.resolve()})
    return {"modeles": elements, "taille": total}


def restaurer(cle_modele: str) -> dict:
    """Le fichier revient à sa place et sa fiche au registre. Refusé si le même modèle a été retéléchargé entre-temps,
    ou s'il est dans la corbeille d'un ancien dossier des modèles (Aria ne lirait pas le fichier restauré)."""
    corbeille_ = _charger_corbeille()
    entree = corbeille_.get(cle_modele)
    if entree is None:
        raise ErreurModele("Ce modèle n'est pas dans la corbeille")
    base = Path(entree.get("dossier_modeles") or DOSSIER)
    if base.resolve() != DOSSIER.resolve():
        raise ErreurModele(f"Ce modèle est dans la corbeille de l'ancien dossier des modèles ({base}) : reprends ce "
                           "dossier dans Modèles pour le restaurer, ou vide la corbeille")
    source, cible = _fichier_corbeille(cle_modele, entree), _dans(DOSSIER, cle_modele)
    registre = charger_registre()
    if cible.exists() or cle_modele in registre:
        raise ErreurModele(f"{cible.name} est déjà présent (retéléchargé ?) : la copie de la corbeille n'est pas "
                           "restaurée, vide la corbeille pour libérer la place")
    if not source.is_file():
        del corbeille_[cle_modele]
        _sauver_corbeille(corbeille_)
        raise ErreurModele(f"{source.name} n'est plus dans la corbeille (effacé à la main ?) : rien à restaurer")
    cible.parent.mkdir(parents=True, exist_ok=True)
    try:
        source.rename(cible)
    except OSError as e:
        raise ErreurModele(f"{source.name} n'a pas pu être restauré ({e.strerror or e})") from e
    registre[cle_modele] = {k: v for k, v in entree.items() if k not in ("corbeille", "dossier_modeles")}
    _sauver_registre(registre)
    del corbeille_[cle_modele]
    _sauver_corbeille(corbeille_)
    _supprimer_dossiers_vides(DOSSIER / CORBEILLE)  # les sous-dossiers vidés par la restauration
    return {"restaure": cle_modele}


def vider_corbeille() -> dict:
    """La seule suppression définitive : chaque fichier de la corbeille (anciens dossiers des modèles compris)."""
    corbeille_ = _charger_corbeille()
    effaces, liberes, restes = 0, 0, {}
    for cle_modele, entree in corbeille_.items():
        try:
            chemin = _fichier_corbeille(cle_modele, entree)
            if chemin.is_file():
                taille = chemin.stat().st_size
                chemin.unlink()
                liberes += taille
            effaces += 1
        except (OSError, ErreurModele) as e:
            log.warning("Corbeille : %s n'a pas pu être effacé (%s)", cle_modele, e)
            restes[cle_modele] = entree
    for base in {Path(e.get("dossier_modeles") or DOSSIER) for e in corbeille_.values()}:
        _supprimer_dossiers_vides(base / CORBEILLE)
    _sauver_corbeille(restes)
    return {"effaces": effaces, "liberes": liberes, "restes": len(restes)}


def _supprimer_dossiers_vides(dossier: Path):
    if not dossier.is_dir():
        return
    for d in sorted((p for p in dossier.rglob("*") if p.is_dir()), key=lambda p: len(p.parts), reverse=True):
        try:
            d.rmdir()
        except OSError:
            pass  # pas vide : il reste un fichier, on n'y touche pas
    try:
        dossier.rmdir()
    except OSError:
        pass
