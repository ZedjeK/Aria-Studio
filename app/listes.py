"""Listes de l'onglet Prompt (prompt/listes/*.md) : lecture, règles et assemblage du prompt.

Chaque fichier est un bloc : des sous-listes en tableaux Markdown (Valeur | Prompt image | Remplace | Adulte | Genre)
suivies d'une section « Incompatibilités ». Les fichiers sont relus dès qu'ils changent : modifier une liste suffit.

Règles appliquées à l'assemblage (l'interface les applique aussi, mais c'est ici qu'elles font foi) :
    Remplace         la valeur prend la place des sous-listes indiquées (une robe remplace le Bas) ;
    Incompatibilités deux valeurs incompatibles sont refusées ensemble ;
    Genre            une valeur « femme » ou « homme » n'est proposée qu'à ce genre (androgyne : toutes) ;
    Avec <liste>     colonne « Avec Silhouette », « Avec Longueur des cheveux »… : la valeur n'est proposée qu'avec
                     les valeurs indiquées de cette autre sous-liste du même fichier (la poitrine selon la
                     silhouette, la coiffure selon la longueur) ;
    Adulte           une valeur adulte est refusée pour un personnage mineur (ERREUR AS-18), puis le prompt
                     final passe par securite.verifier comme tous les autres.

Aides au choix, appliquées par l'interface seulement (pas des règles) :
    Famille / Origines  l'origine choisie (ses familles) ne propose que le teint, les yeux et le nez qui lui
                        correspondent ; la case « Toutes les options » lève le filtre ;
    Description         une phrase affichée sous le menu (Silhouette).
"""

import re
from dataclasses import dataclass, field
from pathlib import Path

from app import reglages, securite, workflows
from app.config import ROOT

DOSSIER = ROOT / "prompt" / "listes"

SUJETS = {
    "personnage": {"nom": "Personnage", "blocs": ["personnage", "mise-en-scene", "commun"], "defaut": "person"},
    "creature": {"nom": "Créature / animal", "blocs": ["creature", "mise-en-scene", "commun"], "defaut": "creature"},
    "objet": {"nom": "Objet", "blocs": ["objet", "commun"], "defaut": "object"},
    "lieu": {"nom": "Lieu / paysage", "blocs": ["lieu", "mise-en-scene", "commun"], "defaut": "landscape"},
}
EXCLUES = {("lieu", "mise-en-scene"): {"Lieu"}}  # pour un sujet Lieu, le lieu est le sujet lui-même
PLAN = "video"  # bloc ajouté à tous les sujets quand le modèle cible est un modèle vidéo

# Accordéons de l'interface ; une sous-liste absente d'ici tombe dans « Autres ».
GROUPES = {
    "personnage": {
        "Identité": ["Genre", "Âge", "Origine"],
        "Corps": ["Silhouette", "Taille", "Poitrine", "Hanches", "Teint", "Signe particulier", "Motif de tatouage"],
        "Visage": ["Forme du visage", "Forme des yeux", "Couleur des yeux", "Expression du regard",
                   "Expression du visage", "Nez", "Lèvres", "Sourcils", "Pilosité du visage", "Trait marquant"],
        "Cheveux": ["Couleur des cheveux", "Longueur des cheveux", "Texture des cheveux", "Coiffure"],
        "Tenue": ["Style vestimentaire", "Haut", "Bas", "Chaussures", "Tenue intime", "Accessoires", "Maquillage"],
        "Attitude": ["Pose", "Action"],
    },
    PLAN: {"Caméra": ["Caméra"], "Voix": ["Langue", "Ton"], "Son": ["Ambiance sonore", "Musique"]},
}
# Sous-liste proposée seulement si une autre sous-liste a une valeur contenant ce texte.
CONDITIONS = {("personnage", "Motif de tatouage"): ("Signe particulier", "tatou")}
VETEMENTS = {"Haut", "Bas", "Chaussures", "Tenue intime"}  # réunis en « wearing … »
STYLE = ("commun", "Style")  # placé en tête du prompt
AGE_PRECIS = (3, 120)
LONGUEUR_TEXTE = 200
LONGUEUR_PRECISIONS = 500
LONGUEUR_ACTION = 500
LONGUEUR_REPLIQUE = 200

