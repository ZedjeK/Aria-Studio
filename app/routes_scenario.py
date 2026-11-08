"""Routes du Scénario (onglet ✍️) : listes guidées, assemblage, ✨ Écrire le prompt, 💬 M'aider à préciser, traductions,
personnages enregistrés, et l'écriture des plans d'une séquence. Ce qu'elles partagent avec les autres routes
est dans app/services.py."""

import asyncio
import json

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ValidationError

from app import decoupage, listes, personnages, plateau, prompt, securite, sequences, services, workflows
from app.llm import ErreurLlama
from app.securite import ErreurSecurite

routeur = APIRouter()

# Onglet Prompt : listes guidées

@routeur.get("/api/prompt/listes")
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
    replique: str = ""  # vidéo : les répliques, une par ligne, gardées mot pour mot
    dialogue: bool | None = None  # case 🗣 Dialogue : False = personne ne parle
    musique: str = ""  # case 🎵 Musique d'ambiance : sa description, déjà traduite
    avec_musique: bool | None = None  # False = pas de musique de fond
    sans_sujet: bool = False  # Mélanger / Redessiner : le sujet vient des images, pas de sujet par défaut


@routeur.post("/api/prompt/assembler")
def assembler_prompt(demande: DemandeAssemblage):
    try:
        return listes.assembler(demande.sujet, demande.choix, demande.age, demande.texte, demande.precisions,
                                demande.workflow, demande.action, demande.replique, demande.dialogue,
                                demande.musique, demande.avec_musique, demande.sans_sujet)
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (listes.ErreurListes, workflows.ErreurWorkflow) as e:
        raise HTTPException(400, str(e))


class DemandeOptimisation(BaseModel):
    atelier: str  # image, retouche ou clip (la séquence se découpe : /api/sequences/decouper)
    idee: str = ""  # le prompteur libre, en français
    complements: str = ""  # ce que les compléments imposent (prompt assemblé des listes et des cases)
    images: list[str] = []  # images de référence de l'atelier
    role: str | None = None  # inspirer, variation, animer, retouche
    workflow: str | None = None
    llm: str | None = None
    duree: int = 5


@routeur.post("/api/prompt/optimiser")
async def optimiser_prompt(d: DemandeOptimisation, requete: Request = None):
    """✨ Écrire le prompt d'un atelier du Scénario (et ✍️ Écris-le du chat, avec ce qu'il a compris) : le LLM écrit
    le meilleur prompt pour le modèle cible, d'après l'idée, les compléments (déjà fixés) et les images, qu'il voit.
    Retouche : une consigne d'édition sur l'image. Un LLM spécialisé du modèle cible écrit avec sa propre consigne."""
    services.verifier_llm(d.llm)
    idee, complements_ = d.idee.strip(), d.complements.strip()
    if d.atelier not in ("image", "retouche", "clip"):
        raise HTTPException(400, "Atelier inconnu : image, retouche ou clip")
    if not (idee or complements_):
        raise HTTPException(400, "Écris ton idée (ou choisis des compléments)")
    if len(idee) > 2000 or len(complements_) > 4000 or len(d.images) > workflows.REFERENCES_MAX:
        raise HTTPException(400, "Idée, compléments ou images en trop")
    if d.atelier == "retouche" and len(d.images) != 1:
        raise HTTPException(400, "Choisis l'image à retoucher")
    texte = f"{idee}\n\nAlso fixed (keep it): {complements_}" if idee and complements_ else idee or complements_

    async def ecrire():
        origines = plateau.textes_origine(d.images)  # AS-18 : un rush né d'un mineur le reste
        securite.verifier(texte, *origines)
        vues = await asyncio.to_thread(lambda: [plateau.vue(i) for i in d.images]) or None
        llm = d.llm or (workflows.llm_de(d.workflow) if d.workflow else None)
        regles = workflows.regles_prompt(d.workflow)
        if d.atelier == "retouche":
            debut = asyncio.get_running_loop().time()
            consigne = await prompt.traduire_instruction(services.orchestrateur, texte, llm, regles, d.workflow, vues[0])
            r = {"prompt": consigne, "duree_s": round(asyncio.get_running_loop().time() - debut, 2)}
        else:
            video = ("animer" if d.role == "animer" else "texte") if d.atelier == "clip" else None
            r = await prompt.reecrire(services.orchestrateur, texte, llm, len(vues) if vues and d.role == "inspirer" else 0,
                                      video=video, duree=d.duree, regles=regles, workflow=d.workflow,
                                      vues=vues if d.role in ("inspirer", "variation", "animer") else None)
        securite.verifier(texte, r["prompt"], *origines)
        return {**r, "mots": len(r["prompt"].split())}

    try:
        return await services.tant_que_connecte(requete, ecrire())
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (plateau.ErreurPlateau, workflows.ErreurWorkflow) as e:
        raise HTTPException(400, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))


