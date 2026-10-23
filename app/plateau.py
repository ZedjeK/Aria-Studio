"""Plateau : file de créations (idée → Llama → ComfyUI) et galerie persistante dans data/creations/."""

import asyncio
import json
import random
import re
import struct
import time
import uuid
from dataclasses import asdict, dataclass, field
from datetime import datetime

from app import modeles, prompt, reglages, retouche, securite, workflows
from app.config import CONFIG, ROOT
from app.orchestrateur import Orchestrateur

DOSSIER = ROOT / "data" / "creations"

FORMATS = workflows.FORMATS
NOMBRE_MAX = 4
_ID = re.compile(r"^\d{8}-\d{6}-[0-9a-f]{6}$")


class ErreurPlateau(ValueError):
    pass


@dataclass
class Commande:
    idee: str  # idée de création, ou instruction de retouche / précision d'agrandissement (en français)
    prompt: str | None  # fourni : on saute la réécriture (« Régénérer », « Variante », « Sans réécriture »)
    llm: str | None
    workflow: str
    format: str
    seed: int | None
    nombre: int
    parent: str | None = None
    modele: str | None = None  # variante déposée dans <Famille>/modeles ; None = modèle d'origine du workflow
    loras: list[dict] = field(default_factory=list)  # [{fichier, force, declencheurs}] de <Famille>/loras
    type: str = "creation"  # creation, retouche, agrandissement
    masque: bytes | None = field(default=None, repr=False)  # PNG peint dans l'interface
    fondu: int = 24
    largeur: int | None = None
    hauteur: int | None = None
    ancrage: str = "centre"
    references: list[str] = field(default_factory=list)  # rushes de référence (S'inspirer : 1 à 3, Variation : 1)
    mode_reference: str | None = None  # inspirer, variation
    liberte: float = 0.6  # Variation : denoise (0 = copie, 1 = libre)
    dossier: str | None = None  # dossier des rushes où ranger toutes les images de la commande
    mode_video: str | None = None  # Montage : animer (un rush) ou texte
    qualite: str = "compromis"
    duree: int = 5  # secondes
    sequence: str | None = None  # Montage, mode Séquence : la séquence à tourner…
    plans: list[int] = field(default_factory=list)  # … et ses plans (indices), dans l'ordre
    description: str = ""  # essai de voix : qui parle, et comment (la phrase est dans idee, le personnage dans parent)
    voix: str | None = None  # essai de voix : le brouillon produit (data/voix)
    # AS-18 : textes d'origine des rushes utilisés (une image n'est pas lue, sa fiche si).
    textes_origine: list[str] = field(default_factory=list, repr=False)
    id: str = field(default_factory=lambda: uuid.uuid4().hex[:12])
    etat: str = "attente"  # attente, en_cours, termine, erreur
    etape: str = ""
    progression: float = 0.0  # avancement des steps de la génération en cours
    images: list[str] = field(default_factory=list)
    erreur: str | None = None

    def to_dict(self) -> dict:
        d = asdict(self)
        d["masque"] = self.masque is not None
        d.pop("textes_origine")
        return d


MODES_REFERENCE = ("inspirer", "variation")

# Montage : qualités de MiniMax H3 (mesurées sur la RTX 4080 de Jérémy pour 5 s, à chaud) et durées proposées.
QUALITES_VIDEO = {
    "rapide": {"nom": "⚡ Rapide", "megapixels": 0.4, "steps": 8, "estimation_s": 80},
    "compromis": {"nom": "⚖️ Compromis", "megapixels": 0.6, "steps": 12, "estimation_s": 170},
    "qualite": {"nom": "💎 Qualité", "megapixels": 0.98, "steps": 12, "estimation_s": 425},
}
DUREES_VIDEO = (5, 8, 10, 15)  # H3 est entraîné de 5 à 15 s
MODES_VIDEO = {"animer": "image_vers_video", "texte": "texte_vers_video"}
CLIPS_MAX = 3
LIBERTE = (0.05, 1.0)

