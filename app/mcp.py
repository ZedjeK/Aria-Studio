"""Serveur MCP d'Aria - Studio : Claude (Claude Code, Claude Desktop) pilote Aria par ses outils.

    claude mcp add --scope user aria-studio -- "<python.exe d'Aria>" "<dossier d'Aria>\\app\\mcp.py"

La commande exacte est dans Réglages → Système, carte « Claude (MCP) ». Claude lance ce script lui-même et lui parle
sur l'entrée et la sortie standard (transport stdio : un message JSON-RPC 2.0 par ligne ; spécification MCP 2025-11-25,
relue le 2026-11-19). Il n'ouvre aucun port et n'a aucun secret : il lit le port et le jeton du lancement en cours
dans cache\\session.json, comme le lanceur, et passe par l'API locale d'Aria. Tout ce que l'interface contrôle l'est
donc ici aussi, filtre de protection des mineurs compris. Aria doit être lancée.

Outils, volontairement limités : l'état, écrire un prompt, lancer un rendu, le suivre, lire la fiche d'une image.
Rien ne supprime, ne déplace, ne change un réglage ni n'installe. Les images ne sont pas envoyées à Claude : il reçoit
leur chemin et leur fiche (texte).

Écrit sans le SDK MCP : le transport stdio et les trois méthodes utiles tiennent ici, sans dépendance à télécharger.
"""

import json
import sys
import urllib.error
import urllib.request
from pathlib import Path

if __package__ in (None, ""):  # lancé comme script (la commande de Claude) : le dossier d'Aria devient importable
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from app import VERSION  # noqa: E402
from app.emplacements import CACHE, DATA  # noqa: E402

PROTOCOLE = "2025-11-25"
PROTOCOLES = ("2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05")  # même forme pour les outils
SESSION = CACHE / "session.json"
CREATIONS = DATA / "creations"
DELAI_S = 600  # écrire un prompt en réfléchissant prend une à deux minutes ; charger un modèle aussi
ARIA_ABSENTE = "Aria Studio n'est pas lancée : ouvre-la (raccourci ou Aria-Studio.exe), puis réessaie."

