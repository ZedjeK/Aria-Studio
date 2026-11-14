"""Serveur FastAPI d'Aria - Studio : sert l'interface web et l'API."""

import asyncio
import logging
import os
import re
import threading
import uuid
import webbrowser
from contextlib import asynccontextmanager
from pathlib import Path

import httpx
import uvicorn
from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from app import (assistant, fichier_reglages, casting, cles, collection, decouvrir, emplacements, estimation, fond, garde, mesures,
                 migration_modeles, mises_a_jour, modeles, moteurs, notifications, optimiseur, plateau, prompt, reglages,
                 retouche, routes_montage, routes_scenario, rushes, securite, services, workflows)
from app.securite import ErreurSecurite
from app.comfy import ErreurComfy
from app.config import CONFIG, ROOT, dossier_comfyui_livre
from app.emplacements import moteur
from app.sources import ErreurSource, Fiche, civitai, fiche_depuis_url
from app.telechargement import ErreurTelechargement

WEB = ROOT / "web"

# Partagés avec les routeurs (app/routes_*.py) : app/services.py.
telechargeur, orchestrateur, plateau_ = services.telechargeur, services.orchestrateur, services.plateau_
fiches: dict[str, Fiche] = {}  # fiches préparées, en attente de confirmation
_taches_fond: set[asyncio.Task] = set()


@asynccontextmanager
async def cycle_de_vie(_app: FastAPI):
    try:  # avant la migration des modèles, qui réécrit les empreintes : les fichiers recalculables vont dans cache\
        deplaces = emplacements.ranger_cache()
        if deplaces:
            logging.getLogger(__name__).info("Rangés dans cache : %s", ", ".join(deplaces))
    except OSError:  # jamais un empêchement de démarrer : ils seront simplement recalculés
        logging.getLogger(__name__).exception("Rangement du cache impossible")
    try:  # avant ComfyUI : l'ancienne organisation des modèles rangée côté image et vidéo (app/migration_modeles.py)
        migration_modeles.migrer()
    except Exception:  # noqa: BLE001 : la migration ne doit jamais empêcher Aria de démarrer
        logging.getLogger(__name__).exception("Migration des modèles impossible")
    await orchestrateur.reprendre()  # un ComfyUI laissé par une Aria fermée brutalement : repris, arrêté en quittant
    telechargeur.demarrer()
    plateau_.demarrer()
    verification = asyncio.create_task(mises_a_jour.verifier())  # au plus une fois par jour, sans bloquer le démarrage
    yield
    verification.cancel()
    await plateau_.arreter()
    await telechargeur.arreter()
    await orchestrateur.arreter()


SANS_CACHE = {"Cache-Control": "no-cache"}


class InterfaceSansCache(StaticFiles):
    """Le navigateur revérifie l'interface à chaque chargement (réponse 304 si rien n'a changé) :
    il ne garde jamais un ancien JS ou CSS après une mise à jour."""

    async def get_response(self, path, scope):
        reponse = await super().get_response(path, scope)
        reponse.headers.update(SANS_CACHE)
        return reponse


app = FastAPI(title="Aria Studio", lifespan=cycle_de_vie)
app.mount("/static", InterfaceSansCache(directory=WEB), name="static")
app.include_router(routes_scenario.routeur)
app.include_router(routes_montage.routeur)


def hotes_locaux() -> set[str]:
    """Les en-têtes Host acceptés : 127.0.0.1, localhost et [::1] sur le port d'Aria (config.yaml, section app), plus
    l'hôte de config.yaml s'il en nomme un autre (pas 0.0.0.0 ni ::, qui veulent dire « toutes les adresses »)."""
    port = CONFIG["app"]["port"]
    noms = {"127.0.0.1", "localhost", "[::1]"}
    hote = str(CONFIG["app"].get("host") or "").strip()
    if hote and hote not in ("0.0.0.0", "::", "[::]"):
        noms.add(f"[{hote}]" if ":" in hote and not hote.startswith("[") else hote)
    return {f"{nom}:{port}" for nom in noms}


@app.middleware("http")
async def local_seulement(requete: Request, suite):
    """N'accepte que l'interface d'Aria Studio elle-même (même garde qu'Aria Pilote).

    - Hôte : bloque le « DNS rebinding » (un site qui se ferait passer pour 127.0.0.1 pour lire la galerie, les
      réglages, ou piloter ComfyUI et Llama).
    - Origine : bloque les requêtes envoyées par une autre page ouverte dans le navigateur (quitter Aria, lancer un
      téléchargement, enregistrer une clé…). Le navigateur n'envoie pas d'Origin pour l'interface elle-même en GET,
      ni le lanceur ni curl : seule une origine présente et étrangère est refusée, quelle que soit la méthode.
    """
    hotes = hotes_locaux()
    if requete.headers.get("host") not in hotes:
        return JSONResponse(status_code=403, content={"detail": "Hôte refusé."})
    origine = requete.headers.get("origin")
    if origine is not None and origine not in {f"http://{h}" for h in hotes}:
        return JSONResponse(status_code=403, content={"detail": "Origine refusée."})
    return await suite(requete)


@app.exception_handler(garde.ErreurGarde)
async def attente_annulee(_requete, e: garde.ErreurGarde):
    """L'utilisateur a annulé une demande qui attendait la carte (réécriture, découpage…)."""
    return JSONResponse(status_code=409, content={"detail": str(e)})


_RESSOURCE = re.compile(r'"/static/([\w./-]+\.(?:js|css))"')


