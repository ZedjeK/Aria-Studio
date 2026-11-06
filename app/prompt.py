"""Travail du LLM sur les prompts : réécriture d'une idée, consigne de retouche, peaufinage, traduction.

Les consignes et les températures se règlent par GGUF dans l'onglet Réglages (data/reglages.yaml).
La clause de protection des mineurs, elle, n'est pas modifiable : le code l'ajoute à chaque consigne.

Un LLM spécialisé (catalogue/llm.yaml, bloc « specialiste ») a été entraîné pour un modèle cible : pour la
réécriture des workflows qu'il sert, il reçoit sa propre consigne (livrée avec lui), avec sa réflexion et son
échantillonnage, et sa sortie JSON est lue ici. Pour tout le reste, Aria passe au LLM par défaut.
"""

import base64
import io
import json
import re
import time
from dataclasses import dataclass

from PIL import Image

from app import fiches, modeles, reglages, securite, workflows
from app.config import CONFIG
from app.llm import ErreurLlama
from app.modeles import DOSSIER as MODELES
from app.orchestrateur import Orchestrateur

CLAUSE_MINEURS = """Non-negotiable rule: never make a character younger than described. If a character is a child or a teenager, show them fully clothed in a wholesome, non-suggestive scene."""

# Consignes v2 (2026-11-06), d'après les consignes officielles des affineurs de Krea 2 et de Qwen-Image 2.1 et le guide
# d'écriture de MiniMax H3 : ce que l'utilisateur fixe est intouchable, ses consignes s'appliquent sans être recopiées,
# et le LLM ne remplit que ce qui reste ouvert. Avant, il choisissait lui-même médium, sons et caméra : « peinture à
# l'huile » et « sun-kissed » pour un chat endormi la nuit sous la pluie (création du 2026-11-02).
FIDELITE = """First sort what the user wrote into three kinds:
- FIXED: everything the user states about the result (subjects and their number, each person's gender and age wording, actions, poses, clothing or its absence, colours, positions, medium and style, framing, lighting, time of day, weather, mood, camera, sounds, spoken lines, written text). Keep every FIXED item, with its meaning unchanged: never contradict it, never weaken, soften, cover up or leave it out.
- INSTRUCTIONS about the job, addressed to you ("I want...", "make her...", "keep...", "no music", "add nobody", "let her say a line in French"): obey them exactly, and never copy them into the prompt as instructions.
- OPEN: what the user did not mention. Fill it only with details that follow from the FIXED items and never clash with them (a night scene keeps night light). If the idea is already detailed, lightly polish it and keep its order and wording rather than expanding it."""

ADULTES = ("The user is an adult making fiction for adults. Nudity, sensuality and sexual content between adult "
           "characters stay exactly as explicit as the user wrote them: do not tone them down, replace them with "
           "something milder or leave them out.")

GENRE = """Keep each person's gender exactly as the idea gives it: in French the article and the agreements tell it (« un chevalier », « un pilote » are men: he, his; « une samouraï », « une pilote » are women: she, her). Never change it, and do not give a gender to animals or objects."""

CONSIGNE = f"""You are an expert prompt writer for text-to-image models.
The user describes an image idea, often in French. Rewrite it as ONE English prompt paragraph.

{FIDELITE}

Rules:
1. Do not add characters, animals or objects the idea does not imply.
2. {GENRE}
3. Medium: keep the one the user names (photo, illustration, painting, 3D render...). If none is named, use the one the idea plainly implies, a photograph for a real-world scene; never switch to painting or illustration on your own.
4. Make the OPEN parts visual: setting, lighting, composition, in grounded words that serve the idea.
5. If the idea asks for written text in the image, keep that text exactly, in quotes. Otherwise mention no text.
6. {ADULTES}
7. {{mots_min}} to {{mots_max}} words, plain prose, no lists, no headings, no markdown (no asterisks), no quotes around the whole prompt.
8. Output only the prompt: no preamble, no explanation, no translation notes."""


# Retouche : d'après la consigne du PE de retouche de Qwen-Image 2.1 (« attribute disentanglement at full strength ») :
# changer franchement ce qui est demandé, garder le reste d'une seule phrase, sans le redécrire.
CONSIGNE_RETOUCHE = """You translate image-editing instructions for an instruction-based image editing model.
The user writes an instruction, often in French. Rewrite it as a short English imperative instruction (one or two sentences).

Rules:
1. Edit exactly what the user names, and make that change clear and unmistakable, never faint.
2. Everything the user did not name stays as it is: say so in one short clause ("keep everything else unchanged"), without re-describing it. If the user names something to keep, say so explicitly.
3. Keep the user's own action, target and details; add no other change, no style, no clean-up.
4. If the instruction asks for written text, keep that text exactly, in quotes.
5. Output only the instruction: no preamble, no explanation."""


