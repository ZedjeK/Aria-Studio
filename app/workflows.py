"""Catalogue des workflows : workflows/<id>/api.json + fiche.yaml (format des fiches : app/fiches.py)."""

import copy
import json
from dataclasses import dataclass
from pathlib import Path

from app import emplacements, fiches, memo, modeles, reglages, securite
from app.config import ROOT
from app.repliques import vers_balises

DOSSIER = ROOT / "workflows"
# Valeurs acceptées par le nœud ResolutionSelector de ComfyUI (comfy_extras/nodes_resolution.py).
FORMATS = [
    "1:1 (Square)", "2:3 (Portrait Photo)", "3:2 (Photo)", "3:4 (Portrait Standard)",
    "4:3 (Standard)", "9:16 (Portrait Widescreen)", "16:9 (Widescreen)", "21:9 (Ultrawide)",
]


class ErreurWorkflow(ValueError):
    pass


# Encodeurs d'édition qui acceptent plusieurs images de référence :
# entrée de l'image n, nombre maximal d'images, nœud de latent vide adapté au modèle.
ENCODEURS_REFERENCES = {
    "TextEncodeQwenImageEditPlus": ("image{}", 3, "EmptySD3LatentImage"),
    "TextEncodeQwenImage21": ("images.image_{}", 16, "EmptyLatentImage"),
}
# Plafond d'Aria : 10 images, ce que les modèles ComfyUI officiels de Qwen-Image 2.1 Edit reçoivent (son encodeur a 16
# entrées, mais seules 10 sont câblées). Chaque workflow garde sa propre limite : le 2511 s'arrête à 3 (son encodeur).
REFERENCES_MAX = 10