CONSIGNE_AGRANDISSEMENT = (
    "Replace the flat gray border areas with a natural continuation of the scene, as if the camera had captured "
    "a wider view: extend the environment, objects and lighting seamlessly, matching the style, colors and perspective. "
    "No gray areas, no frames, no borders must remain. Keep the existing picture unchanged."
)


def llm_au_choix() -> list[dict]:
    """Tous les GGUF de modeles/llm : les favoris de config.yaml d'abord, avec leur nom."""
    favoris = {m["fichier"]: m["nom"] for m in CONFIG["llama"].get("modeles_au_choix", [])}
    presents = modeles.llm_disponibles()
    choix = [{"fichier": f, "nom": nom, "favori": True} for f, nom in favoris.items() if f in presents]
    return choix + [{"fichier": f, "nom": f.removesuffix(".gguf"), "favori": False}
                    for f in presents if f not in favoris]


def _dimensions_png(donnees: bytes) -> tuple[int, int]:
    return struct.unpack(">II", donnees[16:24])


class Plateau:
    def __init__(self, orchestrateur: Orchestrateur):
        self.orchestrateur = orchestrateur
        self.commandes: dict[str, Commande] = {}
        self._file: asyncio.Queue[Commande] = asyncio.Queue()
        self._boucle_tache: asyncio.Task | None = None

    def demarrer(self):
        self._boucle_tache = asyncio.create_task(self._boucle())

    async def arreter(self):
        if self._boucle_tache:
            self._boucle_tache.cancel()
            await asyncio.gather(self._boucle_tache, return_exceptions=True)

    def options(self) -> dict:
        """Listes de l'interface, relues à chaque appel : un fichier déposé dans un dossier y apparaît aussitôt."""
        from app import sequences, voix  # sequences et voix importent plateau
        prets = [wf for wf in workflows.catalogue() if not wf.manquants()]
        return {
            "formats": FORMATS,
            "nombre_max": NOMBRE_MAX,
            "workflows": {
                tache: {
                    "defaut": CONFIG.get("generation", {}).get(tache),
                    "choix": [{"id": wf.id, "nom": wf.fiche["nom"], "famille": wf.famille,
                               "origine": wf.modele_origine, "modeles": wf.variantes(),
                               "loras": modeles.loras(wf.famille),
                               "format": reglages.valeurs_workflow(wf).get("ratio"),  # réglable (onglet Réglages)
                               "references": wf.accepte_references(), "variation": wf.accepte_variation()}
                              for wf in prets if wf.fiche["tache"] == tache],
                }
                for tache in ("texte_vers_image", "retouche", "texte_vers_video")  # vidéo : modèle cible du Scénario
            },
            "references_defaut": CONFIG.get("generation", {}).get("references"),
            "video": {
                "qualites": [{"cle": k, "nom": q["nom"], "megapixels": q["megapixels"], "steps": q["steps"],
                              "estimation_s": q["estimation_s"]} for k, q in QUALITES_VIDEO.items()],
                "defaut_qualite": "compromis", "durees": list(DUREES_VIDEO), "nombre_max": CLIPS_MAX,
                "workflows": {mode: CONFIG.get("generation", {}).get(tache) for mode, tache in MODES_VIDEO.items()},
                # Mode Séquence : de quoi estimer comme le serveur (sequences.estimation).
                "sequence": {"plans_max": sequences.PLANS_MAX, "surcout": sequences.SURCOUT_SEQUENCE,
                             "surcout_parole": sequences.SURCOUT_PAROLE, "steps_parole": sequences.STEPS_PAROLE},
                "voix": voix.options(),
            },
            "references_max": workflows.REFERENCES_MAX,
            "llm": {"defaut": CONFIG["llama"]["modele"], "choix": llm_au_choix()},
        }

    def commander(self, idee: str, prompt_force: str | None, llm: str | None, workflow: str | None,
                  format: str, seed: int | None, nombre: int, parent: str | None = None,
                  modele: str | None = None, loras: list[dict] | None = None,
                  references: list[str] | None = None, mode_reference: str | None = None,
                  liberte: float = 0.6, dossier: str | None = None) -> Commande:
        references = list(references or [])
        if references:
            workflow = self._valider_references(references, mode_reference, workflow, liberte)
        else:
            mode_reference = None
            workflow = workflow or CONFIG.get("generation", {}).get("texte_vers_image")
            if workflows.obtenir(workflow).fiche["tache"] != "texte_vers_image":
                raise ErreurPlateau(f"Workflow indisponible pour créer : {workflow}")
        wf = workflows.obtenir(workflow)
        if wf.manquants():
            raise ErreurPlateau(f"Workflow indisponible pour créer : {workflow}")
        textes = textes_origine(references)
        self._valider_modele(wf, modele)
        if format not in FORMATS:
            raise ErreurPlateau(f"Format inconnu : {format}")
        if not 1 <= nombre <= NOMBRE_MAX:
            raise ErreurPlateau(f"Nombre d'images entre 1 et {NOMBRE_MAX}")
        if prompt_force is None:
            if not idee.strip():
                raise ErreurPlateau("Écris ton idée")
            llm = self._valider_llm(llm)
            securite.verifier(idee, *textes)  # refus immédiat, avant même la file
        else:
            securite.verifier(idee, prompt_force, *textes)
        loras = self._valider_loras(wf, loras, idee, textes)
        return self._mettre_en_file(Commande(
            idee, prompt_force, None if prompt_force is not None else llm, workflow, format, seed, nombre,
            parent or (references[0] if references else None), modele, loras,
            references=references, mode_reference=mode_reference, liberte=liberte, textes_origine=textes,
            dossier=dossier))

    @staticmethod
    def _valider_references(references: list[str], mode: str | None, workflow: str | None, liberte: float) -> str:
        """Vérifie les rushes de référence et renvoie le workflow qui les recevra."""
        if mode not in MODES_REFERENCE:
            raise ErreurPlateau(f"Mode de référence inconnu : {mode}")
        for id_ref in references:
            fiche(id_ref)  # le rush doit exister
        if mode == "variation":
            if len(references) != 1:
                raise ErreurPlateau("La variation part d'une seule image de référence")
            if not LIBERTE[0] <= liberte <= LIBERTE[1]:
                raise ErreurPlateau(f"Liberté entre {LIBERTE[0]:g} et {LIBERTE[1]:g}")
            workflow = workflow or CONFIG.get("generation", {}).get("texte_vers_image")
            wf = workflows.obtenir(workflow)
            if wf.fiche["tache"] != "texte_vers_image" or not wf.accepte_variation():
                raise ErreurPlateau(f"{wf.fiche['nom']} ne permet pas la variation")
            return workflow
        workflow = workflow or CONFIG.get("generation", {}).get("references")
        wf = workflows.obtenir(workflow)
        if not 1 <= len(references) <= wf.accepte_references():
            raise ErreurPlateau(f"{wf.fiche['nom']} accepte de 1 à {wf.accepte_references()} images de référence")
        return workflow

    def commander_clip(self, idee: str, prompt_force: str | None, llm: str | None, mode: str,
                       source: str | None, qualite: str, duree: int, format: str, seed: int | None,
                       nombre: int, dossier: str | None = None) -> Commande:
        """Montage : un clip H3 (avec le son), en animant un rush ou à partir d'un texte."""
        if mode not in MODES_VIDEO:
            raise ErreurPlateau(f"Mode inconnu : {mode}")
        if qualite not in QUALITES_VIDEO:
            raise ErreurPlateau(f"Qualité inconnue : {qualite}")
        if duree not in DUREES_VIDEO:
            raise ErreurPlateau(f"Durée proposée : {', '.join(map(str, DUREES_VIDEO))} s")
        if not 1 <= nombre <= CLIPS_MAX:
            raise ErreurPlateau(f"De 1 à {CLIPS_MAX} clips à la fois")
        if mode == "animer" and not source:
            raise ErreurPlateau("Choisis le rush à animer")
        if mode == "texte" and format not in FORMATS:
            raise ErreurPlateau(f"Format inconnu : {format}")
        workflow = CONFIG.get("generation", {}).get(MODES_VIDEO[mode])
        wf = workflows.obtenir(workflow)
        if wf.manquants():
            raise ErreurPlateau(f"Workflow vidéo incomplet : {workflow}")
        textes = textes_origine([source] if mode == "animer" else [])
        if prompt_force is None:
            if not idee.strip():
                raise ErreurPlateau("Écris ton idée")
            llm = self._valider_llm(llm)
            securite.verifier(idee, *textes)
        else:
            securite.verifier(idee, prompt_force, *textes)
        return self._mettre_en_file(Commande(
            idee, prompt_force, None if prompt_force is not None else llm, workflow, format, seed, nombre,
            source if mode == "animer" else None, type="clip", mode_video=mode, qualite=qualite, duree=duree,
            textes_origine=textes, dossier=dossier))

    def commander_sequence(self, id_sequence: str, plans: list[int] | None = None) -> Commande:
        """Montage, mode Séquence : tourne les plans demandés (et ceux qui en dépendent), puis assemble le film."""
        from app import sequences
        s = sequences.lire(id_sequence)
        if any(c.sequence == id_sequence and c.etat in ("attente", "en_cours") for c in self.commandes.values()):
            raise ErreurPlateau("Cette séquence est déjà en tournage")
        if plans and any(not 0 <= k < len(s["plans"]) for k in plans):
            raise ErreurPlateau("Plan inconnu")
        indices = sequences.a_tourner(s, plans)
        if not indices:
            raise ErreurPlateau("Tous les plans sont à jour : choisis ceux à retourner")
        sequences._valider(s, pour_tourner=True)  # AS-18, rushes encore présents, transitions complètes
        if s["llm"]:
            self._valider_llm(s["llm"])
        workflow = CONFIG.get("generation", {}).get("image_vers_video")
        if workflows.obtenir(workflow).manquants():
            raise ErreurPlateau(f"Workflow vidéo incomplet : {workflow}")
        return self._mettre_en_file(Commande(
            s["titre"], None, s["llm"], workflow, s["format"], None, 1, type="sequence", qualite=s["qualite"],
            sequence=id_sequence, plans=indices, dossier=s["dossier"]))

    def commander_voix(self, phrase: str, description: str, personnage: str | None, seed: int | None) -> Commande:
        """Casting d'une séquence : un essai de voix (gros plan en cabine silencieuse, seul le son est gardé)."""
        from app import voix  # voix importe plateau
        phrase, description, duree = voix.valider_essai(phrase, description, personnage)
        textes = textes_origine([personnage] if personnage else [])
        securite.verifier(phrase, description, *textes)
        workflow = CONFIG.get("generation", {}).get("image_vers_video")
        if workflows.obtenir(workflow).manquants():
            raise ErreurPlateau(f"Workflow vidéo incomplet : {workflow}")
        return self._mettre_en_file(Commande(
            phrase, None, None, workflow, "voix", seed, 1, personnage, type="voix", qualite=voix.QUALITE,
            duree=duree, description=description, textes_origine=textes))

    def commander_retouche(self, source: str, instruction: str, reecrire: bool, llm: str | None,
                           workflow: str | None, masque: bytes | None, fondu: int, seed: int | None,
                           modele: str | None = None, loras: list[dict] | None = None) -> Commande:
        textes = textes_origine([source])  # la source doit exister ; son texte d'origine compte pour AS-18
        if not instruction.strip():
            raise ErreurPlateau("Écris ta consigne de retouche")
        securite.verifier(instruction, *textes)
        workflow = self._workflow_retouche(workflow)
        self._valider_modele(workflows.obtenir(workflow), modele)
        loras = self._valider_loras(workflows.obtenir(workflow), loras, instruction, textes)
        return self._mettre_en_file(Commande(
            instruction, None if reecrire else instruction, self._valider_llm(llm) if reecrire else None,
            workflow, "retouche", seed, 1, source, modele, loras,
            type="retouche", masque=masque, fondu=max(0, min(fondu, 128)), textes_origine=textes,
        ))

    def commander_agrandissement(self, source: str, largeur: int, hauteur: int, ancrage: str, precision: str,
                                 reecrire: bool, llm: str | None, workflow: str | None, fondu: int,
                                 seed: int | None, modele: str | None = None,
                                 loras: list[dict] | None = None) -> Commande:
        f = fiche(source)
        textes = textes_origine([source])
        if largeur < f["largeur"] or hauteur < f["hauteur"] or (largeur, hauteur) == (f["largeur"], f["hauteur"]):
            raise ErreurPlateau(f"La nouvelle taille doit être plus grande que {f['largeur']}×{f['hauteur']}")
        if ancrage not in retouche.ANCRAGES:
            raise ErreurPlateau(f"Ancrage inconnu : {ancrage}")
        securite.verifier(precision, *textes)
        if not precision.strip():
            consigne = CONSIGNE_AGRANDISSEMENT
        elif not reecrire:
            consigne = f"{CONSIGNE_AGRANDISSEMENT} {precision.strip()}"
        else:
            consigne = None  # la précision sera traduite par le LLM, puis ajoutée à la consigne
        workflow = self._workflow_retouche(workflow)
        self._valider_modele(workflows.obtenir(workflow), modele)
        loras = self._valider_loras(workflows.obtenir(workflow), loras, precision, textes)
        return self._mettre_en_file(Commande(
            precision, consigne, self._valider_llm(llm) if consigne is None else None,
            workflow, "agrandi", seed, 1, source, modele, loras,
            type="agrandissement", fondu=max(0, min(fondu, 128)),
            largeur=largeur, hauteur=hauteur, ancrage=ancrage, textes_origine=textes,
        ))

    @staticmethod
    def _valider_llm(llm: str | None) -> str:
        llm = llm or CONFIG["llama"]["modele"]
        if llm not in modeles.llm_disponibles():
            raise ErreurPlateau(f"GGUF absent de modeles/llm : {llm}")
        return llm

    @staticmethod
    def _valider_modele(wf: workflows.Workflow, modele: str | None):
        if modele is not None and modele not in wf.variantes():
            raise ErreurPlateau(f"Modèle absent de modeles/{wf.famille}/modeles : {modele}")

    @staticmethod
    def _valider_loras(wf: workflows.Workflow, loras: list[dict] | None, texte: str,
                       origine: list[str] = ()) -> list[dict]:
        """Normalise les LoRAs choisies, vérifie qu'elles sont de la famille, passe leurs mots déclencheurs
        au filtre AS-18 avec le texte de la demande, et retient les réglages pour la prochaine fois."""
        if not loras:
            return []
        disponibles = modeles.fichiers(f"{wf.famille}/loras", modeles.EXTENSIONS_IMAGE)
        propres = []
        for lora in loras:
            if lora.get("fichier") not in disponibles:
                raise ErreurPlateau(f"LoRA absente de modeles/{wf.famille}/loras : {lora.get('fichier')}")
            force = float(lora.get("force", 1.0))
            if not 0 <= force <= 2:
                raise ErreurPlateau(f"Force de LoRA entre 0 et 2 : {force}")
            propres.append({"fichier": lora["fichier"], "force": force,
                            "declencheurs": str(lora.get("declencheurs") or "").strip()})
        securite.verifier(texte, *origine, *(l["declencheurs"] for l in propres))
        modeles.memoriser_loras(wf.famille, propres)
        return propres

    @staticmethod
    def _workflow_retouche(workflow: str | None) -> str:
        workflow = workflow or CONFIG.get("generation", {}).get("retouche")
        wf = workflows.obtenir(workflow)
        if wf.fiche["tache"] != "retouche" or wf.manquants():
            raise ErreurPlateau(f"Workflow indisponible pour retoucher : {workflow}")
        return workflow

    def _mettre_en_file(self, commande: Commande) -> Commande:
        self.commandes[commande.id] = commande
        self._file.put_nowait(commande)
        return commande

    async def _boucle(self):
        while True:
            commande = await self._file.get()
            try:
                await self._executer(commande)
                commande.etat = "termine"
            except Exception as e:
                commande.etat, commande.erreur = "erreur", str(e) or repr(e)
            finally:
                commande.etape = ""

    async def _executer(self, c: Commande):
        c.etat = "en_cours"
        if c.type == "creation":
            await self._creer(c)
        elif c.type == "clip":
            await self._tourner(c)
        elif c.type == "sequence":
            from app import sequences  # sequences importe plateau
            await sequences.tourner(self, c)
        elif c.type == "voix":
            from app import voix  # voix importe plateau
            await voix.tourner_essai(self, c)
        else:
            await self._retoucher(c)

    @staticmethod
    def _avec_declencheurs(c: Commande) -> str:
        """Ajoute en tête du prompt les mots déclencheurs des LoRAs choisies."""
        declencheurs = ", ".join(l["declencheurs"] for l in c.loras if l["declencheurs"])
        return f"{declencheurs}, {c.prompt}" if declencheurs else c.prompt

    def _suivi(self, c: Commande):
        return lambda valeur, maximum: setattr(c, "progression", valeur / maximum)

    async def _retoucher(self, c: Commande):
        """Retouche (avec ou sans masque) ou agrandissement d'une image de la galerie."""
        debut = time.perf_counter()
        source = retouche.ouvrir(chemin_image(c.parent).read_bytes())
        if c.prompt is None:
            c.etape = "Traduction de la consigne"
            traduction = await prompt.traduire_instruction(self.orchestrateur, c.idee, c.llm)
            c.prompt = traduction if c.type == "retouche" else f"{CONSIGNE_AGRANDISSEMENT} {traduction}"
        securite.verifier(c.idee, c.prompt, *c.textes_origine)

        if c.type == "agrandissement":
            entree, position = retouche.canevas_agrandi(source, c.largeur, c.hauteur, c.ancrage)
            masque = None
        else:
            entree, position = source, None
            masque = retouche.masque_net(c.masque, source.size) if c.masque else None

        seed = c.seed if c.seed is not None else random.randrange(2**48)
        wf = workflows.obtenir(c.workflow)
        c.etape = "Agrandissement" if c.type == "agrandissement" else "Retouche"
        async with self.orchestrateur.comfyui() as comfy:
            nom = await comfy.televerser((f"aria_{c.id}.png", retouche.png(entree)))
            images = await comfy.executer(
                wf.preparer({"prompt": self._avec_declencheurs(c), "image": nom, "seed": seed, "modele": c.modele,
                             "loras": c.loras, "prefixe": "aria_plateau"}),
                progression=self._suivi(c),
            )
        if not images:
            raise ErreurPlateau("ComfyUI n'a renvoyé aucune image")

        c.etape = "Recollage"
        resultat = retouche.ouvrir(images[0][1])
        if c.type == "agrandissement":
            resultat = resultat.resize(entree.size, retouche.Image.Resampling.LANCZOS)
            final = retouche.recoller_agrandissement(source, position, resultat, c.fondu)
        elif masque is not None:
            final = retouche.recoller(source, resultat, masque, c.fondu)
        else:
            final = resultat.resize(source.size, retouche.Image.Resampling.LANCZOS)
        c.images.append(enregistrer(retouche.png(final), {
            "type": c.type,
            "idee": c.idee,
            "prompt": c.prompt,
            "llm": c.llm,
            "workflow": c.workflow,
            "modele": c.modele or wf.modele_origine,
            "loras": c.loras,
            "format": c.format,
            "seed": seed,
            "parent": c.parent,
            "masque": masque is not None,
            "fondu": c.fondu,
            "ancrage": c.ancrage if c.type == "agrandissement" else None,
            "duree_s": round(time.perf_counter() - debut, 2),
            **({"dossier": d} if (d := fiche(c.parent).get("dossier")) else {}),
        }))

    async def _tourner(self, c: Commande):
        """Montage : réécriture (consigne vidéo), puis un clip MP4 + sa vignette par seed."""
        debut = time.perf_counter()
        if c.prompt is None:
            c.etape = "Réécriture du prompt"
            c.prompt = (await prompt.reecrire(self.orchestrateur, c.idee, c.llm, video=c.mode_video,
                                              duree=c.duree))["prompt"]
        securite.verifier(c.idee, c.prompt, *c.textes_origine)
        duree_reecriture = time.perf_counter() - debut

        wf = workflows.obtenir(c.workflow)
        q = QUALITES_VIDEO[c.qualite]
        base = c.seed if c.seed is not None else random.randrange(2**48)
        source = None
        for i in range(c.nombre):
            seed = base + i
            c.etape = f"Tournage {i + 1}/{c.nombre}" if c.nombre > 1 else "Tournage"
            c.progression = 0.0
            debut_clip = time.perf_counter()
            valeurs = {"prompt": c.prompt, "seed": seed, "duree": c.duree, "megapixels": q["megapixels"],
                       "steps": q["steps"], "prefixe": "aria_montage"}
            async with self.orchestrateur.comfyui() as comfy:
                if c.mode_video == "animer":
                    source = source or await comfy.televerser(chemin_image(c.parent))
                    valeurs["image"] = source
                else:
                    valeurs["ratio"] = c.format
                api = wf.preparer(valeurs)
                wf.ajouter_affiche(api, "aria_montage")
                fichiers = await comfy.executer(api, progression=self._suivi(c))
            video = next((o for n, o in fichiers if n.lower().endswith(".mp4")), None)
            affiche = next((o for n, o in fichiers if n.lower().endswith(".png")), None)
            if not (video and affiche):
                raise ErreurPlateau("ComfyUI n'a pas renvoyé la vidéo attendue")
            c.images.append(enregistrer_clip(video, affiche, {
                "type": "clip",
                "mode_video": c.mode_video,
                "idee": c.idee,
                "prompt": c.prompt,
                "llm": c.llm,
                "workflow": c.workflow,
                "qualite": c.qualite,
                "duree": c.duree,
                "megapixels": q["megapixels"],
                "steps": q["steps"],
                "format": c.format if c.mode_video == "texte" else "image source",
                "seed": seed,
                "parent": c.parent,
                **({"dossier": c.dossier} if c.dossier else {}),
                "duree_s": round(time.perf_counter() - debut_clip + (duree_reecriture if i == 0 else 0), 2),
            }))

    async def _creer(self, c: Commande):
        debut = time.perf_counter()
        if c.prompt is None:
            c.etape = "Réécriture du prompt"
            nombre_refs = len(c.references) if c.mode_reference == "inspirer" else 0
            c.prompt = (await prompt.reecrire(self.orchestrateur, c.idee, c.llm, nombre_refs))["prompt"]
        securite.verifier(c.idee, c.prompt, *c.textes_origine)
        duree_reecriture = time.perf_counter() - debut

        wf = workflows.obtenir(c.workflow)
        base = c.seed if c.seed is not None else random.randrange(2**48)
        noms_refs: list[str] = []
        for i in range(c.nombre):
            seed = base + i
            c.etape = f"Génération {i + 1}/{c.nombre}" if c.nombre > 1 else "Génération"
            c.progression = 0.0
            debut_image = time.perf_counter()
            valeurs = {"prompt": self._avec_declencheurs(c), "seed": seed, "modele": c.modele, "loras": c.loras,
                       "prefixe": "aria_plateau"}
            async with self.orchestrateur.comfyui() as comfy:
                if c.references and not noms_refs:
                    noms_refs = [await comfy.televerser(chemin_image(r)) for r in c.references]
                if c.mode_reference == "inspirer":
                    valeurs.update(image=noms_refs[0], references=noms_refs[1:], format_sortie=c.format)
                elif c.mode_reference == "variation":
                    valeurs["variation"] = {"image": noms_refs[0], "denoise": c.liberte}
                else:
                    valeurs["ratio"] = c.format
                images = await comfy.executer(wf.preparer(valeurs), progression=self._suivi(c))
            if not images:
                raise ErreurPlateau("ComfyUI n'a renvoyé aucune image")
            c.images.append(enregistrer(images[0][1], {
                "type": "creation",
                "idee": c.idee,
                "prompt": c.prompt,
                "llm": c.llm,
                "workflow": c.workflow,
                "modele": c.modele or wf.modele_origine,
                "loras": c.loras,
                "format": c.format,
                "seed": seed,
                "parent": c.parent,
                "references": c.references,
                "mode_reference": c.mode_reference,
                "liberte": c.liberte if c.mode_reference == "variation" else None,
                **({"dossier": c.dossier} if c.dossier else {}),
                "duree_s": round(time.perf_counter() - debut_image + (duree_reecriture if i == 0 else 0), 2),
            }))


