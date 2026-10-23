"""Réglages fins des LLM et des workflows (onglet Réglages), rangés dans data/reglages.yaml.

Le fichier ne contient que les changements de Jérémy : les valeurs d'origine restent dans config.yaml,
les fiches et les api.json des workflows, qui ne sont jamais modifiés. Supprimer une clé (ou tout le
fichier) revient aux valeurs d'origine. Chaque valeur est contrôlée avant d'être enregistrée.

    llm:
        <fichier.gguf>: {temperature_reecriture: 0.5, contexte: 8192, consignes: {peaufinage: "…"}}
    workflows:
        <id>: {steps: 30, cfg: 1.0, sampler: euler, scheduler: simple, shift: 3.1, megapixels: 1, …}
"""

import json
from dataclasses import dataclass
from pathlib import Path

import yaml

from app.config import CONFIG, ROOT

FICHIER = ROOT / "data" / "reglages.yaml"
FICHIER_SAMPLERS = ROOT / "data" / "comfyui_samplers.json"
LONGUEUR_CONSIGNE = 4000
CONSIGNES = ("reecriture", "retouche", "peaufinage", "precisions", "references", "video", "decoupage")

# Liste de secours tant que ComfyUI n'a jamais démarré (sinon : celle qu'il annonce, voir memoriser_samplers).
SAMPLERS_SECOURS = ["euler", "euler_ancestral", "heun", "dpm_2", "dpm_2_ancestral", "lms", "dpmpp_2m",
                    "dpmpp_2m_sde", "dpmpp_3m_sde", "dpmpp_sde", "ddim", "uni_pc", "res_multistep", "lcm"]
SCHEDULERS_SECOURS = ["simple", "normal", "karras", "exponential", "sgm_uniform", "beta", "ddim_uniform",
                      "linear_quadratic", "kl_optimal"]


class ErreurReglages(ValueError):
    pass


@dataclass(frozen=True)
class Param:
    cle: str
    libelle: str
    type: str  # entier, reel, booleen, choix
    defaut: object
    min: float | None = None
    max: float | None = None
    pas: float | None = None
    choix: tuple = ()
    aide: str = ""

    def valider(self, valeur):
        if self.type == "booleen":
            if not isinstance(valeur, bool):
                raise ErreurReglages(f"{self.libelle} : oui ou non attendu")
            return valeur
        if self.type == "choix":
            if valeur not in self.choix:
                raise ErreurReglages(f"{self.libelle} : « {valeur} » n'est pas proposé")
            return valeur
        if isinstance(valeur, bool) or not isinstance(valeur, (int, float)):
            raise ErreurReglages(f"{self.libelle} : nombre attendu")
        if self.type == "entier":
            if valeur != int(valeur):
                raise ErreurReglages(f"{self.libelle} : nombre entier attendu")
            valeur = int(valeur)
        else:
            valeur = round(float(valeur), 4)
        if not self.min <= valeur <= self.max:
            raise ErreurReglages(f"{self.libelle} : entre {self.min:g} et {self.max:g}")
        return valeur

    def public(self, valeur) -> dict:
        return {"cle": self.cle, "libelle": self.libelle, "type": self.type, "defaut": self.defaut,
                "valeur": valeur, "modifie": valeur != self.defaut, "min": self.min, "max": self.max,
                "pas": self.pas, "choix": list(self.choix), "aide": self.aide}


# Fichier

class _Dumper(yaml.SafeDumper):
    pass


# Les consignes sur plusieurs lignes restent lisibles dans le fichier (bloc « | »).
_Dumper.add_representer(str, lambda d, s: d.represent_scalar(
    "tag:yaml.org,2002:str", s, style="|" if "\n" in s else None))

_cache: tuple[int, dict] | None = None


def lire() -> dict:
    global _cache
    if not FICHIER.is_file():
        return {"llm": {}, "workflows": {}}
    empreinte = FICHIER.stat().st_mtime_ns
    if _cache is None or _cache[0] != empreinte:
        donnees = yaml.safe_load(FICHIER.read_text(encoding="utf-8")) or {}
        _cache = (empreinte, {"llm": donnees.get("llm") or {}, "workflows": donnees.get("workflows") or {}})
    return _cache[1]


