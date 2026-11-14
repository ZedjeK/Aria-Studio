"""Jeton tiré au hasard à chaque lancement d'Aria : il est exigé à chaque requête (contrat des applis Aria, « l'interface
ne s'ouvre que par l'appli »).

- Au démarrage (app/main.py, lancer), le serveur écrit le jeton dans cache/session.json ; il l'efface en s'arrêtant.
- Le lanceur (tools/launcher/Launcher.cs) le lit et ouvre « /?jeton=… » ; le serveur le range alors dans un cookie
  HttpOnly et SameSite=Strict (illisible par une page, jamais envoyé par un autre site) et redirige vers « / ».
- Chaque requête porte ce cookie, ou l'en-tête X-Aria-Jeton (tests, scripts). Seule /api/sante s'en passe : le
  lanceur s'en sert pour savoir si Aria tourne, et elle ne dit rien d'autre.

Limite, dite dans le README : un programme lancé sous le compte de l'utilisateur peut lire cache/session.json.
"""

import hmac
import json
import os
import secrets
from pathlib import Path

from app.emplacements import CACHE

JETON = secrets.token_urlsafe(32)  # un par processus : chaque lancement d'Aria en tire un nouveau
COOKIE = "aria_studio_jeton"  # les cookies ignorent le port : le nom évite de croiser une autre appli de 127.0.0.1
EN_TETE = "x-aria-jeton"
PARAMETRE = "jeton"
SANS_JETON = frozenset({"/api/sante"})
FICHIER = CACHE / "session.json"


def valide(valeur: str | None) -> bool:
    return bool(valeur) and hmac.compare_digest(str(valeur).encode(), JETON.encode())


def ecrire_session(port: int, fichier: Path | None = None) -> Path:
    """Pour le lanceur : le port et le jeton du serveur qui tourne."""
    fichier = fichier or FICHIER
    fichier.parent.mkdir(parents=True, exist_ok=True)
    temporaire = fichier.with_suffix(".tmp")
    temporaire.write_text(json.dumps({"port": port, "jeton": JETON, "pid": os.getpid()}), encoding="utf-8")
    temporaire.replace(fichier)
    return fichier


def effacer_session(fichier: Path | None = None):
    """À l'arrêt ; seulement si le fichier est encore le nôtre (un autre lancement a pu le réécrire)."""
    fichier = fichier or FICHIER
    try:
        if json.loads(fichier.read_text(encoding="utf-8")).get("jeton") == JETON:
            fichier.unlink()
    except (OSError, ValueError, AttributeError):
        pass


PAGE_SANS_JETON = """<!doctype html>
<html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Aria Studio</title>
<style>
body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #030812; color: #ddebff;
       font: 16px/1.6 "Space Grotesk", system-ui, sans-serif; }
main { max-width: 520px; margin: 24px 16px; padding: 24px; background: #06101f; border: 1px solid #7df9ff;
       clip-path: polygon(0 0, calc(100% - 16px) 0, 100% 16px, 100% 100%, 16px 100%, 0 calc(100% - 16px)); }
h1 { font-size: 20px; margin: 0 0 8px; }
p { color: #93a8c8; margin: 0; }
code { font-family: "IBM Plex Mono", Consolas, monospace; color: #7df9ff; }
</style></head>
<body><main>
<h1>Aria Studio s'ouvre par son lanceur</h1>
<p>Cette adresse vient d'un lancement précédent, ou d'ailleurs. Double-clique sur <code>Aria-Studio.exe</code> (ou son
raccourci) : l'interface se rouvre ici, avec un nouveau jeton.</p>
</main></body></html>
"""
