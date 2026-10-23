"""Travail du LLM sur les prompts : réécriture d'une idée, consigne de retouche, peaufinage, traduction.

Les consignes et les températures se règlent par GGUF dans l'onglet Réglages (data/reglages.yaml).
La clause de protection des mineurs, elle, n'est pas modifiable : le code l'ajoute à chaque consigne.
"""

import re
import time

from app import reglages, securite
from app.orchestrateur import Orchestrateur

CLAUSE_MINEURS = """Non-negotiable rule: never make a character younger than described. If a character is a child or a teenager, show them fully clothed in a wholesome, non-suggestive scene."""

CONSIGNE = """You are an expert prompt writer for text-to-image models.
The user describes an image idea, often in French. Rewrite it as ONE English prompt paragraph.

Rules:
1. Stay faithful: keep every subject, action, color, attribute and spatial relation from the idea. Do not add new characters or objects the idea does not imply.
2. Make it visual: precise subject description, setting, lighting, mood, composition and camera or medium (photo, illustration, painting...). Choose them to serve the idea.
3. If the idea asks for written text in the image, keep that text exactly, in quotes. Otherwise mention no text.
4. 40 to 120 words, plain prose, no lists, no headings, no quotes around the whole prompt.
5. Output only the prompt: no preamble, no explanation, no translation notes."""


CONSIGNE_RETOUCHE = """You translate image-editing instructions for an instruction-based image editing model.
The user writes an instruction, often in French. Rewrite it as a short English imperative instruction (one or two sentences).

Rules:
1. Keep exactly what is asked: same change, same target, same details. Add no style, no new elements, no embellishment.
2. If the user names something to keep unchanged, say so explicitly.
3. If the instruction asks for written text, keep that text exactly, in quotes.
4. Output only the instruction: no preamble, no explanation."""


CONSIGNE_PEAUFINER = """You polish English prompts for text-to-image models.
The user gives a prompt assembled from keywords. Rewrite it as fluent, natural English prose.

Rules:
1. Keep every element exactly: subject, age, gender, origin, body, face, hair, clothing, pose, action, setting, time, weather, style, lighting, framing, palette. Do not remove, soften, replace or censor any of them.
2. Add nothing new: no extra characters, objects, details, style or mood.
3. Keep the age wording exactly as given. Never make a character younger.
4. If the prompt contains text in quotes, keep that text exactly, in quotes.
5. At most {mots_max} words, plain prose, no lists, no headings.
6. Output only the prompt: no preamble, no explanation."""


CONSIGNE_PRECISIONS = """You translate short details for a text-to-image prompt.
The user writes a detail, often in French. Translate it into a short English phrase that can be appended to an image prompt.

Rules:
1. Keep exactly the meaning: same elements, same details. Add nothing, remove nothing.
2. If the detail is already in English, return it unchanged.
3. If it contains text in quotes, keep that text exactly, in quotes.
4. Output only the translation: no preamble, no explanation."""

CONSIGNE_REFERENCES = """You write instructions for an image model that receives reference pictures, numbered Picture 1, Picture 2, Picture 3.
The user describes, often in French, a new image to create from those references (« image 1 » means Picture 1). Rewrite it as ONE English instruction.

Rules:
1. Refer to the references by their number: "the woman from Picture 1", "the dragon from Picture 2". Do not re-describe what the references already show (face, hair, clothes, colors): the model sees them.
2. Ask explicitly to keep the identity (face, hair) and outfit of the characters from the references, unless the user asks to change them, but NOT their pose, seat, props or background: the characters take the new pose and place described by the user.
3. Describe what is new, as the user asks: scene, pose, action, setting, lighting, framing, style. Add nothing the user did not imply.
4. If the user asks for written text in the image, keep that text exactly, in quotes.
5. 1 to 4 sentences, at most 90 words, plain prose.
6. Output only the instruction: no preamble, no explanation."""

CONSIGNE_VIDEO = """You write prompts for an audio-video generation model: it creates the picture AND the sound together.
The user describes, often in French, a short video clip. Rewrite it as ONE English prompt.

Rules:
1. A short production brief in the present tense: shot type, subject, action, setting, ONE named camera movement with its speed (for example "the camera slowly pushes in"), then the sounds.
2. Dialogue: keep every spoken line EXACTLY as the user wrote it, in its original language, inside double quotes. Say who speaks, in which language and with which tone. Never translate the lines.
3. Sounds: name concrete sounds tied to the action (footsteps, rain, wind...). Mention music only if the user asks for it, separately from the sounds.
4. If the clip animates a provided image, describe only what moves (motion, camera, sounds), not what the image already shows.
5. Fit the clip duration: few actions, at most one or two short lines of dialogue.
6. 40 to 110 words, plain prose, no lists, no headings.
7. Output only the prompt: no preamble, no explanation."""

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
6. Character: {regle_personnage}
7. Keep the look of the setting (place, light, style) unless a shot's note says otherwise.

