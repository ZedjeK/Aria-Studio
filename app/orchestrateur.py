"""Chef d'orchestre : un seul moteur (ComfyUI ou llama) occupe la VRAM à la fois.

Le verrou est tenu pendant toute l'utilisation d'un moteur, pour qu'aucune bascule
ne puisse survenir au milieu d'une génération.

Bascules :
- vers llama : ComfyUI vide sa VRAM (/free) mais reste lancé, pour repartir vite ;
- vers ComfyUI : llama-server est arrêté complètement.
"""

import asyncio
from contextlib import asynccontextmanager

from app import modeles, reglages
from app.comfy import ComfyUI
from app.config import CONFIG, chemin
from app.llm import Llama
from app.modeles import DOSSIER as MODELES


class Orchestrateur:
    def __init__(self):
        self.comfy = ComfyUI(
            chemin(CONFIG["comfyui"]["dossier"]),
            CONFIG["comfyui"]["port"],
            modeles.CHEMINS_COMFYUI,
        )
        self.llama = Llama(
            chemin(CONFIG["llama"]["server"]),
            CONFIG["llama"]["port"],
            CONFIG["llama"]["contexte"],
            CONFIG["llama"]["reflexion"],
        )
        self.actif: str | None = None
        self._verrou = asyncio.Lock()
        self._samplers_lus = False

    async def _lire_samplers(self):
        """Samplers et schedulers que ce ComfyUI accepte vraiment, retenus pour l'onglet Réglages."""
        try:
            reglages.memoriser_samplers(*await self.comfy.options_sampler())
            self._samplers_lus = True
        except Exception:  # simple confort : la liste de secours reste utilisée
            pass

    @asynccontextmanager
    async def comfyui(self):
        async with self._verrou:
            if self.actif != "comfyui":
                await self.llama.arreter()
                if self.comfy.process is None:
                    modeles.ecrire_chemins_comfyui()  # familles ajoutées depuis le dernier démarrage
                await self.comfy.demarrer()
                self.actif = "comfyui"
                if not self._samplers_lus:
                    await self._lire_samplers()
            yield self.comfy

    @asynccontextmanager
    async def llm(self, modele: str | None = None):
        """`modele` : nom de fichier dans modeles/llm (par défaut celui de config.yaml)."""
        nom = modele or CONFIG["llama"]["modele"]
        fichier = MODELES / "llm" / nom
        specifique = CONFIG["llama"].get("serveurs_specifiques", {}).get(nom)
        async with self._verrou:
            if self.actif == "comfyui" and await self.comfy.est_pret():
                await self.comfy.liberer_vram()
            # Ne redémarre que si le modèle, son serveur ou ses réglages de lancement changent.
            regle = reglages.valeurs_llm(nom)
            await self.llama.demarrer(fichier, chemin(specifique) if specifique else None,
                                      contexte=regle["contexte"], reflexion=regle["reflexion"])
            self.actif = "llama"
            yield self.llama

    async def liberer(self):
        """Rend la VRAM du moteur actif : /free pour ComfyUI, arrêt pour llama."""
        async with self._verrou:
            if self.actif == "comfyui":
                await self.comfy.liberer_vram()
            elif self.actif == "llama":
                await self.llama.arreter()
            self.actif = None

    async def arreter(self):
        async with self._verrou:
            await self.llama.arreter()
            await self.comfy.arreter()
            self.actif = None
