"""Point d'entrée « python -m app », celui du lanceur Aria-Studio.exe.

Lancé par pythonw, Aria n'a pas de console : tout ce qu'elle écrit, erreurs au chargement comprises, part dans
data/logs/aria.log (le lanceur y renvoie si le serveur ne répond pas), borné à 5 Mo, le précédent gardé en aria.log.1
(app/log_borne.py).
"""

import sys

if sys.stdout is None:
    from app.emplacements import DATA
    from app.log_borne import LogBorne

    sys.stdout = sys.stderr = LogBorne(DATA / "logs" / "aria.log")

from app.main import lancer  # noqa: E402

lancer()
