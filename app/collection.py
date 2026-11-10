"""Onglet Modèles : « Ma collection » (ce qui est installé, d'où ça vient, à quoi ça sert), les usages (workflows
des onglets Texte → image, Image → image et Vidéo, avec leurs favoris) et « Retrouver les sources ».

Retrouver les sources : un fichier déposé à la main n'a ni page, ni déclencheurs, ni licence. Son SHA-256 (calculé
une fois, gardé dans data/empreintes.json) est d'abord comparé aux fiches d'Aria, puis demandé à Civitai
(/model-versions/by-hash). Seulement sur un clic : envoyer une empreinte, c'est dire à Civitai qu'on a ce fichier.
Ce qui est retrouvé entre au registre avec « retrouve » ; le reste est dit introuvable.
"""

import asyncio
import copy

import httpx

from app import assistant, emplacements, fiches, migration_modeles, modeles, plateau, reglages, workflows
from app.config import CONFIG
from app.sources import Fiche, civitai

GROUPES = {"image": "Modèles image", "video": "Modèles vidéo", "lora": "LoRA", "llm": "LLM",
           "commun_image": "Fichiers de base · image (encodeurs, VAE…)",
           "commun_video": "Fichiers de base · vidéo (encodeurs, VAE…)"}

# Onglets d'usage : les tâches de leurs workflows, et les favoris qu'on y règle (config.yaml, section generation).
USAGES = {
    "texte_vers_image": {"nom": "Texte → image", "taches": ["texte_vers_image"],
                         "favoris": {"texte_vers_image": "Créer une image"}},
    "image_vers_image": {"nom": "Image → image", "taches": ["retouche"],
                         "favoris": {"retouche": "Retoucher", "references": "Créer depuis des références"}},
    "video": {"nom": "Vidéo", "taches": ["image_vers_video", "texte_vers_video"],
              "favoris": {"image_vers_video": "Animer une image", "texte_vers_video": "Clip depuis un texte"}},
}


class ErreurCollection(ValueError):
    pass


def _groupe(dossier: str, infos: dict) -> tuple[str, str | None]:
    """(groupe, famille) d'un sous-dossier des modèles."""
    if dossier == "llm":
        return "llm", None
    nature, famille, sous = modeles.decouper(dossier)
    if famille is None:
        return f"commun_{nature}", None
    if sous == "loras":
        return "lora", famille
    return infos.get(famille, {}).get("nature", "image"), famille


def _utilisations(catalogue: list) -> dict[str, list[str]]:
    """Clé d'un fichier -> les workflows qui le chargent (fichiers des fiches, checkpoints et LoRA de la famille)."""
    par_cle = {}
    for wf in catalogue:
        for f in wf.fiche["fichiers"]:
            par_cle.setdefault(f"{f['dossier']}/{f['nom']}", []).append(wf.fiche["nom"])
        for sous in ("modeles", "loras"):
            dossier = modeles.dossier_famille(wf.famille, sous)
            for nom in modeles.fichiers(dossier, modeles.EXTENSIONS_IMAGE):
                noms = par_cle.setdefault(f"{dossier}/{nom}", [])
                if wf.fiche["nom"] not in noms:
                    noms.append(wf.fiche["nom"])
    return par_cle


def _steps_dits_par_la_fiche(wf) -> list[dict]:
    """Ce que la fiche du workflow dit des steps de son checkpoint d'origine (défaut du template, recommandations)."""
    dits = []
    if (defaut := wf.fiche.get("defauts", {}).get("steps")) is not None:
        dits.append({"valeur": defaut, "origine": "défaut du template du workflow", "source": None})
    for r in (wf.fiche.get("recommandations") or {}).get("valeurs", []):
        if r.get("cle") == "steps":
            dits.append({"valeur": r["valeur"], "origine": r.get("note") or "recommandé", "source": r.get("source")})
    return dits


def _steps_des_checkpoints(catalogue: list) -> dict[str, dict]:
    """Clé d'un checkpoint de famille -> {"valides": steps connus} ou {"a_valider": de quoi décider}. Le premier
    workflow de la famille qui règle des steps fait foi (valider les pose dans tous)."""
    sortie = {}
    for wf in catalogue:
        if "steps" not in wf.fiche["entrees"]:
            continue
        for nom in wf.presents():
            cle = f"{modeles.dossier_famille(wf.famille)}/{nom}"
            if cle in sortie:
                continue
            if wf.a_valider(nom):
                sortie[cle] = {"a_valider": {
                    "steps": modeles.suggestion_steps(cle),  # lue sur sa page au téléchargement
                    "fiche": _steps_dits_par_la_fiche(wf) if nom == wf.modele_origine else [],
                }}
            else:
                sortie[cle] = {"valides": wf.steps_de(nom)}
    return sortie


