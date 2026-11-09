"""Travail du LLM sur les prompts : réécriture d'une idée, consigne de retouche, traduction.

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

from app import fiches, listes, modeles, reglages, securite, workflows
from app.llm import ErreurLlama
from app.modeles import DOSSIER as MODELES
from app.orchestrateur import Orchestrateur, lancement_llm
from app.repliques import vers_balises

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
3. Dialogue: describe the speaker the first time (role, age range, gender, voice pitch, timbre, pace or accent) and give them an ID in parentheses, (S1), kept for that speaker. Write every spoken line inside a <d> tag that starts with its language: The young woman with a quiet, breathy voice (S1) says: <d>[French] Je descends ici.</d> Keep each line EXACTLY as the user wrote it, words and punctuation, in its original language; never translate it, no ellipses. Double quotes are only for text visible on screen (a sign), never for a spoken line. If the user asks you to invent a line, write one short line in the language they ask for, in the same tag.
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
3. Dialogue: use only the lines in quotes from the story or the shot notes, and each shot's "line", word for word. Write every spoken line inside a <d> tag that starts with its language, after the speaker's description and an ID in parentheses, (S1) for the character: The character (S1) says: <d>[French] Je descends ici.</d> No ellipses; double quotes are only for text visible on screen, never for a spoken line. A shot's "line" belongs to that shot, at the moment it gives: tie it to the visible action in the timeline of the brief (at the start: the character speaks, then the action begins; in the middle: the line falls during the action; at the end: after the action, as a reaction), never to timecodes. A line is spoken in the language it is written in (a French line is spoken in French), whatever the character's origin. Never translate a line, never invent one: a shot without a line has nobody speaking, nobody murmurs or says anything.
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

Answer with exactly one block per shot to write (every shot, unless the user asks for one only), in order, and nothing else:
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
# Un clip plus long porte plus d'actions : la fiche donne la longueur d'un clip de 5 s, chaque seconde de plus en ajoute
# MOTS_PAR_SECONDE_EN_PLUS. Une estimation d'Aria (110 mots à 5 s, 150 à 10 s, 190 à 15 s), aucun guide n'en donne.
DUREE_DE_BASE = 5
MOTS_PAR_SECONDE_EN_PLUS = 8

FORMAT_CHAMPS = """Output format of the target model (it replaces the plain-prose layout above; every other rule stays):
exactly these three fields, one per paragraph, in this order, with these exact labels.
integrated_multimodal_description: [Shot 1] then the whole picture-and-action brief with the dialogue, as described above. No timestamp on Shot 1. Add a second shot only if the user asks for a cut: "[Shot 2] At 00:03.500, the camera cuts to ...".
overall_soundscape: one paragraph of 1 to 4 sentences: ambient sounds, sounds of the actions, non-verbal human sounds. Never put dialogue or music in it. Write N/A only if the user asks for silence.
non_diegetic_music: the background score only if the user asks for it (1 to 3 sentences: instruments, tempo, rhythm, dynamics, no mood words); otherwise N/A.
The word limit counts the three fields together."""
ALIGNEMENT_I2V = "For the target video, at 0.00 seconds into the target video, <Picture 1> (from [Shot 1]) is fully referenced."
MOTS_MAX_DEFAUT = {"reecriture": 120, "video": 110}  # sans modèle cible (avant : 40 à 120 mots même pour Qwen à 200)

CONSIGNES_DEFAUT = {"reecriture": CONSIGNE, "retouche": CONSIGNE_RETOUCHE,
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


NOM_IMAGE_DEFAUT = "Picture {n}"


def avec_noms_images(texte: str, workflow: str | None) -> str:
    """Les « Picture 1, Picture 2… » d'un texte sur les images de référence, dans le nom que le modèle cible leur
    donne (profil_prompt.nom_image de sa fiche : `<image{n}>` pour Qwen-Image 2.1, d'après la doc ComfyUI et son
    prompt enhancer d'édition). Sans modèle ou sans nom dans la fiche : « Picture N », le nom d'origine."""
    nom = NOM_IMAGE_DEFAUT
    if workflow:
        try:
            nom = workflows.obtenir(workflow).profil.get("nom_image") or nom
        except workflows.ErreurWorkflow:
            pass
    if nom == NOM_IMAGE_DEFAUT:
        return texte
    return re.sub(r"Picture (\d+)", lambda m: nom.format(n=int(m.group(1))), texte)


def montrer(texte: str, vues: list[Vue] | None, usage: str, voit: bool,
            workflow: str | None = None) -> str | list[dict]:
    """Le message de l'utilisateur, avec les images si le LLM les voit, sinon ce qu'en disent leurs fiches.
    `workflow` : le modèle cible, qui nomme ses images à sa façon (avec_noms_images)."""
    if not vues:
        return texte
    note = avec_noms_images(NOTES_VUES[usage], workflow)
    if voit:
        return [*map(_image_url, (v.png for v in vues)), {"type": "text", "text": f"{texte}\n\n({note})"}]
    connues = [(i, v.texte) for i, v in enumerate(vues, 1) if v.texte]
    if not connues:
        return texte  # images importées, sans fiche : à l'aveugle
    if len(vues) == 1:
        return f"{texte}\n\n(You cannot see the picture; it was created from this prompt: {connues[0][1]}\n{note})"
    lignes = "\n".join(f"{avec_noms_images(f'Picture {i}', workflow)}: {t}" for i, t in connues)
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


def mots_pour_duree(mots_max: int | None, duree: int | None) -> int | None:
    """Longueur conseillée d'un clip : celle de la fiche jusqu'à 5 s, puis MOTS_PAR_SECONDE_EN_PLUS par seconde en plus."""
    if not mots_max or not duree or duree <= DUREE_DE_BASE:
        return mots_max
    return mots_max + MOTS_PAR_SECONDE_EN_PLUS * (duree - DUREE_DE_BASE)


CHAMP_DESCRIPTION = "integrated_multimodal_description"
_CHAMPS = re.compile(r"(integrated_multimodal_description|overall_soundscape|non_diegetic_music)\s*:", re.I)


def separer_champs(texte: str) -> tuple[str, str | None, str | None]:
    """(description, ambiance, musique) d'un prompt H3 en trois champs. Un champ absent vaut None ; le [Shot 1] de
    tête de la description est retiré (assembler_champs le remet). Sans aucun champ, tout est la description."""
    reperes = list(_CHAMPS.finditer(texte))
    morceaux = {}
    if not reperes or texte[:reperes[0].start()].strip():
        morceaux[CHAMP_DESCRIPTION] = texte[:reperes[0].start()] if reperes else texte
    for i, m in enumerate(reperes):
        fin = reperes[i + 1].start() if i + 1 < len(reperes) else len(texte)
        nom = m.group(1).lower()
        morceaux[nom] = f"{morceaux.get(nom, '')} {texte[m.end():fin]}"
    propres = {nom: " ".join(valeur.split()) for nom, valeur in morceaux.items()}
    description = re.sub(r"^\[Shot 1\]\s*", "", propres.get(CHAMP_DESCRIPTION, ""), flags=re.I)
    return description, propres.get("overall_soundscape") or None, propres.get("non_diegetic_music") or None


def assembler_champs(description: str, ambiance: str | None, musique: str | None) -> str:
    champs = [f"{CHAMP_DESCRIPTION}: [Shot 1] {description}".rstrip()]
    if ambiance:
        champs.append(f"overall_soundscape: {ambiance}")
    if musique:
        champs.append(f"non_diegetic_music: {musique}")
    return "\n\n".join(champs)


def mettre_en_champs(texte: str, animer: bool = False) -> str:
    """Prompt H3 en trois champs, un par paragraphe. Le LLM a écrit sur une ligne (nettoyer_markdown) : on remet les
    sauts de ligne, on garantit le premier champ et [Shot 1], et on ajoute la ligne d'alignement de l'image à animer.
    Rien n'est inventé : un champ que le LLM a omis reste omis."""
    champs = assembler_champs(*separer_champs(texte.strip()))
    return f"{ALIGNEMENT_I2V}\n\n{champs}" if animer else champs


def dans_la_description(texte: str, ajout: str, debut: bool = False) -> str | None:
    """Ajoute une phrase (contrainte, mots déclencheurs, référence) à la description d'un prompt en champs, avant la
    ligne d'alignement éventuelle et sans toucher aux autres champs. None si le prompt n'est pas en champs."""
    repere = _CHAMPS.search(texte)
    if not repere or repere.group(1).lower() != CHAMP_DESCRIPTION:
        return None
    description, ambiance, musique = separer_champs(texte[repere.start():])
    description = f"{ajout} {description}" if debut else f"{description} {ajout}"
    return texte[:repere.start()] + assembler_champs(description.strip(), ambiance, musique)


def consigne(nom: str, modele: str | None = None, mots_max: int | None = None,
             regles: list[str] | None = None, workflow: str | None = None, champs: bool = False) -> str:
    """Consigne du GGUF (réglée ou d'origine), puis les règles propres au modèle cible (profil_prompt.regles de sa
    fiche), toujours suivie de la clause verrouillée. `workflow` : le modèle cible, pour nommer ses images de
    référence (avec_noms_images) ; une consigne réglée à la main y passe aussi."""
    texte = reglages.consigne_llm(modele, nom) or CONSIGNES_DEFAUT[nom]
    texte = (texte.replace("{mots_max}", str(mots_max or MOTS_MAX_DEFAUT.get(nom, 150)))
             .replace("{mots_min}", str(MOTS_MIN))).rstrip()
    if nom == "references":
        texte = avec_noms_images(texte, workflow)
    if regles:
        texte += f"\n\n{ENTETE_REGLES}\n" + "\n".join(f"- {r}" for r in regles)
    if champs and nom in ("video", "decoupage"):
        texte += f"\n\n{FORMAT_CHAMPS}"
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
    texte = garde_cjk(nettoyer_markdown(reponse.texte), instruction)
    securite.verifier(instruction, texte)
    return texte


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
    texte = garde_cjk(reponse.texte, precisions)
    securite.verifier(precisions, texte)
    return texte


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
        champs = workflows.champs_video(workflow)
        echanges = [{"role": "system", "content": consigne("video", modele, mots_pour_duree(mots_max_de(workflow), duree),
                                                           regles, champs=champs)},
                    {"role": "user", "content": f"{idee}\n\n(Clip: {duree} s, {cadre})"}]
    elif references:
        echanges = [{"role": "system", "content": consigne("references", modele, regles=regles, workflow=workflow)},
                    {"role": "user", "content": f"{idee}\n\n(Reference pictures: {references})"}]
    else:
        echanges = messages(idee, modele, regles, mots_max_de(workflow))
    usage = "animer" if video else "references" if references else "variation"
    async with orchestrateur.llm(modele) as llama:
        echanges[-1]["content"] = montrer(echanges[-1]["content"], vues, usage, voit=_voit(llama), workflow=workflow)
        debut = time.perf_counter()
        reponse = await llama.generer(echanges, temperature=regle["temperature_reecriture"],
                                      max_tokens=regle["max_tokens"], **reglages.echantillonnage(modele))
        duree = time.perf_counter() - debut
    texte = garde_cjk(nettoyer_markdown(reponse.texte), idee)  # **gras**, *italique*, titres : ni ComfyUI ni H3 n'en veulent
    if video and workflows.dialogue_en_balise(workflow):
        texte = vers_balises(texte)  # une réplique entre guillemets passe dans la balise <d> de H3
    if video and workflows.champs_video(workflow):
        texte = mettre_en_champs(texte, animer=video == "animer")
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
# à cliquer) au lieu de deviner ce que l'utilisateur n'a pas dit, tient une fiche de ce qu'il a compris, puis écrit le
# prompt et l'ajuste. Chaque tour rend un objet JSON ; la conversation est gardée par l'interface (brouillon du
# Scénario).

CONSIGNE_LIBRE = f"""You help a French-speaking adult user write ONE prompt for a generation model, together, through a short conversation in French.

Target: {{cible}}.
Already fixed by the user in the other tabs (keep it as it is, never ask about it again): {{contexte}}

Each turn:
1. Update "fiche": what the user has decided so far, as short French phrases, one entry per topic (Sujet, Action, Décor, Style, Lumière, Cadrage{{sujets_video}}). Only what the user said or accepted. When the user lets you choose a topic ("je te laisse choisir"), choose it and add " (choix d'Aria)".
2. Ask at most 3 short questions in French, only about essential topics still open for this target ({{essentiels}}). Give each question 2 to 4 short answer choices. Ask nothing that is already in the fiche or in the fixed part.
3. Write "prompt" ({{langue_prompt}}) only when the user asks you to write it (message "ÉCRIS LE PROMPT"), or when nothing essential is open anymore (a first message that already covers every essential topic is enough: write the prompt at once, with no question). The prompt keeps every fiche item and every fixed item, adds nothing that clashes with them, and follows the target's rules below. {{longueur}}
4. Once a prompt exists, each new user message is an adjustment: change only what is asked, keep the rest of the prompt word for word, and give the whole new prompt.
5. {GENRE}
6. {ADULTES}
7. "message": one or two friendly French sentences to the user (what you understood, or what you changed). No question in it: questions go in "questions". Always say « tu » to the user, never « vous ».

Answer ONLY with one JSON object, no markdown, nothing before or after:
{{{{"message": "...", "fiche": {{{{"Sujet": "..."}}}}, "questions": [{{{{"question": "...", "choix": ["...", "..."], "liste": "optional guide topic"}}}}], "prompt": null}}}}"""

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
              "plan": "what happens in this shot, the framing, the camera movement, the sounds",
              "video": "subject, action, setting, visual style, camera, sounds, spoken line if any"}