_COLONNES = {
    "Valeur": "nom", "Prompt image": "prompt", "Prompt vidéo": "prompt", "Remplace": "remplace", "Adulte": "adulte", "Genre": "genre",
    "Mineur": "mineur", "Prompt femme": "femme", "Prompt homme": "homme", "Prompt androgyne": "androgyne",
    "Famille": "famille", "Origines": "origines", "Description": "description", "Catégorie": "categorie",
}
AVEC = "Avec "  # « Avec <sous-liste> » : colonne de cohérence avec une autre sous-liste du même fichier
FAMILLES = ("europe", "asie-est", "asie-sud-est", "asie-sud", "moyen-orient", "afrique", "ameriques", "oceanie")
_BORNES = [(re.compile(r"\((\d+)-(\d+) ans\)"), None), (re.compile(r"\((\d+) ans et plus\)"), AGE_PRECIS[1])]


class ErreurListes(ValueError):
    pass


@dataclass(frozen=True)
class Valeur:
    nom: str
    prompt: str = ""
    remplace: tuple[str, ...] = ()
    adulte: bool = False
    genre: str | None = None
    prompts_genre: dict | None = None  # Âge : prompt selon le genre (femme / homme / androgyne)
    mineur: bool = False
    bornes: tuple[int, int] | None = None  # Âge : tranche lue dans « (18-25 ans) »
    familles: tuple[str, ...] = ()  # Origine : familles de cette origine
    origines: tuple[str, ...] = ()  # familles pour lesquelles la valeur est proposée ; vide = toutes
    description: str = ""
    avec: tuple[tuple[str, tuple[str, ...]], ...] = ()  # ((sous-liste, valeurs cohérentes), …) ; absente = toutes
    categorie: str = ""  # groupe du menu (Pose, Action)

    def public(self) -> dict:
        d = {"nom": self.nom, "prompt": self.prompt, "remplace": list(self.remplace), "adulte": self.adulte,
             "genre": self.genre, "familles": list(self.familles), "origines": list(self.origines),
             "description": self.description, "avec": {sl: list(noms) for sl, noms in self.avec},
             "categorie": self.categorie}
        if self.prompts_genre:
            d.update(prompts_genre=self.prompts_genre, mineur=self.mineur, bornes=list(self.bornes or ()))
        return d


@dataclass
class SousListe:
    nom: str
    note: str = ""
    valeurs: dict[str, Valeur] = field(default_factory=dict)


@dataclass
class Bloc:
    id: str
    titre: str
    sous_listes: dict[str, SousListe]
    incompatibilites: list[tuple[tuple[str, str], tuple[str, str]]]


# Lecture des fichiers

def _cellules(ligne: str) -> list[str]:
    return [c.strip() for c in ligne.strip().strip("|").split("|")]


def _oui(texte: str, ou: str) -> bool:
    if texte not in ("", "oui"):
        raise ErreurListes(f"{ou} : « {texte} » au lieu de « oui » ou vide")
    return texte == "oui"


def _familles(texte: str, ou: str) -> tuple[str, ...]:
    familles = tuple(f.strip() for f in texte.split(",") if f.strip())
    if inconnues := [f for f in familles if f not in FAMILLES]:
        raise ErreurListes(f"{ou} : famille « {inconnues[0]} » inconnue ({', '.join(FAMILLES)})")
    return familles


def _valeur(entete: list[str], cellules: list[str], ou: str) -> Valeur:
    if len(cellules) != len(entete):
        raise ErreurListes(f"{ou} : {len(cellules)} colonnes au lieu de {len(entete)}")
    c = dict(zip(entete, cellules))
    if not c.get("nom"):
        raise ErreurListes(f"{ou} : valeur vide")
    genre = c.get("genre") or None
    if genre not in (None, "femme", "homme"):
        raise ErreurListes(f"{ou} : genre « {genre} » inconnu (femme, homme ou vide)")
    prompts_genre = {g: c[g] for g in ("femme", "homme", "androgyne") if g in c} or None
    bornes = None
    for motif, maximum in _BORNES:
        if m := motif.search(c["nom"]):
            bornes = (int(m.group(1)), maximum or int(m.group(2)))
    return Valeur(
        nom=c["nom"],
        prompt=c.get("prompt", ""),
        remplace=tuple(r.strip() for r in c.get("remplace", "").split(",") if r.strip()),
        adulte=_oui(c.get("adulte", ""), ou),
        genre=genre,
        prompts_genre=prompts_genre,
        mineur=_oui(c.get("mineur", ""), ou),
        bornes=bornes,
        familles=_familles(c.get("famille", ""), ou),
        origines=_familles(c.get("origines", ""), ou),
        description=c.get("description", ""),
        avec=tuple((cle.removeprefix(AVEC), noms) for cle, texte in c.items() if cle.startswith(AVEC)
                   if (noms := tuple(n.strip() for n in texte.split(",") if n.strip()))),
        categorie=c.get("categorie", ""),
    )