def _ecrire(donnees: dict):
    propre = {k: {n: v for n, v in d.items() if v} for k, d in donnees.items()}  # pas de sections vides
    FICHIER.parent.mkdir(parents=True, exist_ok=True)
    temporaire = FICHIER.with_suffix(".tmp")
    temporaire.write_text("# Réglages de l'onglet Réglages : seulement ce qui diffère des valeurs d'origine.\n"
                          + yaml.dump(propre, Dumper=_Dumper, allow_unicode=True, sort_keys=False, width=120),
                          encoding="utf-8")
    temporaire.replace(FICHIER)


def _appliquer(section: dict, params: dict[str, Param], changements: dict):
    """Valide puis range les changements ; None ou la valeur d'origine retire la clé."""
    for cle, valeur in changements.items():
        if cle not in params:
            raise ErreurReglages(f"Réglage inconnu : {cle}")
        p = params[cle]
        if valeur is None:
            section.pop(cle, None)
            continue
        valeur = p.valider(valeur)
        if valeur == p.defaut:
            section.pop(cle, None)
        else:
            section[cle] = valeur


# LLM

def nom_llm(modele: str | None) -> str:
    return modele or CONFIG["llama"]["modele"]


def params_llm() -> dict[str, Param]:
    llama = CONFIG["llama"]
    temperature = dict(type="reel", min=0.0, max=2.0, pas=0.05)
    return {p.cle: p for p in [
        Param("temperature_reecriture", "Température · réécriture", defaut=0.7, aide="Idée → prompt (Plateau)",
              **temperature),
        Param("temperature_peaufinage", "Température · peaufinage", defaut=0.3,
              aide="Plus bas = plus fidèle, moins de broderie", **temperature),
        Param("temperature_retouche", "Température · consigne de retouche", defaut=0.2, **temperature),
        Param("temperature_traduction", "Température · traduction des précisions", defaut=0.2, **temperature),
        # Échantillonnage : valeurs d'origine = défauts de llama-server.
        Param("top_p", "Top-p", "reel", 0.95, 0.0, 1.0, 0.05, aide="Garde les mots les plus probables jusqu'à ce total"),
        Param("top_k", "Top-k", "entier", 40, 0, 200, 1, aide="Nombre de mots candidats (0 = sans limite)"),
        Param("min_p", "Min-p", "reel", 0.05, 0.0, 1.0, 0.01, aide="Écarte les mots trop improbables"),
        Param("presence_penalty", "Pénalité de présence", "reel", 0.0, -2.0, 2.0, 0.1,
              aide="Positif = moins de répétitions"),
        Param("max_tokens", "Longueur maximale de réponse (tokens)", "entier", 512, 64, 4096, 64),
        Param("contexte", "Contexte (tokens)", "entier", int(llama["contexte"]), 1024, 262144, 1024,
              aide="Plus grand = plus de VRAM ; changer redémarre llama-server"),
        Param("reflexion", "Réflexion (thinking)", "booleen", bool(llama["reflexion"]),
              aide="Pour les modèles qui la proposent ; plus lent"),
    ]}


def valeurs_llm(modele: str | None) -> dict:
    propres = lire()["llm"].get(nom_llm(modele), {})
    return {cle: propres.get(cle, p.defaut) for cle, p in params_llm().items()}


ECHANTILLONNAGE = ("top_p", "top_k", "min_p", "presence_penalty")


def echantillonnage(modele: str | None) -> dict:
    """Paramètres d'échantillonnage du GGUF, à passer à Llama.generer."""
    valeurs = valeurs_llm(modele)
    return {cle: valeurs[cle] for cle in ECHANTILLONNAGE}


def consigne_llm(modele: str | None, nom: str) -> str | None:
    """Consigne réécrite par Jérémy pour ce GGUF, ou None (consigne d'origine)."""
    return (lire()["llm"].get(nom_llm(modele), {}).get("consignes") or {}).get(nom)