def _versionner(m: re.Match) -> str:
    fichier = WEB / m.group(1)
    version = fichier.stat().st_mtime_ns if fichier.is_file() else 0
    return f'"/static/{m.group(1)}?v={version}"'


@app.get("/")
def accueil():
    # Chaque JS / CSS porte sa date de modification dans son adresse : une nouvelle version a une
    # nouvelle adresse, donc une vieille copie gardée en cache ne peut jamais être réutilisée.
    page = _RESSOURCE.sub(_versionner, (WEB / "index.html").read_text(encoding="utf-8"))
    return HTMLResponse(page, headers=SANS_CACHE)


def commande_en_cours() -> dict | None:
    """Ce que fait la file du Plateau (image, retouche, clip, séquence, essai de voix) : la barre d'état le montre."""
    actives = [c for c in plateau_.commandes.values() if c.etat in ("attente", "en_cours")]
    if not actives:
        return None
    c = next((c for c in actives if c.etat == "en_cours"), actives[0])
    return {"type": c.type, "etat": c.etat, "etape": c.etape, "progression": round(c.progression, 2),
            "file": len(actives)}


@app.get("/api/etat")
def etat_direct():
    """Pastilles de la barre d'onglets : chaque moteur est chargé, en veille ou introuvable, plus la VRAM.

    « actif » = le moteur qui tient la carte (orchestrateur.actif), pas seulement un processus en vie : ComfyUI reste
    lancé quand un LLM prend la main (sa VRAM est vidée, son processus non), et doit alors s'afficher en veille."""
    sante_ = sante()

    def etat(nom: str, processus, installe: bool) -> str:
        if processus is not None and processus.poll() is None:
            return "actif" if orchestrateur.actif == nom else "veille"
        return "veille" if installe else "absent"

    llama = orchestrateur.llama
    favoris = {m["fichier"]: m["nom"] for m in CONFIG["llama"].get("modeles_au_choix", [])}
    try:
        vram = {"vram_utilisee_go": round(mesures.vram_utilisee() / 2**30, 1),
                "vram_totale_go": round(mesures.vram_totale() / 2**30, 1)}
    except Exception:  # pas de GPU NVIDIA lisible : on affiche seulement les moteurs
        vram = {"vram_utilisee_go": None, "vram_totale_go": None}
    return {
        "comfyui": {"etat": etat("comfyui", orchestrateur.comfy.process, sante_["comfyui"]["installe"])},
        "llama": {"etat": etat("llama", llama.process, sante_["llama"]["installe"]),
                  "modele": favoris.get(llama.modele.name, llama.modele.stem) if llama.modele else None},
        **vram,
        "commande": commande_en_cours(),
        # Garde-VRAM : une demande attend la carte (bandeau), ou une autre appli la tient (pastille orange).
        "garde": {"attente": orchestrateur.garde.attente,
                  "alerte": None if orchestrateur.garde.attente else garde.alerte(orchestrateur.actif)},
    }


class DemandeMiseAJour(BaseModel):
    active: bool


@app.get("/api/mise-a-jour")
async def mise_a_jour():
    """Version installée et dernière release (GitHub n'est interrogé que si la réponse a plus d'un jour)."""
    return await mises_a_jour.verifier()


@app.post("/api/mise-a-jour/verifier")
async def verifier_mise_a_jour():
    return await mises_a_jour.verifier(force=True)


@app.put("/api/mise-a-jour")
def regler_mise_a_jour(d: DemandeMiseAJour):
    return mises_a_jour.activer(d.active)


class DemandeInstallation(BaseModel):
    proposee: bool


@app.get("/api/installation")
def lire_installation():
    """Installation par l'exe : installé dans %USERPROFILE%\\.Aria-Studio\\programme ou non, et la proposition du lanceur."""
    return emplacements.installation()


@app.put("/api/installation")
def regler_installation(d: DemandeInstallation):
    return emplacements.proposer_installation(d.proposee)


# Sauvegardes de reglages.json (app/fichier_reglages.py) : une avant chaque changement, les 20 dernières gardées.
@app.get("/api/sauvegardes")
def lister_sauvegardes():
    return fichier_reglages.sauvegardes()


class DemandeRestaurationReglages(BaseModel):
    nom: str


@app.post("/api/sauvegardes/restaurer")
def restaurer_sauvegarde(d: DemandeRestaurationReglages):
    """Les réglages actuels sont sauvegardés d'abord ; certains (dossier des modèles) ne valent qu'au redémarrage."""
    try:
        return fichier_reglages.restaurer(d.nom)
    except fichier_reglages.ErreurSauvegarde as e:
        raise HTTPException(400, str(e))


class DemandeNotifications(BaseModel):
    active: bool | None = None
    son: bool | None = None


@app.get("/api/notifications")
def lire_notifications():
    """Prévenir à la fin d'un job (bandeau, notification Windows) et avec un son : les choix de Réglages → Système."""
    return notifications.etat()


@app.put("/api/notifications")
def regler_notifications(d: DemandeNotifications):
    return notifications.regler(d.active, d.son)


class DemandeFond(BaseModel):
    actif: bool | None = None
    vitesse: str | None = None
    minimum: int | None = None
    maximum: int | None = None


@app.get("/api/fond")
def lire_fond():
    """Fond animé de l'interface : les quatre réglages de Réglages → Système → Fond animé."""
    return fond.etat()