def _resoudre(cote: str, sous_listes: dict[str, SousListe], ou: str) -> tuple[str, str]:
    """« Sous-liste : Valeur » ou « Valeur » seule, qui doit alors être unique dans le fichier."""
    nom_sl, sep, valeur = cote.partition(" : ")
    if sep and nom_sl in sous_listes:
        if valeur not in sous_listes[nom_sl].valeurs:
            raise ErreurListes(f"{ou} : « {valeur} » absent de la sous-liste {nom_sl}")
        return nom_sl, valeur
    trouves = [sl.nom for sl in sous_listes.values() if cote in sl.valeurs]
    if not trouves:
        raise ErreurListes(f"{ou} : valeur inconnue « {cote} »")
    if len(trouves) > 1:
        raise ErreurListes(f"{ou} : « {cote} » est dans {', '.join(trouves)} ; précise « Sous-liste : {cote} »")
    return trouves[0], cote


def lire(chemin: Path) -> Bloc:
    lignes = chemin.read_text(encoding="utf-8").splitlines()
    debut = 0
    if lignes and lignes[0].strip() == "---":  # en-tête YAML
        debut = next((i + 1 for i in range(1, len(lignes)) if lignes[i].strip() == "---"), len(lignes))
    titre, sous_listes, incompatibilites_brutes = chemin.stem, {}, []
    courante: SousListe | None = None
    entete: list[str] | None = None
    dans_incompatibilites = False
    for numero, ligne in enumerate(lignes[debut:], start=debut + 1):
        ou = f"{chemin.name}:{numero}"
        texte = ligne.strip()
        if texte.startswith("# "):
            titre = texte[2:].strip()
        elif texte.startswith("## "):
            nom = texte[3:].strip()
            dans_incompatibilites = nom == "Incompatibilités"
            courante, entete = None, None
            if not dans_incompatibilites:
                if nom in sous_listes:
                    raise ErreurListes(f"{ou} : sous-liste « {nom} » en double")
                courante = sous_listes[nom] = SousListe(nom)
        elif dans_incompatibilites and texte.startswith("- "):
            if "↔" not in texte:
                raise ErreurListes(f"{ou} : incompatibilité sans « ↔ »")
            gauche, droite = (x.strip() for x in texte[2:].split("↔", 1))
            incompatibilites_brutes.append((gauche, droite, ou))
        elif courante and texte.startswith(">"):
            courante.note = f"{courante.note} {texte.lstrip('> ')}".strip()
        elif courante and texte.startswith("|"):
            cellules = _cellules(texte)
            if entete is None:
                inconnues = [c for c in cellules if c not in _COLONNES and not c.startswith(AVEC)]
                if inconnues:
                    raise ErreurListes(f"{ou} : colonne inconnue « {inconnues[0]} »")
                entete = [_COLONNES.get(c, c) for c in cellules]  # « Avec … » garde son nom
            elif not all(set(c) <= set("-: ") for c in cellules):  # pas la ligne de séparation
                valeur = _valeur(entete, cellules, ou)
                if valeur.nom in courante.valeurs:
                    raise ErreurListes(f"{ou} : valeur « {valeur.nom} » en double dans {courante.nom}")
                courante.valeurs[valeur.nom] = valeur
    for sl in sous_listes.values():
        if not sl.valeurs:
            raise ErreurListes(f"{chemin.name} : sous-liste « {sl.nom} » sans valeur")
        for v in sl.valeurs.values():
            for cible in v.remplace:
                if cible not in sous_listes:
                    raise ErreurListes(f"{chemin.name} : « {v.nom} » remplace « {cible} », sous-liste inconnue")
            for sl_ref, noms in v.avec:
                if sl_ref not in sous_listes:
                    raise ErreurListes(f"{chemin.name} : colonne « {AVEC}{sl_ref} », sous-liste inconnue")
                if inconnues := [n for n in noms if n not in sous_listes[sl_ref].valeurs]:
                    raise ErreurListes(f"{chemin.name} : « {v.nom} » ({sl.nom}) cite « {inconnues[0]} », "
                                       f"absent de {sl_ref}")
    incompatibilites = [(_resoudre(g, sous_listes, ou), _resoudre(d, sous_listes, ou))
                        for g, d, ou in incompatibilites_brutes]
    return Bloc(chemin.stem, re.sub(r"\s*\(.*\)$", "", titre), sous_listes, incompatibilites)


