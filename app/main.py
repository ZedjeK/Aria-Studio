"""Serveur FastAPI d'Aria - Studio : sert l'interface web et l'API."""

import asyncio
import re
import threading
import uuid
import webbrowser
from contextlib import asynccontextmanager

import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from app import (casting, decouvrir, garde, listes, mesures, modeles, optimiseur, personnages, plateau, prompt,
                 reglages, retouche, rushes, sequences, voix, workflows)
from app.llm import ErreurLlama
from app.securite import ErreurSecurite
from app.config import CONFIG, ROOT, chemin
from app.orchestrateur import Orchestrateur
from app.sources import ErreurSource, Fiche, fiche_depuis_url
from app.telechargement import ErreurTelechargement, Telechargeur

WEB = ROOT / "web"

telechargeur = Telechargeur()
orchestrateur = Orchestrateur()
plateau_ = plateau.Plateau(orchestrateur)
fiches: dict[str, Fiche] = {}  # fiches préparées, en attente de confirmation
banc_actif: casting.Banc | None = None
_taches_fond: set[asyncio.Task] = set()


@asynccontextmanager
async def cycle_de_vie(_app: FastAPI):
    telechargeur.demarrer()
    plateau_.demarrer()
    yield
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


app = FastAPI(title="Aria - Studio", lifespan=cycle_de_vie)
app.mount("/static", InterfaceSansCache(directory=WEB), name="static")


@app.exception_handler(garde.ErreurGarde)
async def attente_annulee(_requete, e: garde.ErreurGarde):
    """Jérémy a annulé une demande qui attendait la carte (réécriture, peaufinage…)."""
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


@app.get("/api/etat")
def etat_direct():
    """Pastilles de la barre d'onglets : chaque moteur est chargé, en veille ou introuvable, plus la VRAM."""
    sante_ = sante()

    def etat(processus, installe: bool) -> str:
        if processus is not None and processus.poll() is None:
            return "actif"
        return "veille" if installe else "absent"

    llama = orchestrateur.llama
    favoris = {m["fichier"]: m["nom"] for m in CONFIG["llama"].get("modeles_au_choix", [])}
    try:
        vram = {"vram_utilisee_go": round(mesures.vram_utilisee() / 2**30, 1),
                "vram_totale_go": round(mesures.vram_totale() / 2**30, 1)}
    except Exception:  # pas de GPU NVIDIA lisible : on affiche seulement les moteurs
        vram = {"vram_utilisee_go": None, "vram_totale_go": None}
    return {
        "comfyui": {"etat": etat(orchestrateur.comfy.process, sante_["comfyui"]["installe"])},
        "llama": {"etat": etat(llama.process, sante_["llama"]["installe"]),
                  "modele": favoris.get(llama.modele.name, llama.modele.stem) if llama.modele else None},
        **vram,
        # Garde-VRAM : une demande attend la carte (bandeau), ou une autre appli la tient (pastille orange).
        "garde": {"attente": orchestrateur.garde.attente,
                  "alerte": None if orchestrateur.garde.attente else garde.alerte(orchestrateur.actif)},
    }


@app.post("/api/garde/forcer")
def forcer_garde():
    """« Lancer quand même » : la demande qui attend la carte part, lentement s'il le faut."""
    return {"ok": orchestrateur.garde.forcer()}


@app.post("/api/garde/annuler")
def annuler_garde():
    return {"ok": orchestrateur.garde.annuler()}


@app.get("/api/sante")
def sante():
    comfy = chemin(CONFIG["comfyui"]["dossier"])
    llama = chemin(CONFIG["llama"]["server"])
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


@app.delete("/api/modeles/{cle:path}")
def supprimer_modele(cle: str):
    try:
        modeles.supprimer(cle)
    except modeles.ErreurModele as e:
        raise HTTPException(400, str(e))
    return {"supprime": cle}


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


@app.get("/api/workflows")
def liste_workflows():
    return [wf.resume() for wf in workflows.catalogue()]


@app.get("/api/workflows/erreurs")
def erreurs_workflows():
    """Fiches écartées du catalogue (ajoutées à la main, format incomplet) et pourquoi."""
    return workflows.erreurs()


class DemandeBanc(BaseModel):
    tache: str
    workflows: list[str] | None = None