@app.put("/api/fond")
def regler_fond(d: DemandeFond):
    try:
        return fond.regler(d.actif, d.vitesse, d.minimum, d.maximum)
    except ValueError as e:
        raise HTTPException(422, str(e))


class DemandeOuverture(BaseModel):
    dossier: str  # racine, data, modeles ou moteurs
    sous: str | None = None  # sous-dossier des modèles (« video/MiniMax-H3/modeles ») ou d'un moteur (« comfyui »)


class DemandeDossierModeles(BaseModel):
    chemin: str | None = None  # None : revenir au dossier par défaut


@app.get("/api/emplacements")
def etat_emplacements():
    return emplacements.etat()


@app.post("/api/emplacements/ouvrir")
def ouvrir_emplacement(d: DemandeOuverture):
    """📂 : ouvre le dossier dans l'Explorateur de Windows, sur la machine du serveur."""
    connus = {"modeles": modeles.dossiers(), "moteurs": [m["dossier"] for m in moteurs.catalogue()["moteurs"].values()]}
    if d.sous and d.sous not in connus.get(d.dossier, []):
        raise HTTPException(400, f"Sous-dossier inconnu : {d.sous}")
    try:
        return {"ouvert": str(emplacements.ouvrir(d.dossier, d.sous))}
    except emplacements.ErreurEmplacement as e:
        raise HTTPException(400, str(e))


@app.post("/api/emplacements/modeles/parcourir")
def parcourir_modeles():
    """Sélecteur de dossier natif ; rien n'est enregistré tant que l'interface n'a pas confirmé."""
    try:
        return {"chemin": emplacements.parcourir()}
    except emplacements.ErreurEmplacement as e:
        raise HTTPException(400, str(e))


@app.put("/api/emplacements/modeles")
def choisir_dossier_modeles(d: DemandeDossierModeles):
    try:
        return emplacements.choisir_modeles(d.chemin)
    except emplacements.ErreurEmplacement as e:
        raise HTTPException(400, str(e))


# Moteurs (app/moteurs.py) : ComfyUI portable et llama-server aux versions du catalogue


@app.get("/api/moteurs")
def etat_moteurs():
    return {"carte": moteurs.carte(), "moteurs": moteurs.etat(), "tache": moteurs.tache()}


@app.post("/api/moteurs/annuler")
def annuler_moteur():
    return {"annule": moteurs.annuler()}


@app.post("/api/moteurs/{id_moteur}/installer")
async def installer_moteur(id_moteur: str):
    """Télécharge (si besoin), vérifie et installe ; l'échange final attend que l'orchestrateur ait arrêté le moteur."""
    try:
        return moteurs.lancer(id_moteur, verrou=lambda: orchestrateur.sans_moteur(id_moteur))
    except moteurs.ErreurMoteur as e:
        raise HTTPException(409, str(e))


@app.post("/api/moteurs/{id_moteur}/revenir")
async def revenir_moteur(id_moteur: str):
    try:
        async with orchestrateur.sans_moteur(id_moteur):
            fiche = moteurs.revenir(id_moteur)
    except moteurs.ErreurMoteur as e:
        raise HTTPException(409, str(e))
    return {"version": (fiche or {}).get("version")}


# Assistant de premier lancement (app/assistant.py) : packs, licences, téléchargement, favoris


class DemandePacks(BaseModel):
    packs: list[str]


@app.get("/api/assistant")
def etat_assistant():
    return assistant.etat()


def _assistant(action, *args):
    try:
        return action(*args)
    except assistant.ErreurAssistant as e:
        raise HTTPException(400, str(e))


@app.post("/api/assistant/plan")
def plan_assistant(d: DemandePacks):
    return _assistant(assistant.plan, d.packs)


@app.post("/api/assistant/licences")
def licences_assistant(d: DemandePacks):
    """« J'ai lu et j'accepte » pour chaque pack coché (daté dans aria.yaml)."""
    return _assistant(assistant.accepter, d.packs)


@app.post("/api/assistant/telecharger")
def telecharger_assistant(d: DemandePacks):
    return _assistant(assistant.telecharger, d.packs, telechargeur)


@app.post("/api/assistant/terminer")
def terminer_assistant(d: DemandePacks):
    return _assistant(assistant.terminer, d.packs)


@app.post("/api/assistant/essai")
def essai_assistant():
    """Une image rapide (peu de steps) pour vérifier que toute la chaîne tourne ; suivie par /api/commandes."""
    return services.commander(lambda: _assistant(assistant.essayer, plateau_))


class DemandeComfyuiExterne(BaseModel):
    chemin: str | None = None  # None : revenir au ComfyUI d'Aria


@app.post("/api/moteurs/comfyui/parcourir")
def parcourir_comfyui():
    try:
        return {"chemin": emplacements.parcourir(str(Path.home()), "Dossier de ton ComfyUI portable", doit_exister=True)}
    except emplacements.ErreurEmplacement as e:
        raise HTTPException(400, str(e))


