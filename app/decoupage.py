"""Découpage d'une séquence en plans (Scénario, atelier Séquence) : le LLM écrit le prompt de chaque plan, tous
ensemble (✨ Tout écrire) ou un seul (✨ et 💬 d'un plan), puis les garde-fous d'ici le contrôlent (étiquettes,
répliques inventées, réplique du plan mot pour mot à son moment, description du personnage). Le stockage, les graphes
et le tournage restent dans app/sequences.py.
"""

import re
import time

from app import plateau, prompt, reglages, securite, workflows
from app.config import CONFIG
from app.repliques import balise, langue, vers_balises  # langue : aussi lue par voix.py (decoupage.langue)
from app.sequences import (DEPARTS, ENCHAINEMENTS, LONGUEUR_CONTRAINTE, LONGUEUR_MUSIQUE, PLANS_MAX, ErreurSequence,
                           _extrait, _FIN_PHRASE, _simple, garder_repliques, repliques, sans_repliques_inventees)

LONGUEUR_SYNOPSIS = 2000
LONGUEUR_NOTE = 500
TYPES_DECOUPAGE = {"rush": "start from the picture", "texte": "start", "prolonger": "continue", "couper": "cut",
                   "transition": "transition"}
_BLOC_PLAN = re.compile(r"SHOT\s+(\d+)\s*:?\s*\n\s*FR\s*:\s*(.+?)\n\s*EN\s*:\s*(.+?)(?=\n\s*SHOT\s+\d+|\Z)", re.S | re.I)


LONGUEUR_REPLIQUE = 300  # la réplique d'un plan
LONGUEUR_PROMPT_PLAN = 2000  # le prompt déjà écrit d'un plan, donné en contexte à ses voisins
# Le moment de la réplique dans le plan. Le guide de H3 suit une chronologie (image d'ouverture, début de l'action,
# développement, résultat) et cale une réplique sur ce qu'on voit, jamais sur un minutage.
MOMENTS = {"debut": "at the start of the shot, before the action begins",
           "pendant": "in the middle of the action, while it goes on",
           "fin": "at the end of the shot, after the action, as a reaction"}
MOTS_PAR_SECONDE = 2.5  # débit de parole confortable : estimation d'Aria, aucun guide de H3 n'en donne


def _valider_decoupage(synopsis: str, personnage_: str, decor: str, plans: list[dict],
                       reference: str | None, cible: int | None = None) -> list[dict]:
    """Les plans, propres. `cible` : le seul plan à écrire (✨ ou 💬 d'un plan), les autres servent de contexte."""
    if max(len(synopsis), len(personnage_), len(decor)) > LONGUEUR_SYNOPSIS:
        raise ErreurSequence(f"Fil de l'histoire de {LONGUEUR_SYNOPSIS} caractères au plus")
    if not 2 <= len(plans) <= PLANS_MAX:
        raise ErreurSequence(f"De 2 à {PLANS_MAX} plans")
    if cible is not None and not 0 <= cible < len(plans):
        raise ErreurSequence("Ce plan n'existe pas")
    propres = []
    for k, p in enumerate(plans):
        enchainement = p.get("enchainement")
        permis = DEPARTS if k == 0 else ENCHAINEMENTS
        if enchainement not in permis:
            raise ErreurSequence(f"Plan {k + 1} : enchaînement possible : {', '.join(permis)}")
        if enchainement == "rush" and not reference:
            raise ErreurSequence("Plan 1 : pour partir d'un rush, choisis l'image de référence")
        duree = p.get("duree", 5)
        if duree not in plateau.DUREES_VIDEO:
            raise ErreurSequence(f"Plan {k + 1} : durée proposée : {', '.join(map(str, plateau.DUREES_VIDEO))} s")
        note = str(p.get("note") or "").strip()
        if len(note) > LONGUEUR_NOTE:
            raise ErreurSequence(f"Plan {k + 1} : « ce qui se passe » de {LONGUEUR_NOTE} caractères au plus")
        replique = " ".join(str(p.get("replique") or "").split()).strip("«»\"“” ")
        if len(replique) > LONGUEUR_REPLIQUE:
            raise ErreurSequence(f"Plan {k + 1} : réplique de {LONGUEUR_REPLIQUE} caractères au plus")
        moment = p.get("moment") or "pendant"
        if moment not in MOMENTS:
            raise ErreurSequence(f"Plan {k + 1} : la réplique se dit au début, pendant l'action ou à la fin")
        rush = (reference if enchainement == "rush" else p.get("rush")) if enchainement in ("rush", "transition") else None
        if rush:
            if plateau.fiche(rush).get("type") == "clip":
                raise ErreurSequence(f"Plan {k + 1} : il faut une image, pas un clip")
            plateau.chemin_image(rush)
        propres.append({"enchainement": enchainement, "duree": duree, "note": note, "rush": rush,
                        "replique": replique, "moment": moment, "off": bool(p.get("off")) and bool(replique),
                        "prompt": " ".join(str(p.get("prompt") or "").split())[:LONGUEUR_PROMPT_PLAN]})
    if reference:
        plateau.chemin_image(reference)
    vide = lambda p: not (p["note"] or p["replique"])
    if not synopsis.strip():
        if cible is None and all(map(vide, propres)):
            raise ErreurSequence("Écris le fil de l'histoire, ou ce qui se passe dans les plans")
        if cible is not None and vide(propres[cible]):
            raise ErreurSequence(f"Plan {cible + 1} : écris ce qui s'y passe (ou le fil de l'histoire)")
    return propres