@app.get("/api/bancs")
def liste_bancs():
    return {"actif": banc_actif.etat_public() if banc_actif else None, "bancs": casting.liste()}


@app.post("/api/bancs")
async def lancer_banc(demande: DemandeBanc):
    global banc_actif
    if banc_actif and banc_actif.etat in ("attente", "en_cours"):
        raise HTTPException(409, "Un banc est déjà en cours")
    try:
        banc_actif = casting.creer(demande.tache, demande.workflows)
    except (casting.ErreurBanc, KeyError) as e:
        raise HTTPException(400, str(e))
    tache = asyncio.create_task(banc_actif.executer(orchestrateur, telechargeur))
    _taches_fond.add(tache)
    tache.add_done_callback(_taches_fond.discard)
    return banc_actif.etat_public()


def _en_cours(id_banc: str) -> bool:
    return bool(banc_actif and banc_actif.id == id_banc and banc_actif.etat in ("attente", "en_cours"))


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


class DemandeReecriture(BaseModel):
    idee: str
    modele: str | None = None


@app.get("/api/llm/modeles")
def modeles_llm():
    """Tous les GGUF de modeles/llm (favoris d'abord), et celui présélectionné."""
    return {"defaut": CONFIG["llama"]["modele"], "choix": [{**m, "present": True} for m in plateau.llm_au_choix()]}


@app.post("/api/prompt/reecrire")
async def reecrire_prompt(demande: DemandeReecriture):
    if banc_actif and banc_actif.etat in ("attente", "en_cours"):
        raise HTTPException(409, "Un banc est en cours")
    if demande.modele and demande.modele not in modeles.llm_disponibles():
        raise HTTPException(400, f"GGUF absent de modeles/llm : {demande.modele}")
    try:
        return await prompt.reecrire(orchestrateur, demande.idee, demande.modele)
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))


# Onglet Prompt : listes guidées

@app.get("/api/prompt/listes")
def listes_prompt(sujet: str | None = None):
    """Sans sujet : les sujets proposés. Avec : blocs, accordéons, valeurs et incompatibilités."""
    try:
        return listes.structure(sujet) if sujet else {"sujets": listes.sujets()}
    except listes.ErreurListes as e:
        raise HTTPException(400, str(e))


class DemandeAssemblage(BaseModel):
    sujet: str
    choix: dict[str, dict[str, str | None]] = {}  # {bloc: {sous-liste: valeur}}
    age: int | None = None  # âge précis, en plus de la tranche
    texte: str = ""  # texte à écrire dans l'image
    precisions: str = ""  # texte libre ajouté tel quel (traduit par Llama s'il est rempli)
    workflow: str | None = None  # modèle cible, pour son profil de prompt
    action: str = ""  # vidéo : l'action, déjà traduite en anglais
    replique: str = ""  # vidéo : la réplique, gardée mot pour mot


@app.post("/api/prompt/assembler")
def assembler_prompt(demande: DemandeAssemblage):
    try:
        return listes.assembler(demande.sujet, demande.choix, demande.age, demande.texte, demande.precisions,
                                demande.workflow, demande.action, demande.replique)
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (listes.ErreurListes, workflows.ErreurWorkflow) as e:
        raise HTTPException(400, str(e))


def _verifier_llm(modele: str | None):
    if banc_actif and banc_actif.etat in ("attente", "en_cours"):
        raise HTTPException(409, "Un banc est en cours")
    if modele and modele not in modeles.llm_disponibles():
        raise HTTPException(400, f"GGUF absent de modeles/llm : {modele}")


class DemandePeaufinage(BaseModel):
    prompt: str
    llm: str | None = None
    workflow: str | None = None  # pour la longueur conseillée du modèle cible
    duree: int = 5  # vidéo : durée du clip, en secondes


@app.post("/api/prompt/peaufiner")
async def peaufiner_prompt(demande: DemandePeaufinage):
    _verifier_llm(demande.llm)
    try:
        mots_max = reglages.valeurs_workflow(workflows.obtenir(demande.workflow)).get("mots_max") \
            if demande.workflow else None
        duree = demande.duree if listes.est_video(demande.workflow) else None
        return await prompt.peaufiner(orchestrateur, demande.prompt, demande.llm, mots_max, duree,
                                      workflows.regles_prompt(demande.workflow))
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except workflows.ErreurWorkflow as e:
        raise HTTPException(400, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))