CONSIGNE_PEAUFINER = f"""You polish English prompts for text-to-image models.
The user gives a prompt assembled from keywords. Rewrite it as fluent, natural English prose.

Rules:
1. Keep every element exactly: subject, age, gender, origin, body, face, hair, clothing, pose, action, setting, time, weather, style, lighting, framing, palette. Do not remove, soften, replace or censor any of them.
2. Add nothing new: no extra characters, objects, details, style or mood.
3. Keep the age wording exactly as given. Never make a character younger.
4. If the prompt contains text in quotes, keep that text exactly, in quotes.
5. {ADULTES}
6. At most {{mots_max}} words, plain prose, no lists, no headings.
7. Output only the prompt: no preamble, no explanation."""


CONSIGNE_PRECISIONS = """You translate short details for a text-to-image prompt.
The user writes a detail, often in French. Translate it into a short English phrase that can be appended to an image prompt.

Rules:
1. Keep exactly the meaning: same elements, same details. Add nothing, remove nothing.
2. If the detail is already in English, return it unchanged.
3. If it contains text in quotes, keep that text exactly, in quotes.
4. Output only the translation: no preamble, no explanation."""

CONSIGNE_REFERENCES = f"""You write instructions for an image model that receives reference pictures, numbered Picture 1, Picture 2, Picture 3.
The user describes, often in French, a new image to create from those references (« image 1 » means Picture 1). Rewrite it as ONE English instruction.

{FIDELITE}

Rules:
1. Refer to the references by their number: "the woman from Picture 1", "the dragon from Picture 2". Do not re-describe what the references already show (face, hair, clothes, colors): the model sees them.
2. Ask explicitly to keep the identity (face, hair) and outfit of the characters from the references, unless the user asks to change them, but NOT their pose, seat, props or background: the characters take the new pose and place described by the user.
3. Describe what is new, as the user asks: scene, pose, action, setting, lighting, framing, style.
4. {GENRE}
5. If the user asks for written text in the image, keep that text exactly, in quotes.
6. {ADULTES}
7. 1 to 4 sentences, at most 90 words, plain prose, no markdown.
8. Output only the instruction: no preamble, no explanation."""

# Vidéo : d'après le guide d'écriture de MiniMax H3 : la chronologie, la caméra de l'utilisateur ou une seule simple,
# les sons de ce qu'on voit toujours, la musique seulement si elle est demandée, les répliques mot pour mot.
CONSIGNE_VIDEO = f"""You write prompts for an audio-video generation model: it creates the picture AND the sound together.
The user describes, often in French, a short video clip. Rewrite it as ONE English prompt.

{FIDELITE}

Rules:
1. A short production brief in the present tense, following the timeline: opening state, the action as it starts, how it develops, the result.
2. Camera: use the user's camera choice if there is one (a fixed shot stays fixed). Otherwise one simple named movement with its speed, or a static shot, whichever serves the action. One continuous take: no cuts, no fade to black, unless the user asks for them.
3. Dialogue: keep every spoken line EXACTLY as the user wrote it, in its original language, inside double quotes. Say who speaks, in which language and with which tone. Never translate the lines. If the user asks you to invent a line, write one short line in the language they ask for, inside double quotes.
4. Sounds: end with the concrete sounds of what is seen (footsteps, rain, fabric, breath...). No background music unless the user asks for it.
5. Fit the clip duration: few actions, at most one or two short lines of dialogue.
6. {GENRE}
7. {ADULTES}
8. {{mots_min}} to {{mots_max}} words, plain prose, no lists, no headings, no markdown.
9. Output only the prompt: no preamble, no explanation."""