# Galerie : une image PNG + une fiche JSON par création.

def enregistrer(png: bytes, infos: dict) -> str:
    DOSSIER.mkdir(parents=True, exist_ok=True)
    id_creation = f"{datetime.now():%Y%m%d-%H%M%S}-{uuid.uuid4().hex[:6]}"
    largeur, hauteur = _dimensions_png(png)
    (DOSSIER / f"{id_creation}.png").write_bytes(png)
    fiche = {"id": id_creation, "date": datetime.now().isoformat(timespec="seconds"),
             "largeur": largeur, "hauteur": hauteur, **infos}
    (DOSSIER / f"{id_creation}.json").write_text(json.dumps(fiche, indent=4, ensure_ascii=False), encoding="utf-8")
    return id_creation


def enregistrer_clip(mp4: bytes, affiche: bytes, infos: dict) -> str:
    """Un clip : la vidéo, sa première image (vignette) et la fiche, sous le même identifiant."""
    id_clip = enregistrer(affiche, infos)
    (DOSSIER / f"{id_clip}.mp4").write_bytes(mp4)
    return id_clip


def importer(donnees: bytes, nom: str, dossier: str | None = None) -> str:
    """Ajoute une image personnelle à la galerie, pour la retoucher ou l'agrandir."""
    image = retouche.ouvrir(donnees)
    if max(image.size) > retouche.TAILLE_MAX:
        image.thumbnail((retouche.TAILLE_MAX, retouche.TAILLE_MAX), retouche.Image.Resampling.LANCZOS)
    return enregistrer(retouche.png(image), {
        "type": "import", "idee": "", "prompt": "", "llm": None, "workflow": None,
        "format": "importée", "seed": None, "parent": None, "nom_origine": nom[:120], "duree_s": 0,
        **({"dossier": dossier} if dossier else {}),
    })