@dataclass
class Workflow:
    fiche: dict
    api: dict

    @property
    def id(self) -> str:
        return self.fiche["id"]

    @property
    def famille(self) -> str:
        return self.fiche["famille"]

    @property
    def licence(self) -> dict:
        return self.fiche["licence"]

    @property
    def materiel(self) -> dict:
        return self.fiche["materiel"]

    @property
    def profil(self) -> dict:
        return self.fiche["profil_prompt"]

    @property
    def modele_origine(self) -> str:
        """Le modèle de diffusion livré avec le workflow (dans <Famille>/modeles) : celui qu'on télécharge quand la
        famille n'en a aucun, pas un passage obligé."""
        return next(f["nom"] for f in self.fiche["fichiers"] if f["dossier"] == f"{self.famille}/modeles")

    def variantes(self) -> list[str]:
        """Checkpoints présents dans <Famille>/modeles, celui d'origine d'abord s'il est là. C'est la famille qui
        compte : l'origine supprimée, les autres checkpoints de la famille servent toujours. Un checkpoint téléchargé et
        pas encore validé (Modèles → « À valider ») n'est pas proposé."""
        dossier = f"{self.famille}/modeles"
        deposes = [m for m in modeles.fichiers(dossier, modeles.EXTENSIONS_IMAGE) if not modeles.en_attente(dossier, m)]
        return sorted(deposes, key=lambda m: m != self.modele_origine)

    def manquants(self) -> list[dict]:
        """Fichiers de la fiche absents. Le checkpoint d'origine n'en est que si la famille n'a aucun checkpoint ; les
        fichiers communs (encodeurs, VAE, LoRA du workflow) restent nécessaires."""
        dossier, un_checkpoint = f"{self.famille}/modeles", bool(self.variantes())
        return [f for f in self.fiche["fichiers"] if not (f["dossier"] == dossier and un_checkpoint)
                and not modeles.destination(f["dossier"], f["nom"]).is_file()]

    def resume(self) -> dict:
        manquants = self.manquants()
        return {
            "id": self.id,
            "nom": self.fiche["nom"],
            "tache": self.fiche["tache"],
            "famille": self.famille,
            "licence": self.licence["nom"],
            "licence_fiche": {**self.licence, "verifiee": str(self.licence["verifiee"])},
            "vram_estimee_go": self.materiel["vram_estimee_go"],
            "vram_min_go": self.materiel["vram_min_go"],
            "vram_confort_go": self.materiel["vram_confort_go"],
            "taille": sum(f["taille"] for f in self.fiche["fichiers"]),
            "style_prompt": self.profil["style"],
            "pret": not manquants,
            "manquants": manquants,
            "a_telecharger": sum(f["taille"] for f in manquants),
            "modeles": self.variantes(),
            "origine": self.modele_origine,
            "loras": modeles.loras(self.famille),
        }

    def preparer(self, valeurs: dict) -> dict:
        """Copie de api.json avec les valeurs injectées (défauts de la fiche complétés par `valeurs`)."""
        api = copy.deepcopy(self.api)
        entrees = self.fiche["entrees"]
        # Ordre de priorité : valeurs de l'appel > réglages de l'utilisateur (data/reglages.yaml) > défauts de la fiche.
        regle = reglages.valeurs_workflow(self)
        if valeurs.get("modele") is None:  # rien de choisi : le modèle par défaut des réglages, pas l'origine
            valeurs = {k: v for k, v in valeurs.items() if k != "modele"}
        tout = {**self.fiche.get("defauts", {}), **{k: v for k, v in regle.items() if k in entrees}, **valeurs}
        if tout.get("modele") is None:
            tout.pop("modele", None)  # le modèle d'origine, déjà dans api.json
        elif tout["modele"] not in self.variantes():
            raise ErreurWorkflow(f"Modèle absent de {self.famille}/modeles : {tout['modele']}")
        variante = reglages.valeurs_variante(self, tout.get("modele") or self.modele_origine)
        if variante.get("steps") and "steps" in entrees:
            tout["steps"] = variante["steps"]  # ce checkpoint impose les siens (réglage par variante)
        choix_loras = tout.pop("loras", None) or []
        references = tout.pop("references", None) or []  # images 2 et 3 (déjà téléversées) ; l'image 1 = « image »
        format_sortie = tout.pop("format_sortie", None)  # S'inspirer : format choisi au lieu de celui de l'image 1
        variation = tout.pop("variation", None)  # {"image": nom téléversé, "denoise": 0.3 à 0.9}
        # Point de passage obligé de tout prompt vers ComfyUI (génération, retouche, casting).
        if "prompt" in tout:
            securite.verifier(str(tout["prompt"]))
            if self.profil.get("balise_dialogue"):  # H3 : une réplique entre guillemets part dans sa balise <d>
                tout["prompt"] = vers_balises(str(tout["prompt"]))
        for cle, valeur in tout.items():
            if cle not in entrees:
                raise ErreurWorkflow(f"{self.id} n'a pas d'entrée « {cle} »")
            noeud, entree = entrees[cle]
            api[noeud]["inputs"][entree] = valeur
        reglages.appliquer_noeuds(self, api, regle)
        if not variante.get("lora_workflow", True):
            self._retirer_loras_workflow(api)
        if references or format_sortie:
            self._brancher_references(api, references, format_sortie)
        if variation:
            self._brancher_variation(api, variation["image"], variation["denoise"])
        if choix_loras:
            self._inserer_loras(api, choix_loras)
        return api

    def accepte_references(self) -> int:
        """Nombre d'images de référence que ce workflow sait recevoir (0 : aucune)."""
        types = {n["class_type"] for n in self.api.values()}
        if "image" not in self.fiche["entrees"]:  # pas d'entrée pour l'image 1 (workflow texte vers image)
            return 0
        return min([REFERENCES_MAX] + [ENCODEURS_REFERENCES[t][1] for t in types if t in ENCODEURS_REFERENCES]) \
            if types & ENCODEURS_REFERENCES.keys() else 0

    def ajouter_affiche(self, api: dict, prefixe: str):
        """Vidéo : enregistre aussi la première image, qui sert de vignette dans les Bobines."""
        cv = reglages.noeud_unique(api, "CreateVideo")
        if not cv:
            raise ErreurWorkflow(f"{self.id} ne produit pas de vidéo")
        api["aria_affiche_lot"] = {"class_type": "ImageFromBatch", "inputs": {
            "image": api[cv]["inputs"]["images"], "batch_index": 0, "length": 1}}
        api["aria_affiche"] = {"class_type": "SaveImage", "inputs": {
            "images": ["aria_affiche_lot", 0], "filename_prefix": f"{prefixe}_affiche"}}

    def reserve_retouche(self) -> bool:
        """Réservé à la Retouche (fiche : `reserve: retouche`) : S'inspirer ne le propose pas et ne l'accepte pas."""
        return self.fiche.get("reserve") == "retouche"

    def accepte_variation(self) -> bool:
        return bool(reglages.noeud_unique(self.api, "KSampler") and reglages.noeud_unique(self.api, "VAELoader"))

    def _brancher_references(self, api: dict, noms: list[str], format_sortie: str | None):
        """Images 2 et 3 sur tous les encodeurs d'édition, et latent vide au format choisi (S'inspirer)."""
        encodeurs = [(nid, ENCODEURS_REFERENCES[n["class_type"]]) for nid, n in api.items()
                     if n["class_type"] in ENCODEURS_REFERENCES]
        if not encodeurs or len(noms) + 1 > self.accepte_references():
            raise ErreurWorkflow(f"{self.id} n'accepte pas {len(noms) + 1} images de référence")
        for i, nom in enumerate(noms, 2):
            api[f"aria_ref_{i}"] = {"class_type": "LoadImage", "inputs": {"image": nom}}
            for nid, (entree, _, _) in encodeurs:
                api[nid]["inputs"][entree.format(i)] = [f"aria_ref_{i}", 0]
        if format_sortie:
            ks = reglages.noeud_unique(api, "KSampler")
            api["aria_format"] = {"class_type": "ResolutionSelector",
                                  "inputs": {"aspect_ratio": format_sortie, "megapixels": 1, "multiple": 32}}
            api["aria_latent"] = {"class_type": encodeurs[0][1][2], "inputs": {
                "width": ["aria_format", 0], "height": ["aria_format", 1], "batch_size": 1}}
            api[ks]["inputs"]["latent_image"] = ["aria_latent", 0]

    def _brancher_variation(self, api: dict, nom: str, denoise: float):
        """img2img : le latent part de l'image de référence au lieu d'un latent vide ; denoise = liberté."""
        ks, vae = reglages.noeud_unique(api, "KSampler"), reglages.noeud_unique(api, "VAELoader")
        if not (ks and vae):
            raise ErreurWorkflow(f"{self.id} ne permet pas la variation")
        if not 0.05 <= denoise <= 1:
            raise ErreurWorkflow(f"Liberté entre 0,05 et 1 : {denoise}")
        megapixels = reglages.valeurs_workflow(self).get("megapixels", 1.0)
        api["aria_var_image"] = {"class_type": "LoadImage", "inputs": {"image": nom}}
        api["aria_var_echelle"] = {"class_type": "ImageScaleToTotalPixels", "inputs": {
            "image": ["aria_var_image", 0], "upscale_method": "lanczos", "megapixels": megapixels,
            "resolution_steps": 16}}
        api["aria_var_latent"] = {"class_type": "VAEEncode",
                                  "inputs": {"pixels": ["aria_var_echelle", 0], "vae": [vae, 0]}}
        api[ks]["inputs"]["latent_image"] = ["aria_var_latent", 0]
        api[ks]["inputs"]["denoise"] = denoise

    def choisi(self, modele: str | None) -> str:
        """Le checkpoint réellement utilisé : celui demandé, sinon le défaut des réglages (l'origine si elle est là,
        sinon le premier checkpoint de la famille)."""
        return modele or reglages.valeurs_workflow(self).get("modele") or self.modele_origine

    @staticmethod
    def _retirer_loras_workflow(api: dict):
        """Court-circuite les LoRAs livrées avec le workflow (un checkpoint qui intègre déjà le Turbo) : ce qui lisait
        une LoRA lit directement le modèle qu'elle recevait."""
        for nid in [n for n, v in api.items() if v.get("class_type") == "LoraLoaderModelOnly"]:
            source = api.pop(nid)["inputs"]["model"]
            for noeud in api.values():
                for entree, valeur in noeud["inputs"].items():
                    if valeur == [nid, 0]:
                        noeud["inputs"][entree] = source

    def _inserer_loras(self, api: dict, choix: list[dict]):
        """Chaîne des LoraLoaderModelOnly juste après le chargeur du modèle de diffusion :
        tout ce qui lisait le modèle lit désormais la sortie de la dernière LoRA."""
        disponibles = modeles.fichiers(f"{self.famille}/loras", modeles.EXTENSIONS_IMAGE)
        chargeur = self.fiche["entrees"]["modele"][0]
        lecteurs = [(nid, entree) for nid, noeud in api.items() for entree, valeur in noeud["inputs"].items()
                    if valeur == [chargeur, 0]]
        precedent = [chargeur, 0]
        for i, lora in enumerate(choix, 1):
            if lora["fichier"] not in disponibles:
                raise ErreurWorkflow(f"LoRA absente de modeles/{self.famille}/loras : {lora['fichier']}")
            force = float(lora["force"])
            if not modeles.FORCE_LORA[0] <= force <= modeles.FORCE_LORA[1]:
                raise ErreurWorkflow(f"Force de LoRA entre {modeles.FORCE_LORA[0]:g} et {modeles.FORCE_LORA[1]:g} : {force}")
            nid = f"aria_lora_{i}"
            api[nid] = {"class_type": "LoraLoaderModelOnly",
                        "inputs": {"lora_name": lora["fichier"], "strength_model": force, "model": precedent}}
            precedent = [nid, 0]
        for nid, entree in lecteurs:
            api[nid]["inputs"][entree] = precedent