# Scénario, mode Séquence : tout le découpage en un seul appel, pour que Llama voie l'histoire entière.
# Règles tirées des essais du mode Séquence (2026-10-21) : une seule prise par plan, transition progressive,
# répliques jamais inventées, identité tenue par l'image de référence ou par une description répétée à l'identique.
CONSIGNE_DECOUPAGE = """You are the storyboard writer for an audio-video generation model: it creates the picture AND the sound together.
The user gives, often in French, a short story, the look of the character and of the setting, and the list of shots with their type and duration. Write ONE English prompt per shot.

Rules for every shot:
1. A short production brief in the present tense: shot size, what the character does, the setting, ONE named camera movement with its speed, then concrete sounds. 40 to {mots_max} words, plain prose.
2. One continuous take: never write "cut to", never switch place or angle inside a shot (the next shot does that).
3. Dialogue: use only the lines in quotes from the story or the shot notes, word for word, inside double quotes; say who speaks, in which language and with which tone. A line is spoken in the language it is written in (a French line is spoken in French), whatever the character's origin. Never translate a line, never invent one: a shot without a line has nobody speaking, nobody murmurs or says anything.
4. Tell the story in order, spread over the shots; follow each shot's note when there is one. Few actions per shot: fit its duration.
5. Shot types:
- start: the first shot; it sets up the character and the place.
- start from the picture: it animates the reference picture; describe only what moves (motion, camera, sounds).
- continue: the same take goes on from the last image of the previous shot: same place, same light, same framing at first. Begin with "Continuous shot, same scene."
- cut: a new angle or a new moment of the story.
- transition: ONE continuous camera movement travelling from the last image of the previous shot to the given arrival; the scene changes progressively over the whole clip, never all at once at the end.
6. Character: {regle_personnage} Keep each person's gender exactly as the idea gives it: in French the article and the agreements tell it (« un chevalier », « un pilote » are men: he, his; « une samouraï », « une pilote » are women: she, her). Never change it, and do not give a gender to animals or objects.
7. Keep the look of the setting (place, light, style) unless a shot's note says otherwise.
8. Keep everything the user wrote (story events, character, setting, constraint, notes): never contradict, weaken or leave it out. No background music unless the user asks for it.
9. """ + ADULTES + """

Answer with exactly one block per shot, in order, and nothing else:
SHOT 1
FR: one short French sentence summarising the shot
EN: the English prompt"""

REGLE_AVEC_REFERENCE = ('a reference picture gives the character\'s face, age, body and outfit: never describe them, '
                        'call her or him simply "the character".')
REGLE_SANS_REFERENCE = ("no picture shows the character: in every start and cut shot, copy the character description "
                        "given by the user word for word; in continue and transition shots, call her or him "
                        "\"the character\".")

# Les règles du modèle cible (tirées de ses guides officiels) ne servent qu'à ce que l'utilisateur a laissé ouvert : celles
# de Qwen-Image (couleurs nuancées, matières, lumière) poussaient le LLM à repeindre une idée déjà précise.
ENTETE_REGLES = ("Style rules of the target model. They shape the OPEN parts only: whenever one clashes with what "
                 "the user fixed or asked, the user wins.")
MOTS_MIN = 40
MOTS_MAX_DEFAUT = {"reecriture": 120, "video": 110}  # sans modèle cible (avant : 40 à 120 mots même pour Qwen à 200)

CONSIGNES_DEFAUT = {"reecriture": CONSIGNE, "retouche": CONSIGNE_RETOUCHE, "peaufinage": CONSIGNE_PEAUFINER,
                    "precisions": CONSIGNE_PRECISIONS, "references": CONSIGNE_REFERENCES,
                    "video": CONSIGNE_VIDEO, "decoupage": CONSIGNE_DECOUPAGE}


# Images montrées au LLM (Montage « animer », plan de séquence parti d'un rush, retouche, références du Plateau) :
# il les voit s'il a un projecteur de vision (reglages.projecteur_llm), sinon il lit ce qu'en dit leur fiche. Sans
# l'un ni l'autre, il réécrivait à l'aveugle : « the man » pour un chat roux (essai du 2026-11-02).

PIXELS_VUE = 1024 * 1024  # au-delà, l'image est réduite : ≈ 600 tokens de vision (Bonsai 2, mesuré)


@dataclass
class Vue:
    png: bytes
    texte: str | None = None  # ce que montre l'image d'après sa fiche (None : image importée)

    @classmethod
    def depuis(cls, image: bytes, texte: str | None = None) -> "Vue":
        with Image.open(io.BytesIO(image)) as im:
            if im.width * im.height > PIXELS_VUE:
                f = (PIXELS_VUE / (im.width * im.height)) ** 0.5
                sortie = io.BytesIO()
                im.convert("RGB").resize((round(im.width * f), round(im.height * f)),
                                         Image.Resampling.LANCZOS).save(sortie, "PNG")
                image = sortie.getvalue()
        return cls(image, texte)


NOTES_VUES = {
    # Guide d'écriture de MiniMax H3 : la première image est d'abord ancrée (style, sujets, décor), puis l'action.
    "animer": "The picture is the first frame of the clip. Open with one short sentence anchoring it as it really is "
              "(visual style, subject, setting), then describe what happens from there. Keep the subjects' gender, "
              "looks and clothing, the setting and the visual style consistent (a painting stays a painting).",
    "variation": "The picture is the starting image, partly redrawn: keep its subjects (their gender, looks, "
                 "clothing), setting and visual style unless the idea changes them.",
    "retouche": "The picture is the image to edit: name what is really in it (subjects and their gender, objects, "
                "setting) so that the instruction fits it.",
    "references": "The pictures are the reference pictures, in order (Picture 1, Picture 2…): take from each what "
                  "the idea asks for, as it really looks.",
}


def _image_url(png: bytes) -> dict:
    return {"type": "image_url", "image_url": {"url": "data:image/png;base64," + base64.b64encode(png).decode()}}


