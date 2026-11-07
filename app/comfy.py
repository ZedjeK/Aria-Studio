"""Pilote du ComfyUI portable : processus, VRAM, exécution de workflows au format API."""

import asyncio
import json
import os
import socket
import subprocess
import uuid
from pathlib import Path
from typing import Callable

import httpx
import psutil
import websockets

from app import garde
from app.emplacements import DATA

LOGS = DATA / "logs"
# Fichiers téléversés dans input/ : images, vidéos (LoadVideo) et sons (LoadAudio, voix de référence).
TYPES_MIME = {"mp4": "video/mp4", "flac": "audio/flac", "wav": "audio/wav", "mp3": "audio/mpeg", "ogg": "audio/ogg"}


class ErreurComfy(RuntimeError):
    pass


def _chemin(p: Path | str) -> str:
    return os.path.normcase(os.path.realpath(p))



def video_et_affiche(fichiers: list[tuple[str, bytes]]) -> tuple[bytes | None, bytes | None]:
    """La vidéo (.mp4) et son affiche (.png) parmi les fichiers rendus par ComfyUI ; None pour ce qui manque."""
    def trouver(extension: str) -> bytes | None:
        return next((octets for nom, octets in fichiers if nom.lower().endswith(extension)), None)
    return trouver(".mp4"), trouver(".png")