class EchangeLibre(BaseModel):
    role: str  # user ou assistant (le JSON rendu au tour précédent)
    content: str


class DemandeLibre(BaseModel):
    echanges: list[EchangeLibre]
    forme: str = "image"  # image, clip, retouche, sequence (le fil de l'histoire) ou plan (un plan d'une séquence)
    workflow: str | None = None
    contexte: str = ""  # ce que les autres onglets du Scénario ont déjà fixé (le prompt assemblé)
    sequence: dict | None = None  # forme « plan » : la demande du découpage (DemandeDecoupage), « plan » = son numéro
    images: list[str] = []  # images de référence du Scénario
    role: str | None = None  # leur rôle : inspirer, variation, animer
    llm: str | None = None
    duree: int = 5


ECHANGES_LIBRE_MAX, LONGUEUR_ECHANGE_LIBRE = 60, 4000


@routeur.post("/api/prompt/libre")
async def chat_libre(d: DemandeLibre, requete: Request = None):
    """Scénario, 💬 M'aider à préciser : un tour de conversation (questions, fiche « Ce que j'ai compris », prompt).
    ✕ Annuler coupe la connexion : le LLM s'arrête (services.tant_que_connecte)."""
    services.verifier_llm(d.llm)
    if d.forme not in ("image", "clip", "retouche", "sequence", "plan"):
        raise HTTPException(400, "Atelier inconnu : image, retouche, clip, séquence ou plan")
    if d.forme == "plan" and not (d.sequence and isinstance(d.sequence.get("plan"), int)):
        raise HTTPException(400, "Quel plan de la séquence ?")
    if not d.echanges or d.echanges[-1].role != "user" or len(d.echanges) > ECHANGES_LIBRE_MAX \
            or any(e.role not in ("user", "assistant") or len(e.content) > LONGUEUR_ECHANGE_LIBRE for e in d.echanges):
        raise HTTPException(400, "Conversation invalide ou trop longue : recommence-la")
    try:
        contexte, duree, plan_ = d.contexte, d.duree, None
        if d.forme == "plan":  # 💬 d'un plan : le chat sait ce que sait le découpage, ce plan seul à écrire
            s = DemandeDecoupage(**d.sequence)
            desc = listes.description_sequence(s.sujet, s.choix, s.age)
            plan_ = decoupage.preparer_ecriture(s.synopsis, desc["personnage"], desc["decor"], s.plans, s.reference,
                                                desc["mineur"], s.contrainte, s.musique, s.plan)
            contexte = f"{plan_['demande']}\nCharacter rule: {plan_['regle_personnage']}"
            duree = plan_["plans"][s.plan]["duree"]
        origines = plateau.textes_origine(d.images)  # AS-18 : un rush né d'un mineur le reste
        entrees = [*origines, *(plan_["entrees"] if plan_ else [])]
        securite.verifier(*entrees, *(e.content for e in d.echanges if e.role == "user"))
        vues = await asyncio.to_thread(lambda: [plateau.vue(i) for i in d.images[:workflows.REFERENCES_MAX]]) or None
        llm = d.llm or (workflows.llm_de(d.workflow) if d.workflow else None)
        r = await services.tant_que_connecte(requete, prompt.libre(
            services.orchestrateur, [e.model_dump() for e in d.echanges], d.forme, d.workflow, contexte, vues, d.role,
            llm, duree))
        if plan_ and r["prompt"]:  # les garde-fous du découpage : réplique du plan à son moment, rien d'inventé
            texte, alertes = decoupage.finir_plan(s.plan + 1, plan_["plans"][s.plan], r["prompt"],
                                                  plan_["personnage"], plan_["source"])
            r = {**r, "prompt": texte, "alertes": alertes,
                 "brut": json.dumps({**json.loads(r["brut"]), "prompt": texte}, ensure_ascii=False)}
        securite.verifier(*entrees, r["prompt"] or "", *r["fiche"].values())
        return r
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except ValidationError as e:
        raise HTTPException(400, f"Séquence illisible : {e.errors()[0].get('msg', '')}")
    except (plateau.ErreurPlateau, workflows.ErreurWorkflow, sequences.ErreurSequence, listes.ErreurListes) as e:
        raise HTTPException(400, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))