def montrer(texte: str, vues: list[Vue] | None, usage: str, voit: bool) -> str | list[dict]:
    """Le message de l'utilisateur, avec les images si le LLM les voit, sinon ce qu'en disent leurs fiches."""
    if not vues:
        return texte
    note = NOTES_VUES[usage]
    if voit:
        return [*map(_image_url, (v.png for v in vues)), {"type": "text", "text": f"{texte}\n\n({note})"}]
    connues = [(i, v.texte) for i, v in enumerate(vues, 1) if v.texte]
    if not connues:
        return texte  # images importées, sans fiche : à l'aveugle
    if len(vues) == 1:
        return f"{texte}\n\n(You cannot see the picture; it was created from this prompt: {connues[0][1]}\n{note})"
    lignes = "\n".join(f"Picture {i}: {t}" for i, t in connues)
    return f"{texte}\n\n(You cannot see the pictures; they were created from these prompts:\n{lignes}\n{note})"


# LLM spécialisés

def generaliste(modele: str | None) -> str | None:
    """Un LLM spécialisé ne fait que sa réécriture : pour une autre tâche, c'est le LLM par défaut."""
    return modeles.llm_defaut() if fiches.specialiste(modele) else modele


def specialiste_pour(modele: str | None, usage: str, workflow: str | None) -> dict | None:
    """Le bloc « specialiste » de ce GGUF s'il sert cet usage (creation, retouche) pour ce workflow."""
    s = fiches.specialiste(modele)
    return s if s and s["usage"] == usage and workflow in s["workflows"] else None


def demande_specialiste(s: dict, texte: str, image: bytes | None = None) -> tuple[list[dict], dict]:
    """Messages et paramètres d'un LLM spécialisé : sa consigne officielle (plus la clause verrouillée), l'image à
    retoucher s'il la voit, et l'échantillonnage de son entraînement."""
    chemin = MODELES / "llm" / s["consigne"]["nom"]
    if not chemin.is_file():
        raise ErreurLlama(f"Consigne du LLM spécialisé absente ({chemin.name}) : réinstalle-le depuis l'onglet Modèles")
    systeme = f"{chemin.read_text(encoding='utf-8').strip()}\n\n{CLAUSE_MINEURS}"
    contenu = texte if image is None else [_image_url(image), {"type": "text", "text": texte}]
    parametres = dict(s.get("echantillonnage") or {})
    parametres.setdefault("temperature", 1.0)
    parametres["max_tokens"] = s["max_tokens"]
    parametres["budget_reflexion"] = 0  # son max_tokens compte déjà sa réflexion
    return [{"role": "system", "content": systeme}, {"role": "user", "content": contenu}], parametres


def lire_specialiste(texte: str, s: dict) -> str:
    """Retire la réflexion (<think>…</think>) puis lit la sortie : le texte, ou la clé du JSON."""
    texte = texte.rpartition("</think>")[2].strip()
    if s["sortie"] == "texte":
        valeur = texte
    else:
        try:
            valeur = json.loads(texte[texte.find("{"):texte.rfind("}") + 1])[s["cle"]]
        except (ValueError, KeyError, TypeError):
            valeur = None
    if not isinstance(valeur, str) or not valeur.strip():
        raise ErreurLlama("Réponse du LLM spécialisé illisible : réflexion trop longue, ou sortie hors format")
    return valeur.strip()


async def _par_specialiste(orchestrateur: Orchestrateur, modele: str, s: dict, texte: str,
                           image: bytes | None = None):
    messages_, parametres = demande_specialiste(s, texte, image)
    async with orchestrateur.llm(modele) as llama:
        debut = time.perf_counter()
        reponse = await llama.generer(messages_, **parametres)
        duree = time.perf_counter() - debut
    return lire_specialiste(reponse.texte, s), reponse, duree, llama


def consigne(nom: str, modele: str | None = None, mots_max: int | None = None,
             regles: list[str] | None = None) -> str:
    """Consigne du GGUF (réglée ou d'origine), puis les règles propres au modèle cible (profil_prompt.regles de sa
    fiche), toujours suivie de la clause verrouillée."""
    texte = reglages.consigne_llm(modele, nom) or CONSIGNES_DEFAUT[nom]
    texte = (texte.replace("{mots_max}", str(mots_max or MOTS_MAX_DEFAUT.get(nom, 150)))
             .replace("{mots_min}", str(MOTS_MIN))).rstrip()
    if regles:
        texte += f"\n\n{ENTETE_REGLES}\n" + "\n".join(f"- {r}" for r in regles)
    return f"{texte}\n\n{CLAUSE_MINEURS}"


