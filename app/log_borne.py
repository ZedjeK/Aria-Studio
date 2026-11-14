"""aria.log borné : au-delà de LIMITE octets, il devient aria.log.1 (le précédent est remplacé) et repart à zéro.

Lancée par pythonw, Aria n'a pas de console : app/__main__.py y renvoie stdout et stderr. Sans limite, le fichier
grossissait d'une session à l'autre. Au plus deux fichiers, donc 2 × LIMITE sur le disque.
"""

import io
import os
import threading
from pathlib import Path

LIMITE = 5 * 1024 * 1024


class LogBorne(io.TextIOBase):
    def __init__(self, chemin: Path, limite: int = LIMITE):
        self.chemin, self.limite = Path(chemin), limite
        self._verrou = threading.Lock()
        self.chemin.parent.mkdir(parents=True, exist_ok=True)
        if self.chemin.is_file() and self.chemin.stat().st_size > self.limite:
            self._tourner_fichier()
        self._f = open(self.chemin, "a", encoding="utf-8", buffering=1)

    def _tourner_fichier(self):
        try:
            os.replace(self.chemin, self.chemin.with_name(self.chemin.name + ".1"))
        except OSError:
            pass  # ouvert ailleurs (un éditeur) : on continue d'écrire au bout, on réessaiera

    def writable(self) -> bool:
        return True

    def write(self, texte: str) -> int:
        with self._verrou:
            n = self._f.write(texte)
            if self._f.tell() > self.limite:
                self._f.close()
                self._tourner_fichier()
                self._f = open(self.chemin, "a", encoding="utf-8", buffering=1)
            return n

    def flush(self):
        with self._verrou:
            if not self._f.closed:
                self._f.flush()

    def close(self):
        if self.closed:
            return
        self.flush()
        with self._verrou:
            self._f.close()
        super().close()  # IOBase.close appelle flush(), qui ne touche plus au fichier fermé