def _charger(dossier: Path) -> Workflow:
    fiche = memo.lire_yaml(dossier / "fiche.yaml")
    if problemes := fiches.problemes_workflow(fiche):
        raise ErreurWorkflow(f"Fiche {dossier.name} : " + " ; ".join(problemes))
    api = json.loads((dossier / "api.json").read_text(encoding="utf-8"))
    return Workflow(fiche, api)


_lus = memo.SelonFichiers()


def _lire(dossier: Path) -> tuple[dict | None, dict | None, str | None]:
    try:
        wf = _charger(dossier)
        return wf.fiche, wf.api, None
    except ErreurWorkflow as e:
        return None, None, str(e)


def _fiches_lues() -> list[tuple[dict | None, dict | None, str | None]]:
    """(fiche, api, erreur) de chaque dossier de workflow, relus seulement quand un fichier change."""
    dossiers = [d for d in sorted(DOSSIER.iterdir()) if (d / "fiche.yaml").is_file()]
    cle = memo.empreinte([f for d in dossiers for f in (d / "fiche.yaml", d / "api.json")])
    return _lus.lire(cle, lambda: [_lire(d) for d in dossiers])


def valider_checkpoint(cle_modele: str, steps: int = 0) -> dict:
    """Met en service un checkpoint téléchargé. steps > 0 : ils deviennent ses « Steps imposés » dans les workflows de sa
    famille qui en ont ; 0 : ceux du réglage et de la qualité, comme avant."""
    dossier, _, nom = cle_modele.rpartition("/")
    if not modeles.est_checkpoint(dossier):
        raise ErreurWorkflow(f"Pas un checkpoint de famille : {cle_modele}")
    if not 0 <= steps <= 150:
        raise ErreurWorkflow("Steps : un entier entre 1 et 150, ou 0 pour garder ceux du réglage")
    modeles.valider(cle_modele)
    famille = dossier.removesuffix("/modeles")
    if steps:
        for wf in catalogue():
            if wf.famille == famille and "steps" in wf.fiche["entrees"]:
                reglages.modifier_variante(wf.id, nom, {"steps": steps})
    return {"cle": cle_modele, "steps": steps}