class DemandeTraduction(BaseModel):
    texte: str
    llm: str | None = None


@routeur.post("/api/prompt/traduire")
async def traduire_precisions(demande: DemandeTraduction):
    services.verifier_llm(demande.llm)
    if not demande.texte.strip() or len(demande.texte) > listes.LONGUEUR_PRECISIONS:
        raise HTTPException(400, f"Précisions vides ou trop longues (plus de {listes.LONGUEUR_PRECISIONS} caractères)")
    try:
        return {"texte": await prompt.traduire_precisions(services.orchestrateur, demande.texte.strip(), demande.llm)}
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))


@routeur.get("/api/prompt/personnages")
def liste_personnages():
    return personnages.liste()


class DemandePersonnage(BaseModel):
    choix: dict[str, str | None] = {}  # {sous-liste: valeur} du bloc « personnage »
    age: int | None = None


@routeur.put("/api/prompt/personnages/{nom}")
def enregistrer_personnage(nom: str, demande: DemandePersonnage):
    try:
        return personnages.enregistrer(nom, demande.choix, demande.age)
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except listes.ErreurListes as e:
        raise HTTPException(400, str(e))


@routeur.delete("/api/prompt/personnages/{nom}")
def supprimer_personnage(nom: str):
    try:
        personnages.supprimer(nom)
    except KeyError:
        raise HTTPException(404, f"Personnage inconnu : {nom}")
    return {"supprime": nom}


class DemandeDecoupage(BaseModel):
    synopsis: str = ""  # le fil de l'histoire (facultatif), en français, répliques entre guillemets
    sujet: str = "personnage"  # l'état du Scénario : le personnage et le décor en sont tirés
    choix: dict[str, dict[str, str | None]] = {}
    age: int | None = None
    plans: list[dict]  # [{enchainement, duree, note, rush, replique, moment, off, prompt (déjà écrit)}]
    reference: str | None = None  # rush du personnage (galerie du Plateau)
    contrainte: str = ""  # impérative sur tous les plans
    llm: str | None = None
    musique: str | None = None  # case 🎵 Musique d'ambiance : sa description ; None = pas de musique de fond
    plan: int | None = None  # ✨ d'un plan : son numéro (0 = le premier) ; None = tous les plans
    contrainte_en: dict | None = None  # {fr, en} : traduction déjà faite de la contrainte, reprise si elle vaut


@routeur.post("/api/sequences/decouper")
async def decouper_sequence(d: DemandeDecoupage, requete: Request = None):
    """Scénario, atelier Séquence : les prompts de tous les plans écrits ensemble par Llama (✨ Tout écrire), ou
    d'un seul (✨ d'un plan), les autres servant de contexte. ✕ Annuler coupe la connexion : le LLM s'arrête."""
    services.verifier_llm(d.llm)
    try:
        desc = listes.description_sequence(d.sujet, d.choix, d.age)
        return await services.tant_que_connecte(requete, decoupage.decouper(
            services.orchestrateur, d.synopsis, desc["personnage"], desc["decor"], d.plans, d.reference, d.llm,
            desc["mineur"], d.contrainte, musique=d.musique, cible=d.plan, contrainte_en=d.contrainte_en))
    except ErreurSecurite as e:
        raise HTTPException(422, str(e))
    except (sequences.ErreurSequence, plateau.ErreurPlateau, listes.ErreurListes) as e:
        raise HTTPException(400, str(e))
    except ErreurLlama as e:
        raise HTTPException(500, str(e))
