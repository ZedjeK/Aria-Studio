"""Catalogue des workflows : workflows/<id>/api.json + fiche.yaml (format des fiches : app/fiches.py)."""

import copy
import json
from dataclasses import dataclass
from pathlib import Path

import yaml

from app import fiches, modeles, reglages, securite
from app.config import ROOT

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
REFERENCES_MAX = 3  # limite de l'interface (celle du 2511)


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
        """Le modèle de diffusion livré avec le workflow (dans <Famille>/modeles)."""
        return next(f["nom"] for f in self.fiche["fichiers"] if f["dossier"] == f"{self.famille}/modeles")

    def variantes(self) -> list[str]:
        """Modèles utilisables : l'origine d'abord, puis tout ce qui est déposé dans <Famille>/modeles."""
        deposes = modeles.fichiers(f"{self.famille}/modeles", modeles.EXTENSIONS_IMAGE)
        return [self.modele_origine] + [m for m in deposes if m != self.modele_origine]

    def manquants(self) -> list[dict]:
        return [f for f in self.fiche["fichiers"] if not modeles.destination(f["dossier"], f["nom"]).is_file()]

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
            "loras": modeles.loras(self.famille),
        }

    def preparer(self, valeurs: dict) -> dict:
        """Copie de api.json avec les valeurs injectées (défauts de la fiche complétés par `valeurs`)."""
        api = copy.deepcopy(self.api)
        entrees = self.fiche["entrees"]
        # Ordre de priorité : valeurs de l'appel > réglages de Jérémy (data/reglages.yaml) > défauts de la fiche.
        regle = reglages.valeurs_workflow(self)
        tout = {**self.fiche.get("defauts", {}), **{k: v for k, v in regle.items() if k in entrees}, **valeurs}
        if tout.get("modele") is None:
            tout.pop("modele", None)  # le modèle d'origine, déjà dans api.json
        elif tout["modele"] not in self.variantes():
            raise ErreurWorkflow(f"Modèle absent de {self.famille}/modeles : {tout['modele']}")
        choix_loras = tout.pop("loras", None) or []
        references = tout.pop("references", None) or []  # images 2 et 3 (déjà téléversées) ; l'image 1 = « image »
        format_sortie = tout.pop("format_sortie", None)  # S'inspirer : format choisi au lieu de celui de l'image 1
        variation = tout.pop("variation", None)  # {"image": nom téléversé, "denoise": 0.3 à 0.9}
        # Point de passage obligé de tout prompt vers ComfyUI (génération, retouche, casting).
        if "prompt" in tout:
            securite.verifier(str(tout["prompt"]))
        for cle, valeur in tout.items():
            if cle not in entrees:
                raise ErreurWorkflow(f"{self.id} n'a pas d'entrée « {cle} »")
            noeud, entree = entrees[cle]
            api[noeud]["inputs"][entree] = valeur
        reglages.appliquer_noeuds(self, api, regle)
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
            if not 0 <= force <= 2:
                raise ErreurWorkflow(f"Force de LoRA entre 0 et 2 : {force}")
            nid = f"aria_lora_{i}"
            api[nid] = {"class_type": "LoraLoaderModelOnly",
                        "inputs": {"lora_name": lora["fichier"], "strength_model": force, "model": precedent}}
            precedent = [nid, 0]
        for nid, entree in lecteurs:
            api[nid]["inputs"][entree] = precedent


def _charger(dossier: Path) -> Workflow:
    fiche = yaml.safe_load((dossier / "fiche.yaml").read_text(encoding="utf-8"))
    if problemes := fiches.problemes_workflow(fiche):
        raise ErreurWorkflow(f"Fiche {dossier.name} : " + " ; ".join(problemes))
    api = json.loads((dossier / "api.json").read_text(encoding="utf-8"))
    return Workflow(fiche, api)


def catalogue() -> list[Workflow]:
    """Les workflows aux fiches valides ; une fiche invalide (ajoutée à la main) est écartée, et dite par erreurs()."""
    valides = []
    for d in sorted(DOSSIER.iterdir()):
        if (d / "fiche.yaml").is_file():
            try:
                valides.append(_charger(d))
            except ErreurWorkflow:
                pass
    return valides


def erreurs() -> list[str]:
    """Pourquoi certaines fiches sont écartées du catalogue."""
    sortie = []
    for d in sorted(DOSSIER.iterdir()):
        if (d / "fiche.yaml").is_file():
            try:
                _charger(d)
            except ErreurWorkflow as e:
                sortie.append(str(e))
    return sortie


def regles_prompt(id_workflow: str | None) -> list[str]:
    """Règles propres au modèle cible, ajoutées à la consigne de Llama (profil_prompt.regles de sa fiche)."""
    if not id_workflow:
        return []
    try:
        return list(obtenir(id_workflow).profil.get("regles") or [])
    except ErreurWorkflow:
        return []


def obtenir(id_workflow: str) -> Workflow:
    for wf in catalogue():
        if wf.id == id_workflow:
            return wf
    raise ErreurWorkflow(f"Workflow inconnu : {id_workflow}")
