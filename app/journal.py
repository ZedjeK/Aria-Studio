"""Journal des opérations : <dossier d'Aria>\\journal.json, les MAX dernières, la plus récente en premier (contrat des
applis Aria).

    [{"date": "2026-11-14T10:12:03", "action": "Modèle mis à la corbeille", "objet": "image/…/x.safetensors",
      "resultat": "ok", "detail": "…"}, …]

Ce qu'Aria fait sur ce qui t'appartient y est noté : téléchargements de modèles, installations et retours de moteurs,
corbeilles (mise, restauration, vidage), migrations (réglages, modèles), changements et restaurations de réglages.
Pas les générations : elles ont leurs propres fiches.

noter() ne lève jamais : un journal illisible ou impossible à écrire n'empêche aucune opération (il est alors mis de
côté et recommencé, ou l'échec est seulement écrit dans aria.log).
"""

import json
import logging
import shutil
import threading
from datetime import datetime
from pathlib import Path

from app import fichier_reglages

MAX = 500
VISIBLES = 50  # ce que montre Réglages → Système
RESULTATS = ("ok", "erreur", "partiel", "annule")
log = logging.getLogger(__name__)
_verrou = threading.Lock()


def fichier() -> Path:
    """À la racine du dossier d'Aria, à côté de reglages.json (mode portable compris)."""
    return fichier_reglages.RACINE_DE() / "journal.json"


def _lire(chemin: Path) -> list[dict]:
    try:
        entrees = json.loads(chemin.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return []
    except (OSError, ValueError):
        entrees = None
    if not isinstance(entrees, list):
        cote = chemin.with_name(f"journal-illisible-{datetime.now():%Y%m%d-%H%M%S}.json")
        try:
            shutil.copy2(chemin, cote)
            log.warning("%s illisible : mis de côté dans %s, un nouveau journal commence", chemin, cote)
        except OSError:
            pass
        return []
    return [e for e in entrees if isinstance(e, dict)]


def noter(action: str, objet: str = "", resultat: str = "ok", detail: str | None = None):
    entree = {"date": datetime.now().isoformat(timespec="seconds"), "action": action, "objet": str(objet or ""),
              "resultat": resultat if resultat in RESULTATS else "ok"}
    if detail:
        entree["detail"] = str(detail)[:500]
    try:
        with _verrou:
            chemin = fichier()
            entrees = [entree, *_lire(chemin)][:MAX]
            chemin.parent.mkdir(parents=True, exist_ok=True)
            temporaire = chemin.with_suffix(".tmp")
            temporaire.write_text(json.dumps(entrees, indent=1, ensure_ascii=False), encoding="utf-8")
            temporaire.replace(chemin)
    except OSError as e:
        log.warning("Journal : « %s » non noté (%s)", action, e)


def lire(n: int = VISIBLES) -> list[dict]:
    """Les n dernières opérations, la plus récente en premier."""
    with _verrou:
        try:
            entrees = json.loads(fichier().read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return []
    return [e for e in entrees if isinstance(e, dict)][:max(0, n)] if isinstance(entrees, list) else []
