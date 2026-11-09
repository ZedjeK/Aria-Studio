"""⚡ Optimiser (onglet Réglages) : propositions de réglages selon le matériel et les recommandations des modèles.

Rien n'est appliqué ici : on renvoie un tableau « actuel → proposé », avec la raison et la source de chaque
ligne, et c'est l'utilisateur qui coche ce qu'il garde (l'interface passe ensuite par les routes de réglages).

Sources des recommandations, relevées sur les pages officielles des modèles :
    workflows : bloc « recommandations » de workflows/<id>/fiche.yaml ;
    LLM       : bloc « recommandations » des fiches de catalogue/llm.yaml, complété par l'ancien fichier local
                modeles/llm/recommandations.yaml (clé = nom du GGUF) pour les GGUF hors catalogue.
"""

import os

import psutil
import yaml

from app import fiches, mesures, modeles, reglages, workflows
from app.config import CONFIG
from app.emplacements import MODELES
from app.orchestrateur import lancement_llm

FICHIER_LLM = MODELES / "llm" / "recommandations.yaml"
GO = 2**30
SURCOUT_LLM_GO = 1.5  # contexte 4096 + tampons de calcul, au-delà du poids du GGUF (comme Découvrir)
CONTEXTE_BASE = 4096  # le contexte que SURCOUT_LLM_GO compte
# Cache du contexte (KV) par tranche de 1024 tokens au-delà de la base, en part du poids du GGUF : estimation (14B dense
# ≈ 0,17 Go, 27B ≈ 0,3 Go) ; prudente pour les modèles à attention locale ou MoE, qui en demandent moins.
KV_PAR_1024_PART_DU_POIDS = 0.02


def surcout_llm_go(poids_go: float, contexte: int) -> float:
    """VRAM au-delà du poids du GGUF : tampons, et cache du contexte qui grandit avec lui (Réflexion ou vision : 16 384)."""
    return SURCOUT_LLM_GO + max(0, contexte - CONTEXTE_BASE) / 1024 * poids_go * KV_PAR_1024_PART_DU_POIDS


def profil_materiel() -> dict:
    totale = mesures.vram_totale()
    # La part des autres applis (bureau, navigateur…) ; plafonnée pour ne pas compter un moteur d'Aria déjà chargé.
    autres = min(mesures.vram_utilisee(), int(CONFIG["casting"]["vram_max_occupee_go"] * GO))
    return {
        "gpu": mesures.nom_gpu(),
        "vram_totale_go": round(totale / GO, 1),
        "vram_autres_go": round(autres / GO, 1),
        "vram_libre_go": round((totale - autres) / GO, 1),
        "ram_go": round(psutil.virtual_memory().total / GO, 1),
        "coeurs": psutil.cpu_count(logical=False),
        "threads": psutil.cpu_count(logical=True),
    }


def _plafond_megapixels(marge_go: float, natif: float) -> float:
    """Mégapixels que la VRAM restante encaisse sans déborder (estimation prudente)."""
    if marge_go >= 12:
        return natif
    if marge_go >= 6:
        return min(natif, 2.0)
    return min(natif, 1.0)  # jamais au-delà du natif (H3 : 0,98 MP)


def _proposer(sortie: list, type_cible: str, cible: str, nom: str, p: reglages.Param, actuel, propose,
              raison: str, source: str | None):
    try:
        propose = p.valider(propose)
    except reglages.ErreurReglages:
        return  # valeur que ce réglage n'accepte pas ici (sampler absent de ComfyUI…) : on ne propose rien
    if propose != actuel:
        sortie.append({"type": type_cible, "cible": cible, "nom": nom, "cle": p.cle, "libelle": p.libelle,
                       "actuel": actuel, "propose": propose, "raison": raison, "source": source})


def _recommandations_llm() -> dict:
    """Celles des fiches du catalogue (catalogue/llm.yaml, livré avec Aria), complétées par l'ancien fichier local
    pour les GGUF ajoutés à la main."""
    locales = yaml.safe_load(FICHIER_LLM.read_text(encoding="utf-8")) or {} if FICHIER_LLM.is_file() else {}
    catalogue = {f["id"]: f["recommandations"] for f in fiches.catalogue_llm() if f.get("recommandations")}
    return {**locales, **catalogue}