_cache: tuple[tuple, dict[str, Bloc]] | None = None


def blocs() -> dict[str, Bloc]:
    """Tous les blocs de prompt/listes, relus seulement si un fichier a changé."""
    global _cache
    fichiers = sorted(DOSSIER.glob("*.md"))
    empreinte = tuple((f.name, f.stat().st_mtime_ns) for f in fichiers)
    if _cache is None or _cache[0] != empreinte:
        _cache = (empreinte, {f.stem: lire(f) for f in fichiers})
    return _cache[1]


# Structure pour l'interface

def _sujet(sujet: str) -> dict:
    if sujet not in SUJETS:
        raise ErreurListes(f"Sujet inconnu : {sujet}")
    manquants = [b for b in SUJETS[sujet]["blocs"] if b not in blocs()]
    if manquants:
        raise ErreurListes(f"Liste absente de prompt/listes : {manquants[0]}.md")
    return SUJETS[sujet]


def _sous_listes(sujet: str, id_bloc: str) -> dict[str, SousListe]:
    exclues = EXCLUES.get((sujet, id_bloc), set())
    return {n: sl for n, sl in blocs()[id_bloc].sous_listes.items() if n not in exclues}


def sujets() -> list[dict]:
    return [{"id": i, "nom": s["nom"]} for i, s in SUJETS.items()]


def est_video(workflow: str | None) -> bool:
    return bool(workflow) and workflows.obtenir(workflow).fiche.get("tache") == "texte_vers_video"


def structure(sujet: str) -> dict:
    """Blocs, accordéons, valeurs et incompatibilités d'un sujet, pour l'interface.
    Le bloc Plan (vidéo) est toujours livré, marqué « video » : l'interface ne l'affiche que pour un modèle vidéo."""
    definition = _sujet(sujet)
    sortie, incompatibilites = [], []
    for id_bloc in definition["blocs"] + ([PLAN] if PLAN in blocs() else []):
        bloc = blocs()[id_bloc]
        sous_listes = _sous_listes(sujet, id_bloc)
        groupes = {nom: [n for n in noms if n in sous_listes] for nom, noms in GROUPES.get(id_bloc, {}).items()}
        ranges = {n for noms in groupes.values() for n in noms}
        reste = [n for n in sous_listes if n not in ranges]
        if reste:
            groupes["Autres" if groupes else bloc.titre] = reste

        def public(sl: SousListe) -> dict:
            condition = CONDITIONS.get((id_bloc, sl.nom))
            return {
                "nom": sl.nom,
                "note": sl.note,
                "adulte": all(v.adulte for v in sl.valeurs.values()),
                "condition": {"sous_liste": condition[0], "contient": condition[1]} if condition else None,
                "valeurs": [v.public() for v in sl.valeurs.values()],
            }

        sortie.append({
            "id": id_bloc,
            "titre": bloc.titre,
            "video": id_bloc == PLAN,
            "groupes": [{"nom": nom, "sous_listes": [public(sous_listes[n]) for n in noms]}
                        for nom, noms in groupes.items() if noms],
        })
        incompatibilites += [[[id_bloc, *g], [id_bloc, *d]] for g, d in bloc.incompatibilites
                             if g[0] in sous_listes and d[0] in sous_listes]
    return {"sujet": sujet, "nom": definition["nom"], "blocs": sortie, "incompatibilites": incompatibilites,
            "age_precis": list(AGE_PRECIS), "longueurs": {"action": LONGUEUR_ACTION, "replique": LONGUEUR_REPLIQUE}}