def mots_max_de(workflow: str | None) -> int | None:
    """Longueur conseillée par la fiche du modèle cible (réglable dans l'onglet Réglages)."""
    if not workflow:
        return None
    try:
        return reglages.valeurs_workflow(workflows.obtenir(workflow)).get("mots_max")
    except workflows.ErreurWorkflow:
        return None


async def traduire_instruction(orchestrateur: Orchestrateur, instruction: str, modele: str | None = None,
                               regles: list[str] | None = None, workflow: str | None = None,
                               vue: Vue | None = None) -> str:
    """`vue` : l'image à retoucher. Un LLM spécialisé dans la retouche de ce workflow la voit avec sa propre consigne ;
    un autre la voit s'il a sa vision, sinon il lit sa fiche."""
    securite.verifier(instruction)
    if vue is not None and (s := specialiste_pour(modele, "retouche", workflow)):
        texte = (await _par_specialiste(orchestrateur, modele, s, instruction, vue.png))[0]
        securite.verifier(instruction, texte)
        return texte
    modele = generaliste(modele)
    regle = reglages.valeurs_llm(modele)
    async with orchestrateur.llm(modele) as llama:
        contenu = montrer(instruction, [vue] if vue else None, "retouche", voit=_voit(llama))
        reponse = await llama.generer(
            [{"role": "system", "content": consigne("retouche", modele, regles=regles)}, {"role": "user", "content": contenu}],
            temperature=regle["temperature_retouche"], max_tokens=200, reflechir=False,
            **reglages.echantillonnage(modele),
        )
    texte = nettoyer_markdown(reponse.texte)
    securite.verifier(instruction, texte)
    return texte


async def peaufiner(orchestrateur: Orchestrateur, assemble: str, modele: str | None = None,
                    mots_max: int | None = None, duree_video: int | None = None,
                    regles: list[str] | None = None) -> dict:
    """Rend fluide un prompt assemblé par l'onglet Scénario, sans rien retirer ni ajouter.
    `duree_video` : la cible est un modèle vidéo ; la consigne vidéo garde les répliques mot pour mot."""
    securite.verifier(assemble)
    modele = generaliste(modele)
    regle = reglages.valeurs_llm(modele)
    if duree_video:
        echanges = [{"role": "system", "content": consigne("video", modele, mots_max, regles)},
                    {"role": "user", "content": f"{assemble}\n\n(Clip: {duree_video} s, from text only)"}]
    else:
        echanges = [{"role": "system", "content": consigne("peaufinage", modele, mots_max, regles)},
                    {"role": "user", "content": assemble}]
    async with orchestrateur.llm(modele) as llama:
        debut = time.perf_counter()
        reponse = await llama.generer(
            echanges,
            temperature=regle["temperature_peaufinage"], max_tokens=regle["max_tokens"],
            **reglages.echantillonnage(modele))
        duree = time.perf_counter() - debut
    texte = nettoyer_markdown(reponse.texte)
    # Ensemble : un mineur de l'original reste un mineur, même si le LLM l'a reformulé.
    securite.verifier(assemble, texte)
    return {"prompt": texte, "modele": llama.modele.name if llama.modele else modele,
            "duree_s": round(duree, 2), "mots": len(texte.split())}


async def traduire_precisions(orchestrateur: Orchestrateur, precisions: str, modele: str | None = None) -> str:
    securite.verifier(precisions)
    modele = generaliste(modele)
    regle = reglages.valeurs_llm(modele)
    async with orchestrateur.llm(modele) as llama:
        reponse = await llama.generer(
            [{"role": "system", "content": consigne("precisions", modele)}, {"role": "user", "content": precisions}],
            temperature=regle["temperature_traduction"], max_tokens=200, reflechir=False,
            **reglages.echantillonnage(modele),
        )
    securite.verifier(precisions, reponse.texte)
    return reponse.texte


def _voit(llama) -> bool:
    """Le LLM chargé a-t-il sa vision (projecteur d'images) ?"""
    return getattr(llama, "vision", None) is not None


def nettoyer_markdown(texte: str) -> str:
    """Prompt vidéo en prose simple : Llama glisse parfois **gras**, *bruitages* ou titres, que H3 n'a pas à lire."""
    texte = re.sub(r"\*{1,2}([^*]+?)\*{1,2}", r"\1", texte)
    texte = re.sub(r"^\s*#+\s*", "", texte, flags=re.MULTILINE)
    return " ".join(texte.split())


def messages(idee: str, modele: str | None = None, regles: list[str] | None = None,
             mots_max: int | None = None) -> list[dict]:
    return [{"role": "system", "content": consigne("reecriture", modele, mots_max, regles)},
            {"role": "user", "content": idee}]