class DemandeTraduction(BaseModel):
    texte: str
    llm: str | None = None


@app.post("/api/prompt/traduire")
async def traduire_precisions(demande: DemandeTraduction):
    _verifier_llm(demande.llm)
    if not demande.texte.strip() or len(demande.texte) > listes.LONGUEUR_PRECISIONS:
        raise HTTPException(400, f"Précisions vides ou trop longues (plus de {listes.LONGUEUR_PRECISIONS} caractères)")
    try:
        return {"texte": await prompt.traduire_precisions(orchestrateur, demande.texte.strip(), demande.llm)}
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))


@app.get("/api/prompt/personnages")
def liste_personnages():
    return personnages.liste()


class DemandePersonnage(BaseModel):
    choix: dict[str, str | None] = {}  # {sous-liste: valeur} du bloc « personnage »
    age: int | None = None


@app.put("/api/prompt/personnages/{nom}")
def enregistrer_personnage(nom: str, demande: DemandePersonnage):
    try:
        return personnages.enregistrer(nom, demande.choix, demande.age)
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except listes.ErreurListes as e:
        raise HTTPException(400, str(e))


@app.delete("/api/prompt/personnages/{nom}")
def supprimer_personnage(nom: str):
    try:
        personnages.supprimer(nom)
    except KeyError:
        raise HTTPException(404, f"Personnage inconnu : {nom}")
    return {"supprime": nom}


# Onglet Réglages : LLM et workflows (data/reglages.yaml)

@app.get("/api/reglages")
def lire_reglages():
    return reglages.etat()


class DemandeReglagesLlm(BaseModel):
    valeurs: dict = {}  # {réglage: valeur} ; None = valeur d'origine
    consignes: dict[str, str | None] = {}  # {reecriture|retouche|peaufinage|precisions: texte} ; None = d'origine


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
    """Propositions seulement : rien n'est appliqué sans l'accord de Jérémy (routes PUT ci-dessus et dessous)."""
    return optimiseur.propositions()


@app.put("/api/reglages/workflows/{id_workflow}")
def regler_workflow(id_workflow: str, demande: DemandeReglagesWorkflow):
    try:
        return reglages.modifier_workflow(id_workflow, demande.valeurs)
    except (reglages.ErreurReglages, workflows.ErreurWorkflow) as e:
        raise HTTPException(400, str(e))


# Plateau : créations et rushes

class DemandeCreation(BaseModel):
    idee: str = ""
    prompt: str | None = None  # fourni : pas de réécriture (« Régénérer », « Variante »)
    llm: str | None = None
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


@app.get("/api/plateau/options")
def options_plateau():
    return plateau_.options()


