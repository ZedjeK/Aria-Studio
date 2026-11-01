"""Point d'entrée « python -m app », celui du lanceur Aria-Studio.exe.

Lancé par pythonw, Aria n'a pas de console : tout ce qu'elle écrit, erreurs au chargement comprises, part dans
data/logs/aria.log (le lanceur y renvoie si le serveur ne répond pas).
"""

import sys

if sys.stdout is None:
    from app.emplacements import DATA

    journal = DATA / "logs" / "aria.log"
    journal.parent.mkdir(parents=True, exist_ok=True)
    sys.stdout = sys.stderr = open(journal, "a", encoding="utf-8", buffering=1)

from app.main import lancer  # noqa: E402

lancer()