class ComfyUI:
    """Le ComfyUI d'Aria. Un seul à la fois : celui qu'elle a lancé (`process`), ou celui qu'une Aria précédente a
    laissé orphelin (`adopte`, fermeture brutale) ; elle arrête l'un comme l'autre en quittant.

    Vu le 2026-11-06 : occupé (une vingtaine de Go de H3 à décharger), ComfyUI a mis plus de 2 s à répondre ; Aria
    l'a cru mort et en a lancé un second, mort aussitôt sur le port pris ; elle n'a plus suivi que ce second et a
    laissé le vrai tourner après sa fermeture, 11,8 Go gardés. D'où : jamais de second lancement tant que le nôtre
    vit ou que le port est pris, et l'adoption des orphelins."""

    def __init__(self, dossier: Path, port: int, chemins_modeles: Path):
        self.dossier = dossier
        self.port = port
        self.chemins_modeles = chemins_modeles
        self.url = f"http://127.0.0.1:{port}"
        self.python = dossier / "python_embeded" / "python.exe"
        self.process: subprocess.Popen | None = None
        self.adopte: psutil.Process | None = None
        self._log = None

    async def est_pret(self) -> bool:
        try:
            async with httpx.AsyncClient(timeout=2) as client:
                r = await client.get(f"{self.url}/system_stats")
            return r.status_code == 200
        except httpx.HTTPError:
            return False

    def vivant(self) -> bool:
        """Le nôtre tourne-t-il (lancé ou adopté), même s'il ne répond pas tout de suite ?"""
        if self.process is not None and self.process.poll() is None:
            return True
        try:
            return self.adopte is not None and self.adopte.is_running()
        except psutil.Error:
            return False

    def _port_pris(self) -> bool:
        with socket.socket() as s:
            s.settimeout(1)
            return s.connect_ex(("127.0.0.1", self.port)) == 0

    def orphelins(self) -> list[psutil.Process]:
        """Les ComfyUI d'Aria qu'aucune Aria ne tient plus : lancés depuis son dossier, sur son port, et dont le
        parent n'existe plus (ou dont le numéro a été repris par un processus plus jeune). Le ComfyUI d'une Aria
        vivante n'en est jamais un, même si elle partage les moteurs."""
        dossier, sortie = _chemin(self.dossier), []
        for p in psutil.process_iter(["name"]):
            try:
                if not (p.info["name"] or "").lower().startswith("python"):
                    continue
                if self.process is not None and p.pid == self.process.pid:
                    continue
                cmd = p.cmdline()
                if not ("ComfyUI/main.py" in cmd and "--port" in cmd and str(self.port) in cmd
                        and _chemin(p.cwd()) == dossier):
                    continue
                try:
                    orphelin = psutil.Process(p.ppid()).create_time() > p.create_time()
                except psutil.NoSuchProcess:
                    orphelin = True
                if orphelin:
                    sortie.append(p)
            except psutil.Error:
                continue
        return sortie

    def adopter(self) -> bool:
        """Reprend un ComfyUI d'Aria resté orphelin : Aria s'en sert, le garde-VRAM le compte comme à elle, et elle
        l'arrête en quittant. Rien à faire si le nôtre tourne déjà."""
        if self.vivant():
            return False
        orphelins = self.orphelins()
        if not orphelins:
            return False
        self.adopte = orphelins[0]
        garde.ADOPTES.add(self.adopte.pid)
        return True

    async def demarrer(self, timeout: float = 180):
        if await self.est_pret():
            self.adopter()  # un orphelin d'Aria qui répond : repris, pour l'arrêter en quittant
            return
        if self.vivant() or self._port_pris():
            # Le nôtre tourne mais il est occupé, ou un ComfyUI tient déjà le port : on l'attend. Un second lancement
            # mourrait sur le port pris, et on perdrait la trace du vrai.
            self.adopter()
            await self._attendre(timeout, lance=False)
            return
        LOGS.mkdir(parents=True, exist_ok=True)
        if self._log is not None:
            self._log.close()
        self._log = (LOGS / "comfyui.log").open("w", encoding="utf-8")
        self.process = subprocess.Popen(
            [
                str(self.python), "-s", "ComfyUI/main.py",
                "--listen", "127.0.0.1", "--port", str(self.port), "--disable-auto-launch",
                "--extra-model-paths-config", str(self.chemins_modeles),
            ],
            cwd=self.dossier,
            stdout=self._log,
            stderr=subprocess.STDOUT,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        await self._attendre(timeout, lance=True)

    async def _attendre(self, timeout: float, lance: bool):
        fin = asyncio.get_running_loop().time() + timeout
        while asyncio.get_running_loop().time() < fin:
            if lance and self.process.poll() is not None:
                raise ErreurComfy(f"ComfyUI s'est arrêté au démarrage (code {self.process.returncode}), voir data/logs/comfyui.log")
            if await self.est_pret():
                return
            await asyncio.sleep(1)
        if lance:
            await self.arreter()
            raise ErreurComfy(f"ComfyUI n'a pas répondu en {timeout:.0f} s")
        raise ErreurComfy(f"ComfyUI tourne mais ne répond plus depuis {timeout:.0f} s (occupé ou bloqué)")

    async def arreter(self):
        """Arrête le nôtre : celui qu'on a lancé et celui qu'on a adopté."""
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                await asyncio.to_thread(self.process.wait, 15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                await asyncio.to_thread(self.process.wait)
        if self.adopte is not None:
            try:
                self.adopte.terminate()
                try:
                    await asyncio.to_thread(self.adopte.wait, 15)
                except psutil.TimeoutExpired:
                    self.adopte.kill()
                    await asyncio.to_thread(self.adopte.wait, 15)
            except psutil.NoSuchProcess:
                pass
            garde.ADOPTES.discard(self.adopte.pid)
        self.process = self.adopte = None
        if self._log is not None:
            self._log.close()
            self._log = None

    async def options_sampler(self) -> tuple[list[str], list[str]]:
        """Samplers et schedulers acceptés par le nœud KSampler de ce ComfyUI."""
        async with httpx.AsyncClient(timeout=10) as client:
            r = await client.get(f"{self.url}/object_info/KSampler")
        r.raise_for_status()
        requis = r.json()["KSampler"]["input"]["required"]
        return list(requis["sampler_name"][0]), list(requis["scheduler"][0])

    async def stats(self) -> dict:
        async with httpx.AsyncClient(timeout=5) as client:
            r = await client.get(f"{self.url}/system_stats")
        r.raise_for_status()
        return r.json()

    async def vram_utilisee(self) -> int:
        """Octets de VRAM occupés sur le GPU principal, vus par ComfyUI (tous processus confondus).

        Le compteur de PyTorch (torch_vram_total) ne suffit pas : les poids des modèles
        sont gérés hors de son allocateur et il reste proche de zéro même modèles chargés.
        """
        appareil = (await self.stats())["devices"][0]
        return appareil["vram_total"] - appareil["vram_free"]

    async def liberer_vram(self, timeout: float = 30) -> int:
        """Décharge les modèles et vide le cache. Renvoie la VRAM GPU encore occupée, en octets."""
        async with httpx.AsyncClient(timeout=5) as client:
            r = await client.post(f"{self.url}/free", json={"unload_models": True, "free_memory": True})
        r.raise_for_status()
        # /free ne fait que lever un drapeau : on attend que la VRAM réelle cesse de baisser.
        tolerance = 64 * 2**20
        fin = asyncio.get_running_loop().time() + timeout
        lectures: list[int] = []
        while asyncio.get_running_loop().time() < fin:
            await asyncio.sleep(0.5)
            lectures.append(await self.vram_utilisee())
            if len(lectures) >= 4 and max(lectures[-3:]) - min(lectures[-3:]) < tolerance:
                break
        return lectures[-1]

    async def televerser(self, image: Path | tuple[str, bytes]) -> str:
        """Copie une image, une vidéo ou un son (fichier, ou (nom, octets)) dans le dossier input de ComfyUI ;
        renvoie le nom à donner à LoadImage, LoadVideo ou LoadAudio."""
        nom, octets = (image.name, await asyncio.to_thread(image.read_bytes)) if isinstance(image, Path) else image
        type_mime = TYPES_MIME.get(nom.rsplit(".", 1)[-1].lower(), "image/png")
        async with httpx.AsyncClient(timeout=60) as client:
            r = await client.post(
                f"{self.url}/upload/image",
                files={"image": (nom, octets, type_mime)},
                data={"overwrite": "true"},
            )
        r.raise_for_status()
        reponse = r.json()
        return f"{reponse['subfolder']}/{reponse['name']}" if reponse.get("subfolder") else reponse["name"]

    async def executer(
        self,
        workflow: dict,
        progression: Callable[[int, int], None] | None = None,
    ) -> list[tuple[str, bytes]]:
        """Exécute un workflow au format API et renvoie les fichiers produits (images, vidéos) : [(nom, octets)]."""
        client_id = str(uuid.uuid4())
        ws_url = f"ws://127.0.0.1:{self.port}/ws?clientId={client_id}"
        async with websockets.connect(ws_url, max_size=None) as ws, httpx.AsyncClient(timeout=30) as client:
            r = await client.post(f"{self.url}/prompt", json={"prompt": workflow, "client_id": client_id})
            if r.status_code != 200:
                raise ErreurComfy(f"Workflow refusé : {r.text}")
            prompt_id = r.json()["prompt_id"]
            try:
                return await self._suivre(ws, client, prompt_id, progression)
            except asyncio.CancelledError:
                await self._interrompre(prompt_id)  # ✕ Annuler : sans ça, ComfyUI finirait pour rien
                raise

    async def _interrompre(self, prompt_id: str):
        """Retire le prompt de la file s'il attend, l'arrête s'il tourne (ce ComfyUI ne sert qu'Aria)."""
        try:
            async with httpx.AsyncClient(timeout=5) as client:
                await client.post(f"{self.url}/queue", json={"delete": [prompt_id]})
                await client.post(f"{self.url}/interrupt", json={"prompt_id": prompt_id})
        except httpx.HTTPError:
            pass  # ComfyUI déjà arrêté : rien à interrompre

    async def _suivre(self, ws, client: httpx.AsyncClient, prompt_id: str,
                      progression: Callable[[int, int], None] | None) -> list[tuple[str, bytes]]:
        """Suit l'exécution jusqu'au bout, puis rapatrie les fichiers produits."""
        async for message in ws:
            if isinstance(message, bytes):
                continue  # aperçus binaires
            evenement = json.loads(message)
            donnees = evenement.get("data", {})
            if donnees.get("prompt_id") not in (None, prompt_id):
                continue
            if evenement["type"] == "progress" and progression:
                progression(donnees["value"], donnees["max"])
            elif evenement["type"] == "execution_error":
                raise ErreurComfy(f"{donnees.get('node_type')} : {donnees.get('exception_message')}")
            elif evenement["type"] == "execution_interrupted":
                raise ErreurComfy("Exécution interrompue")
            elif evenement["type"] == "executing" and donnees.get("node") is None and donnees.get("prompt_id") == prompt_id:
                break

        r = await client.get(f"{self.url}/history/{prompt_id}")
        r.raise_for_status()
        sorties = r.json()[prompt_id]["outputs"]

        # Les images arrivent sous « images », les vidéos selon la version de ComfyUI (« images » + animated,
        # « videos », « gifs ») : on prend tout fichier de sortie, jamais les aperçus temporaires.
        fichiers = []
        for sortie in sorties.values():
            for valeur in sortie.values():
                for f in valeur if isinstance(valeur, list) else []:
                    if isinstance(f, dict) and "filename" in f and f.get("type", "output") == "output":
                        r = await client.get(f"{self.url}/view", params=f, timeout=300)
                        r.raise_for_status()
                        fichiers.append((f["filename"], r.content))
        return fichiers