def catalogue() -> list[Workflow]:
    """Les workflows aux fiches valides ; une fiche invalide (ajoutée à la main) est écartée, et dite par erreurs().
    Chaque appel rend ses propres fiches (copiées) : les modifier ne touche pas le cache. Le graphe api.json est
    partagé : preparer() en fait toujours une copie avant d'y écrire."""
    return [Workflow(copy.deepcopy(fiche), api) for fiche, api, erreur in _fiches_lues() if erreur is None]


# Familles : ce que les fiches en disent, pour ranger et chercher un modèle (onglet Modèles).
NATURES = {"texte_vers_image": "image", "retouche": "image", "image_vers_video": "video", "texte_vers_video": "video"}
# Usages d'une famille (le classement des LoRA de Ma collection) : une LoRA se charge dans tous les workflows de sa
# famille, donc Qwen-Image 2.1 sert à la fois l'image et la retouche.
USAGES = {"texte_vers_image": "image", "retouche": "retouche", "image_vers_video": "video", "texte_vers_video": "video"}
ORDRE_USAGES = ("image", "retouche", "video")


def familles() -> dict[str, dict]:
    """{famille: {nature (image ou video), usages (image, retouche, video), bases et nom_contient Civitai, hf
    (dépôts d'origine), workflows}} d'après les fiches."""
    sortie = {}
    for wf in catalogue():
        f = sortie.setdefault(wf.famille, {"nature": "image", "usages": [], "bases": [], "nom_contient": None,
                                           "hf": [], "workflows": []})
        if NATURES.get(wf.fiche["tache"]) == "video":
            f["nature"] = "video"
        usage = USAGES.get(wf.fiche["tache"])
        if usage and usage not in f["usages"]:
            f["usages"] = sorted([*f["usages"], usage], key=ORDRE_USAGES.index)
        civitai = wf.fiche.get("civitai") or {}
        f["bases"] += [b for b in civitai.get("bases", []) if b not in f["bases"]]
        f["nom_contient"] = civitai.get("nom_contient") or f["nom_contient"]
        f["hf"] += [b for b in (wf.fiche.get("hugging_face") or {}).get("bases", []) if b not in f["hf"]]
        f["workflows"].append(wf.id)
    return sortie