class ErreurLibre(ErreurLlama):
    pass


VALEURS_GUIDE_MAX = 8  # valeurs montrées par rubrique : de quoi proposer des choix sans noyer la consigne


CONTEXTE_CATALOGUE = 8192  # en dessous, le catalogue ne garde que les noms de rubriques (4096 tokens : consigne comprise)


def _rubriques_guide(sujet: str | None, sans_sujet: bool, video: bool, ignorees: set[str] = frozenset()):
    """(sous-liste, nom du groupe, noms de ses valeurs) des rubriques de « Me guider » utiles à cette cible."""
    try:
        structure = listes.structure(sujet or "personnage")
    except listes.ErreurListes:
        return
    for bloc in structure["blocs"]:
        if bloc["video"] and not video:
            continue
        for groupe in bloc["groupes"]:
            if sans_sujet and groupe["onglet"] == "sujet":
                continue
            for sl in groupe["sous_listes"]:
                if sl["nom"] in ignorees:
                    continue
                if valeurs := [v["nom"] for v in sl["valeurs"] if not v.get("remplace")]:
                    yield sl, groupe["nom"], valeurs


def noms_guide(sujet: str | None, sans_sujet: bool = False, video: bool = False,
               ignorees: set[str] = frozenset()) -> set[str]:
    return {sl["nom"] for sl, _, _ in _rubriques_guide(sujet, sans_sujet, video, ignorees)}


