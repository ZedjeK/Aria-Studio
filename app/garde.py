"""Garde-VRAM : avant de donner la carte à un moteur d'Aria (ComfyUI ou Llama), vérifie que d'autres applis ne
l'occupent pas déjà. Sinon la demande attend, au lieu de déborder en mémoire partagée Windows (très lent).

Vu trois fois en une semaine (2026-10-22 à 25) : le llama-server d'Aria-Home ou d'Aria-Pilote tenait 9 à 13 Go.
Au repos, le bureau de Jérémy (Opera, Steam, Wallpaper Engine…) en occupe 1 à 2.

Windows (pilote WDDM) ne donne pas la VRAM de chaque processus : NVML répond « inconnu » pour tous (vérifié le
2026-10-25). La VRAM occupée se mesure donc globalement, nos moteurs déjà libérés ; le coupable, lui, se devine
(moteurs connus et gros consommateurs de RAM parmi les processus qui utilisent la carte) et s'annonce « probablement ».
Le garde ne ferme jamais un processus : c'est Jérémy qui décide. Il peut forcer le départ, ou annuler.
"""

import asyncio
import os
import time
from pathlib import PureWindowsPath

import psutil
import pynvml

from app import mesures
from app.config import CONFIG

GO = 2**30
REGLAGES = CONFIG.get("garde_vram", {})
SEUIL = float(REGLAGES.get("externe_max_go", 3.5)) * GO
INTERVALLE = float(REGLAGES.get("intervalle_s", 5))
# Moteurs d'IA qui se reconnaissent à leur nom ; au-delà, un gros consommateur de RAM est suspect.
MOTEURS = {"llama-server", "ollama", "ollama_llama_server", "lm studio", "lms", "koboldcpp", "python", "pythonw",
           "text-generation-launcher", "vllm", "comfyui"}
RAM_SUSPECTE = 2 * GO
DOSSIERS_REPERES = ("desktop", "bureau", "program files", "program files (x86)", "programs", "steamapps")


class ErreurGarde(RuntimeError):
    pass


def appli(exe: str) -> str | None:
    """Nom de l'appli d'après le dossier du programme : …\\Desktop\\Aria-Pilote\\…\\llama-server.exe → Aria-Pilote."""
    parties = PureWindowsPath(exe).parts[:-1]
    for i, partie in enumerate(parties[:-1]):
        if partie.lower() in DOSSIERS_REPERES:
            suivante = parties[i + 1]
            return "Steam" if suivante.lower() == "common" else suivante
    return parties[-1] if parties else None


def pids_aria() -> set[int]:
    """Ce serveur et tout ce qu'il a lancé (son ComfyUI, son Llama)."""
    moi = psutil.Process(os.getpid())
    return {moi.pid} | {p.pid for p in moi.children(recursive=True)}


def coupables(exclus: set[int]) -> list[dict]:
    """Processus qui utilisent la carte, hors Aria et hors Windows, susceptibles d'y tenir beaucoup de VRAM :
    les moteurs d'IA connus et les gros consommateurs de RAM, les plus gros d'abord."""
    suspects = []
    for p in pynvml.nvmlDeviceGetComputeRunningProcesses(mesures._gpu()):
        if p.pid in exclus:
            continue
        try:
            proc = psutil.Process(p.pid)
            nom, exe, ram = proc.name().removesuffix(".exe"), proc.exe(), proc.memory_info().rss
        except psutil.Error:
            continue
        if exe.lower().startswith(os.environ.get("SystemRoot", r"C:\Windows").lower()):
            continue
        if nom.lower() in MOTEURS or ram >= RAM_SUSPECTE:
            origine = appli(exe)
            suspects.append({"pid": p.pid, "nom": f"{nom} ({origine})" if origine else nom,
                             "ram_go": round(ram / GO, 1)})
    return sorted(suspects, key=lambda s: -s["ram_go"])


class Garde:
    def __init__(self, seuil: float = SEUIL, intervalle: float = INTERVALLE):
        self.seuil, self.intervalle = seuil, intervalle
        self.attente: dict | None = None  # ce que le bandeau affiche pendant une attente
        self.commande = None  # commande du Plateau en cours : son étape dit qu'elle attend la carte
        self._forcer = False
        self._annuler = False

    def forcer(self) -> bool:
        """« Lancer quand même » : la demande en attente part, lentement s'il le faut."""
        self._forcer = self.attente is not None
        return self._forcer

    def annuler(self) -> bool:
        self._annuler = self.attente is not None
        return self._annuler

    async def attendre(self, moteur: str, exclus: set[int] | None = None):
        """Rend la main dès que la VRAM occupée (nos moteurs déjà libérés) passe sous le seuil, ou si Jérémy force ;
        lève ErreurGarde s'il annule. Sans GPU NVIDIA lisible, ne bloque rien."""
        self._forcer = self._annuler = False
        etape = self.commande.etape if self.commande is not None else None
        debut = time.monotonic()
        try:
            while True:
                try:
                    occupee = mesures.vram_utilisee()
                except pynvml.NVMLError:
                    return
                if occupee <= self.seuil or self._forcer:
                    return
                if self._annuler:
                    raise ErreurGarde("Annulé : la carte graphique était occupée par une autre appli")
                try:
                    suspects = coupables(exclus if exclus is not None else pids_aria())
                except (pynvml.NVMLError, psutil.Error):
                    suspects = []
                self.attente = {"moteur": moteur, "vram_go": round(occupee / GO, 1),
                                "seuil_go": round(self.seuil / GO, 1), "coupables": suspects,
                                "depuis_s": round(time.monotonic() - debut)}
                if self.commande is not None:
                    self.commande.etape = (f"⏸ En attente de la carte : {occupee / GO:.1f} Go occupés hors Aria"
                                           + (f" ({suspects[0]['nom']}, probablement)" if suspects else ""))
                fin = time.monotonic() + self.intervalle  # revérifie, en réagissant vite à « forcer » ou « annuler »
                while time.monotonic() < fin and not (self._forcer or self._annuler):
                    await asyncio.sleep(0.2)
        finally:
            self.attente = None
            if self.commande is not None and etape is not None:
                self.commande.etape = etape


_cache: dict = {"quand": 0.0, "valeur": None}


def alerte(actif: str | None) -> dict | None:
    """Pastille de la barre d'onglets : y a-t-il, en ce moment, une autre appli qui tient la carte ?
    Un moteur d'IA connu hors Aria suffit ; sans moteur d'Aria chargé, la VRAM occupée au-delà du seuil aussi.
    Relu au plus toutes les 5 s (la barre interroge le serveur toutes les 3 s)."""
    if time.monotonic() - _cache["quand"] < 5:
        return _cache["valeur"]
    try:
        suspects = [s for s in coupables(pids_aria()) if s["nom"].split(" (")[0].lower() in MOTEURS]
        occupee = mesures.vram_utilisee()
    except (pynvml.NVMLError, psutil.Error):
        suspects, occupee = [], 0
    trop = actif is None and occupee > SEUIL
    valeur = {"coupables": suspects, "vram_go": round(occupee / GO, 1)} if suspects or trop else None
    _cache.update(quand=time.monotonic(), valeur=valeur)
    return valeur