def famille_civitai(base: str | None, nom_modele: str = "", infos: dict | None = None) -> str | None:
    """La famille d'Aria où va un modèle Civitai de cette base ; une famille qui exige un mot dans le nom passe
    avant celle qui n'en exige pas (« Qwen » + « edit » : la retouche)."""
    candidates = [(nom, f) for nom, f in (infos or familles()).items() if base and base in f["bases"]]
    candidates.sort(key=lambda c: c[1]["nom_contient"] is None)
    for nom, f in candidates:
        if f["nom_contient"] is None or f["nom_contient"].lower() in nom_modele.lower():
            return nom
    return None


# LLM par workflow : celui qui réécrit les prompts de ce modèle (onglets d'usage de Modèles), retenu dans aria.yaml.
CLE_LLM = "llm_par_workflow"


def llms_choisis() -> dict[str, str]:
    choix = emplacements.lire_choix(CLE_LLM, {})
    return {k: v for k, v in choix.items() if isinstance(v, str)} if isinstance(choix, dict) else {}


def llm_de(id_workflow: str | None) -> str:
    """Le LLM de ce workflow s'il est choisi et installé, sinon le LLM par défaut (config.yaml, favori du Casting)."""
    choisi = llms_choisis().get(id_workflow) if id_workflow else None
    return choisi if choisi and choisi in modeles.llm_disponibles() else modeles.llm_defaut()


def choisir_llm(id_workflow: str, llm: str | None):
    """Retient le LLM d'un workflow ; None le ramène au LLM par défaut."""
    obtenir(id_workflow)  # le workflow doit exister
    if llm is not None and llm not in modeles.llm_disponibles():
        raise ErreurWorkflow(f"GGUF absent de modeles/llm : {llm}")
    choix = llms_choisis()
    if llm is None:
        choix.pop(id_workflow, None)
    else:
        choix[id_workflow] = llm
    emplacements.retenir_choix(CLE_LLM, choix)


def erreurs() -> list[str]:
    """Pourquoi certaines fiches sont écartées du catalogue."""
    return [erreur for _, _, erreur in _fiches_lues() if erreur]


def regles_prompt(id_workflow: str | None) -> list[str]:
    """Règles propres au modèle cible, ajoutées à la consigne de Llama (profil_prompt.regles de sa fiche)."""
    if not id_workflow:
        return []
    try:
        return list(obtenir(id_workflow).profil.get("regles") or [])
    except ErreurWorkflow:
        return []


def rubriques_ignorees(id_workflow: str | None) -> set[str]:
    """Rubriques de « Me guider » inutiles à ce modèle cible (profil_prompt.rubriques_ignorees de sa fiche) : celles
    que son entrée fixe déjà, comme l'image à animer qui porte le style, la palette et l'objectif."""
    if not id_workflow:
        return set()
    try:
        return {str(r) for r in obtenir(id_workflow).profil.get("rubriques_ignorees") or []}
    except ErreurWorkflow:
        return set()


def dialogue_en_balise(id_workflow: str | None) -> bool:
    """Ce modèle lit les répliques dans une balise <d>[Langue] …</d> (profil_prompt.balise_dialogue de sa fiche)."""
    if not id_workflow:
        return False
    try:
        return bool(obtenir(id_workflow).profil.get("balise_dialogue"))
    except ErreurWorkflow:
        return False


def obtenir(id_workflow: str) -> Workflow:
    for fiche, api, erreur in _fiches_lues():
        if erreur is None and fiche["id"] == id_workflow:
            return Workflow(copy.deepcopy(fiche), api)
    raise ErreurWorkflow(f"Workflow inconnu : {id_workflow}")