def ligne_replique(p: dict) -> str:
    """La réplique d'un plan, telle que le LLM la reçoit : mot pour mot, sa langue, son moment, la voix off."""
    off = ", as an off-screen voiceover (the character's lips stay closed)" if p["off"] else ""
    return f'line, word for word, in its <d> tag: {balise(p["replique"])}, spoken {MOMENTS[p["moment"]]}{off}'


def demande_decoupage(synopsis: str, personnage_: str, decor: str, plans: list[dict], reference: str | None,
                      contrainte: str = "", musique: str | None = None, cible: int | None = None) -> str:
    """Message envoyé à Llama : le fil de l'histoire, le personnage (sauf avec une image de référence), le décor, la
    contrainte, la langue de chaque réplique, puis la liste des plans avec leur type, durée, ce qui s'y passe, leur
    réplique et leur arrivée. `cible` : un seul plan à écrire, avec le prompt déjà écrit de ses voisins."""
    lignes = [f"Story: {synopsis.strip()}" if synopsis.strip() else "Story: told by the shots below, in order.",
              "Character: " + ("shown by the reference picture" if reference else (personnage_.strip() or "free")),
              f"Setting and look: {decor.strip() or 'free'}"]
    if contrainte:
        lignes.append(f"Mandatory constraint, respected in EVERY shot: {contrainte}")
    toutes = list(dict.fromkeys(repliques(synopsis) + [r for p in plans for r in repliques(p["note"])]))
    parlent = bool(toutes or any(p.get("replique") for p in plans))
    if not parlent:
        lignes.append("Nobody speaks in the whole film: no dialogue, no voice-over.")
    elif toutes:
        lignes.append("Spoken lines, each in its <d> tag with its language: " + "; ".join(balise(r) for r in toutes))
    lignes.append(f"Background music for the whole film, the same in every shot: {musique}" if musique
                  else "No background music.")
    lignes.append("Shots:")
    for k, p in enumerate(plans, 1):
        morceaux = [f"{k}. {TYPES_DECOUPAGE[p['enchainement']]}, {p['duree']} s"]
        if p["note"]:
            morceaux.append(f"note: {p['note']}")
        if parlent and p.get("replique"):
            morceaux.append(ligne_replique(p))
        if p["enchainement"] == "transition":
            arrivee = ""
            if p["rush"]:
                f = plateau.fiche(p["rush"])
                arrivee = f.get("prompt") or f.get("idee") or ""
            morceaux.append(f"arrival: {_extrait(arrivee) or 'the next moment of the story'}")
        lignes.append(", ".join(morceaux))
    if cible is not None:
        lignes.append(f"Write ONLY shot {cible + 1}: the other shots are context, keep continuity with them.")
        for j in (cible - 1, cible + 1):
            if 0 <= j < len(plans) and plans[j].get("prompt"):
                lignes.append(f"Shot {j + 1} as already written: {_extrait(plans[j]['prompt'], 700)}")
    return "\n".join(lignes)


