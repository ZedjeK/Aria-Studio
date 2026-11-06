"""Pilote de llama-server (llama.cpp) : processus, santé, génération via l'API compatible OpenAI."""

import asyncio
import json
import subprocess
from dataclasses import dataclass
from pathlib import Path

import httpx

from app.emplacements import DATA

LOGS = DATA / "logs"


class ErreurLlama(RuntimeError):
    pass


# Réflexion active : max_tokens borne la réponse, et la réflexion a ce budget en plus. Sans lui, le Bonsai 2 Heretic
# passait ses 512 tokens à réfléchir et rendait une réponse vide (1 600 tokens en tout pour un prompt vidéo, mesuré).
# Banc du 2026-11-06 (Bonsai 2, réécriture Qwen, peaufinage, découpage, 2 seeds) : avec 4096, 5 réponses vides sur 10 ;
# reasoning_effort medium ou low n'y suffit pas (2 et 3 sur 10) ; avec 8192, aucune, pour 52 s en moyenne au lieu de 45.
BUDGET_REFLEXION = 8192


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
        self.vision: Path | None = None
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
                       contexte: int | None = None, reflexion: bool | None = None, vision: Path | None = None):
        """`serveur` : binaire llama-server particulier (fork) ; par défaut celui du constructeur.
        `contexte` et `reflexion` : réglages propres au GGUF ; par défaut ceux du constructeur.
        `vision` : projecteur d'images (mmproj) d'un LLM qui doit voir une image (spécialiste de retouche)."""
        serveur = serveur or self.serveur
        contexte = contexte or self.contexte
        reflexion = self.reflexion if reflexion is None else reflexion
        if self.process is not None and self.process.poll() is None:
            # Ne redémarre que si quelque chose change (modèle, serveur ou réglages de lancement).
            actuel = (self.modele, self._serveur_actif, self.contexte_actif, self.reflexion_active, self.vision)
            if actuel == (modele, serveur, contexte, reflexion, vision):
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
        if vision is not None:
            if not vision.is_file():
                raise ErreurLlama(f"Projecteur d'images introuvable : {vision}")
            arguments += ["--mmproj", str(vision)]
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
        self.vision = vision
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
        budget_reflexion: int = BUDGET_REFLEXION, reflechir: bool = True, **echantillonnage,
    ) -> Reponse:
        """`echantillonnage` : top_p, top_k, min_p, presence_penalty… (absents : défauts de llama-server).
        `budget_reflexion` : tokens de réflexion ajoutés à `max_tokens` quand elle est active (0 : déjà comptés).
        `reflechir` : False coupe la réflexion pour cette demande seulement (petites traductions : 10 à 20 s pour rien)."""
        reflexion = self.reflexion_active and reflechir
        if self.reflexion_active:
            max_tokens += budget_reflexion  # gardé même sans réflexion : un gabarit peut ignorer enable_thinking
        corps = {"messages": messages, "temperature": temperature, "max_tokens": max_tokens, **echantillonnage,
                 "stream": True, "stream_options": {"include_usage": True}}
        if seed is not None:
            corps["seed"] = seed
        if not reflexion:
            # Certains serveurs (fork PrismML) ignorent --reasoning-budget 0 : on coupe aussi la
            # réflexion dans le gabarit de chat. Les gabarits qui ne la gèrent pas ignorent l'option.
            corps["chat_template_kwargs"] = {"enable_thinking": False}
        # En flux : une commande annulée ferme la connexion, et llama-server arrête aussitôt sa génération. Une
        # requête non streamée, elle, va jusqu'au bout dans le vide (encore 90 s après la coupure, mesuré sur PrismML).
        morceaux, fin, donnees = [], None, {}
        async with httpx.AsyncClient(timeout=300) as client:
            async with client.stream("POST", f"{self.url}/v1/chat/completions", json=corps) as r:
                if r.status_code != 200:
                    erreur = (await r.aread()).decode(errors="replace")
                    raise ErreurLlama(f"Génération refusée ({r.status_code}) : {erreur[:300]}")
                async for ligne in r.aiter_lines():
                    if ligne.startswith("error:"):
                        raise ErreurLlama(f"Génération interrompue : {ligne[6:].strip()[:300]}")
                    if not ligne.startswith("data:") or ligne[5:].strip() == "[DONE]":
                        continue
                    morceau = json.loads(ligne[5:])
                    for choix in morceau.get("choices") or []:
                        morceaux.append((choix.get("delta") or {}).get("content") or "")
                        fin = choix.get("finish_reason") or fin
                    if "usage" in morceau:
                        donnees = morceau
        timings = donnees.get("timings", {})
        texte = "".join(morceaux).strip()
        if not texte:
            # Jamais en silence : un prompt vide partirait tel quel vers ComfyUI.
            if fin != "length":
                raise ErreurLlama("Le LLM a rendu une réponse vide")
            if reflexion:
                raise ErreurLlama(f"Le LLM a passé ses {max_tokens} tokens à réfléchir sans répondre : coupe sa "
                                  "Réflexion ou augmente sa longueur maximale dans Réglages")
            raise ErreurLlama(f"Le LLM a atteint sa limite de {max_tokens} tokens sans répondre : augmente sa "
                              "longueur maximale dans Réglages")
        return Reponse(
            texte=texte,
            tokens_entree=donnees.get("usage", {}).get("prompt_tokens", 0),
            tokens_sortie=donnees.get("usage", {}).get("completion_tokens", 0),
            lecture_tps=round(timings.get("prompt_per_second", 0.0), 1),
            ecriture_tps=round(timings.get("predicted_per_second", 0.0), 1),
        )