async def reecrire(orchestrateur: Orchestrateur, idee: str, modele: str | None = None, references: int = 0,
                   video: str | None = None, duree: int = 5, regles: list[str] | None = None,
                   workflow: str | None = None, vues: list[Vue] | None = None) -> dict:
    """`references` : nombre d'images de référence (S'inspirer) ; la consigne dédiée les fait citer par numéro.
    `video` : animer ou texte (Montage) ; consigne vidéo, répliques gardées mot pour mot.
    `regles` : celles du modèle cible (workflows.regles_prompt).
    `workflow` : le modèle cible ; un LLM spécialisé pour lui réécrit avec sa propre consigne.
    `vues` : l'image à animer, les références ou l'image de départ d'une variation, montrées au LLM."""
    securite.verifier(idee)
    s = None if video or references else specialiste_pour(modele, "creation", workflow)
    if s:
        texte, reponse, duree_s, llama = await _par_specialiste(orchestrateur, modele, s, idee)
        securite.verifier(idee, texte)
        return {"idee": idee, "prompt": texte, "modele": llama.modele.name if llama.modele else modele,
                "duree_s": round(duree_s, 2), "tokens_sortie": reponse.tokens_sortie,
                "ecriture_tps": reponse.ecriture_tps}
    modele = generaliste(modele)
    regle = reglages.valeurs_llm(modele)
    if video:
        cadre = "animate the provided image" if video == "animer" else "from text only"
        echanges = [{"role": "system", "content": consigne("video", modele, mots_max_de(workflow), regles)},
                    {"role": "user", "content": f"{idee}\n\n(Clip: {duree} s, {cadre})"}]
    elif references:
        echanges = [{"role": "system", "content": consigne("references", modele, regles=regles)},
                    {"role": "user", "content": f"{idee}\n\n(Reference pictures: {references})"}]
    else:
        echanges = messages(idee, modele, regles, mots_max_de(workflow))
    usage = "animer" if video else "references" if references else "variation"
    async with orchestrateur.llm(modele) as llama:
        echanges[-1]["content"] = montrer(echanges[-1]["content"], vues, usage, voit=_voit(llama))
        debut = time.perf_counter()
        reponse = await llama.generer(echanges, temperature=regle["temperature_reecriture"],
                                      max_tokens=regle["max_tokens"], **reglages.echantillonnage(modele))
        duree = time.perf_counter() - debut
    texte = nettoyer_markdown(reponse.texte)  # **gras**, *italique*, titres : ni ComfyUI ni H3 n'en veulent
    # Idée et prompt ensemble : le LLM peut avoir ajouté un terme sensible à une idée avec un mineur.
    securite.verifier(idee, texte)
    return {
        "idee": idee,
        "prompt": texte,
        "modele": llama.modele.name if llama.modele else modele,
        "duree_s": round(duree, 2),
        "tokens_sortie": reponse.tokens_sortie,
        "ecriture_tps": reponse.ecriture_tps,
    }


# Scénario, onglet Libre : le prompt s'écrit en conversation. Le LLM pose les questions essentielles (avec des réponses
# à cliquer) au lieu de deviner ce que Jérémy n'a pas dit, tient une fiche de ce qu'il a compris, puis écrit le prompt
# et l'ajuste. Chaque tour rend un objet JSON ; la conversation est gardée par l'interface (brouillon du Scénario).

CONSIGNE_LIBRE = f"""You help a French-speaking adult user write ONE prompt for a generation model, together, through a short conversation in French.

Target: {{cible}}.
Already fixed by the user in the other tabs (keep it as it is, never ask about it again): {{contexte}}

Each turn:
1. Update "fiche": what the user has decided so far, as short French phrases, one entry per topic (Sujet, Action, Décor, Style, Lumière, Cadrage{{sujets_video}}). Only what the user said or accepted. When the user lets you choose a topic ("je te laisse choisir"), choose it and add " (choix d'Aria)".
2. Ask at most 3 short questions in French, only about essential topics still open for this target ({{essentiels}}). Give each question 2 to 4 short answer choices. Ask nothing that is already in the fiche or in the fixed part.
3. Write "prompt" ({{langue_prompt}}) only when the user asks you to write it (message "ÉCRIS LE PROMPT"), or when nothing essential is open anymore. The prompt keeps every fiche item and every fixed item, adds nothing that clashes with them, and follows the target's rules below. {{longueur}}
4. Once a prompt exists, each new user message is an adjustment: change only what is asked, keep the rest of the prompt word for word, and give the whole new prompt.
5. {GENRE}
6. {ADULTES}
7. "message": one or two friendly French sentences to the user (what you understood, or what you changed). No question in it: questions go in "questions". Always say « tu » to the user, never « vous ».

Answer ONLY with one JSON object, no markdown, nothing before or after:
{{{{"message": "...", "fiche": {{{{"Sujet": "..."}}}}, "questions": [{{{{"question": "...", "choix": ["...", "..."]}}}}], "prompt": null}}}}"""