def lire_blocs(texte: str, numeros: list[int], exact: bool = True) -> list[tuple[str, str]]:
    """Les blocs SHOT n / FR / EN de la réponse, pour les plans demandés ; refuse une réponse incomplète.
    Ministral habille souvent les étiquettes (**FR:**, ### SHOT 1) et sépare les plans par --- : on l'enlève.
    `exact` faux (un seul plan demandé) : un plan en trop est ignoré, un seul bloc mal numéroté est pris."""
    texte = re.sub(r"\*\*|^\s*#+\s*|^\s*-{3,}\s*$", "", texte, flags=re.M)
    blocs = {int(n): (fr.strip(), en.strip()) for n, fr, en in _BLOC_PLAN.findall(texte)}
    if not exact and len(blocs) == 1 and len(numeros) == 1:
        return list(blocs.values())
    if (sorted(blocs) != list(numeros)) if exact else not set(numeros) <= set(blocs):
        raise ErreurSequence(f"Llama a rendu {len(blocs)} plan(s) au lieu de {len(numeros)}")
    return [blocs[k] for k in numeros]


def lire_decoupage(texte: str, nombre: int) -> list[tuple[str, str]]:
    return lire_blocs(texte, list(range(1, nombre + 1)))


_ETIQUETTE = re.compile(r"(?i)^\s*(?:start from the picture|start|cut|transition)\s*[:—-]\s*")
_ETIQUETTE_INTERNE = re.compile(r"(?i),\s*start from the picture\s*:\s*")
_ETIQUETTE_PLAN = re.compile(r"(?i)\b(?:a|an)\s+(?:start|cut|transition|continue)\s+shot\s*[,:—-]\s*(\w)")


def sans_etiquette(texte: str) -> str:
    """Retire les types de plan recopiés par Llama (« Cut: », « start from the picture: », « A cut shot, ») :
    H3 n'a pas à les lire."""
    texte = _ETIQUETTE_INTERNE.sub(", ", _ETIQUETTE.sub("", texte))
    texte = _ETIQUETTE_PLAN.sub(lambda m: m.group(1).upper(), texte).strip()  # la phrase repart en majuscule
    return texte[:1].upper() + texte[1:]


def avec_personnage(personnage_: str, texte: str) -> str:
    """Sans image de référence, l'identité tient au texte : la description du personnage, si Llama l'a omise,
    est remise en tête du plan (à l'essai, il ne la répétait pas dans les plans coupés)."""
    description = personnage_.strip().rstrip(".")
    if not description or _simple(description)[:60] in _simple(texte):
        return texte
    return f"The character is {description[:1].lower()}{description[1:]}. {texte}"


