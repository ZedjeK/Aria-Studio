"""Format des fiches de modèles (format 2, 2026-10-26) : ce qu'il faut savoir d'un modèle avant de l'installer.

Une fiche de workflow (workflows/<id>/fiche.yaml) et une fiche de LLM (catalogue/llm.yaml) partagent :
    licence   id, nom, url, resume (en français), commercial (true, false, ou null = à lire dans la licence),
              territoires_exclus (liste, ou null = à lire), verifiee (date de lecture de la licence à sa source),
              composants (facultatif : les fichiers sous une autre licence que le modèle principal)
    materiel  vram_estimee_go (poids chargés : une estimation), vram_min_go et vram_confort_go (MESURÉS par le banc
              de palier, null tant que ce n'est pas fait), mesure ({carte, date, methode}), cartes_verifiees
Une fiche de workflow a en plus :
    profil_prompt  style (prose, instruction, brief_video), langue, mots_max (facultatif), negatif, regles (ajoutées
                   à la consigne de Llama quand ce modèle est la cible), conseils et sources (documentation)

Pourquoi des minimums mesurés à part : sur la carte de 16 Go de Jérémy, le pic de VRAM relevé par le Casting tombe
toujours entre 15,7 et 15,9 Go, quel que soit le modèle (ComfyUI occupe la carte disponible) : il mesure la carte,
pas le besoin du modèle (constaté le 2026-10-26).
"""

import datetime

import yaml

from app.config import ROOT

FORMAT = 2
STYLES = ("prose", "instruction", "brief_video")
CATALOGUE_LLM = ROOT / "catalogue" / "llm.yaml"
SERVEURS_LLM = ("llama", "llama-prism")


def _date(valeur) -> bool:
    return isinstance(valeur, datetime.date)


def _nombre(valeur) -> bool:
    return isinstance(valeur, (int, float)) and not isinstance(valeur, bool) and valeur > 0


def problemes_licence(licence) -> list[str]:
    if not isinstance(licence, dict):
        return ["licence : un bloc (id, nom, url, resume, commercial, territoires_exclus, verifiee)"]
    p = [f"licence.{c} manquant" for c in ("id", "nom", "url", "resume") if not str(licence.get(c) or "").strip()]
    if licence.get("commercial") not in (True, False, None) or "commercial" not in licence:
        p.append("licence.commercial : true, false ou null (à lire)")
    if "territoires_exclus" not in licence or not (licence["territoires_exclus"] is None
                                                     or isinstance(licence["territoires_exclus"], list)):
        p.append("licence.territoires_exclus : une liste (vide si aucun), ou null (à lire)")
    composants = licence.get("composants", [])
    if not isinstance(composants, list) or any(not {"fichier", "licence"} <= set(c) for c in composants):
        p.append("licence.composants : une liste de {fichier, licence} (fichiers sous une autre licence)")
    if not _date(licence.get("verifiee")):
        p.append("licence.verifiee : la date de lecture de la licence (AAAA-MM-JJ)")
    return p


def problemes_materiel(materiel) -> list[str]:
    if not isinstance(materiel, dict):
        return ["materiel : un bloc (vram_estimee_go, vram_min_go, vram_confort_go, mesure, cartes_verifiees)"]
    p = [] if _nombre(materiel.get("vram_estimee_go")) else ["materiel.vram_estimee_go : un nombre de Go"]
    for cle in ("vram_min_go", "vram_confort_go"):
        if cle not in materiel or not (materiel[cle] is None or _nombre(materiel[cle])):
            p.append(f"materiel.{cle} : un nombre de Go mesuré, ou null tant que ce n'est pas mesuré")
    mesure = materiel.get("mesure")
    if mesure is not None and not (isinstance(mesure, dict) and {"carte", "date", "methode"} <= set(mesure)):
        p.append("materiel.mesure : null, ou {carte, date, methode}")
    if (materiel.get("vram_min_go") or materiel.get("vram_confort_go")) and mesure is None:
        p.append("materiel : une VRAM minimale ou de confort doit dire comment elle a été mesurée (mesure)")
    if not isinstance(materiel.get("cartes_verifiees"), list):
        p.append("materiel.cartes_verifiees : une liste (vide si aucune)")
    return p


def problemes_profil(profil) -> list[str]:
    if not isinstance(profil, dict):
        return ["profil_prompt : un bloc (style, langue, regles…)"]
    p = [] if profil.get("style") in STYLES else [f"profil_prompt.style : {', '.join(STYLES)}"]
    if not str(profil.get("langue") or "").strip():
        p.append("profil_prompt.langue manquant")
    if "mots_max" in profil and not (isinstance(profil["mots_max"], int) and 20 <= profil["mots_max"] <= 1000):
        p.append("profil_prompt.mots_max : un entier de 20 à 1000")
    for cle in ("regles", "conseils", "sources"):
        if not isinstance(profil.get(cle, []), list):
            p.append(f"profil_prompt.{cle} : une liste")
    return p


def problemes_workflow(fiche: dict) -> list[str]:
    if fiche.get("format") != FORMAT:
        return [f"format : {FORMAT} attendu (fiche d'un ancien format)"]
    p = [f"{c} manquant" for c in ("id", "nom", "tache", "famille", "fichiers", "entrees") if not fiche.get(c)]
    return p + problemes_licence(fiche.get("licence")) + problemes_materiel(fiche.get("materiel")) \
        + problemes_profil(fiche.get("profil_prompt"))


def problemes_llm(fiche: dict) -> list[str]:
    p = [f"{c} manquant" for c in ("id", "nom", "url", "page", "sha256") if not str(fiche.get(c) or "").strip()]
    if not str(fiche.get("id", "")).endswith(".gguf"):
        p.append("id : le nom du fichier GGUF")
    if not _nombre(fiche.get("taille")):
        p.append("taille : en octets")
    if fiche.get("serveur") not in SERVEURS_LLM:
        p.append(f"serveur : {', '.join(SERVEURS_LLM)}")
    return p + problemes_licence(fiche.get("licence")) + problemes_materiel(fiche.get("materiel"))


def catalogue_llm() -> list[dict]:
    """Les fiches des LLM ; une fiche invalide fait échouer le chargement (le catalogue est livré avec Aria)."""
    donnees = yaml.safe_load(CATALOGUE_LLM.read_text(encoding="utf-8")) or {}
    if donnees.get("format") != FORMAT:
        raise ValueError(f"{CATALOGUE_LLM.name} : format {FORMAT} attendu")
    fiches = donnees.get("llm") or []
    for fiche in fiches:
        if p := problemes_llm(fiche):
            raise ValueError(f"{CATALOGUE_LLM.name}, {fiche.get('id', '?')} : " + " ; ".join(p))
    return fiches


def fiche_llm(fichier: str) -> dict | None:
    return next((f for f in catalogue_llm() if f["id"] == fichier), None)