def catalogue_guide(sujet: str | None, sans_sujet: bool = False, video: bool = False, compact: bool = False,
                    ignorees: set[str] = frozenset()) -> str:
    """Les rubriques de « Me guider » pour cet atelier, dites au LLM : il pose ses questions sur ces rubriques et tire les
    choix de chaque question de leur liste, pour que la conversation et le guide parlent des mêmes choses. Seules les
    rubriques utiles à la cible : pas de Sujet quand il vient des images, pas de Caméra / Voix / Son pour une image.
    Les choix déjà faits ailleurs sont dans « Already fixed » : le LLM n'y revient pas."""
    lignes = []
    for sl, groupe, valeurs in _rubriques_guide(sujet, sans_sujet, video, ignorees):
        suite = ", …" if len(valeurs) > VALEURS_GUIDE_MAX else ""
        lignes.append(f"- {sl['nom']} ({groupe})" if compact
                      else f"- {sl['nom']} ({groupe}): {', '.join(valeurs[:VALEURS_GUIDE_MAX])}{suite}")
    if not lignes:
        return ""
    liste = ('When a question is about one of these topics, add "liste": "<topic name exactly as written>" to it: the '
             "user then gets the whole list as a drop-down, and your 2 to 4 choices are shown first as suggestions. ")
    choix = (liste + "Take the choices of such a question from its list below (written exactly as listed, in French); "
             "the user may also answer freely with another value, and that is fine."
             if not compact else liste + "Write the choices of each question yourself, in French.")
    return ("Guide topics. The user can also pick these in the « Me guider » tab, so ask your questions about these "
            f"topics, in this spirit. {choix} Ask about a topic only when it matters for the target and nothing in the "
            "fiche or the fixed part already covers it:\n" + "\n".join(lignes))