def est_lora(dossier: str) -> bool:
    """LoRA d'une famille (celles de _base sont internes aux workflows)."""
    _, famille, sous = modeles.decouper(dossier)
    return famille is not None and sous == "loras"


def collection() -> dict:
    catalogue = workflows.catalogue()
    infos = workflows.familles()
    utilise = _utilisations(catalogue)
    steps = _steps_des_checkpoints(catalogue)
    llm_defaut = modeles.llm_defaut()
    groupes = {g: [] for g in GROUPES}
    for m in modeles.inventaire():
        groupe, famille = _groupe(m["dossier"], infos)
        i = m["infos"] or {}
        origine = "main" if not m["suivi"] else "retrouve" if i.get("retrouve") else "aria"
        utilise_par = utilise.get(m["cle"], [])
        if groupe == "llm":
            fiche = fiches.fiche_llm(m["nom"])
            utilise_par = ["Réécriture des prompts (par défaut)"] if m["nom"] == llm_defaut else []
            if fiche and not i:
                i = {"page": fiche["page"], "licence": fiche["licence"]["nom"]}  # connu d'Aria, déposé à la main
        groupes[groupe].append({
            "cle": m["cle"], "nom": m["nom"], "dossier": m["dossier"], "famille": famille, "taille": m["taille"],
            "origine": origine, "source": i.get("source"), "page": i.get("page"), "licence": i.get("licence"),
            "declencheurs": i.get("mots_declencheurs") or [], "base": i.get("base"), "nom_modele": i.get("nom_modele"),
            "utilise_par": utilise_par, "supprimable": m["suivi"],
            # Checkpoint de famille : ses steps validés, ou ce qu'il faut pour les valider (il n'est proposé nulle part
            # avant : workflows.Workflow.a_valider).
            "steps": (steps.get(m["cle"]) or {}).get("valides"),
            "a_valider": (steps.get(m["cle"]) or {}).get("a_valider"),
            # LoRA d'une famille : mots déclencheurs et force, réglables ici (les LoRA de _base sont internes),
            # et ses usages (image, retouche, video), ceux des workflows de sa famille : les filtres du groupe LoRA.
            "lora": modeles.reglage_lora(m["cle"]) if est_lora(m["dossier"]) else None,
            "usages": infos.get(famille, {}).get("usages", []) if est_lora(m["dossier"]) else [],
        })
    return {
        "groupes": [{"id": g, "nom": GROUPES[g], "elements": e, "taille": sum(x["taille"] for x in e)}
                    for g, e in groupes.items()],
        "orphelins": sum(x["origine"] == "main" for e in groupes.values() for x in e),
        "espace_libre": modeles.espace_libre(),
        "fiches_ecartees": workflows.erreurs(),
        "migration": migration_modeles.etat(),  # rangement image / vidéo du 2026-11-10 : son rapport
    }


# Usages : un onglet par façon de créer

def _fichiers_llm(fiche: dict) -> list[dict]:
    """Ce qu'il faut télécharger pour un LLM du catalogue : le GGUF et sa vision, et pour un spécialiste sa consigne
    et sa vision."""
    s = fiche.get("specialiste") or {}
    return [{"nom": fiche["id"], "url": fiche["url"], "taille": fiche["taille"], "sha256": fiche["sha256"]}] \
        + ([fiche["vision"]] if fiche.get("vision") else []) + [s[k] for k in ("consigne", "vision") if s.get(k)]


def _specialiste(fiche: dict, choisi: str | None) -> dict:
    fichiers = _fichiers_llm(fiche)
    manquants = [f for f in fichiers if not modeles.destination("llm", f["nom"]).is_file()]
    return {"fichier": fiche["id"], "nom": fiche["nom"], "page": fiche["page"], "usage": fiche["specialiste"]["usage"],
            "taille": sum(f["taille"] for f in fichiers), "a_telecharger": sum(f["taille"] for f in manquants),
            "installe": not manquants, "choisi": fiche["id"] == choisi, "vision": bool(fiche["specialiste"].get("vision")),
            "licence": {"nom": fiche["licence"]["nom"], "url": fiche["licence"]["url"], "resume": fiche["licence"]["resume"]}}