ECRIRE = "ÉCRIS LE PROMPT"
# Les images de référence du Scénario, vues par le LLM du chat : ce qu'il en fait selon leur rôle.
IMAGES_LIBRE = {
    "inspirer": "Reference pictures are attached (Picture 1, Picture 2...): look at them first and put what they show "
                "in the fiche, marked « (vu sur l'image) ». The target model receives them too: in the prompt, refer to "
                "them by number (\"the woman from Picture 1\"), keep the identity and outfit they show unless the user "
                "changes them, and do not re-describe faces or clothes. Ask what the user wants to do with them.",
    "variation": "The attached picture is the starting image, partly redrawn by the target model: look at it first and "
                 "put what it shows in the fiche, marked « (vu sur l'image) ». Keep its composition, subjects and "
                 "style unless the user changes them; ask only about what should change.",
    "retouche": "The attached picture is the image to edit: look at it first and put what it shows in the fiche, "
                "marked « (vu sur l'image) ». The prompt is an instruction for it: what changes and where, made "
                "clear and unmistakable, then one short clause saying that everything else stays unchanged.",
    "animer": "The attached picture is the first frame of the clip: look at it first and put what it shows in the "
              "fiche, marked « (vu sur l'image) ». The prompt opens with one short sentence anchoring it as it really "
              "is (style, subject, setting), then describes what happens. Ask about the action, camera, sounds and "
              "spoken lines, never about what the picture already shows.",
}
VIDES_FICHE = {"", "none", "null", "n/a", "-", "—", "aucun", "aucune"}  # « Réplique : None » écrit par Bonsai
DEBUTS_VIDES = ("aucun choix", "aucune réplique", "pas encore", "non défini", "non précisé", "à définir")


def vide_fiche(valeur: str) -> bool:
    """Une rubrique que le LLM remplit pour dire qu'elle est vide (« Aucun choix fait pour le décor ») n'en est pas une."""
    v = valeur.strip().lower()
    return v in VIDES_FICHE or v.startswith(DEBUTS_VIDES)
ESSENTIELS = {"image": "subject, action or pose, setting, medium and style, framing, lighting",
              "retouche": "what changes, where in the picture, how strongly, what must stay exactly as it is",
              "sequence": "the character, the place, the story from beginning to end, the spoken lines, the mood",
              "video": "subject, action, setting, visual style, camera, sounds, spoken line if any"}


class ErreurLibre(ErreurLlama):
    pass


def lire_libre(texte: str) -> dict:
    """L'objet JSON d'un tour, vérifié : message, fiche (texte → texte), 0 à 3 questions à 2-4 choix, prompt ou None."""
    texte = texte.rpartition("</think>")[2]
    try:
        d = json.loads(texte[texte.index("{"):texte.rindex("}") + 1])
    except ValueError:
        raise ErreurLibre("Réponse du LLM illisible (pas de JSON)")
    if not isinstance(d, dict):
        raise ErreurLibre("Réponse du LLM illisible")
    brute = d.get("fiche") if isinstance(d.get("fiche"), dict) else {}
    fiche = {str(k).strip(): " ".join(str(v).split()) for k, v in brute.items()
             if str(k).strip() and v is not None and not vide_fiche(str(v))}
    questions = []
    for q in (d.get("questions") or [])[:3] if isinstance(d.get("questions"), list) else []:
        if isinstance(q, dict) and str(q.get("question") or "").strip():
            choix = [" ".join(str(c).split()) for c in (q.get("choix") or []) if str(c).strip()][:4]
            questions.append({"question": " ".join(str(q["question"]).split()), "choix": choix})
    prompt_ = d.get("prompt")
    prompt_ = nettoyer_markdown(str(prompt_)) if isinstance(prompt_, str) and prompt_.strip() else None
    return {"message": " ".join(str(d.get("message") or "").split()), "fiche": fiche, "questions": questions,
            "prompt": prompt_}


