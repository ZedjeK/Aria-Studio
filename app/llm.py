"""Pilote de llama-server (llama.cpp) : processus, santé, génération via l'API compatible OpenAI."""

import asyncio
import subprocess
from dataclasses import dataclass
from pathlib import Path

import httpx

from app.config import ROOT

LOGS = ROOT / "data" / "logs"


class ErreurLlama(RuntimeError):
    pass


@dataclass
class Reponse:
    texte: str
    tokens_entree: int
    tokens_sortie: int
    lecture_tps: float  # tokens/s pour lire le prompt
    ecriture_tps: float  # tokens/s pour générer la réponse


class Llama:
    def __init__(self, serveur: Path, port: int, contexte: int, reflexion: bool):
        self.serveur = serveur
        self.port = port
        self.contexte = contexte  # valeurs par défaut ; un GGUF peut avoir les siennes (onglet Réglages)
        self.reflexion = reflexion
        self.contexte_actif = contexte
        self.reflexion_active = reflexion
        self.url = f"http://127.0.0.1:{port}"
        self.modele: Path | None = None
        self.process: subprocess.Popen | None = None
        self._serveur_actif: Path | None = None
        self._log = None

    async def est_pret(self) -> bool:
        try:
            async with httpx.AsyncClient(timeout=2) as client:
                r = await client.get(f"{self.url}/health")
            return r.status_code == 200
        except httpx.HTTPError:
            return False

    async def demarrer(self, modele: Path, serveur: Path | None = None, timeout: float = 120,
                       contexte: int | None = None, reflexion: bool | None = None):
        """`serveur` : binaire llama-server particulier (fork) ; par défaut celui du constructeur.
        `contexte` et `reflexion` : réglages propres au GGUF ; par défaut ceux du constructeur."""
        serveur = serveur or self.serveur
        contexte = contexte or self.contexte
        reflexion = self.reflexion if reflexion is None else reflexion
        if self.process is not None and self.process.poll() is None:
            # Ne redémarre que si quelque chose change (modèle, serveur ou réglages de lancement).
            actuel = (self.modele, self._serveur_actif, self.contexte_actif, self.reflexion_active)
            if actuel == (modele, serveur, contexte, reflexion):
                return
            await self.arreter()
        if not modele.is_file():
            raise ErreurLlama(f"Modèle introuvable : {modele}")
        if not serveur.is_file():
            raise ErreurLlama(f"Serveur introuvable : {serveur}")
        LOGS.mkdir(parents=True, exist_ok=True)
        self._log = (LOGS / "llama.log").open("w", encoding="utf-8")
        arguments = [
            str(serveur), "-m", str(modele),
            "--host", "127.0.0.1", "--port", str(self.port),
            # Pas de --n-gpu-layers : llama.cpp place lui-même couches et experts MoE pour tenir
            # dans la VRAM (--fit). Forcer « tout sur le GPU » déborde en mémoire partagée Windows.
            "--ctx-size", str(contexte),
        ]
        if not reflexion:
            arguments += ["--reasoning-budget", "0"]
        self.process = subprocess.Popen(
            arguments,
            cwd=serveur.parent,
            stdout=self._log,
            stderr=subprocess.STDOUT,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        self.modele = modele
        self._serveur_actif = serveur
        self.contexte_actif = contexte
        self.reflexion_active = reflexion
        fin = asyncio.get_running_loop().time() + timeout
        while asyncio.get_running_loop().time() < fin:
            if self.process.poll() is not None:
                code = self.process.returncode
                await self.arreter()
                raise ErreurLlama(f"llama-server s'est arrêté au démarrage (code {code}), voir data/logs/llama.log")
            if await self.est_pret():
                return
            await asyncio.sleep(0.5)
        await self.arreter()
        raise ErreurLlama(f"llama-server n'a pas répondu en {timeout:.0f} s")

    async def arreter(self):
        """Arrêt complet du processus : toute la VRAM de Llama est rendue."""
        if self.process is None:
            return
        if self.process.poll() is None:
            self.process.terminate()
            try:
                await asyncio.to_thread(self.process.wait, 15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                await asyncio.to_thread(self.process.wait)
        self.process = None
        self.modele = None
        self._serveur_actif = None
        self._log.close()
        self._log = None

    async def generer(
        self, messages: list[dict], temperature: float = 0.7, max_tokens: int = 512, seed: int | None = None,
        **echantillonnage,
    ) -> Reponse:
        """`echantillonnage` : top_p, top_k, min_p, presence_penalty… (absents : défauts de llama-server)."""
        corps = {"messages": messages, "temperature": temperature, "max_tokens": max_tokens, **echantillonnage}
        if seed is not None:
            corps["seed"] = seed
        if not self.reflexion_active:
            # Certains serveurs (fork PrismML) ignorent --reasoning-budget 0 : on coupe aussi la
            # réflexion dans le gabarit de chat. Les gabarits qui ne la gèrent pas ignorent l'option.
            corps["chat_template_kwargs"] = {"enable_thinking": False}
        async with httpx.AsyncClient(timeout=300) as client:
            r = await client.post(f"{self.url}/v1/chat/completions", json=corps)
        if r.status_code != 200:
            raise ErreurLlama(f"Génération refusée ({r.status_code}) : {r.text[:300]}")
        donnees = r.json()
        timings = donnees.get("timings", {})
        return Reponse(
            texte=donnees["choices"][0]["message"]["content"].strip(),
            tokens_entree=donnees["usage"]["prompt_tokens"],
            tokens_sortie=donnees["usage"]["completion_tokens"],
            lecture_tps=round(timings.get("prompt_per_second", 0.0), 1),
            ecriture_tps=round(timings.get("predicted_per_second", 0.0), 1),
        )