def propositions() -> dict:
    materiel = profil_materiel()
    libre = materiel["vram_libre_go"]
    sortie, alertes, sans_reco = [], [], []

    for wf in workflows.catalogue():
        params, actuelles = reglages.params_workflow(wf), reglages.valeurs_workflow(wf)
        nom = wf.fiche["nom"]
        reco = wf.fiche.get("recommandations") or {}
        if not reco.get("valeurs"):
            sans_reco.append(nom)
        for r in reco.get("valeurs") or []:
            if r["cle"] in params:
                _proposer(sortie, "workflow", wf.id, nom, params[r["cle"]], actuelles[r["cle"]], r["valeur"],
                          r.get("note", "Recommandation officielle"), r.get("source"))
        estimee = float(wf.materiel["vram_estimee_go"])
        if estimee > libre:
            alertes.append(f"{nom} demande environ {estimee:g} Go de VRAM pour {libre:g} Go libres : ComfyUI en "
                           "décharge une partie en RAM, ça fonctionne mais plus lentement.")
        if wf.fiche["tache"].endswith("video") and (natif := reco.get("resolution_native_mp")):
            # Vidéo : la taille coûte des minutes par clip ; c'est un choix de résolution (0,4 / 0,6 / 1 MP), pas un
            # réglage à corriger. On informe sans proposer.
            alertes.append(f"{nom} est entraîné vers {natif:g} MP. Dans le Montage, c'est la résolution choisie qui "
                           f"décide (1 MP ≈ natif) ; ici, {actuelles.get('megapixels'):g} MP "
                           "pour le Casting. Plus haut = plus fin mais nettement plus lent (voir les notes de sa fiche).")
        elif "megapixels" in params and (natif := reco.get("resolution_native_mp")):
            cible = _plafond_megapixels(libre - estimee, float(natif))
            if cible < natif:
                alertes.append(f"{nom} est entraîné jusqu'à {natif:g} MP ; avec ta VRAM, je conseille de rester "
                               f"à {cible:g} MP (au-delà, risque de débordement et lenteur).")
            _proposer(sortie, "workflow", wf.id, nom, params["megapixels"], actuelles["megapixels"], cible,
                      f"Résolution native {natif:g} MP, plafonnée à ce que ta VRAM encaisse",
                      reco.get("source_resolution"))

    recos_llm = _recommandations_llm()
    params = reglages.params_llm()
    favoris = {m["fichier"]: m["nom"] for m in CONFIG["llama"].get("modeles_au_choix", [])}
    for fichier in modeles.llm_disponibles():
        nom = favoris.get(fichier, fichier.removesuffix(".gguf"))
        actuelles = reglages.valeurs_llm(fichier)
        reco = recos_llm.get(fichier) or {}
        if not reco.get("valeurs"):
            sans_reco.append(nom)
        for r in reco.get("valeurs") or []:
            if r["cle"] in params:
                _proposer(sortie, "llm", fichier, nom, params[r["cle"]], actuelles[r["cle"]], r["valeur"],
                          r.get("note", "Recommandation officielle"), r.get("source") or reco.get("source"))
        # Le contexte ne se réduit pas : llama.cpp (--fit) place lui-même ce qui ne tient pas, et un MoE comme Gemma 26B
        # A4B reste rapide malgré le débordement (81 tokens/s mesurés au Casting). On ne propose que de le remonter
        # au minimum que la Réflexion ou la vision exige (voir plus bas) ; le débordement, lui, est signalé.
        taille = os.path.getsize(modeles.DOSSIER / "llm" / fichier) / GO
        lancement = lancement_llm(fichier)
        vision = lancement["vision"]  # projecteur d'images, chargé d'office avec le LLM
        if vision is not None and vision.is_file():
            taille += os.path.getsize(vision) / GO
        else:
            vision = None
        # Le contexte réellement lancé : la Réflexion et la vision le relèvent d'office (à 16 384 au minimum), même si
        # le réglage affiché est plus bas. On propose d'aligner le réglage sur ce qui tourne, et la VRAM le compte.
        contexte = lancement["contexte"]
        if (p := params.get("contexte")) and actuelles["contexte"] < contexte:
            cause = "La Réflexion" if actuelles["reflexion"] else "Le LLM"
            if vision and actuelles["reflexion"]:
                cause = "La Réflexion et la vision"
            elif vision:
                cause = "La vision"
            _proposer(sortie, "llm", fichier, nom, p, actuelles["contexte"], contexte,
                      f"{cause} demande {contexte} tokens au minimum (consigne, images, réflexion, réponse) : Aria les "
                      f"lance déjà ainsi ; ce réglage le montre au lieu de {actuelles['contexte']}", None)
        if taille + surcout_llm_go(taille, contexte) > libre:
            avec = ", vision comprise" if vision else ""
            conseil = ""
            if actuelles["reflexion"] and not vision and contexte > actuelles["contexte"] \
                    and taille + surcout_llm_go(taille, actuelles["contexte"]) <= libre:
                conseil = (f" Sans la Réflexion, le contexte retomberait à {actuelles['contexte']} tokens et le modèle "
                           "tiendrait.")
            alertes.append(f"{nom} ({taille:.1f} Go{avec}, contexte {contexte}) dépasse ta VRAM libre ({libre:g} Go, "
                           "estimation) : llama.cpp en place une partie sur le processeur. Peu d'effet pour un modèle "
                           "MoE, nettement plus lent pour un modèle dense." + conseil)

    return {"materiel": materiel, "propositions": sortie, "alertes": alertes, "sans_recommandation": sans_reco}