@app.put("/api/moteurs/comfyui/externe")
async def choisir_comfyui(d: DemandeComfyuiExterne):
    """« J'ai déjà ComfyUI » : vérifié en le lançant une fois (jusqu'à une minute), puis utilisé tout de suite."""
    try:
        dossier = moteurs.examiner_comfyui(d.chemin) if d.chemin else None
        async with orchestrateur.sans_moteur("comfyui"):
            if dossier:
                manquants = await moteurs.verifier_comfyui(dossier)
                if manquants:
                    raise moteurs.ErreurMoteur("Il manque à ce ComfyUI des nœuds dont Aria se sert : " + ", ".join(manquants))
            emplacements.choisir_comfyui(dossier)
            CONFIG["comfyui"]["dossier"] = str(dossier) if dossier else dossier_comfyui_livre()
            orchestrateur.comfy.dossier = moteur(CONFIG["comfyui"]["dossier"])
    except (moteurs.ErreurMoteur, ErreurComfy) as e:
        raise HTTPException(409, str(e))
    return {"moteurs": moteurs.etat()}


@app.post("/api/garde/forcer")
def forcer_garde():
    """« Lancer quand même » : la demande qui attend la carte part, lentement s'il le faut."""
    return {"ok": orchestrateur.garde.forcer()}


@app.post("/api/garde/annuler")
def annuler_garde():
    return {"ok": orchestrateur.garde.annuler()}


@app.get("/api/sante")
def sante():
    comfy = moteur(CONFIG["comfyui"]["dossier"])
    llama = moteur(CONFIG["llama"]["server"])
    return {
        "serveur": "ok",
        "comfyui": {
            "installe": (comfy / "ComfyUI" / "main.py").is_file(),
            "port": CONFIG["comfyui"]["port"],
        },
        "llama": {
            "installe": llama.is_file(),
            "port": CONFIG["llama"]["port"],
        },
    }


class DemandeFiche(BaseModel):
    url: str


class DemandeTelechargement(BaseModel):
    fiche_id: str
    dossier: str


@app.get("/api/modeles")
def liste_modeles():
    return {
        "dossiers": modeles.dossiers(),
        "espace_libre": modeles.espace_libre(),
        "modeles": modeles.inventaire(),
    }


@app.post("/api/modeles/fiche")
async def preparer_fiche(demande: DemandeFiche):
    try:
        fiche = await fiche_depuis_url(demande.url)
    except ErreurSource as e:
        raise HTTPException(400, str(e))
    fiche_id = uuid.uuid4().hex[:12]
    fiches[fiche_id] = fiche
    return {"id": fiche_id, **fiche.to_dict(), "espace_libre": modeles.espace_libre()}


class DemandeValidation(BaseModel):
    cle: str
    steps: int  # 1 à 150 : les steps validés par l'utilisateur


@app.post("/api/modeles/valider")
def valider_modele(demande: DemandeValidation):
    """Met en service un checkpoint (téléchargé, déposé à la main ou d'origine) avec les steps que l'utilisateur a
    validés."""
    try:
        return workflows.valider_checkpoint(demande.cle, demande.steps)
    except (modeles.ErreurModele, workflows.ErreurWorkflow, reglages.ErreurReglages) as e:
        raise HTTPException(400, str(e))


# Corbeille des modèles (app/modeles.py) : ses routes passent avant /api/modeles/{cle:path}, qui sinon les prendrait.
@app.get("/api/modeles/corbeille")
def corbeille_modeles():
    return modeles.corbeille()


class DemandeRestauration(BaseModel):
    cle: str


@app.post("/api/modeles/corbeille/restaurer")
def restaurer_modele(d: DemandeRestauration):
    try:
        return modeles.restaurer(d.cle)
    except modeles.ErreurModele as e:
        raise HTTPException(400, str(e))


@app.delete("/api/modeles/corbeille")
def vider_corbeille_modeles():
    """La seule suppression définitive d'un modèle."""
    return modeles.vider_corbeille()


@app.delete("/api/modeles/{cle:path}")
def supprimer_modele(cle: str):
    """Met le modèle à la corbeille ; seul « Vider la corbeille » l'efface pour de bon."""
    try:
        return modeles.mettre_a_la_corbeille(cle)
    except modeles.ErreurModele as e:
        raise HTTPException(400, str(e))


@app.get("/api/telechargements")
def liste_telechargements():
    return [t.to_dict() for t in telechargeur.taches.values()]


@app.post("/api/telechargements")
def lancer_telechargement(demande: DemandeTelechargement):
    fiche = fiches.get(demande.fiche_id)
    if fiche is None:
        raise HTTPException(404, "Fiche expirée : prépare-la à nouveau")
    try:
        tache = telechargeur.ajouter(fiche, demande.dossier)
    except (ErreurTelechargement, modeles.ErreurModele) as e:
        raise HTTPException(400, str(e))
    del fiches[demande.fiche_id]
    return tache.to_dict()


@app.post("/api/telechargements/{id_tache}/annuler")
def annuler_telechargement(id_tache: str):
    try:
        telechargeur.annuler(id_tache)
    except ErreurTelechargement as e:
        raise HTTPException(404, str(e))
    return telechargeur.taches[id_tache].to_dict()


# Onglet Modèles : Ma collection, usages (Texte → image, Image → image, Vidéo), sources retrouvées (app/collection.py)

def _collection(action, *args):
    try:
        return action(*args)
    except collection.ErreurCollection as e:
        raise HTTPException(400, str(e))


@app.get("/api/collection")
def ma_collection():
    return collection.collection()


class DemandeReglageLora(BaseModel):
    cle: str  # <Famille>/loras/<fichier>
    declencheurs: str = ""
    force: float = 1.0


class DemandeLora(BaseModel):
    cle: str