def placer_replique(texte: str, replique: str, moment: str, off: bool) -> str:
    """La réplique d'un plan, mot pour mot, à son moment. Écrite par le LLM : rien à faire. Perdue : remise dans la
    chronologie du brief, après la phrase d'ouverture (début), au milieu (pendant l'action) ou à la fin."""
    if not replique or _simple(replique) in _simple(texte):
        return texte
    dit = (f"The character (S1) says in an off-screen voiceover: {balise(replique)} The character's lips stay closed."
           if off else f"The character (S1) says: {balise(replique)}")
    fins = [m.end() for m in _FIN_PHRASE.finditer(texte)]
    if moment == "fin" or len(fins) < 2:
        coupure = len(texte)
    else:
        coupure = fins[0] if moment == "debut" else fins[(len(fins) - 1) // 2]
    return " ".join(f"{texte[:coupure]} {dit} {texte[coupure:]}".split())


def alerte_replique(k: int, p: dict) -> list[str]:
    """Une réplique trop longue pour son plan : H3 la presse ou la coupe."""
    if not p["replique"]:
        return []
    mots, permis = len(p["replique"].split()), int(MOTS_PAR_SECONDE * p["duree"])  # comme la page (Math.floor)
    return [f"Plan {k} : réplique de {mots} mots pour {p['duree']} s (environ {permis} au plus) : elle risque d'être "
            "pressée ou coupée"] if mots > permis else []


def source_repliques(synopsis: str, contrainte: str, plans: list[dict]) -> str:
    """Ce qui a le droit d'être dit (ou écrit entre guillemets) : le reste est une réplique inventée par Llama. Sans
    aucune réplique nulle part, tout texte entre guillemets du LLM en est une."""
    if not (repliques(synopsis) or any(repliques(p["note"]) or p["replique"] for p in plans)):
        return ""
    return " ".join([synopsis, contrainte, *(p["note"] for p in plans),
                     *(f'"{p["replique"]}"' for p in plans if p["replique"])])


def finir_plan(k: int, p: dict, en: str, personnage_: str, source: str, champs: bool = False) -> tuple[str, list[str]]:
    """Les garde-fous du prompt d'un plan écrit par Llama (découpage, ✨ ou 💬 d'un plan) : étiquettes et Markdown
    retirés, répliques inventées retirées, « Continuous shot » d'un plan prolongé, description du personnage, et la
    réplique du plan mot pour mot, à son moment. Renvoie le prompt et les alertes.
    `champs` : le prompt est en trois champs (H3) ; les garde-fous s'appliquent à la description, l'ambiance et la
    musique sont remises telles quelles."""
    if champs:
        description, ambiance, musique = prompt.separer_champs(prompt.nettoyer_markdown(en))
        texte, alertes = finir_plan(k, p, description, personnage_, source)
        return prompt.assembler_champs(texte, ambiance, musique), alertes
    texte, inventees = sans_repliques_inventees(source, sans_etiquette(prompt.nettoyer_markdown(en)))
    alertes = [f"Plan {k} : réplique inventée par Llama, retirée : « {r} »" for r in inventees]
    if p["enchainement"] == "prolonger" and not texte.lower().startswith("continuous shot"):
        texte = f"Continuous shot, same scene. {texte}"
    if p["enchainement"] in ("texte", "couper"):
        texte = avec_personnage(personnage_, texte)
    texte = garder_repliques(p["note"], texte)  # une réplique de la note est à sa place, mot pour mot
    texte = placer_replique(texte, p["replique"], p["moment"], p["off"])
    texte = vers_balises(texte)  # filet : une réplique que le LLM a écrite entre guillemets passe dans la balise
    return texte, alertes + alerte_replique(k, p)


def preparer_ecriture(synopsis: str, personnage_: str, decor: str, plans: list[dict], reference: str | None,
                      mineur: bool = False, contrainte: str = "", musique: str | None = None,
                      cible: int | None = None) -> dict:
    """Ce que le découpage, ✨ et 💬 d'un plan partagent : les entrées validées et passées par AS-18."""
    plans = _valider_decoupage(synopsis, personnage_, decor, plans, reference, cible)
    contrainte = " ".join(str(contrainte or "").split())
    if len(contrainte) > LONGUEUR_CONTRAINTE:
        raise ErreurSequence(f"Contrainte de {LONGUEUR_CONTRAINTE} caractères au plus")
    musique = " ".join(str(musique or "").split()) or None
    if len(musique or "") > LONGUEUR_MUSIQUE:
        raise ErreurSequence(f"Musique de {LONGUEUR_MUSIQUE} caractères au plus")
    if reference:
        personnage_ = ""  # l'image fait l'identité : la décrire ferait dériver H3 (essais du mode Séquence)
    rushes_ = [r for r in [reference, *(p["rush"] for p in plans)] if r]
    entrees = [synopsis, personnage_, decor, contrainte, musique or "", *(p["note"] for p in plans),
               *(p["replique"] for p in plans), *plateau.textes_origine(rushes_), *(["minor"] if mineur else [])]
    securite.verifier(*entrees)
    return {"plans": plans, "personnage": personnage_, "contrainte": contrainte, "musique": musique,
            "entrees": entrees, "source": source_repliques(synopsis, contrainte, plans),
            "demande": demande_decoupage(synopsis, personnage_, decor, plans, reference, contrainte, musique, cible),
            "regle_personnage": prompt.REGLE_AVEC_REFERENCE if reference else prompt.REGLE_SANS_REFERENCE}


async def decouper(orchestrateur, synopsis: str, personnage_: str, decor: str, plans: list[dict],
                   reference: str | None, llm: str | None = None, mineur: bool = False, contrainte: str = "",
                   musique: str | None = None, cible: int | None = None, contrainte_en: dict | None = None) -> dict:
    """Prompts des plans d'une séquence écrits par Llama, puis contrôlés ici : tous ensemble (✨ Tout écrire), ou
    un seul (`cible`, ✨ d'un plan), les autres servant de contexte.
    `mineur` : le personnage du Scénario est mineur (AS-18 le sait, même si aucun mot ne le dit).
    `contrainte` : impérative sur tous les plans ; la séquence créée la garde au casting, et le tournage l'ajoute
    au prompt de chaque plan. `contrainte_en` : sa traduction déjà faite ({fr, en}), reprise si elle vaut toujours."""
    e = preparer_ecriture(synopsis, personnage_, decor, plans, reference, mineur, contrainte, musique, cible)
    plans, contrainte = e["plans"], e["contrainte"]
    cibles = list(range(len(plans))) if cible is None else [cible]

    wf = workflows.obtenir(CONFIG.get("generation", {}).get("texte_vers_video"))
    mots_max = reglages.valeurs_workflow(wf).get("mots_max") or 110
    # Le LLM de ce workflow (onglet Modèles), sinon celui par défaut ; un spécialiste ne fait pas de découpage.
    llm = prompt.generaliste(llm or workflows.llm_de(wf.id))
    champs = workflows.champs_video(wf.id)
    systeme = prompt.consigne("decoupage", llm, mots_max, wf.profil.get("regles"), champs=champs)
    systeme = (systeme.replace("{regle_personnage}", e["regle_personnage"]) if "{regle_personnage}" in systeme
               else f"{systeme}\n\nCharacter: {e['regle_personnage']}")  # consigne retouchée sans le repère
    echanges = [{"role": "system", "content": systeme}, {"role": "user", "content": e["demande"]}]
    regle = reglages.valeurs_llm(llm)
    debut = time.perf_counter()
    async with orchestrateur.llm(llm) as llama:
        for essai in range(2):  # une réponse mal formée est redemandée une fois
            reponse = await llama.generer(echanges, temperature=regle["temperature_reecriture"],
                                          max_tokens=max(regle["max_tokens"], 260 * len(cibles) + 100),
                                          **reglages.echantillonnage(llm))
            try:
                blocs = lire_blocs(reponse.texte, [k + 1 for k in cibles], exact=cible is None)
                break
            except ErreurSequence:
                if essai:
                    raise

    alertes, sortie = [], []
    for k, (fr, en) in zip(cibles, blocs):
        p = plans[k]
        texte, alertes_plan = finir_plan(k + 1, p, en, e["personnage"], e["source"], champs)
        fr, _ = sans_repliques_inventees(e["source"], prompt.nettoyer_markdown(fr))
        alertes += alertes_plan
        if len(texte.split()) > mots_max + 20:
            alertes.append(f"Plan {k + 1} : {len(texte.split())} mots, au-delà des {mots_max} conseillés")
        sortie.append({**p, "idee": fr, "prompt": texte})
    tout = " ".join(p["prompt"] for p in sortie)
    if cible is None:
        for r in repliques(synopsis):  # une réplique du fil : le LLM la place dans le plan qu'il veut
            if _simple(r) not in _simple(tout):
                alertes.append(f"Réplique « {r} » absente du découpage : ajoute-la au plan voulu")
    # Ensemble : un mineur des textes d'origine reste un mineur, même si Llama l'a reformulé.
    securite.verifier(*e["entrees"], tout)
    # La contrainte en anglais, ajoutée telle quelle à chaque plan au tournage (où le LLM n'écrit plus).
    if not contrainte:
        contrainte_en = None
    elif (contrainte_en or {}).get("fr") == contrainte and str((contrainte_en or {}).get("en") or "").strip():
        contrainte_en = {"fr": contrainte, "en": " ".join(str(contrainte_en["en"]).split())[:LONGUEUR_CONTRAINTE * 2]}
        securite.verifier(contrainte, contrainte_en["en"], *e["entrees"])
    else:
        contrainte_en = {"fr": contrainte, "en": prompt.nettoyer_markdown(
            await prompt.traduire_precisions(orchestrateur, contrainte, llm))}
    return {"plans": sortie, "cible": cible, "alertes": alertes, "duree_s": round(time.perf_counter() - debut, 2),
            "reference": reference, "contrainte": contrainte, "contrainte_en": contrainte_en}