@app.post("/api/creations")
def creer(demande: DemandeCreation):
    if banc_actif and banc_actif.etat in ("attente", "en_cours"):
        raise HTTPException(409, "Un banc est en cours")
    try:
        commande = plateau_.commander(demande.idee, demande.prompt, demande.llm, demande.workflow,
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


class DemandeImport(BaseModel):
    image: str  # data URL
    nom: str = ""
    dossier: str | None = None  # dossier des rushes ouvert au moment de l'import


def _commander(action):
    if banc_actif and banc_actif.etat in ("attente", "en_cours"):
        raise HTTPException(409, "Un banc est en cours")
    try:
        return action().to_dict()
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (plateau.ErreurPlateau, retouche.ErreurRetouche, workflows.ErreurWorkflow, rushes.ErreurRushes,
            sequences.ErreurSequence, voix.ErreurVoix) as e:
        raise HTTPException(400, str(e))


@app.post("/api/retouches")
def retoucher(d: DemandeRetouche):
    def action():
        masque = retouche.depuis_data_url(d.masque) if d.masque else None
        return plateau_.commander_retouche(d.source, d.instruction, d.reecrire, d.llm, d.workflow, masque,
                                           d.fondu, d.seed, d.modele, d.loras)
    return _commander(action)


@app.post("/api/agrandissements")
def agrandir(d: DemandeAgrandissement):
    return _commander(lambda: plateau_.commander_agrandissement(
        d.source, d.largeur, d.hauteur, d.ancrage, d.precision, d.reecrire, d.llm, d.workflow, d.fondu, d.seed,
        d.modele, d.loras))


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


class DemandeClip(BaseModel):
    idee: str = ""
    prompt: str | None = None  # fourni : pas de réécriture (« Retourner », « Variante »)
    llm: str | None = None
    mode: str = "animer"  # animer (un rush) ou texte
    source: str | None = None  # rush à animer
    qualite: str = "compromis"  # rapide, compromis, qualite
    duree: int = 5
    format: str = "16:9 (Widescreen)"  # texte → vidéo seulement
    seed: int | None = None
    nombre: int = 1
    dossier: str | None = None


@app.post("/api/clips")
def tourner_clip(d: DemandeClip):
    return _commander(lambda: plateau_.commander_clip(
        d.idee, d.prompt, d.llm, d.mode, d.source, d.qualite, d.duree, d.format, d.seed, d.nombre,
        rushes.valider_dossier(d.dossier)))


# Montage, mode Séquence

class DemandeSequence(BaseModel):
    titre: str
    qualite: str = "compromis"
    format: str = "16:9 (Widescreen)"  # si le premier plan ne part pas d'un rush
    # {personnage: rush, voix: "auto" | clip | "voix:<id>" | None, ambiance: texte, contrainte: texte,
    #  reference_partout: bool}
    casting: dict = {}
    plans: list[dict] = []  # [{id, idee, prompt, duree, enchainement, rush, seed, ambiance}]
    llm: str | None = None


class DemandeTournageSequence(BaseModel):
    plans: list[int] | None = None  # None : les plans pas encore à jour


def _sequence(action):
    try:
        return action()
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (sequences.ErreurSequence, plateau.ErreurPlateau, rushes.ErreurRushes) as e:
        raise HTTPException(404 if "inconnue" in str(e) else 400, str(e))


class DemandeDecoupage(BaseModel):
    synopsis: str  # l'histoire, en français, répliques entre guillemets
    sujet: str = "personnage"  # l'état du Scénario : le personnage et le décor en sont tirés
    choix: dict[str, dict[str, str | None]] = {}
    age: int | None = None
    plans: list[dict]  # [{enchainement, duree, note, rush}]
    reference: str | None = None  # rush du personnage (galerie du Plateau)
    contrainte: str = ""  # impérative sur tous les plans
    llm: str | None = None


@app.post("/api/sequences/decouper")
async def decouper_sequence(d: DemandeDecoupage):
    """Scénario, mode Séquence : les prompts de tous les plans, écrits ensemble par Llama."""
    _verifier_llm(d.llm)
    try:
        desc = listes.description_sequence(d.sujet, d.choix, d.age)
        return await sequences.decouper(orchestrateur, d.synopsis, desc["personnage"], desc["decor"], d.plans,
                                        d.reference, d.llm, desc["mineur"], d.contrainte)
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (sequences.ErreurSequence, plateau.ErreurPlateau, listes.ErreurListes) as e:
        raise HTTPException(400, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))


@app.get("/api/sequences")
def lister_sequences():
    return sequences.liste()


@app.post("/api/sequences")
def creer_sequence(d: DemandeSequence):
    return _sequence(lambda: sequences.creer(d.model_dump()))


@app.get("/api/sequences/{id_sequence}")
def lire_sequence(id_sequence: str):
    return _sequence(lambda: sequences.public(sequences.lire(id_sequence)))


@app.put("/api/sequences/{id_sequence}")
def modifier_sequence(id_sequence: str, d: DemandeSequence):
    return _sequence(lambda: sequences.modifier(id_sequence, d.model_dump()))


@app.delete("/api/sequences/{id_sequence}")
def supprimer_sequence(id_sequence: str):
    _sequence(lambda: sequences.supprimer(id_sequence))
    return {"ok": True}


@app.post("/api/sequences/{id_sequence}/tourner")
def tourner_sequence(id_sequence: str, d: DemandeTournageSequence):
    return _commander(lambda: plateau_.commander_sequence(id_sequence, d.plans))


# Voix de référence des séquences (bibliothèque data/voix)