# Assemblage

def _article(texte: str) -> str:
    return f"{'an' if texte[:1].lower() in 'aeiou' else 'a'} {texte}"


def _majuscule(texte: str) -> str:
    return texte[:1].upper() + texte[1:]


def _selection(sujet: str, choix: dict, video: bool = False) -> dict[tuple[str, str], Valeur]:
    autorises = SUJETS[sujet]["blocs"] + ([PLAN] if video else [])
    selection = {}
    for id_bloc, valeurs in (choix or {}).items():
        if id_bloc not in autorises:
            raise ErreurListes(f"La liste « {id_bloc} » n'est pas proposée pour le sujet {SUJETS[sujet]['nom']}")
        sous_listes = _sous_listes(sujet, id_bloc)
        for nom_sl, nom_valeur in (valeurs or {}).items():
            if not nom_valeur:
                continue
            if nom_sl not in sous_listes:
                raise ErreurListes(f"Sous-liste inconnue : {nom_sl} ({id_bloc})")
            if nom_valeur not in sous_listes[nom_sl].valeurs:
                raise ErreurListes(f"Valeur inconnue : « {nom_valeur} » ({nom_sl})")
            selection[(id_bloc, nom_sl)] = sous_listes[nom_sl].valeurs[nom_valeur]
    return selection


def _tranche_d_age(age: int) -> Valeur:
    tranches = blocs()["personnage"].sous_listes["Âge"].valeurs.values()
    for t in tranches:
        if t.bornes and t.bornes[0] <= age <= t.bornes[1]:
            return t
    raise ErreurListes(f"Aucune tranche d'âge ne couvre {age} ans")


def _qui_parle(sujet: str, genre: str | None) -> str:
    if sujet == "personnage":
        return {"femme": "She says", "homme": "He says"}.get(genre, "They say")
    return "The creature says" if sujet == "creature" else "A voice-over says"