async def libre(orchestrateur: Orchestrateur, echanges: list[dict], forme: str, workflow: str | None,
                contexte: str = "", vues: list[Vue] | None = None, role: str | None = None,
                modele: str | None = None, duree: int = 5) -> dict:
    """Un tour du chat Libre. `echanges` : [{role: user|assistant, content}], les tours d'assistant étant les objets
    JSON déjà rendus. Seul le tour « ÉCRIS LE PROMPT » réfléchit : les questions et les ajustements partent en
    quelques secondes. `vues`, `role` : les images de référence du Scénario (animer, inspirer, variation)."""
    textes = [e["content"] for e in echanges if e["role"] == "user"]
    securite.verifier(contexte, *textes)
    modele = generaliste(modele)
    nom = workflows.obtenir(workflow).fiche["nom"] if workflow else "the target model"
    cibles = {  # forme → (cible, rubriques de la fiche, langue du « prompt »)
        "image": (f"{nom}, a still image", "", "in English"),
        "clip": (f"{nom}, a {duree}-second video clip with sound", ", Caméra, Son, Réplique", "in English"),
        "retouche": (f"{nom}, an instruction-based image editing model: the prompt is an English edit instruction "
                     "for the attached picture, one or two sentences", ", Retouche, À garder", "in English"),
        "sequence": (f"the STORY of a short video sequence of several shots, that {nom} will film shot by shot",
                     ", Histoire, Réplique", "in French: the whole story from beginning to end as a short plain "
                     "narrative, the way the user would tell it: no shots, no camera directions, no English, and "
                     "spoken lines in quotes only if the user gave or accepted them"),
    }
    cible, rubriques, langue_prompt = cibles.get(forme, cibles["image"])
    mots = mots_max_de(workflow) if forme in ("image", "clip") else None
    systeme = CONSIGNE_LIBRE.format(
        cible=cible, contexte=contexte.strip() or "nothing yet", sujets_video=rubriques,
        essentiels=ESSENTIELS.get(forme, ESSENTIELS["video" if forme == "clip" else "image"]),
        langue_prompt=langue_prompt, longueur=f"At most {mots} words." if mots else "")
    if vues and role in IMAGES_LIBRE:
        systeme += f"\n\n{IMAGES_LIBRE[role]}"
    # Les règles du modèle cible écrivent ses prompts ; l'histoire d'une séquence n'en est pas un (le découpage
    # les applique ensuite plan par plan). À l'essai, elles en faisaient un brief anglais en plans.
    if forme != "sequence" and (regles := workflows.regles_prompt(workflow)):
        systeme += f"\n\n{ENTETE_REGLES}\n" + "\n".join(f"- {r}" for r in regles)
    systeme += f"\n\n{CLAUSE_MINEURS}"
    ecrire = bool(echanges) and echanges[-1]["role"] == "user" and echanges[-1]["content"].strip() == ECRIRE
    regle = reglages.valeurs_llm(modele)
    async with orchestrateur.llm(modele) as llama:
        messages_ = [{"role": "system", "content": systeme}]
        for k, e in enumerate(echanges):
            contenu = e["content"]
            if k == 0 and vues:  # les images de référence : il les voit, ou lit leur fiche
                usage = {"animer": "animer", "variation": "variation", "retouche": "retouche"}.get(role, "references")
                contenu = montrer(contenu, vues, usage, voit=_voit(llama))
            messages_.append({"role": e["role"], "content": contenu})
        debut = time.perf_counter()
        for essai in range(2):  # une réponse sans JSON est redemandée une fois
            reponse = await llama.generer(messages_, temperature=regle["temperature_reecriture"], max_tokens=1200,
                                          reflechir=ecrire, **reglages.echantillonnage(modele))
            try:
                tour = lire_libre(reponse.texte)
                break
            except ErreurLibre:
                if essai:
                    raise
                messages_ += [{"role": "assistant", "content": reponse.texte},
                              {"role": "user", "content": "Answer ONLY with the JSON object described."}]
        duree_s = time.perf_counter() - debut
    securite.verifier(contexte, *textes, *tour["fiche"].values(), tour["prompt"] or "")
    return {**tour, "brut": json.dumps({k: tour[k] for k in ("message", "fiche", "questions", "prompt")},
                                       ensure_ascii=False),
            "modele": llama.modele.name if llama.modele else modele, "duree_s": round(duree_s, 2)}


# Contrôles automatiques d'un prompt produit (Casting LLM).

_MOTS_EN = {"the", "a", "an", "of", "with", "in", "and", "on", "is", "at", "her", "his", "its", "under", "by"}
_MOTS_FR = {"le", "la", "les", "un", "une", "des", "du", "de", "et", "avec", "dans", "sur", "sous", "est", "au"}
_PREAMBULES = re.compile(r"^\s*(here('s| is)|sure|certainly|okay|voici|prompt\s*:)", re.IGNORECASE)
_CITATIONS = re.compile(r"[«\"“]\s*([^»\"”]+?)\s*[»\"”]")


def controler(texte: str, idee: str) -> dict:
    mots = re.findall(r"[a-zàâçéèêëîïôûùüÿœ']+", texte.lower())
    en = sum(m in _MOTS_EN for m in mots)
    fr = sum(m in _MOTS_FR for m in mots)
    citations = _CITATIONS.findall(idee)
    return {
        "anglais": en > 2 * fr,
        "longueur": 40 <= len(mots) <= 120,
        "sans_preambule": not _PREAMBULES.match(texte),
        "texte_conserve": all(c in texte for c in citations) if citations else None,
        "mots": len(mots),
    }