def _reecriture(wf) -> dict:
    """Ce que le workflow attend du LLM (profil de sa fiche, longueur réglable), le LLM qui réécrit pour lui, et les
    LLM entraînés pour ce modèle (installables, ou connus sans GGUF)."""
    profil = wf.profil
    choisi = workflows.llms_choisis().get(wf.id)
    s = fiches.specialiste(choisi)
    noms = {w.id: w.fiche["nom"] for w in workflows.catalogue()}
    return {
        "llm": choisi,  # None : le LLM par défaut
        "effectif": workflows.llm_de(wf.id),
        "style": profil["style"], "langue": profil["langue"],
        "mots_max": reglages.valeurs_workflow(wf).get("mots_max") or profil.get("mots_max"),
        "regles": len(profil.get("regles") or []),
        "specialistes": [_specialiste(f, choisi) for f in fiches.specialistes(wf.id)],
        "a_venir": [{"nom": a["nom"], "page": a["page"], "raison": a["raison"]} for a in fiches.a_venir(wf.id)],
        # Un spécialiste choisi pour un autre modèle : Aria passera au LLM par défaut pour ce workflow.
        "ailleurs": [noms.get(i, i) for i in s["workflows"]] if s and wf.id not in s["workflows"] else None,
    }


def usages() -> dict:
    catalogue = workflows.catalogue()
    generation = CONFIG.get("generation", {})
    sortie = {"llm": {"defaut": modeles.llm_defaut(), "choix": plateau.llm_au_choix()}}
    for id_usage, u in USAGES.items():
        liste = []
        for wf in catalogue:
            if wf.fiche["tache"] not in u["taches"]:
                continue
            possibles = [c for c in u["favoris"] if c == wf.fiche["tache"] or (c == "references" and wf.accepte_references())]
            liste.append({**wf.resume(), "modele_defaut": wf.choisi(None), "possibles": possibles,
                          "favori_pour": [c for c in possibles if generation.get(c) == wf.id],
                          "references": wf.accepte_references(), "variation": wf.accepte_variation(),
                          "reecriture": _reecriture(wf)})
        sortie[id_usage] = {
            "nom": u["nom"], "workflows": liste,
            "favoris": [{"cle": c, "libelle": l, "valeur": generation.get(c)} for c, l in u["favoris"].items()],
            # Image → image : la Variation part d'une image avec un modèle texte vers image.
            "variation": [wf.fiche["nom"] for wf in catalogue
                          if id_usage == "image_vers_image" and wf.fiche["tache"] == "texte_vers_image" and wf.accepte_variation()],
        }
    return sortie


def definir_favori(cle: str, id_workflow: str) -> dict:
    """Le workflow utilisé par défaut pour un usage (retenu dans aria.yaml, comme les favoris du Casting)."""
    usage = next((u for u in USAGES.values() if cle in u["favoris"]), None)
    if usage is None:
        raise ErreurCollection(f"Usage inconnu : {cle}")
    try:
        wf = workflows.obtenir(id_workflow)
    except workflows.ErreurWorkflow as e:
        raise ErreurCollection(str(e)) from e
    possible = wf.fiche["tache"] == cle or (cle == "references" and wf.fiche["tache"] in usage["taches"]
                                           and wf.accepte_references())
    if not possible:
        raise ErreurCollection(f"{wf.fiche['nom']} ne sert pas à « {usage['favoris'][cle]} »")
    if wf.manquants():
        raise ErreurCollection(f"{wf.fiche['nom']} n'est pas complet : télécharge d'abord ce qui manque")
    emplacements.retenir_favori("generation", cle, id_workflow)
    CONFIG.setdefault("generation", {})[cle] = id_workflow
    return usages()


def definir_llm(id_workflow: str, llm: str | None) -> dict:
    """Le LLM qui réécrit les prompts de ce workflow (None : le LLM par défaut)."""
    try:
        workflows.choisir_llm(id_workflow, llm)
    except workflows.ErreurWorkflow as e:
        raise ErreurCollection(str(e)) from e
    return usages()