@app.put("/api/collection/lora")
def regler_lora(d: DemandeReglageLora):
    """Ma collection : les mots déclencheurs et la force d'une LoRA (ceux que le Plateau proposera)."""
    try:
        securite.verifier(d.declencheurs)
        return modeles.regler_lora(d.cle, d.declencheurs, d.force)
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except modeles.ErreurModele as e:
        raise HTTPException(400, str(e))


@app.post("/api/collection/lora/oublier")
def oublier_reglage_lora(d: DemandeLora):
    """Revient aux valeurs de la source : mots déclarés par l'auteur, force de ses images d'exemple."""
    try:
        return modeles.oublier_reglage_lora(d.cle)
    except modeles.ErreurModele as e:
        raise HTTPException(400, str(e))


@app.post("/api/collection/lora/relire")
async def relire_lora(d: DemandeLora):
    """Relit la version Civitai d'une LoRA : mots déclarés, force et prompt de ses images d'exemple."""
    try:
        modeles._famille_lora(d.cle)
        infos = modeles.charger_registre().get(d.cle) or {}
        if infos.get("source") != "civitai" or not (id_version := civitai.id_version(infos.get("page"))):
            raise HTTPException(400, "Cette LoRA ne vient pas de Civitai (ou sa page est inconnue)")
        version = await civitai.lire_version(id_version)
        modeles.mettre_a_jour_source(d.cle, {
            "mots_declencheurs": [m.strip() for m in version.get("trainedWords") or [] if m.strip()],
            **civitai.usage_auteur(version, d.cle.rsplit("/", 1)[-1])})
        return modeles.reglage_lora(d.cle)
    except modeles.ErreurModele as e:
        raise HTTPException(400, str(e))
    except (ErreurSource, httpx.HTTPError) as e:
        raise HTTPException(502, f"Civitai n'a pas répondu : {e}")


@app.post("/api/collection/sources")
async def retrouver_sources():
    """Seulement sur un clic : l'empreinte de chaque fichier déposé à la main est demandée à Civitai."""
    return _collection(collection.lancer_identification)


@app.get("/api/collection/sources")
def suivi_sources():
    return collection.identification()


@app.get("/api/usages")
def liste_usages():
    return collection.usages()


class DemandeFavoriUsage(BaseModel):
    cle: str  # texte_vers_image, retouche, references, image_vers_video, texte_vers_video
    workflow: str


@app.put("/api/usages/favori")
def favori_usage(d: DemandeFavoriUsage):
    return _collection(collection.definir_favori, d.cle, d.workflow)


class DemandeLlmWorkflow(BaseModel):
    workflow: str
    llm: str | None = None  # None : le LLM par défaut


@app.put("/api/usages/llm")
def llm_usage(d: DemandeLlmWorkflow):
    """Le LLM qui réécrit les prompts d'un workflow, repris par le Plateau, le Montage et le Scénario."""
    return _collection(collection.definir_llm, d.workflow, d.llm)


@app.post("/api/llm/{fichier}/installer")
def installer_llm(fichier: str):
    """Télécharge un LLM du catalogue (un spécialiste avec sa consigne et sa vision) : le clic vaut accord."""
    return _collection(collection.installer_llm, fichier, telechargeur)


@app.post("/api/workflows/{id_workflow}/telecharger")
def telecharger_workflow(id_workflow: str):
    """Met en file ce qui manque à un workflow (le clic, après lecture de sa licence, vaut accord)."""
    return _collection(collection.telecharger_manquants, id_workflow, telechargeur)


class DemandeBanc(BaseModel):
    tache: str
    workflows: list[str] | None = None


@app.get("/api/bancs")
def liste_bancs(liste: bool = True):
    """`liste=false` : le banc actif seulement (le suivi d'un banc en cours, toutes les 2 s)."""
    return {"actif": services.banc_actif.etat_public() if services.banc_actif else None, "bancs": casting.liste() if liste else None}


@app.post("/api/bancs")
async def lancer_banc(demande: DemandeBanc):
    if services.banc_en_cours():
        raise HTTPException(409, "Un banc est déjà en cours")
    try:
        services.banc_actif = casting.creer(demande.tache, demande.workflows)
    except (casting.ErreurBanc, KeyError) as e:
        raise HTTPException(400, str(e))
    tache = asyncio.create_task(services.banc_actif.executer(orchestrateur, telechargeur))
    _taches_fond.add(tache)
    tache.add_done_callback(_taches_fond.discard)
    return services.banc_actif.etat_public()


def _en_cours(id_banc: str) -> bool:
    return bool(services.banc_actif and services.banc_actif.id == id_banc and services.banc_actif.etat in ("attente", "en_cours"))


@app.get("/api/bancs/{id_banc}")
def detail_banc(id_banc: str):
    try:
        resultats = casting.vue(id_banc)
    except casting.ErreurBanc as e:
        raise HTTPException(404, str(e))
    return {"en_cours": _en_cours(id_banc), **resultats}


@app.get("/api/bancs/{id_banc}/images/{cle}/{fichier}")
def image_banc(id_banc: str, cle: str, fichier: str):
    try:
        return FileResponse(casting.chemin_image(id_banc, cle, fichier))
    except casting.ErreurBanc as e:
        raise HTTPException(404, str(e))


class DemandeImage(BaseModel):
    cle: str
    fichier: str


class DemandeNote(DemandeImage):
    note: int


class DemandeFavori(BaseModel):
    workflow: str


def _modifier_banc(id_banc: str, action):
    if _en_cours(id_banc):
        raise HTTPException(409, "Attends la fin du banc")
    try:
        action()
    except casting.ErreurBanc as e:
        raise HTTPException(400, str(e))
    return casting.vue(id_banc)