Answer with exactly one block per shot, in order, and nothing else:
SHOT 1
FR: one short French sentence summarising the shot
EN: the English prompt"""

REGLE_AVEC_REFERENCE = ('a reference picture gives the character\'s face, age, body and outfit: never describe them, '
                        'call her or him simply "the character".')
REGLE_SANS_REFERENCE = ("no picture shows the character: in every start and cut shot, copy the character description "
                        "given by the user word for word; in continue and transition shots, call her or him "
                        "\"the character\".")

CONSIGNES_DEFAUT = {"reecriture": CONSIGNE, "retouche": CONSIGNE_RETOUCHE, "peaufinage": CONSIGNE_PEAUFINER,
                    "precisions": CONSIGNE_PRECISIONS, "references": CONSIGNE_REFERENCES,
                    "video": CONSIGNE_VIDEO, "decoupage": CONSIGNE_DECOUPAGE}


def consigne(nom: str, modele: str | None = None, mots_max: int | None = None) -> str:
    """Consigne du GGUF (réglée ou d'origine), toujours suivie de la clause verrouillée."""
    texte = reglages.consigne_llm(modele, nom) or CONSIGNES_DEFAUT[nom]
    texte = texte.replace("{mots_max}", str(mots_max or 150))
    return f"{texte.rstrip()}\n\n{CLAUSE_MINEURS}"


async def traduire_instruction(orchestrateur: Orchestrateur, instruction: str, modele: str | None = None) -> str:
    securite.verifier(instruction)
    regle = reglages.valeurs_llm(modele)
    async with orchestrateur.llm(modele) as llama:
        reponse = await llama.generer(
            [{"role": "system", "content": consigne("retouche", modele)}, {"role": "user", "content": instruction}],
            temperature=regle["temperature_retouche"], max_tokens=200, **reglages.echantillonnage(modele),
        )
    securite.verifier(instruction, reponse.texte)
    return reponse.texte


async def peaufiner(orchestrateur: Orchestrateur, assemble: str, modele: str | None = None,
                    mots_max: int | None = None, duree_video: int | None = None) -> dict:
    """Rend fluide un prompt assemblé par l'onglet Scénario, sans rien retirer ni ajouter.
    `duree_video` : la cible est un modèle vidéo ; la consigne vidéo garde les répliques mot pour mot."""
    securite.verifier(assemble)
    regle = reglages.valeurs_llm(modele)
    if duree_video:
        echanges = [{"role": "system", "content": consigne("video", modele)},
                    {"role": "user", "content": f"{assemble}\n\n(Clip: {duree_video} s, from text only)"}]
    else:
        echanges = [{"role": "system", "content": consigne("peaufinage", modele, mots_max)},
                    {"role": "user", "content": assemble}]
    async with orchestrateur.llm(modele) as llama:
        debut = time.perf_counter()
        reponse = await llama.generer(
            echanges,
            temperature=regle["temperature_peaufinage"], max_tokens=regle["max_tokens"],
            **reglages.echantillonnage(modele))
        duree = time.perf_counter() - debut
    # Ensemble : un mineur de l'original reste un mineur, même si le LLM l'a reformulé.
    securite.verifier(assemble, reponse.texte)
    return {"prompt": reponse.texte, "modele": llama.modele.name if llama.modele else modele,
            "duree_s": round(duree, 2), "mots": len(reponse.texte.split())}


async def traduire_precisions(orchestrateur: Orchestrateur, precisions: str, modele: str | None = None) -> str:
    securite.verifier(precisions)
    regle = reglages.valeurs_llm(modele)
    async with orchestrateur.llm(modele) as llama:
        reponse = await llama.generer(
            [{"role": "system", "content": consigne("precisions", modele)}, {"role": "user", "content": precisions}],
            temperature=regle["temperature_traduction"], max_tokens=200, **reglages.echantillonnage(modele),
        )
    securite.verifier(precisions, reponse.texte)
    return reponse.texte


def nettoyer_markdown(texte: str) -> str:
    """Prompt vidéo en prose simple : Llama glisse parfois **gras**, *bruitages* ou titres, que H3 n'a pas à lire."""
    texte = re.sub(r"\*{1,2}([^*]+?)\*{1,2}", r"\1", texte)
    texte = re.sub(r"^\s*#+\s*", "", texte, flags=re.MULTILINE)
    return " ".join(texte.split())


def messages(idee: str, modele: str | None = None) -> list[dict]:
    return [{"role": "system", "content": consigne("reecriture", modele)}, {"role": "user", "content": idee}]


async def reecrire(orchestrateur: Orchestrateur, idee: str, modele: str | None = None, references: int = 0,
                   video: str | None = None, duree: int = 5) -> dict:
    """`references` : nombre d'images de référence (S'inspirer) ; la consigne dédiée les fait citer par numéro.
    `video` : animer ou texte (Montage) ; consigne vidéo, répliques gardées mot pour mot."""
    securite.verifier(idee)
    regle = reglages.valeurs_llm(modele)
    if video:
        cadre = "animate the provided image" if video == "animer" else "from text only"
        echanges = [{"role": "system", "content": consigne("video", modele)},
                    {"role": "user", "content": f"{idee}\n\n(Clip: {duree} s, {cadre})"}]
    elif references:
        echanges = [{"role": "system", "content": consigne("references", modele)},
                    {"role": "user", "content": f"{idee}\n\n(Reference pictures: {references})"}]
    else:
        echanges = messages(idee, modele)
    async with orchestrateur.llm(modele) as llama:
        debut = time.perf_counter()
        reponse = await llama.generer(echanges, temperature=regle["temperature_reecriture"],
                                      max_tokens=regle["max_tokens"], **reglages.echantillonnage(modele))
        duree = time.perf_counter() - debut
    texte = nettoyer_markdown(reponse.texte) if video else reponse.texte
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