def installer_llm(fichier: str, telechargeur) -> dict:
    """Met en file un LLM du catalogue (le clic, après lecture de sa licence, vaut accord) : son GGUF, et pour un
    spécialiste sa consigne officielle et son projecteur d'images, chacun vérifié par son SHA-256."""
    fiche = fiches.fiche_llm(fichier)
    if fiche is None:
        raise ErreurCollection(f"LLM inconnu du catalogue : {fichier}")
    ajoutes = []
    for f in _fichiers_llm(fiche):
        if modeles.destination("llm", f["nom"]).is_file():
            continue
        source = Fiche(source="huggingface", nom=f["nom"], taille=f["taille"], sha256=f["sha256"], url=f["url"],
                       page=fiche["page"], dossier="llm", licence=fiche["licence"]["nom"], jeton_requis=False)
        try:
            ajoutes.append(telechargeur.ajouter(source, "llm").id)
        except Exception as e:  # déjà dans la file, place insuffisante : dit tel quel
            if "déjà dans la file" not in str(e):
                raise ErreurCollection(f"{f['nom']} : {e}") from e
    return {"ajoutes": ajoutes}


def telecharger_manquants(id_workflow: str, telechargeur) -> dict:
    """Met en file les fichiers manquants d'un workflow (ceux de sa fiche : URL, taille et SHA-256 figés)."""
    try:
        wf = workflows.obtenir(id_workflow)
    except workflows.ErreurWorkflow as e:
        raise ErreurCollection(str(e)) from e
    ajoutes = []
    for f in wf.manquants():
        fiche = Fiche(source="huggingface", nom=f["nom"], taille=f["taille"], sha256=f["sha256"], url=f["url"],
                      page=assistant._page(f["url"]), dossier=f["dossier"], licence=wf.licence["nom"], jeton_requis=False)
        try:
            ajoutes.append(telechargeur.ajouter(fiche, f["dossier"]).id)
        except Exception as e:  # déjà dans la file, place insuffisante : dit tel quel
            if "déjà dans la file" not in str(e):
                raise ErreurCollection(f"{f['nom']} : {e}") from e
    return {"ajoutes": ajoutes}


# Retrouver les sources des fichiers déposés à la main

_identification = {"etat": "repos"}  # repos, en_cours, termine
_tache_identification: asyncio.Task | None = None


def identification() -> dict:
    return copy.deepcopy(_identification)


def _connus_du_catalogue() -> dict[str, tuple[dict, dict]]:
    """SHA-256 -> (fichier de fiche, licence) pour tout ce qu'Aria sait télécharger elle-même."""
    connus = {}
    for wf in workflows.catalogue():
        for f in wf.fiche["fichiers"]:
            connus[f["sha256"].lower()] = (f, wf.licence)
    for llm in fiches.catalogue_llm():
        connus[llm["sha256"].lower()] = ({"url": llm["url"], "page": llm["page"]}, llm["licence"])
    return connus


async def _identifier(orphelins: list):
    connus = _connus_du_catalogue()
    for i, chemin in enumerate(orphelins):
        _identification.update(fichier=chemin.name, fait=i, phase="empreinte")
        try:
            sha = await asyncio.to_thread(modeles.empreinte, chemin)
            if sha in connus:
                f, licence = connus[sha]
                modeles.enregistrer(chemin, {"source": "huggingface", "page": f.get("page") or assistant._page(f["url"]),
                                             "url": f["url"], "licence": licence["nom"]}, sha, retrouve=True)
                _identification["trouves"].append({"nom": chemin.name, "ou": "catalogue d'Aria"})
                continue
            _identification["phase"] = "civitai"
            infos = await civitai.par_empreinte(sha)
            if infos is None:
                _identification["introuvables"].append(chemin.name)
                continue
            modeles.enregistrer(chemin, infos, sha, retrouve=True, nom_modele=infos["nom_modele"],
                                version=infos["version"], type=infos["type"])
            _identification["trouves"].append({"nom": chemin.name, "ou": f"Civitai : {infos['nom_modele']}"})
        except (OSError, httpx.HTTPError) as e:
            _identification["erreurs"].append({"nom": chemin.name, "erreur": str(e) or repr(e)})
    _identification.update(etat="termine", fait=len(orphelins), fichier=None, phase=None)


def lancer_identification() -> dict:
    """Retrouve la source de chaque fichier déposé à la main (tâche de fond, suivie par identification())."""
    global _tache_identification
    if _identification["etat"] == "en_cours":
        raise ErreurCollection("La recherche des sources est déjà en cours")
    orphelins = [modeles.DOSSIER / m["cle"] for m in modeles.inventaire() if not m["suivi"]]
    _identification.clear()
    _identification.update(etat="en_cours", total=len(orphelins), fait=0, fichier=None, phase=None,
                           trouves=[], introuvables=[], erreurs=[])
    _tache_identification = asyncio.get_running_loop().create_task(_identifier(orphelins))
    return identification()