def assembler(sujet: str, choix: dict, age: int | None = None, texte: str = "", precisions: str = "",
              workflow: str | None = None, action: str = "", replique: str = "") -> dict:
    """Prompt anglais assemblé à partir des choix {bloc: {sous-liste: valeur}}, après contrôle des règles.
    Pour un modèle vidéo, le prompt devient un brief de production : rendu, sujet, action, caméra, réplique, sons.
    `action` arrive déjà traduite en anglais ; `replique` est gardée mot pour mot, dans sa langue."""
    definition = _sujet(sujet)
    video = est_video(workflow)
    choix = dict(choix or {})
    if not video:
        choix.pop(PLAN, None)  # le Plan reste en mémoire dans l'interface, mais ne compte que pour la vidéo
        action = replique = ""
    selection = _selection(sujet, choix, video)
    texte, precisions, action, replique = texte.strip(), precisions.strip(), action.strip(), replique.strip()
    alertes = []
    if video and texte:
        alertes.append("Texte dans l'image ignoré : il ne concerne que les images")
        texte = ""
    if len(texte) > LONGUEUR_TEXTE:
        raise ErreurListes(f"Texte dans l'image trop long (plus de {LONGUEUR_TEXTE} caractères)")
    if len(precisions) > LONGUEUR_PRECISIONS:
        raise ErreurListes(f"Précisions trop longues (plus de {LONGUEUR_PRECISIONS} caractères)")
    if len(action) > LONGUEUR_ACTION:
        raise ErreurListes(f"Action trop longue (plus de {LONGUEUR_ACTION} caractères)")
    if len(replique) > LONGUEUR_REPLIQUE:
        raise ErreurListes(f"Réplique trop longue (plus de {LONGUEUR_REPLIQUE} caractères)")

    # Genre et âge
    genre_v = selection.get(("personnage", "Genre"))
    genre = genre_v.nom.lower() if genre_v else None
    tranche = selection.get(("personnage", "Âge"))
    if age is not None:
        if sujet != "personnage":
            raise ErreurListes("L'âge précis ne concerne que les personnages")
        if not AGE_PRECIS[0] <= age <= AGE_PRECIS[1]:
            raise ErreurListes(f"Âge précis entre {AGE_PRECIS[0]} et {AGE_PRECIS[1]} ans")
        juste = _tranche_d_age(age)
        if tranche and tranche is not juste:
            alertes.append(f"Tranche d'âge ajustée à « {juste.nom} » pour {age} ans")
        tranche = selection[("personnage", "Âge")] = juste
    mineur = bool(tranche and tranche.mineur) or (age is not None and age < securite.AGE_MAJORITE)

    # Règles
    for (id_bloc, nom_sl), v in selection.items():
        if v.genre and genre in ("femme", "homme") and v.genre != genre:
            raise ErreurListes(f"« {v.nom} » ({nom_sl}) n'est pas proposé pour le genre {genre_v.nom}")
        for sl_ref, noms in v.avec:
            if (ref := selection.get((id_bloc, sl_ref))) and ref.nom not in noms:
                raise ErreurListes(f"« {v.nom} » ({nom_sl}) ne va pas avec « {ref.nom} » ({sl_ref})")
        if mineur and v.adulte:
            raise securite.ErreurSecurite(f"« {v.nom} » ({nom_sl}) est réservé aux personnages adultes")
    for id_bloc in definition["blocs"] + ([PLAN] if video else []):
        for (sl_g, val_g), (sl_d, val_d) in blocs()[id_bloc].incompatibilites:
            g, d = selection.get((id_bloc, sl_g)), selection.get((id_bloc, sl_d))
            if g and d and g.nom == val_g and d.nom == val_d:
                raise ErreurListes(f"« {val_g} » ({sl_g}) et « {val_d} » ({sl_d}) sont incompatibles")
    for (id_bloc, nom_sl), (sl_requise, contient) in CONDITIONS.items():
        if (id_bloc, nom_sl) in selection:
            requise = selection.get((id_bloc, sl_requise))
            if not requise or contient not in requise.nom.lower():
                del selection[(id_bloc, nom_sl)]
                alertes.append(f"{nom_sl} ignoré : il faut d'abord un {sl_requise.lower()} qui convient")
    for (id_bloc, nom_sl), v in list(selection.items()):
        for cible in v.remplace:
            if remplacee := selection.pop((id_bloc, cible), None):
                alertes.append(f"{cible} (« {remplacee.nom} ») remplacé par « {v.nom} »")

    # Sujet
    if sujet == "personnage":
        if tranche:
            nom = tranche.prompts_genre[genre or "androgyne"]
        else:
            nom = genre_v.prompt if genre_v else definition["defaut"]
        if age is not None:
            nom += f", {age} years old"
        ignorees = {"Genre", "Âge", "Motif de tatouage"}
    else:
        premiere = next(iter(_sous_listes(sujet, sujet)))  # Type, Catégorie, Type de lieu
        principal = selection.get((sujet, premiere))
        nom = principal.prompt if principal and principal.prompt else definition["defaut"]
        ignorees = {premiere}

    # Détails, dans l'ordre des fichiers
    details, rendu, vetements, position_vetements = [], [], [], None
    for id_bloc in definition["blocs"]:
        for nom_sl in _sous_listes(sujet, id_bloc):
            v = selection.get((id_bloc, nom_sl))
            if not v or not v.prompt or (id_bloc == sujet and nom_sl in ignorees) or (id_bloc, nom_sl) == STYLE:
                continue
            morceau = v.prompt
            if id_bloc == "personnage" and nom_sl in VETEMENTS:
                position_vetements = len(details) if position_vetements is None else position_vetements
                vetements.append(morceau)
                continue
            if id_bloc == "personnage" and nom_sl == "Signe particulier":
                if motif := selection.get(("personnage", "Motif de tatouage")):
                    morceau += f" featuring {_article(motif.prompt)}"
            elif id_bloc == "personnage" and nom_sl == "Accessoires":
                morceau = f"with {morceau}"
            (rendu if id_bloc == "commun" else details).append(morceau)
    if vetements:
        details.insert(position_vetements, "wearing " + ", ".join(vetements))

    def phrase(morceau: str) -> str:
        return _majuscule(morceau) + ("" if morceau[-1] in ".!?" else ".")

    phrases = []
    if style := selection.get(STYLE):
        phrases.append(_majuscule(style.prompt) + ".")
    sujet_phrase = _majuscule(", ".join([_article(nom), *details])) + "."
    if video:
        # Brief de production (bonnes pratiques H3) : le cadrage d'abord, puis le sujet, l'action, la caméra et le son.
        if rendu:
            phrases.append(_majuscule(", ".join(rendu)) + ".")
        phrases.append(sujet_phrase)
        if action:
            phrases.append(phrase(action))
        if precisions:
            phrases.append(phrase(precisions))
        plan = {nom_sl: v.prompt for (id_bloc, nom_sl), v in selection.items() if id_bloc == PLAN and v.prompt}
        if "Caméra" in plan:
            phrases.append(phrase(plan["Caméra"]))
        if replique:
            ton = f", {plan['Ton']}" if "Ton" in plan else ""
            phrases.append(f'{_qui_parle(sujet, genre)} in {plan.get("Langue", "French")}{ton}: '
                           f'"{replique.replace(chr(34), chr(39))}"')
        elif "Langue" in plan or "Ton" in plan:
            alertes.append("Langue et ton ignorés : écris d'abord une réplique")
        if "Ambiance sonore" in plan:
            phrases.append(f"Sounds: {plan['Ambiance sonore']}.")
        if musique := plan.get("Musique"):
            phrases.append(phrase(musique) if musique.startswith("no ") else f"Music: {musique}.")
    else:
        phrases.append(sujet_phrase)
        if rendu:
            phrases.append(_majuscule(", ".join(rendu)) + ".")
        if texte:
            phrases.append(f'The text "{texte.replace(chr(34), chr(39))}" appears in the image.')
        if precisions:
            phrases.append(phrase(precisions))
    prompt = " ".join(phrases)

    # Le mineur est signalé explicitement : AS-18 ne dépend pas de la façon dont la tranche est formulée.
    securite.verifier(prompt, *(["minor"] if mineur else []))

    mots = len(prompt.split())
    mots_max = None
    if workflow:
        mots_max = reglages.valeurs_workflow(workflows.obtenir(workflow)).get("mots_max")
        if mots_max and mots > mots_max:
            alertes.append(f"{mots} mots : au-delà de {mots_max}, ce modèle risque d'ignorer la fin du prompt")
    return {"prompt": prompt, "mots": mots, "mots_max": mots_max, "alertes": alertes, "mineur": mineur,
            "tranche": tranche.nom if tranche else None, "a_traduire": bool(precisions or action), "video": video}


