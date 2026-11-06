"""Chef d'orchestre : un seul moteur (ComfyUI ou llama) occupe la VRAM à la fois.

Le verrou est tenu pendant toute l'utilisation d'un moteur, pour qu'aucune bascule
ne puisse survenir au milieu d'une génération.

Bascules :
- vers llama : ComfyUI vide sa VRAM (/free) mais reste lancé, pour repartir vite ;
- vers ComfyUI : llama-server est arrêté complètement.
À chaque bascule, une fois notre autre moteur libéré, le garde-VRAM (app/garde.py) attend que les autres applis
aient rendu la carte.
"""

import asyncio
from contextlib import asynccontextmanager

from app import fiches, modeles, reglages
from app.comfy import ComfyUI
from app.garde import Garde
from app.config import CONFIG
from app.emplacements import moteur
from app.llm import Llama
from app.modeles import DOSSIER as MODELES

# Contexte minimal d'un LLM qui réfléchit ou qui voit : consigne et images (≈ 600 tokens chacune), le budget de
# réflexion (llm.BUDGET_REFLEXION) et la réponse.
CONTEXTE_REFLEXION = 16384


def lancement_llm(nom: str) -> dict:
    """Comment Aria lance ce GGUF : serveur (moteur), contexte, réflexion et vision, d'après ses réglages. Le banc de
    palier s'en sert aussi, pour mesurer le LLM tel qu'Aria le fait tourner."""
    regle = reglages.valeurs_llm(nom)
    contexte, reflexion = regle["contexte"], regle["reflexion"]
    # Vision chargée d'office quand le LLM en a une : pas de redémarrage entre une réécriture avec image
    # (Montage, retouche, références) et une sans.
    projecteur = reglages.projecteur_llm(nom)[0]
    vision = MODELES / "llm" / projecteur if projecteur else None
    if reflexion or vision:  # consigne + images (≈ 600 tokens chacune) + réflexion + réponse
        contexte = max(contexte, CONTEXTE_REFLEXION)
    if specialiste := fiches.specialiste(nom):
        # Un LLM spécialisé a ses réglages d'entraînement : réflexion, contexte assez grand, vision.
        contexte, reflexion = max(contexte, specialiste["contexte"]), specialiste["reflexion"]
        if specialiste.get("vision"):
            vision = MODELES / "llm" / specialiste["vision"]["nom"]
    return {"serveur": moteur(reglages.serveur_llm(reglages.moteur_llm(nom)[0])), "contexte": contexte,
            "reflexion": reflexion, "vision": vision}


class Orchestrateur:
    def __init__(self):
        self.comfy = ComfyUI(
            moteur(CONFIG["comfyui"]["dossier"]),
            CONFIG["comfyui"]["port"],
            modeles.CHEMINS_COMFYUI,
        )
        self.llama = Llama(
            moteur(CONFIG["llama"]["server"]),
            CONFIG["llama"]["port"],
            CONFIG["llama"]["contexte"],
            CONFIG["llama"]["reflexion"],
        )
        self.actif: str | None = None
        self.garde = Garde()
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
                await self.garde.attendre("ComfyUI")
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
        nom = modele or modeles.llm_defaut()
        async with self._verrou:
            if self.actif == "comfyui" and await self.comfy.est_pret():
                await self.comfy.liberer_vram()
            if self.actif != "llama":
                await self.garde.attendre("Llama")
            # Ne redémarre que si le modèle, son serveur ou ses réglages de lancement changent.
            await self.llama.demarrer(MODELES / "llm" / nom, **lancement_llm(nom))
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

    async def reprendre(self) -> bool:
        """Au démarrage : un ComfyUI d'Aria resté orphelin (fermeture brutale d'une Aria précédente) est repris.
        S'il répond, c'est le moteur actif : sa VRAM est à nous, un LLM la fera libérer avant de se charger, et la
        pastille ne la compte pas comme une autre appli."""
        async with self._verrou:
            if not self.comfy.adopter():
                return False
            if await self.comfy.est_pret():
                self.actif = "comfyui"
            return True

    @asynccontextmanager
    async def sans_moteur(self, id_moteur: str):
        """Arrête notre moteur et le tient arrêté le temps d'un échange de dossiers (app/moteurs.py) ; attend la fin
        d'une génération en cours. « llama » et « llama-prism » sont tous deux servis par self.llama."""
        async with self._verrou:
            if id_moteur == "comfyui":
                await self.comfy.arreter()
                if self.actif == "comfyui":
                    self.actif = None
            else:
                await self.llama.arreter()
                if self.actif == "llama":
                    self.actif = None
            yield