def modifier_llm(modele: str, changements: dict | None = None, consignes: dict | None = None) -> dict:
    from app import modeles  # import tardif : modeles → workflows → reglages
    if modele not in modeles.llm_disponibles():
        raise ErreurReglages(f"GGUF absent de modeles/llm : {modele}")
    donnees = lire()
    donnees = {"llm": {k: dict(v) for k, v in donnees["llm"].items()}, "workflows": dict(donnees["workflows"])}
    section = donnees["llm"].setdefault(modele, {})
    _appliquer(section, params_llm(), changements or {})
    textes = dict(section.get("consignes") or {})
    for nom, texte in (consignes or {}).items():
        if nom not in CONSIGNES:
            raise ErreurReglages(f"Consigne inconnue : {nom}")
        if texte is None or not texte.strip():
            textes.pop(nom, None)
        elif len(texte) > LONGUEUR_CONSIGNE:
            raise ErreurReglages(f"Consigne trop longue (plus de {LONGUEUR_CONSIGNE} caractères)")
        else:
            textes[nom] = texte.strip()
    if textes:
        section["consignes"] = textes
    else:
        section.pop("consignes", None)
    _ecrire(donnees)
    return valeurs_llm(modele)


# Workflows

def options_sampler() -> tuple[list[str], list[str]]:
    if FICHIER_SAMPLERS.is_file():
        d = json.loads(FICHIER_SAMPLERS.read_text(encoding="utf-8"))
        return d["samplers"], d["schedulers"]
    return SAMPLERS_SECOURS, SCHEDULERS_SECOURS


def memoriser_samplers(samplers: list[str], schedulers: list[str]):
    """Listes annoncées par ComfyUI (/object_info/KSampler), retenues pour quand il est arrêté."""
    if samplers and schedulers:
        FICHIER_SAMPLERS.parent.mkdir(parents=True, exist_ok=True)
        FICHIER_SAMPLERS.write_text(json.dumps({"samplers": samplers, "schedulers": schedulers}), encoding="utf-8")


def noeud_unique(api: dict, type_noeud: str) -> str | None:
    trouves = [nid for nid, n in api.items() if n.get("class_type") == type_noeud]
    return trouves[0] if len(trouves) == 1 else None


def params_workflow(wf) -> dict[str, Param]:
    """Réglages proposés pour ce workflow, selon ce que son graphe contient vraiment."""
    from app.workflows import FORMATS
    entrees, defauts, params = wf.fiche["entrees"], wf.fiche.get("defauts", {}), []
    # Vidéo : la qualité choisie dans le Montage fixe steps et mégapixels ; ces deux réglages servent au Casting.
    montage = " · Montage : la qualité choisie (Rapide / Compromis / Qualité) l'emporte" if wf.fiche.get("video") else ""
    if "steps" in entrees:
        params.append(Param("steps", "Steps", "entier", int(defauts.get("steps", 20)), 1, 150, 1,
                            aide="Plus = plus lent ; les modèles Turbo / Lightning sont faits pour peu de steps" + montage))
    if ks := noeud_unique(wf.api, "KSampler"):
        entree = wf.api[ks]["inputs"]
        samplers, schedulers = options_sampler()
        params += [
            Param("cfg", "CFG", "reel", float(entree["cfg"]), 0.0, 30.0, 0.1,
                  aide="Modèles distillés (Turbo, Lightning) : 1. Au-dessus, environ deux fois plus lent"),
            Param("sampler", "Sampler", "choix", entree["sampler_name"],
                  choix=tuple(dict.fromkeys([entree["sampler_name"], *samplers]))),
            Param("scheduler", "Scheduler", "choix", entree["scheduler"],
                  choix=tuple(dict.fromkeys([entree["scheduler"], *schedulers]))),
        ]
    else:  # graphes « avancés » (MiniMax H3…) : sampler et scheduler dans deux nœuds séparés
        samplers, schedulers = options_sampler()
        if ksel := noeud_unique(wf.api, "KSamplerSelect"):
            actuel = wf.api[ksel]["inputs"]["sampler_name"]
            params.append(Param("sampler", "Sampler", "choix", actuel, choix=tuple(dict.fromkeys([actuel, *samplers]))))
        if bs := noeud_unique(wf.api, "BasicScheduler"):
            actuel = wf.api[bs]["inputs"]["scheduler"]
            params.append(Param("scheduler", "Scheduler", "choix", actuel,
                                choix=tuple(dict.fromkeys([actuel, *schedulers]))))
    if ms := noeud_unique(wf.api, "ModelSamplingAuraFlow"):
        params.append(Param("shift", "Shift", "reel", float(wf.api[ms]["inputs"]["shift"]), 0.0, 20.0, 0.1))
    if "megapixels" in entrees:
        params.append(Param("megapixels", "Mégapixels", "reel", float(defauts.get("megapixels", 1)), 0.25, 6.0, 0.05,
                            aide="Taille de l'image : plus = plus de VRAM et plus lent" + montage))
    if "ratio" in entrees:
        params.append(Param("ratio", "Format par défaut", "choix", defauts.get("ratio", FORMATS[0]),
                            choix=tuple(FORMATS)))
    if "mots_max" in (profil := wf.fiche.get("profil_prompt") or {}):
        params.append(Param("mots_max", "Prompt : mots conseillés", "entier", int(profil["mots_max"]), 20, 1000, 10))
    return {p.cle: p for p in params}