def cadre_format(format_: str | None) -> str:
    """Le format choisi au Scénario, dit au LLM : « 16:9 (Widescreen) » → rapport, orientation et ce que ça change à la
    composition. Vide sans format (Retouche, Redessiner : l'image garde le sien)."""
    m = re.match(r"\s*(\d+(?:\.\d+)?)\s*:\s*(\d+(?:\.\d+)?)", format_ or "")
    if not m:
        return ""
    largeur, hauteur = float(m[1]), float(m[2])
    if largeur == hauteur:
        forme = "square"
        conseil = "centered, balanced compositions work best"
    elif largeur > hauteur:
        forme = "landscape (wider than tall)"
        conseil = "spread the scene horizontally, give room to the setting on the sides"
    else:
        forme = "portrait (taller than wide)"
        conseil = "stack the scene vertically, favour a tall framing of the subject"
    return (f"Output frame chosen by the user: {format_.strip()}, {forme}. Compose the prompt for this frame ({conseil}). "
            "It is fixed: never ask about it. If a prompt already exists and the frame differs from the one it was "
            "written for, adapt its framing and composition to the new frame and give the whole new prompt.")


def lire_libre(texte: str, rubriques: set[str] | None = None) -> dict:
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
            question = {"question": " ".join(str(q["question"]).split()), "choix": choix}
            if (liste := " ".join(str(q.get("liste") or "").split())) in (rubriques or ()):
                question["liste"] = liste  # une rubrique du guide : l'interface en fait une liste déroulante
            questions.append(question)
    prompt_ = d.get("prompt")
    prompt_ = nettoyer_markdown(str(prompt_)) if isinstance(prompt_, str) and prompt_.strip() else None
    return {"message": " ".join(str(d.get("message") or "").split()), "fiche": fiche, "questions": questions,
            "prompt": prompt_}


