"""Assistant de premier lancement : packs à cocher (catalogue/packs.yaml), licences, téléchargement, favoris.

Les fichiers d'un pack sont ceux des fiches (workflows/*/fiche.yaml, catalogue/llm.yaml) : URL, taille et SHA-256
y sont figés, aucune adresse ne vient de l'interface. Ils partent dans la file du téléchargeur habituel (pause,
reprise, empreinte vérifiée, source et licence au registre), seulement une fois la licence de chaque pack acceptée.
Ce qui est retenu va dans aria.yaml, clé « assistant » : licences acceptées (date), et la date de fin.
"""

from datetime import datetime
from urllib.parse import urlparse

import yaml

from app import emplacements, fiches, modeles, moteurs, workflows
from app.config import CONFIG, ROOT
from app.sources import Fiche
from app.telechargement import ErreurTelechargement

CATALOGUE = ROOT / "catalogue" / "packs.yaml"
MARGE_GO = 10  # place laissée libre après le téléchargement (Windows, rushes, fichiers temporaires)


class ErreurAssistant(ValueError):
    pass


def catalogue() -> dict:
    return yaml.safe_load(CATALOGUE.read_text(encoding="utf-8"))["packs"]


def _memoire() -> dict:
    m = emplacements.lire_choix("assistant", {})
    return m if isinstance(m, dict) else {}


def _retenir(**valeurs):
    emplacements.retenir_choix("assistant", {**_memoire(), **valeurs})


def _choisis(ids: list[str]) -> dict:
    packs = catalogue()
    inconnus = [i for i in ids if i not in packs]
    if inconnus or not ids:
        raise ErreurAssistant(f"Pack inconnu : {inconnus[0]}" if inconnus else "Aucun pack choisi")
    return {i: p for i, p in packs.items() if i in ids}  # dans l'ordre du catalogue (les favoris en dépendent)


def _fichiers(pack: dict) -> list[dict]:
    """Les fichiers du pack, tels que les fiches les décrivent (dossier, nom, url, taille, sha256)."""
    sortie = [{**f, "licence": wf.licence["nom"]}
              for wf in map(workflows.obtenir, pack.get("workflows", [])) for f in wf.fiche["fichiers"]]
    for g in pack.get("llm", []):
        llm = fiches.fiche_llm(g)
        sortie.append({"dossier": "llm", "nom": llm["id"], "url": llm["url"], "taille": llm["taille"],
                       "sha256": llm["sha256"], "licence": llm["licence"]["nom"]})
        if llm.get("vision"):  # son projecteur d'images : il voit l'image à animer ou à retoucher
            sortie.append({"dossier": "llm", **llm["vision"], "licence": llm["licence"]["nom"]})
    return sortie


def _present(f: dict) -> bool:
    return modeles.destination(f["dossier"], f["nom"]).is_file()


def _uniques(fichiers: list[dict]) -> list[dict]:
    vus = {}
    for f in fichiers:
        vus.setdefault((f["dossier"], f["nom"]), f)
    return list(vus.values())


def _licences(pack: dict) -> list[dict]:
    """Licences distinctes du pack (workflows et LLM), avec ce qu'il faut savoir avant de les accepter."""
    vues = {}
    for w in pack.get("workflows", []):
        lic = workflows.obtenir(w).licence
        vues.setdefault(lic["id"], lic)
    for g in pack.get("llm", []):
        lic = fiches.fiche_llm(g)["licence"]
        vues.setdefault(lic["id"], lic)
    return [{"nom": l["nom"], "url": l["url"], "resume": l["resume"], "commercial": l.get("commercial"),
             "territoires_exclus": l.get("territoires_exclus"),
             "composants": [f"{c['fichier']} : {c['licence']}" for c in l.get("composants") or []]}
            for l in vues.values()]