def valeurs_workflow(wf) -> dict:
    propres = lire()["workflows"].get(wf.id, {})
    return {cle: propres.get(cle, p.defaut) for cle, p in params_workflow(wf).items()}


def appliquer_noeuds(wf, api: dict, valeurs: dict):
    """Réglages hors « entrees » de la fiche : écrits directement dans les nœuds du graphe."""
    if ks := noeud_unique(api, "KSampler"):
        for cle, entree in (("cfg", "cfg"), ("sampler", "sampler_name"), ("scheduler", "scheduler")):
            if cle in valeurs:
                api[ks]["inputs"][entree] = valeurs[cle]
    else:
        if (ksel := noeud_unique(api, "KSamplerSelect")) and "sampler" in valeurs:
            api[ksel]["inputs"]["sampler_name"] = valeurs["sampler"]
        if (bs := noeud_unique(api, "BasicScheduler")) and "scheduler" in valeurs:
            api[bs]["inputs"]["scheduler"] = valeurs["scheduler"]
    if (ms := noeud_unique(api, "ModelSamplingAuraFlow")) and "shift" in valeurs:
        api[ms]["inputs"]["shift"] = valeurs["shift"]


def modifier_workflow(id_workflow: str, changements: dict) -> dict:
    from app import workflows
    wf = workflows.obtenir(id_workflow)
    donnees = lire()
    donnees = {"llm": dict(donnees["llm"]), "workflows": {k: dict(v) for k, v in donnees["workflows"].items()}}
    _appliquer(donnees["workflows"].setdefault(wf.id, {}), params_workflow(wf), changements)
    _ecrire(donnees)
    return valeurs_workflow(wf)


# Pour l'interface

def etat() -> dict:
    from app import modeles, prompt, workflows
    favoris = {m["fichier"]: m["nom"] for m in CONFIG["llama"].get("modeles_au_choix", [])}
    llms = []
    for fichier in modeles.llm_disponibles():
        valeurs = valeurs_llm(fichier)
        llms.append({
            "fichier": fichier, "nom": favoris.get(fichier, fichier.removesuffix(".gguf")),
            "params": [p.public(valeurs[cle]) for cle, p in params_llm().items()],
            "consignes": [{"cle": nom, "defaut": prompt.CONSIGNES_DEFAUT[nom],
                           "valeur": consigne_llm(fichier, nom) or prompt.CONSIGNES_DEFAUT[nom],
                           "modifie": consigne_llm(fichier, nom) is not None} for nom in CONSIGNES],
        })
    return {
        "llm": llms,
        "defaut_llm": CONFIG["llama"]["modele"],
        "clause_mineurs": prompt.CLAUSE_MINEURS,
        "workflows": [{"id": wf.id, "nom": wf.fiche["nom"], "tache": wf.fiche["tache"],
                       "params": [p.public(valeurs_workflow(wf)[cle]) for cle, p in params_workflow(wf).items()]}
                      for wf in workflows.catalogue()],
        "samplers_de_comfyui": FICHIER_SAMPLERS.is_file(),
    }
