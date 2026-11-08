"""La réplique dans un prompt H3.

Le guide officiel de MiniMax (VIDEO_PROMPT_WRITING_GUIDE_base_en.md, §4.4 et §4.5) met chaque réplique dans une
balise, la langue en tête : `The young woman with a quiet voice (S1) says: <d>[French] Je descends ici.</d>`.
Les guillemets doubles y sont réservés au texte visible à l'écran (enseigne, panneau). Aria garde les guillemets
partout ailleurs (ce que tu écris, tes séquences déjà enregistrées) : ce module lit les deux, écrit la balise, et
convertit en dernier recours une réplique que le LLM aurait quand même écrite entre guillemets.

Aucune dépendance interne : sequences, decoupage, listes, prompt et workflows l'importent tous.
"""

import re

# Une réplique : la balise (le contenu, sans la langue), ou « », "", “” (ce que tu écris, les anciens prompts).
REPLIQUES = re.compile(r"<d>\s*(?:\[[^\]]*\]\s*)?(.+?)\s*</d>|«\s*([^»]+?)\s*»|\"([^\"]+)\"|“([^”]+)”", re.S | re.I)
UNE_REPLIQUE = re.compile(r"<d>|[«\"“]", re.I)
# Fin de phrase : le point peut être suivi de la balise fermante ou d'un guillemet fermant.
FIN_PHRASE = re.compile(r"[.!?…](?:</d>|[\"”»])?(?:\s|$)", re.I)

# Un guillemet qui suit un verbe de parole est une réplique ; derrière « a sign says », c'est un texte visible.
_PAROLE = re.compile(
    r"(\b(?:says?|saying|said|speaks?|replies|asks?|whispers?|murmurs?|shouts?|exclaims?|answers?|calls out)\b"
    r"(?P<entre>[^.!?\"«“<>]{0,80}?)[:,]?\s*)(?:«\s*([^»]+?)\s*»|\"([^\"]+)\"|“([^”]+)”)", re.I)
_TEXTE_VISIBLE = re.compile(r"\b(sign|signs|text|label|banner|neon|screen|poster|board|written|caption|subtitles?)\b",
                            re.I)


def langue(replique: str) -> str:
    """Langue d'une réplique d'après son écriture ; à l'essai, Ministral faisait parler en coréen une réplique française
    parce que le personnage est coréen. Écriture latine : le français, langue de l'utilisateur."""
    if re.search(r"[぀-ヿ]", replique):
        return "Japanese"
    if re.search(r"[가-힯]", replique):
        return "Korean"
    if re.search(r"[一-鿿]", replique):
        return "Mandarin Chinese"
    if re.search(r"[Ѐ-ӿ]", replique):
        return "Russian"
    return "French"


def balise(replique: str, langue_: str | None = None) -> str:
    """`<d>[Langue] réplique</d>`. Les chevrons sont retirés : une réplique ne referme pas la balise."""
    texte = " ".join(re.sub(r"[<>]", "", replique).split())
    return f"<d>[{langue_ or langue(texte)}] {texte}</d>"


def contenus(texte: str) -> list[str]:
    """Les répliques d'un texte, sans leurs guillemets ni leur balise."""
    return [next(g for g in m.groups() if g).strip() for m in REPLIQUES.finditer(texte)]


def vers_balises(texte: str) -> str:
    """Les répliques écrites entre guillemets derrière un verbe de parole (« says: "Salut" ») passent dans la balise.
    Un texte déjà en balise, ou un guillemet sans verbe de parole (une enseigne), n'est pas touché."""
    def remplacer(m: re.Match) -> str:
        # Le sujet du verbe est avant lui (« A neon sign above the door says "OPEN" ») : on remonte jusqu'à la
        # dernière ponctuation ou guillemet, pour ne pas confondre « sign saying "OPEN", she says "Salut" ».
        sujet = re.split(r"[.!?,;\"”»“«]", m.string[max(0, m.start() - 80):m.start()])[-1]
        if _TEXTE_VISIBLE.search(sujet + m.group("entre")):
            return m.group(0)
        dit = next(g for g in m.groups()[2:] if g)
        entre = m.group(1).rstrip(" ,")
        return f"{entre}: {balise(dit)}" if not entre.rstrip().endswith(":") else f"{entre} {balise(dit)}"

    return _PAROLE.sub(remplacer, texte)