# Garde-fou : certains LLM (Qwen surtout) glissent du chinois ou du japonais dans une réponse française ou anglaise.
_CJK = re.compile("[\u2e80-\u303f\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uac00-\ud7af\uf900-\ufaff\uff00-\uffef]")


def cjk_inattendu(sortie: list[str], entrees: list[str]) -> bool:
    """Vrai si la sortie du LLM contient du chinois, du japonais ou du coréen alors que rien de ce que l'utilisateur
    a écrit n'en contient (un mot en japonais voulu par l'utilisateur reste donc permis)."""
    return any(_CJK.search(t) for t in sortie) and not any(_CJK.search(t) for t in entrees)


def _sans_cjk(valeur):
    if isinstance(valeur, str):
        return " ".join(_CJK.sub(" ", valeur).split())
    if isinstance(valeur, list):
        return [_sans_cjk(v) for v in valeur]
    if isinstance(valeur, dict):
        return {k: _sans_cjk(v) for k, v in valeur.items()}
    return valeur


def garde_cjk(texte: str, *entrees: str) -> str:
    """Le texte sans caractères CJK que le LLM a ajoutés de lui-même (les mots voulus par l'utilisateur restent)."""
    return _sans_cjk(texte) if cjk_inattendu([texte], list(entrees)) else texte


def _textes_du_tour(tour: dict) -> list[str]:
    q = [t for x in tour["questions"] for t in [x["question"], *x["choix"]]]
    return [tour["message"], *tour["fiche"].values(), *q, tour["prompt"] or ""]