class DemandeEssaiVoix(BaseModel):
    phrase: str = ""  # vide : la phrase par défaut
    description: str = ""  # qui parle et comment (obligatoire sans personnage)
    personnage: str | None = None  # rush du personnage (le casting de la séquence)
    seed: int | None = None


class DemandeImportVoix(BaseModel):
    audio: str  # data URL
    nom: str
    nom_origine: str = ""


class DemandeVoix(BaseModel):
    nom: str


def _voix(action):
    try:
        return action()
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (voix.ErreurVoix, plateau.ErreurPlateau, sequences.ErreurSequence) as e:
        raise HTTPException(404 if "inconnue" in str(e) else 400, str(e))


@app.get("/api/voix")
def lister_voix():
    return voix.liste()


@app.post("/api/voix/essais")
def essayer_voix(d: DemandeEssaiVoix):
    """Un essai de voix dans la file du Plateau ; la commande terminée donne le brouillon (champ voix)."""
    return _commander(lambda: plateau_.commander_voix(d.phrase, d.description, d.personnage, d.seed))


@app.post("/api/voix/import")
def importer_voix(d: DemandeImportVoix):
    def action():
        try:
            son = retouche.depuis_data_url(d.audio)
        except retouche.ErreurRetouche:
            raise voix.ErreurVoix("Données audio invalides")
        return voix.importer(son, d.nom, d.nom_origine)
    return _voix(action)


@app.get("/api/voix/{id_voix}")
def lire_voix(id_voix: str):
    return _voix(lambda: voix.lire(id_voix))


@app.put("/api/voix/{id_voix}")
def garder_voix(id_voix: str, d: DemandeVoix):
    """Garde un essai sous ce nom, ou renomme une voix."""
    return _voix(lambda: voix.garder(id_voix, d.nom))


@app.delete("/api/voix/{id_voix}")
def supprimer_voix(id_voix: str):
    """À la corbeille ; renvoie les séquences repassées en Auto."""
    return {"sequences": _voix(lambda: voix.supprimer(id_voix))}


@app.get("/api/voix/{id_voix}/son")
def son_voix(id_voix: str):
    return _voix(lambda: FileResponse(voix.chemin_son(id_voix), media_type=voix.type_mime(id_voix)))


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
def creer_dossier(demande: DemandeDossier):
    try:
        return {"nom": rushes.creer_dossier(demande.nom)}
    except rushes.ErreurRushes as e:
        raise HTTPException(400, str(e))


@app.put("/api/rushes/dossiers/{nom}")
def renommer_dossier(nom: str, demande: DemandeDossier):
    try:
        return {"nom": rushes.renommer_dossier(nom, demande.nom)}
    except rushes.ErreurRushes as e:
        raise HTTPException(400, str(e))


@app.delete("/api/rushes/dossiers/{nom}")
def supprimer_dossier(nom: str):
    try:
        return {"liberes": rushes.supprimer_dossier(nom)}
    except rushes.ErreurRushes as e:
        raise HTTPException(404, str(e))


# Découvrir (onglet Modèles) : Civitai / Hugging Face

@app.get("/api/decouvrir")
async def decouvrir_modeles(source: str, type: str, texte: str = "", famille: str | None = None,
                   tri: str = "telechargements", curseur: str | None = None):
    try:
        if source == "civitai":
            return await decouvrir.chercher_civitai(texte, type, famille, tri, curseur)
        if source == "huggingface":
            return await decouvrir.chercher_hf(texte, type, tri)
    except decouvrir.ErreurDecouvrir as e:
        raise HTTPException(400, str(e))
    raise HTTPException(400, f"Source inconnue : {source}")


@app.get("/api/decouvrir/hf/fichiers")
async def decouvrir_fichiers_hf(depot: str, type: str = "llm"):
    try:
        return await decouvrir.fichiers_hf(depot, type)
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


def lancer():
    host = CONFIG["app"]["host"]
    port = CONFIG["app"]["port"]
    if CONFIG["app"].get("open_browser"):
        threading.Timer(1.5, webbrowser.open, args=[f"http://{host}:{port}"]).start()
    uvicorn.run(app, host=host, port=port)


if __name__ == "__main__":
    lancer()