@app.post("/api/bancs/{id_banc}/notes")
def noter_image(id_banc: str, demande: DemandeNote):
    return _modifier_banc(id_banc, lambda: casting.noter(id_banc, demande.cle, demande.fichier, demande.note))


@app.post("/api/bancs/{id_banc}/reveler")
def reveler_banc(id_banc: str):
    return _modifier_banc(id_banc, lambda: casting.reveler(id_banc))


@app.post("/api/bancs/{id_banc}/favori")
def favori_banc(id_banc: str, demande: DemandeFavori):
    return _modifier_banc(id_banc, lambda: casting.choisir_favori(id_banc, demande.workflow))


@app.post("/api/bancs/{id_banc}/source")
def source_banc(id_banc: str, demande: DemandeImage):
    return _modifier_banc(id_banc, lambda: casting.utiliser_comme_source(id_banc, demande.cle, demande.fichier))


# Onglet Réglages : LLM et workflows (data/reglages.yaml)

@app.get("/api/reglages")
def lire_reglages():
    return reglages.etat()


class DemandeReglagesLlm(BaseModel):
    valeurs: dict = {}  # {réglage: valeur} ; None = valeur d'origine
    consignes: dict[str, str | None] = {}  # {reecriture|retouche|precisions|…: texte} ; None = d'origine


@app.put("/api/reglages/llm/{fichier}")
def regler_llm(fichier: str, demande: DemandeReglagesLlm):
    try:
        return reglages.modifier_llm(fichier, demande.valeurs, demande.consignes)
    except reglages.ErreurReglages as e:
        raise HTTPException(400, str(e))


class DemandeReglagesWorkflow(BaseModel):
    valeurs: dict = {}


@app.get("/api/reglages/optimiser")
def optimiser_reglages():
    """Propositions seulement : rien n'est appliqué sans l'accord de l'utilisateur (routes PUT ci-dessus et dessous)."""
    return optimiseur.propositions()


@app.put("/api/reglages/workflows/{id_workflow}")
def regler_workflow(id_workflow: str, demande: DemandeReglagesWorkflow):
    try:
        return reglages.modifier_workflow(id_workflow, demande.valeurs)
    except (reglages.ErreurReglages, workflows.ErreurWorkflow) as e:
        raise HTTPException(400, str(e))


@app.put("/api/reglages/workflows/{id_workflow}/variantes/{fichier}")
def regler_variante(id_workflow: str, fichier: str, demande: DemandeReglagesWorkflow):
    """Réglages propres à un checkpoint : LoRA du workflow gardée ou non, steps imposés."""
    try:
        return reglages.modifier_variante(id_workflow, fichier, demande.valeurs)
    except (reglages.ErreurReglages, workflows.ErreurWorkflow) as e:
        raise HTTPException(400, str(e))


# Plateau : créations et rushes

class DemandeCreation(BaseModel):
    idee: str = ""
    prompt: str = ""  # tourné tel quel : le LLM n'écrit qu'au Scénario
    workflow: str | None = None
    format: str = "1:1 (Square)"
    seed: int | None = None
    nombre: int = 1
    parent: str | None = None
    modele: str | None = None  # variante de <Famille>/modeles ; absent = modèle d'origine
    loras: list[dict] | None = None  # [{fichier, force, declencheurs}] de <Famille>/loras
    references: list[str] | None = None  # rushes de référence (zone « Références » du Plateau)
    mode_reference: str | None = None  # inspirer, variation
    liberte: float = 0.6  # Variation : 0,05 (copie) à 1 (libre)
    dossier: str | None = None  # dossier des rushes où ranger les images
    ecrit_par: str | None = None  # le LLM qui a écrit le prompt au Scénario (noté sur la fiche, rien n'est réécrit)


@app.get("/api/plateau/options")
def options_plateau():
    return plateau_.options()


@app.get("/api/plateau/estimation")
def estimation_tournage(mode: str, qualite: str, duree: int, nombre: int = 1, modele: str | None = None):
    """Temps estimé d'un tournage du Montage : checkpoint (steps, poids), résolution, durée, nombre de clips, VRAM libre
    (app/estimation.py)."""
    if (mode not in plateau.MODES_VIDEO or qualite not in plateau.QUALITES_VIDEO or duree not in plateau.DUREES_VIDEO
            or not 1 <= nombre <= plateau.CLIPS_MAX):
        raise HTTPException(status_code=400, detail="Mode, résolution, durée ou nombre de clips inconnus")
    return estimation.estimer(mode, modele or None, qualite, duree, nombre)


@app.post("/api/creations")
def creer(demande: DemandeCreation):
    if services.banc_en_cours():
        raise HTTPException(409, "Un banc est en cours")
    if not demande.prompt.strip():
        raise HTTPException(400, "Le prompt est vide : écris-le ici, ou au ✍️ Scénario")
    try:
        commande = plateau_.commander(demande.idee or demande.prompt, demande.prompt.strip(), demande.ecrit_par, demande.workflow,
                                      demande.format, demande.seed, demande.nombre, demande.parent, demande.modele,
                                      demande.loras, demande.references, demande.mode_reference, demande.liberte,
                                      rushes.valider_dossier(demande.dossier))
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (plateau.ErreurPlateau, workflows.ErreurWorkflow, rushes.ErreurRushes) as e:
        raise HTTPException(400, str(e))
    return commande.to_dict()