def textes_origine(ids: list[str], profondeur: int = 20) -> list[str]:
    """Idées et prompts des rushes donnés et de leurs ancêtres (parents, références).
    AS-18 ne lit pas les images : un rush créé avec un personnage mineur le reste quand on le réutilise."""
    textes, vus, a_voir = [], set(), list(ids)
    while a_voir and len(vus) < profondeur:
        id_rush = a_voir.pop()
        if id_rush in vus:
            continue
        vus.add(id_rush)
        f = fiche(id_rush)
        textes += [t for t in (f.get("idee"), f.get("prompt")) if t]
        a_voir += [r for r in [f.get("parent"), *(f.get("references") or [])]
                   if r and _ID.match(r) and (DOSSIER / f"{r}.json").is_file()]
    return textes


def _verifier_id(id_creation: str):
    if not _ID.match(id_creation) or not (DOSSIER / f"{id_creation}.json").is_file():
        raise ErreurPlateau(f"Création inconnue : {id_creation}")


def fiche(id_creation: str) -> dict:
    _verifier_id(id_creation)
    return json.loads((DOSSIER / f"{id_creation}.json").read_text(encoding="utf-8"))


def liste(limite: int = 200) -> list[dict]:
    if not DOSSIER.is_dir():
        return []
    fichiers = sorted(DOSSIER.glob("*.json"), reverse=True)[:limite]
    return [json.loads(f.read_text(encoding="utf-8")) for f in fichiers]


def chemin_image(id_creation: str):
    _verifier_id(id_creation)
    return DOSSIER / f"{id_creation}.png"


def chemin_video(id_creation: str):
    _verifier_id(id_creation)
    video = DOSSIER / f"{id_creation}.mp4"
    if not video.is_file():
        raise ErreurPlateau(f"Pas de vidéo pour {id_creation}")
    return video


def supprimer(id_creation: str):
    _verifier_id(id_creation)
    (DOSSIER / f"{id_creation}.png").unlink(missing_ok=True)
    (DOSSIER / f"{id_creation}.mp4").unlink(missing_ok=True)
    (DOSSIER / f"{id_creation}.json").unlink()