def packs(etat_moteurs: list[dict] | None = None) -> list[dict]:
    """Les packs tels que l'écran « Ce que tu veux faire » les montre."""
    acceptees = _memoire().get("licences", {})
    etats = {e["id"]: e for e in (etat_moteurs if etat_moteurs is not None else moteurs.etat())}
    sortie = []
    for id_pack, p in catalogue().items():
        fichiers = _uniques(_fichiers(p))
        manquants = [f for f in fichiers if not _present(f)]
        sortie.append({
            "id": id_pack, "nom": p["nom"], "usage": p["usage"], "conseille": bool(p.get("conseille")),
            "facultatif": bool(p.get("facultatif")), "testee": p.get("testee"),
            "taille": sum(f["taille"] for f in fichiers), "a_telecharger": sum(f["taille"] for f in manquants),
            "installe": not manquants, "licences": _licences(p), "licence_acceptee": acceptees.get(id_pack),
            "moteurs_absents": [etats[m]["nom"] for m in p.get("moteurs", []) if etats[m]["statut"] == "absent"],
        })
    return sortie


def plan(ids: list[str]) -> dict:
    """Ce que coûte une sélection : fichiers manquants (comptés une fois), place, licences encore à accepter."""
    choisis = _choisis(ids)
    manquants = [f for f in _uniques([f for p in choisis.values() for f in _fichiers(p)]) if not _present(f)]
    a_telecharger = sum(f["taille"] for f in manquants)
    libre = modeles.espace_libre()
    acceptees = _memoire().get("licences", {})
    return {
        "fichiers": [{"dossier": f["dossier"], "nom": f["nom"], "taille": f["taille"]} for f in manquants],
        "a_telecharger": a_telecharger, "libre": libre,
        "place_ok": libre - a_telecharger >= MARGE_GO * 1024 ** 3,
        "licences_a_accepter": [i for i in choisis if i not in acceptees],
    }


def accepter(ids: list[str]) -> dict:
    """« J'ai lu et j'accepte » : daté dans aria.yaml, pack par pack."""
    _choisis(ids)
    maintenant = datetime.now().isoformat(timespec="seconds")
    _retenir(licences={**_memoire().get("licences", {}), **{i: maintenant for i in ids}})
    return plan(ids)


def _page(url: str) -> str:
    """https://huggingface.co/<org>/<dépôt>/resolve/… → la page du dépôt (gardée au registre des modèles)."""
    morceaux = urlparse(url).path.strip("/").split("/")
    return f"https://huggingface.co/{morceaux[0]}/{morceaux[1]}" if len(morceaux) > 1 else url


def telecharger(ids: list[str], telechargeur) -> dict:
    """Met les fichiers manquants dans la file du téléchargeur ; refusé tant qu'une licence n'est pas acceptée."""
    p = plan(ids)
    if p["licences_a_accepter"]:
        raise ErreurAssistant("Accepte d'abord la licence de : " + ", ".join(catalogue()[i]["nom"] for i in p["licences_a_accepter"]))
    if not p["place_ok"]:
        raise ErreurAssistant(f"Place insuffisante : {p['a_telecharger'] / 1024 ** 3:.0f} Go à télécharger, "
                              f"{p['libre'] / 1024 ** 3:.0f} Go libres (il en faut {MARGE_GO} de marge)")
    choisis = _choisis(ids)
    ajoutes, deja = [], []
    for f in _uniques([f for pk in choisis.values() for f in _fichiers(pk)]):
        if _present(f):
            continue
        fiche = Fiche(source="huggingface", nom=f["nom"], taille=f["taille"], sha256=f["sha256"], url=f["url"],
                      page=_page(f["url"]), dossier=f["dossier"], licence=f.get("licence"), jeton_requis=False)
        try:
            ajoutes.append(telechargeur.ajouter(fiche, f["dossier"]).id)
        except ErreurTelechargement as e:
            if "déjà dans la file" not in str(e):
                raise ErreurAssistant(f"{f['nom']} : {e}") from e
            deja.append(f["nom"])
    return {"ajoutes": ajoutes, "deja_en_file": deja}


