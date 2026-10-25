"""Mesures matérielles : VRAM du GPU (NVML) et RAM d'un processus, échantillonnées en continu."""

import threading
import time

import psutil
import pynvml

_initialise = False


def _gpu():
    global _initialise
    if not _initialise:
        pynvml.nvmlInit()
        _initialise = True
    return pynvml.nvmlDeviceGetHandleByIndex(0)


def vram_utilisee() -> int:
    """Octets de VRAM occupés sur le GPU principal, tous processus confondus."""
    return pynvml.nvmlDeviceGetMemoryInfo(_gpu()).used


def vram_totale() -> int:
    return pynvml.nvmlDeviceGetMemoryInfo(_gpu()).total


def nom_gpu() -> str:
    nom = pynvml.nvmlDeviceGetName(_gpu())
    return nom.decode() if isinstance(nom, bytes) else nom


def _ram(pid: int) -> int:
    racine = psutil.Process(pid)
    total = racine.memory_info().rss
    for enfant in racine.children(recursive=True):
        try:
            total += enfant.memory_info().rss
        except psutil.Error:
            pass
    return total


class Sondeur:
    """Relève les pics de VRAM et de RAM pendant un bloc `with`, toutes les `intervalle` secondes."""

    def __init__(self, pid: int | None, intervalle: float = 0.1):
        self.pid = pid
        self.intervalle = intervalle
        self.pic_vram = 0
        self.pic_ram = 0
        self._stop = threading.Event()
        self._fil = threading.Thread(target=self._boucle, daemon=True)

    def _relever(self):
        self.pic_vram = max(self.pic_vram, vram_utilisee())
        if self.pid is not None:
            try:
                self.pic_ram = max(self.pic_ram, _ram(self.pid))
            except psutil.Error:
                pass

    def _boucle(self):
        while not self._stop.is_set():
            self._relever()
            time.sleep(self.intervalle)

    def __enter__(self):
        self._relever()
        self._fil.start()
        return self

    def __exit__(self, *_):
        self._stop.set()
        self._fil.join()
        self._relever()