# Scénario, mode Séquence : le personnage et le décor, séparés, pour le découpage en plans.
ATTITUDE = {"Pose", "Action", "Attitude"}  # change d'un plan à l'autre : pas dans la description du personnage
CAMERA = {"Cadrage", "Angle de vue", "Objectif"}  # choisis plan par plan par le découpage : pas dans le décor commun


def description_sequence(sujet: str, choix: dict, age: int | None = None) -> dict:
    """{personnage, decor, mineur} à partir des choix du Scénario, après contrôle de toutes les règles (AS-18 compris).
    Le personnage sert d'identité répétée dans les plans ; le décor (mise en scène, rendu) de toile de fond."""
    choix = {b: dict(v or {}) for b, v in (choix or {}).items() if b != PLAN}
    complet = assembler(sujet, choix, age)  # refuse ce que le Scénario refuserait (genre, adulte, incompatibilités…)
    propre = {sl: v for sl, v in choix.get(sujet, {}).items() if sl not in ATTITUDE}
    personnage = assembler(sujet, {sujet: propre} if propre else {}, age)["prompt"] if propre or age is not None else ""
    selection = _selection(sujet, {b: v for b, v in choix.items() if b != sujet})
    style = selection.pop(STYLE, None)
    morceaux = [v.prompt for (id_bloc, nom_sl), v in selection.items() if v.prompt and nom_sl not in CAMERA]
    decor = " ".join(x for x in [_majuscule(style.prompt) + "." if style and style.prompt else "",
                                 _majuscule(", ".join(morceaux)) + "." if morceaux else ""] if x)
    return {"personnage": personnage, "decor": decor, "mineur": complet["mineur"]}