USAGES = {  # favoris de packs.yaml, tels que l'écran de fin les nomme
    ("generation", "texte_vers_image"): "Créer une image",
    ("generation", "references"): "Créer depuis des références",
    ("generation", "retouche"): "Retoucher",
    ("generation", "image_vers_video"): "Animer une image",
    ("generation", "texte_vers_video"): "Clip depuis un texte",
    ("llama", "modele"): "Réécrire tes idées",
}

# Essai de l'écran de fin : peu de steps et une petite image, juste pour voir toute la chaîne tourner.
ESSAI = {
    "idee": "Un chat roux endormi sur un rebord de fenêtre, la nuit, sous la pluie, avec les néons de la ville derrière la vitre",
    "prompt": ("A ginger cat asleep on a windowsill at night, rain streaking down the glass, blurred neon city lights "
               "glowing behind the window, cozy warm interior light, photographic"),
    "imposees": {"steps": 10, "megapixels": 0.5},
}


def _nom(section: str, valeur: str) -> str:
    if section == "generation":
        return workflows.obtenir(valeur).fiche["nom"]
    return (fiches.fiche_llm(valeur) or {}).get("nom", valeur)


def terminer(ids: list[str]) -> dict:
    """Règle les favoris sur ce qui est installé (packs dans l'ordre du catalogue) et note la fin de l'assistant.
    Renvoie de quoi faire le récapitulatif : chaque usage avec son modèle, réglé ou pas encore là."""
    regles, manquants = {}, {}  # un pack plus bas l'emporte, s'il est installé ; sinon il est « pas encore là »
    for p in _choisis(ids).values():
        for section, valeurs in (p.get("favoris") or {}).items():
            for cle, valeur in valeurs.items():
                pret = (not workflows.obtenir(valeur).manquants() if section == "generation"
                        else modeles.destination("llm", valeur).is_file())
                (regles if pret else manquants)[(section, cle)] = valeur
    for (section, cle), valeur in regles.items():
        emplacements.retenir_favori(section, cle, valeur)
        CONFIG.setdefault(section, {})[cle] = valeur
    _retenir(termine=datetime.now().isoformat(timespec="seconds"), packs=list(_choisis(ids)))

    def recap(favoris: dict) -> list[dict]:
        return [{"usage": USAGES.get(k, f"{k[0]}.{k[1]}"), "nom": _nom(k[0], v)} for k, v in favoris.items()]

    return {"favoris": {f"{s}.{c}": v for (s, c), v in regles.items()},
            "regles": recap(regles), "manquants": recap(manquants),
            "essai": ("generation", "texte_vers_image") in regles}


def essayer(plateau):
    """Une image rapide avec le favori « Créer une image » : la réécriture par le LLM si le sien est là."""
    workflow = CONFIG.get("generation", {}).get("texte_vers_image")
    if not workflow or workflows.obtenir(workflow).manquants():
        raise ErreurAssistant("Aucun modèle d'image installé pour l'essai")
    avec_llm = CONFIG["llama"]["modele"] in modeles.llm_disponibles()
    return plateau.commander(ESSAI["idee"], None if avec_llm else ESSAI["prompt"], None, workflow, "1:1 (Square)",
                             None, 1, imposees=ESSAI["imposees"])


def etat() -> dict:
    """S'ouvrir au lancement ? Seulement tant qu'il n'a pas été terminé, et s'il manque de quoi générer."""
    memoire = _memoire()
    etat_moteurs = moteurs.etat()
    moteur_absent = any(e["statut"] == "absent" and not e["facultatif"] for e in etat_moteurs)
    aucun_workflow = not any(not wf.manquants() for wf in workflows.catalogue())
    return {"ouvrir": not memoire.get("termine") and (moteur_absent or aucun_workflow),
            "termine": memoire.get("termine"), "packs": packs(etat_moteurs)}