OUTILS = [
    {"name": "etat",
     "description": "État d'Aria Studio : moteurs (ComfyUI, llama), VRAM, rendu en cours, modèles d'image "
                    "prêts (identifiants à passer à generer_image et ecrire_prompt), modèle par défaut et formats.",
     "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False}},
    {"name": "ecrire_prompt",
     "description": "✨ Écrire le prompt : le LLM local d'Aria réécrit une idée (souvent en français) en prompt anglais "
                    "pour un modèle d'image, avec les règles de sa fiche. Peut prendre une à deux minutes.",
     "inputSchema": {"type": "object", "additionalProperties": False, "required": ["idee"], "properties": {
         "idee": {"type": "string", "description": "L'idée de l'image, en langage courant (2 000 caractères au plus)."},
         "modele": {"type": "string", "description": "Identifiant du modèle d'image (voir etat). Absent : celui par défaut."}}}},
    {"name": "generer_image",
     "description": "Lance un rendu d'image dans Aria (Plateau) avec ce prompt, tel quel. Rend l'identifiant de la "
                    "commande, à suivre avec suivre_rendu. Les images rejoignent les Rushes d'Aria.",
     "inputSchema": {"type": "object", "additionalProperties": False, "required": ["prompt"], "properties": {
         "prompt": {"type": "string", "description": "Le prompt, tel qu'il sera rendu (en anglais, idéalement écrit par ecrire_prompt)."},
         "idee": {"type": "string", "description": "L'idée d'origine, notée sur la fiche de l'image."},
         "modele": {"type": "string", "description": "Identifiant du modèle d'image (voir etat). Absent : celui par défaut."},
         "format": {"type": "string", "description": "Format exact, tel que etat le liste (ex. « 1:1 (Square) »)."},
         "seed": {"type": "integer", "description": "Seed à rejouer. Absent : tirée au hasard."},
         "nombre": {"type": "integer", "minimum": 1, "maximum": 4, "description": "Nombre d'images (1 à 4)."}}}},
    {"name": "suivre_rendu",
     "description": "Avancement d'un rendu lancé par generer_image ; une fois terminé, l'identifiant et le chemin de "
                    "chaque image produite.",
     "inputSchema": {"type": "object", "additionalProperties": False, "required": ["commande"], "properties": {
         "commande": {"type": "string", "description": "Identifiant rendu par generer_image."}}}},
    {"name": "fiche_image",
     "description": "La fiche d'une image d'Aria (texte) : idée, prompt, modèle, format, seed, date.",
     "inputSchema": {"type": "object", "additionalProperties": False, "required": ["image"], "properties": {
         "image": {"type": "string", "description": "Identifiant de l'image (rendu par suivre_rendu)."}}}},
]


class ErreurOutil(Exception):
    """Une erreur à rendre à Claude dans le résultat de l'outil (isError), pas une erreur du protocole."""


def _session(fichier: Path | None = None) -> dict:
    try:
        session = json.loads((fichier or SESSION).read_text(encoding="utf-8"))
        return {"port": int(session["port"]), "jeton": str(session["jeton"])}
    except (OSError, ValueError, KeyError, TypeError):
        raise ErreurOutil(ARIA_ABSENTE) from None


def appeler(methode: str, chemin: str, corps: dict | None = None) -> dict:
    """Une requête à l'API locale d'Aria, avec le jeton du lancement en cours. Le message d'erreur d'Aria (« detail »)
    est rendu tel quel à Claude."""
    session = _session()
    donnees = json.dumps(corps).encode() if corps is not None else None
    requete = urllib.request.Request(f"http://127.0.0.1:{session['port']}{chemin}", data=donnees, method=methode,
                                     headers={"X-Aria-Jeton": session["jeton"], "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(requete, timeout=DELAI_S) as r:
            return json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        try:
            detail = json.loads(e.read()).get("detail")
        except (ValueError, AttributeError):
            detail = None
        raise ErreurOutil(str(detail or f"Aria a refusé la demande ({e.code})")) from None
    except (urllib.error.URLError, ConnectionError, TimeoutError):
        raise ErreurOutil(ARIA_ABSENTE) from None


def _etat(_: dict) -> dict:
    etat, options = appeler("GET", "/api/etat"), appeler("GET", "/api/plateau/options")
    images = options["workflows"]["texte_vers_image"]
    return {"moteurs": {"comfyui": etat["comfyui"]["etat"], "llama": etat["llama"]["etat"],
                        "llm_charge": etat["llama"].get("modele")},
            "vram_go": {"utilisee": etat.get("vram_utilisee_go"), "totale": etat.get("vram_totale_go")},
            "rendu_en_cours": etat.get("commande"),
            "modeles_image": [{"id": w["id"], "nom": w["nom"]} for w in images["choix"]],
            "modele_par_defaut": images["defaut"], "formats": options["formats"], "nombre_max": 4}


def _ecrire_prompt(a: dict) -> dict:
    r = appeler("POST", "/api/prompt/optimiser", {"atelier": "image", "idee": a["idee"], "workflow": a.get("modele")})
    return {"prompt": r["prompt"], "mots": r.get("mots"), "llm": r.get("modele"), "duree_s": r.get("duree_s")}


def _generer_image(a: dict) -> dict:
    corps = {"prompt": a["prompt"], "idee": a.get("idee", ""), "workflow": a.get("modele"),
             "seed": a.get("seed"), "nombre": a.get("nombre", 1)}
    if a.get("format"):
        corps["format"] = a["format"]
    if not 1 <= int(corps["nombre"]) <= 4:
        raise ErreurOutil("nombre : de 1 à 4 images")
    c = appeler("POST", "/api/creations", corps)
    return {"commande": c["id"], "etat": c["etat"], "modele": c.get("workflow"), "format": c.get("format"),
            "nombre": c.get("nombre")}


def _suivre_rendu(a: dict) -> dict:
    c = appeler("GET", f"/api/commandes/{_identifiant(a['commande'])}")
    sortie = {"commande": c["id"], "etat": c["etat"], "etape": c.get("etape"),
              "progression": round(float(c.get("progression") or 0), 2)}
    if c.get("erreur"):
        sortie["erreur"] = c["erreur"]
    if c.get("images"):
        sortie["images"] = [{"id": i, "chemin": str(CREATIONS / f"{i}.png")} for i in c["images"]]
    return sortie


def _fiche_image(a: dict) -> dict:
    return appeler("GET", f"/api/creations/{_identifiant(a['image'])}")


def _identifiant(valeur) -> str:
    """Un identifiant d'Aria (lettres, chiffres, tirets, soulignés) : rien qui change le chemin de la requête."""
    valeur = str(valeur)
    if not valeur or not all(c.isascii() and (c.isalnum() or c in "-_") for c in valeur):
        raise ErreurOutil(f"Identifiant invalide : {valeur!r}")
    return valeur


ACTIONS = {"etat": _etat, "ecrire_prompt": _ecrire_prompt, "generer_image": _generer_image,
           "suivre_rendu": _suivre_rendu, "fiche_image": _fiche_image}


def _resultat(contenu, erreur: bool = False) -> dict:
    texte = contenu if isinstance(contenu, str) else json.dumps(contenu, ensure_ascii=False, indent=1)
    return {"content": [{"type": "text", "text": texte}], "isError": erreur}


def traiter(message: dict) -> dict | None:
    """La réponse à un message JSON-RPC (None pour une notification)."""
    methode, ident = message.get("method"), message.get("id")
    if ident is None:  # notification (notifications/initialized…) : pas de réponse
        return None

    def ok(resultat):
        return {"jsonrpc": "2.0", "id": ident, "result": resultat}

    def erreur(code, texte):
        return {"jsonrpc": "2.0", "id": ident, "error": {"code": code, "message": texte}}

    params = message.get("params") or {}
    if methode == "initialize":
        demande = params.get("protocolVersion")
        return ok({"protocolVersion": demande if demande in PROTOCOLES else PROTOCOLE,
                   "capabilities": {"tools": {"listChanged": False}},
                   "serverInfo": {"name": "aria-studio", "title": "Aria Studio", "version": VERSION},
                   "instructions": "Aria Studio génère des images en local sur la carte graphique de l'utilisateur. "
                                   "Commence par etat ; écris le prompt avec ecrire_prompt, lance generer_image, puis "
                                   "suis le rendu avec suivre_rendu."})
    if methode == "ping":
        return ok({})
    if methode == "tools/list":
        return ok({"tools": OUTILS})
    if methode == "tools/call":
        nom, arguments = params.get("name"), params.get("arguments") or {}
        if nom not in ACTIONS:
            return erreur(-32602, f"Outil inconnu : {nom}")
        if not isinstance(arguments, dict):
            return erreur(-32602, "arguments : un objet JSON")
        schema = next(o["inputSchema"] for o in OUTILS if o["name"] == nom)
        if manque := [c for c in schema.get("required", []) if not arguments.get(c)]:
            return ok(_resultat(f"Paramètre manquant : {', '.join(manque)}", True))
        if inconnus := sorted(set(arguments) - set(schema["properties"])):
            return ok(_resultat(f"Paramètre inconnu : {', '.join(inconnus)}", True))
        try:
            return ok(_resultat(ACTIONS[nom](arguments)))
        except ErreurOutil as e:
            return ok(_resultat(str(e), True))
        except (ValueError, TypeError, KeyError) as e:
            return ok(_resultat(f"Réponse inattendue d'Aria : {e}", True))
    return erreur(-32601, f"Méthode inconnue : {methode}")


def servir(entree=None, sortie=None):
    """Lit un message par ligne jusqu'à la fin de l'entrée ; écrit une réponse par ligne. Rien d'autre ne va sur la
    sortie standard : les journaux vont sur la sortie d'erreur."""
    entree = entree or sys.stdin
    sortie = sortie or sys.stdout
    for ligne in entree:
        if not ligne.strip():
            continue
        try:
            message = json.loads(ligne)
        except ValueError:
            reponse = {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "JSON illisible"}}
        else:
            if not isinstance(message, dict):
                reponse = {"jsonrpc": "2.0", "id": None, "error": {"code": -32600, "message": "Requête invalide"}}
            else:
                try:
                    reponse = traiter(message)
                except Exception as e:  # le serveur reste en vie : l'erreur part en réponse et dans le journal
                    print(f"aria-studio mcp : {e!r}", file=sys.stderr, flush=True)
                    reponse = {"jsonrpc": "2.0", "id": message.get("id"),
                               "error": {"code": -32603, "message": "Erreur interne du serveur MCP d'Aria"}}
        if reponse is not None:
            sortie.write(json.dumps(reponse, ensure_ascii=False) + "\n")
            sortie.flush()


if __name__ == "__main__":
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    servir()