class DemandeRetouche(BaseModel):
    source: str
    instruction: str
    reecrire: bool = True
    llm: str | None = None
    workflow: str | None = None
    masque: str | None = None  # PNG en data URL : blanc/opaque = zone à retoucher
    fondu: int = 24
    seed: int | None = None
    modele: str | None = None
    loras: list[dict] | None = None
    nombre: int = 1  # variantes (seed, seed + 1…)
    dossier: str | None = None  # dossier de Rushes ; absent = celui de l'image d'origine
    format: str | None = None  # recadre l'image au centre à ce format avant la retouche ; absent = son format


class DemandeAgrandissement(BaseModel):
    source: str
    largeur: int
    hauteur: int
    ancrage: str = "centre"
    precision: str = ""
    reecrire: bool = True
    llm: str | None = None
    workflow: str | None = None
    fondu: int = 96
    seed: int | None = None
    modele: str | None = None
    loras: list[dict] | None = None
    nombre: int = 1
    dossier: str | None = None


class DemandeImport(BaseModel):
    image: str  # data URL
    nom: str = ""
    dossier: str | None = None  # dossier des rushes ouvert au moment de l'import


@app.post("/api/retouches")
def retoucher(d: DemandeRetouche):
    def action():
        masque = retouche.depuis_data_url(d.masque) if d.masque else None
        return plateau_.commander_retouche(d.source, d.instruction, d.reecrire, d.llm, d.workflow, masque,
                                           d.fondu, d.seed, d.modele, d.loras, d.nombre,
                                           rushes.valider_dossier(d.dossier), d.format)
    return services.commander(action)


@app.post("/api/agrandissements")
def agrandir(d: DemandeAgrandissement):
    return services.commander(lambda: plateau_.commander_agrandissement(
        d.source, d.largeur, d.hauteur, d.ancrage, d.precision, d.reecrire, d.llm, d.workflow, d.fondu, d.seed,
        d.modele, d.loras, d.nombre, rushes.valider_dossier(d.dossier)))


@app.post("/api/creations/import")
def importer_image(d: DemandeImport):
    try:
        return plateau.fiche(plateau.importer(retouche.depuis_data_url(d.image), d.nom, rushes.valider_dossier(d.dossier)))
    except (retouche.ErreurRetouche, rushes.ErreurRushes) as e:
        raise HTTPException(400, str(e))


@app.get("/api/commandes/{id_commande}")
def suivre_commande(id_commande: str):
    commande = plateau_.commandes.get(id_commande)
    if commande is None:
        raise HTTPException(404, "Commande inconnue")
    return commande.to_dict()


@app.post("/api/commandes/{id_commande}/annuler")
def annuler_commande(id_commande: str):
    """✕ Annuler sous une barre de progression (Plateau, Montage, Séquence)."""
    try:
        return plateau_.annuler(id_commande).to_dict()
    except plateau.ErreurPlateau as e:
        raise HTTPException(404, str(e))


@app.get("/api/creations")
def galerie(vue: str = "tous", dossier: str | None = None, limite: int = 500, genre: str = "images"):
    """Rushes (genre images) ou Bobines (genre clips) d'une vue ou d'un dossier."""
    try:
        return rushes.liste(vue, dossier, limite, genre)
    except rushes.ErreurRushes as e:
        raise HTTPException(400, str(e))


@app.get("/api/creations/{id_creation}")
def fiche_creation(id_creation: str):
    try:
        return plateau.fiche(id_creation)
    except plateau.ErreurPlateau as e:
        raise HTTPException(404, str(e))


@app.get("/api/creations/{id_creation}/video")
def video_creation(id_creation: str):
    try:
        return FileResponse(plateau.chemin_video(id_creation), media_type="video/mp4")
    except plateau.ErreurPlateau as e:
        raise HTTPException(404, str(e))


@app.get("/api/creations/{id_creation}/image")
def image_creation(id_creation: str):
    try:
        return FileResponse(plateau.chemin_image(id_creation))
    except plateau.ErreurPlateau as e:
        raise HTTPException(404, str(e))


@app.delete("/api/creations/{id_creation}")
def supprimer_creation(id_creation: str):
    """Met le rush à la corbeille ; seul « Vider la corbeille » supprime pour de bon."""
    try:
        rushes.agir([id_creation], "corbeille")
    except plateau.ErreurPlateau as e:
        raise HTTPException(404, str(e))
    return {"corbeille": id_creation}


# Rushes : sélection, dossiers, archives, corbeille

@app.get("/api/rushes")
def etat_rushes(genre: str = "images"):
    try:
        return rushes.etat(genre)
    except rushes.ErreurRushes as e:
        raise HTTPException(400, str(e))


class DemandeLot(BaseModel):
    ids: list[str]
    action: str  # corbeille, restaurer, archiver, desarchiver, ranger
    dossier: str | None = None  # pour « ranger » ; None = Non rangés


@app.post("/api/rushes/lot")
def agir_sur_lot(demande: DemandeLot):
    try:
        return {"modifies": rushes.agir(demande.ids, demande.action, demande.dossier)}
    except (rushes.ErreurRushes, plateau.ErreurPlateau) as e:
        raise HTTPException(400, str(e))


@app.delete("/api/rushes/corbeille")
def vider_corbeille(genre: str = "images"):
    try:
        return {"supprimes": rushes.vider_corbeille(genre)}
    except rushes.ErreurRushes as e:
        raise HTTPException(400, str(e))