async def libre(orchestrateur: Orchestrateur, echanges: list[dict], forme: str, workflow: str | None,
                contexte: str = "", vues: list[Vue] | None = None, role: str | None = None,
                modele: str | None = None, duree: int = 5, format_: str | None = None,
                sujet: str | None = None, sans_sujet: bool = False) -> dict:
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
        "plan": (f"{nom}, ONE shot of a video sequence: a {duree}-second clip with sound. Write only this shot's "
                 "prompt, in continuity with the shots around it; if the shot has a line, keep it word for word at its "
                 "moment, tied to the visible action, and never invent another line", ", Caméra, Son, Réplique",
                 "in English"),
    }
    cible, rubriques, langue_prompt = cibles.get(forme, cibles["image"])
    mots = mots_max_de(workflow) if forme in ("image", "clip", "plan") else None
    if forme in ("clip", "plan"):
        mots = mots_pour_duree(mots, duree)
    systeme = CONSIGNE_LIBRE.format(
        cible=cible, contexte=contexte.strip() or "nothing yet", sujets_video=rubriques,
        essentiels=ESSENTIELS.get(forme, ESSENTIELS["video" if forme == "clip" else "image"]),
        langue_prompt=langue_prompt, longueur=f"At most {mots} words." if mots else "")
    if vues and role in IMAGES_LIBRE:
        systeme += f"\n\n{avec_noms_images(IMAGES_LIBRE[role], workflow)}"
    if forme in ("image", "clip") and (cadre := cadre_format(format_)):
        systeme += f"\n\n{cadre}"
    if forme in ("image", "clip") and (guide := catalogue_guide(
            sujet, sans_sujet, forme == "clip", lancement_llm(modele)["contexte"] < CONTEXTE_CATALOGUE,
            workflows.rubriques_ignorees(workflow))):
        systeme += f"\n\n{guide}"
    # Les règles du modèle cible écrivent ses prompts ; l'histoire d'une séquence n'en est pas un (le découpage
    # les applique ensuite plan par plan). À l'essai, elles en faisaient un brief anglais en plans.
    if forme != "sequence" and (regles := workflows.regles_prompt(workflow)):
        systeme += f"\n\n{ENTETE_REGLES}\n" + "\n".join(f"- {r}" for r in regles)
    systeme += f"\n\n{CLAUSE_MINEURS}"
    rubriques = noms_guide(sujet, sans_sujet, forme == "clip", workflows.rubriques_ignorees(workflow)) if forme in ("image", "clip") else set()
    ecrire = bool(echanges) and echanges[-1]["role"] == "user" and echanges[-1]["content"].strip() == ECRIRE
    regle = reglages.valeurs_llm(modele)
    async with orchestrateur.llm(modele) as llama:
        messages_ = [{"role": "system", "content": systeme}]
        for k, e in enumerate(echanges):
            contenu = e["content"]
            if k == 0 and vues:  # les images de référence : il les voit, ou lit leur fiche
                usage = {"animer": "animer", "variation": "variation", "retouche": "retouche"}.get(role, "references")
                contenu = montrer(contenu, vues, usage, voit=_voit(llama), workflow=workflow)
            messages_.append({"role": e["role"], "content": contenu})
        debut = time.perf_counter()
        for essai in range(2):  # une réponse sans JSON est redemandée une fois
            reponse = await llama.generer(messages_, temperature=regle["temperature_reecriture"], max_tokens=1200,
                                          reflechir=ecrire, **reglages.echantillonnage(modele))
            try:
                tour = lire_libre(reponse.texte, rubriques)
                if cjk_inattendu(_textes_du_tour(tour), [contexte, *textes]):
                    if essai:  # toujours là au second essai : on retire les caractères, plutôt que d'échouer
                        tour = _sans_cjk(tour)
                        break
                    raise ErreurLibre("caractères chinois, japonais ou coréens inattendus")
                break
            except ErreurLibre as err:
                if essai:
                    raise
                rappel = ("Answer ONLY with the JSON object described, and write ONLY in French or English: "
                          "no Chinese, Japanese or Korean characters." if "inattendus" in str(err)
                          else "Answer ONLY with the JSON object described.")
                messages_ += [{"role": "assistant", "content": reponse.texte},
                              {"role": "user", "content": rappel}]
        duree_s = time.perf_counter() - debut
    if tour["prompt"] and forme in ("clip", "plan") and workflows.dialogue_en_balise(workflow):
        tour = {**tour, "prompt": vers_balises(tour["prompt"])}  # les répliques dans la balise <d> de H3
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