class DemandeDossier(BaseModel):
    nom: str


@app.post("/api/rushes/dossiers")
def creer_dossier(demande: DemandeDossier, genre: str = "images"):
    try:
        return {"nom": rushes.creer_dossier(demande.nom, genre)}
    except rushes.ErreurRushes as e:
        raise HTTPException(400, str(e))


@app.put("/api/rushes/dossiers/{nom}")
def renommer_dossier(nom: str, demande: DemandeDossier, genre: str = "images"):
    try:
        return {"nom": rushes.renommer_dossier(nom, demande.nom, genre)}
    except rushes.ErreurRushes as e:
        raise HTTPException(400, str(e))


@app.delete("/api/rushes/dossiers/{nom}")
def supprimer_dossier(nom: str, genre: str = "images"):
    try:
        return {"liberes": rushes.supprimer_dossier(nom, genre)}
    except rushes.ErreurRushes as e:
        raise HTTPException(404, str(e))


# Découvrir (onglet Modèles) : Civitai / Hugging Face

@app.get("/api/decouvrir")
async def decouvrir_modeles(source: str, type: str, texte: str = "", famille: str | None = None,
                   tri: str = "telechargements", curseur: str | None = None, adulte: bool = False):
    try:
        if source == "civitai":
            return await decouvrir.chercher_civitai(texte, type, famille, tri, curseur, adulte)
        if source == "huggingface":
            return await decouvrir.chercher_hf(texte, type, tri, famille)
    except decouvrir.ErreurDecouvrir as e:
        raise HTTPException(400, str(e))
    raise HTTPException(400, f"Source inconnue : {source}")


@app.get("/api/decouvrir/hf/fichiers")
async def decouvrir_fichiers_hf(depot: str, type: str = "llm", famille: str | None = None):
    try:
        return await decouvrir.fichiers_hf(depot, type, famille)
    except decouvrir.ErreurDecouvrir as e:
        raise HTTPException(400, str(e))


# Clés d'accès (app/cles.py) : saisies ici, rangées dans le coffre Windows ; seule leur origine revient.

@app.get("/api/jetons")
def jetons_presents():
    """Les clés d'accès sont-elles définies ? (Présence seulement : leur valeur ne revient jamais.)"""
    return {s: e["origine"] is not None for s, e in cles.etat().items()}


@app.get("/api/cles")
def etat_cles():
    return cles.etat()


class DemandeCle(BaseModel):
    valeur: str


def _cles(action, *args):
    try:
        return action(*args)
    except cles.ErreurCle as e:
        raise HTTPException(400, str(e))


@app.put("/api/cles/{source}")
def enregistrer_cle(source: str, d: DemandeCle):
    return _cles(cles.enregistrer, source, d.valeur)


@app.delete("/api/cles/{source}")
def supprimer_cle(source: str):
    return _cles(cles.supprimer, source)


@app.post("/api/cles/{source}/tester")
async def tester_cle(source: str):
    try:
        return await cles.tester(source)
    except cles.ErreurCle as e:
        raise HTTPException(400, str(e))


@app.get("/api/decouvrir/familles")
def decouvrir_familles(type: str):
    try:
        return decouvrir.familles(type)
    except decouvrir.ErreurDecouvrir as e:
        raise HTTPException(400, str(e))


class DemandeTelechargementDirect(BaseModel):
    lien: str
    dossier: str


@app.post("/api/decouvrir/telecharger")
async def decouvrir_telecharger(d: DemandeTelechargementDirect):
    """Le clic « Télécharger » de Découvrir vaut confirmation : fiche puis mise en file directe."""
    try:
        fiche = await fiche_depuis_url(d.lien)
        tache = telechargeur.ajouter(fiche, d.dossier)
    except (ErreurSource, ErreurTelechargement, modeles.ErreurModele) as e:
        raise HTTPException(400, str(e))
    return tache.to_dict()


@app.get("/api/sources/{fichier}")
def image_source(fichier: str):
    image = (casting.SOURCES / fichier).resolve()
    if not image.is_relative_to(casting.SOURCES.resolve()) or not image.is_file():
        raise HTTPException(404, "Source introuvable")
    return FileResponse(image)


@app.post("/api/quitter")
async def quitter():
    """Bouton ⏻ : arrête le serveur ; cycle_de_vie coupe alors ComfyUI et Llama (ceux qu'Aria a lancés, jamais un
    autre programme). Un téléchargement en cours garde son .part et reprendra au prochain lancement."""
    if serveur is None:
        raise HTTPException(409, "Aria a été lancée autrement que par son lanceur : ferme la fenêtre de son serveur")
    asyncio.get_running_loop().call_later(0.5, setattr, serveur, "should_exit", True)  # la réponse part d'abord
    return {"quitte": True}


serveur: uvicorn.Server | None = None  # posé par lancer() : /api/quitter lui demande de s'arrêter


def lancer():
    global serveur
    host = CONFIG["app"]["host"]
    port = CONFIG["app"]["port"]
    # Le lanceur Aria-Studio.exe ouvre lui-même le navigateur, une fois le serveur prêt.
    if CONFIG["app"].get("open_browser") and not os.environ.get("ARIA_SANS_NAVIGATEUR"):
        threading.Timer(1.5, webbrowser.open, args=[f"http://{host}:{port}"]).start()
    serveur = uvicorn.Server(uvicorn.Config(app, host=host, port=port))
    serveur.run()


if __name__ == "__main__":
    lancer()
