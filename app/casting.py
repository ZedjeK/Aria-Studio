"""Banc d'essai : mêmes prompts et mêmes seeds sur plusieurs workflows, avec mesures matérielles."""

import json
import random
import re
import shutil
import statistics
import time
from datetime import datetime

from pathlib import Path

import yaml

from app import garde, mesures, prompt, reglages, securite, workflows
from app.config import CONFIG, ROOT, chemin
from app.modeles import DOSSIER as MODELES
from app.orchestrateur import Orchestrateur
from app.telechargement import Telechargeur

DOSSIER = ROOT / "data" / "bancs"
SOURCES = ROOT / "casting" / "sources"
GO = 2**30
TEMOIN = "temoin"  # candidat du Casting LLM qui envoie l'idée brute, sans LLM
AVEC_SOURCE = ("retouche", "image_vers_video")  # tâches dont chaque test part d'une image de casting/sources


class ErreurBanc(RuntimeError):
    pass


def _go(octets: int) -> float:
    return round(octets / GO, 2)


def jeu_de_prompts(tache: str) -> list[dict]:
    fichier = CONFIG["casting"]["idees"] if tache == "reecriture" else CONFIG["casting"]["prompts"]
    with chemin(fichier).open(encoding="utf-8") as f:
        return yaml.safe_load(f)[tache]


def creer(tache: str, ids_workflows: list[str] | None = None) -> "Banc":
    return BancLLM() if tache == "reecriture" else Banc(tache, ids_workflows)


def _garde_vram() -> int:
    """Refuse de lancer un banc si d'autres applis occupent déjà la VRAM ; renvoie la VRAM au repos."""
    base = mesures.vram_utilisee()
    if base > CONFIG["casting"]["vram_max_occupee_go"] * GO:
        autres = ", ".join(s["nom"] for s in garde.coupables(garde.pids_aria())) or "inconnu"
        raise ErreurBanc(f"VRAM déjà occupée : {_go(base)} Go (probablement : {autres})")
    return base


class Banc:
    def __init__(self, tache: str, ids_workflows: list[str] | None = None):
        candidats = [wf for wf in workflows.catalogue() if wf.fiche["tache"] == tache]
        if ids_workflows:
            candidats = [wf for wf in candidats if wf.id in ids_workflows]
        if len(candidats) < 2:
            raise ErreurBanc(f"Il faut au moins deux workflows « {tache} » à comparer")
        pas_prets = [wf.id for wf in candidats if wf.manquants()]
        if pas_prets:
            raise ErreurBanc(f"Workflows incomplets : {', '.join(pas_prets)}")

        self.tache = tache
        self.workflows = candidats
        self.prompts = jeu_de_prompts(tache)
        self.seeds = list(CONFIG["casting"]["seeds"])
        if tache in AVEC_SOURCE:
            absentes = sorted({p["source"] for p in self.prompts if not (SOURCES / p["source"]).is_file()})
            if absentes:
                raise ErreurBanc(f"Images sources absentes de casting/sources : {', '.join(absentes)}")

        self.id = f"{datetime.now():%Y-%m-%d_%H%M%S}_{tache}"
        self.dossier = DOSSIER / self.id
        self.etat = "attente"
        self.erreur: str | None = None
        self.fait = 0
        self.total = len(self.workflows) * len(self.prompts) * len(self.seeds)
        self.courant = ""
        self.resultats = {
            "id": self.id,
            "tache": tache,
            "date": datetime.now().isoformat(timespec="seconds"),
            "seeds": self.seeds,
            "prompts": self.prompts,
            "vram_base_go": None,
            "workflows": {
                wf.id: {"nom": wf.fiche["nom"], "generations": [], "liberation": None, "resume": None}
                for wf in self.workflows
            },
            "aveugle": random.sample([wf.id for wf in self.workflows], len(self.workflows)),
            "revele": False,
            "notes": {},
            "favori": None,
        }

    def etat_public(self) -> dict:
        return {"id": self.id, "tache": self.tache, "etat": self.etat, "erreur": self.erreur,
                "fait": self.fait, "total": self.total, "courant": self.courant}

    def _sauver(self):
        self.dossier.mkdir(parents=True, exist_ok=True)
        (self.dossier / "resultats.json").write_text(json.dumps(self.resultats, indent=4, ensure_ascii=False), encoding="utf-8")

    def _lettre(self, cle: str) -> str:
        return LETTRES[self.resultats["aveugle"].index(cle)]

    async def executer(self, orchestrateur: Orchestrateur, telechargeur: Telechargeur):
        self.etat = "en_cours"
        try:
            self.resultats["vram_base_go"] = _go(_garde_vram())
        except ErreurBanc as e:
            self.etat, self.erreur = "erreur", str(e)
            return
        self._sauver()

        telechargeur.suspendre()
        try:
            async with orchestrateur.comfyui() as comfy:
                for wf in self.workflows:
                    await self._banc_workflow(comfy, wf)
            await orchestrateur.liberer()
            self.etat = "termine"
        except Exception as e:
            self.etat, self.erreur = "erreur", str(e) or repr(e)
            if await comfy_repond(orchestrateur):
                await orchestrateur.liberer()
        finally:
            telechargeur.reprendre()
            self.courant = ""
            self._sauver()

    async def _banc_workflow(self, comfy, wf: workflows.Workflow):
        res = self.resultats["workflows"][wf.id]
        (self.dossier / wf.id).mkdir(parents=True, exist_ok=True)
        await comfy.liberer_vram()  # chaque workflow part d'une VRAM vide : la 1re génération est « à froid »
        pid = comfy.process.pid if comfy.process else None

        for p in self.prompts:
            for seed in self.seeds:
                self.courant = f"Modèle {self._lettre(wf.id)} · {p['id']} · seed {seed}"  # jamais le nom : aveugle
                valeurs = {"prompt": p["prompt"], "seed": seed, "prefixe": f"aria_banc/{self.id}/{wf.id}"}
                if self.tache in AVEC_SOURCE:
                    valeurs["image"] = await comfy.televerser(SOURCES / p["source"])

                with mesures.Sondeur(pid) as sondeur:
                    debut = time.perf_counter()
                    images = await comfy.executer(wf.preparer(valeurs))
                    duree = time.perf_counter() - debut
                if not images:
                    raise ErreurBanc(f"{wf.id} n'a rien produit pour {p['id']}")

                nom = f"{p['id']}_{seed}{Path(images[0][0]).suffix or '.png'}"  # .png ou .mp4
                (self.dossier / wf.id / nom).write_bytes(images[0][1])
                res["generations"].append({
                    "prompt": p["id"],
                    "seed": seed,
                    "image": f"{wf.id}/{nom}",
                    "froid": not res["generations"],
                    "duree_s": round(duree, 2),
                    "pic_vram_go": _go(sondeur.pic_vram),
                    "pic_ram_go": _go(sondeur.pic_ram) if pid else None,
                })
                self.fait += 1
                self._sauver()

        avant = mesures.vram_utilisee()
        debut = time.perf_counter()
        await comfy.liberer_vram()
        res["liberation"] = {
            "vram_avant_go": _go(avant),
            "vram_apres_go": _go(mesures.vram_utilisee()),
            "duree_s": round(time.perf_counter() - debut, 2),
        }
        gens = res["generations"]
        chaud = [g["duree_s"] for g in gens if not g["froid"]]
        res["resume"] = {
            "temps_froid_s": gens[0]["duree_s"],
            "temps_chaud_moyen_s": round(statistics.mean(chaud), 2) if chaud else None,
            "pic_vram_go": max(g["pic_vram_go"] for g in gens),
            "pic_ram_go": max(g["pic_ram_go"] for g in gens) if pid else None,
        }
        self._sauver()


class BancLLM(Banc):
    """Casting LLM : chaque candidat réécrit les idées (phase llama), puis le workflow favori les rend (phase ComfyUI)."""

    def __init__(self):
        self.tache = "reecriture"
        self.candidats = list(CONFIG["casting"]["candidats_llm"])
        absents = [c for c in self.candidats if c != TEMOIN and not (MODELES / "llm" / c).is_file()]
        if absents:
            raise ErreurBanc(f"GGUF absents de modeles/llm : {', '.join(absents)}")
        if len(self.candidats) < 2:
            raise ErreurBanc("Il faut au moins deux candidats à comparer")
        id_rendu = CONFIG.get("generation", {}).get("texte_vers_image")
        if not id_rendu:
            raise ErreurBanc("Choisis d'abord un favori texte vers image dans le Casting")
        self.rendu = workflows.obtenir(id_rendu)
        if self.rendu.manquants():
            raise ErreurBanc(f"Workflow de rendu incomplet : {id_rendu}")

        self.prompts = jeu_de_prompts("reecriture")
        self.seeds = [CONFIG["casting"]["seed_llm"]]
        self.id = f"{datetime.now():%Y-%m-%d_%H%M%S}_reecriture"
        self.dossier = DOSSIER / self.id
        self.etat = "attente"
        self.erreur = None
        self.fait = 0
        self.total = 2 * len(self.candidats) * len(self.prompts)  # réécriture puis rendu
        self.courant = ""
        cles = [self._cle(c) for c in self.candidats]
        self.resultats = {
            "id": self.id,
            "tache": self.tache,
            "date": datetime.now().isoformat(timespec="seconds"),
            "seeds": self.seeds,
            "prompts": self.prompts,
            "rendu": id_rendu,
            "vram_base_go": None,
            "workflows": {
                self._cle(c): {
                    "nom": "Sans LLM (témoin)" if c == TEMOIN else self._cle(c),
                    "fichier": None if c == TEMOIN else c,
                    "generations": [], "liberation": None, "resume": None,
                }
                for c in self.candidats
            },
            "aveugle": random.sample(cles, len(cles)),
            "revele": False,
            "notes": {},
            "favori": None,
        }

    @staticmethod
    def _cle(candidat: str) -> str:
        return candidat.removesuffix(".gguf")

    async def executer(self, orchestrateur: Orchestrateur, telechargeur: Telechargeur):
        self.etat = "en_cours"
        try:
            self.resultats["vram_base_go"] = _go(_garde_vram())
        except ErreurBanc as e:
            self.etat, self.erreur = "erreur", str(e)
            return
        self._sauver()

        telechargeur.suspendre()
        try:
            for candidat in self.candidats:
                await self._reecrire(orchestrateur, candidat)
            async with orchestrateur.comfyui() as comfy:  # arrête llama avant de démarrer ComfyUI
                for candidat in self.candidats:
                    await self._rendre(comfy, candidat)
            await orchestrateur.liberer()
            self.etat = "termine"
        except Exception as e:
            self.etat, self.erreur = "erreur", str(e) or repr(e)
            await orchestrateur.liberer()
        finally:
            telechargeur.reprendre()
            self.courant = ""
            self._sauver()

    async def _reecrire(self, orchestrateur: Orchestrateur, candidat: str):
        cle = self._cle(candidat)
        res = self.resultats["workflows"][cle]
        (self.dossier / cle).mkdir(parents=True, exist_ok=True)
        seed = self.seeds[0]

        if candidat == TEMOIN:
            for p in self.prompts:
                res["generations"].append(self._generation(cle, p, seed, p["prompt"]))
                self.fait += 1
            res["resume"] = {"controles_reussis": self._controles(res["generations"])}
            self._sauver()
            return

        self.courant = f"Modèle {self._lettre(cle)} · chargement"
        debut = time.perf_counter()
        regle = reglages.valeurs_llm(candidat)  # le banc mesure le GGUF tel qu'il est réglé
        async with orchestrateur.llm(candidat) as llama:
            chargement = time.perf_counter() - debut
            for p in self.prompts:
                self.courant = f"Modèle {self._lettre(cle)} · réécriture · {p['id']}"
                securite.verifier(p["prompt"])
                with mesures.Sondeur(llama.process.pid) as sondeur:
                    debut = time.perf_counter()
                    reponse = await llama.generer(prompt.messages(p["prompt"], candidat), seed=seed,
                                                  temperature=regle["temperature_reecriture"],
                                                  max_tokens=regle["max_tokens"],
                                                  **reglages.echantillonnage(candidat))
                    duree = time.perf_counter() - debut
                securite.verifier(p["prompt"], reponse.texte)
                g = self._generation(cle, p, seed, reponse.texte)
                g.update({"duree_s": round(duree, 2), "ecriture_tps": reponse.ecriture_tps,
                          "pic_vram_go": _go(sondeur.pic_vram)})
                res["generations"].append(g)
                self.fait += 1
                self._sauver()

        gens = res["generations"]
        res["resume"] = {
            "chargement_s": round(chargement, 2),
            "duree_moyenne_s": round(statistics.mean(g["duree_s"] for g in gens), 2),
            "ecriture_tps_moyen": round(statistics.mean(g["ecriture_tps"] for g in gens), 1),
            "pic_vram_go": max(g["pic_vram_go"] for g in gens),
            "controles_reussis": self._controles(gens),
        }
        self._sauver()

    @staticmethod
    def _controles(gens: list[dict]) -> str:
        resultats = [v for g in gens for k, v in g["controles"].items() if k != "mots" and v is not None]
        return f"{sum(resultats)}/{len(resultats)}"

    @staticmethod
    def _generation(cle: str, p: dict, seed: int, texte: str) -> dict:
        return {"prompt": p["id"], "seed": seed, "image": f"{cle}/{p['id']}_{seed}.png",
                "texte": texte, "controles": prompt.controler(texte, p["prompt"])}

    async def _rendre(self, comfy, candidat: str):
        cle = self._cle(candidat)
        for g in self.resultats["workflows"][cle]["generations"]:
            self.courant = f"Modèle {self._lettre(cle)} · rendu · {g['prompt']}"
            debut = time.perf_counter()
            images = await comfy.executer(self.rendu.preparer(
                {"prompt": g["texte"], "seed": g["seed"], "prefixe": f"aria_banc/{self.id}/{cle}"}))
            if not images:
                raise ErreurBanc(f"Aucune image rendue pour {cle} · {g['prompt']}")
            (self.dossier / g["image"]).write_bytes(images[0][1])
            g["rendu_s"] = round(time.perf_counter() - debut, 2)
            self.fait += 1
            self._sauver()


async def comfy_repond(orchestrateur: Orchestrateur) -> bool:
    return orchestrateur.actif == "comfyui" and await orchestrateur.comfy.est_pret()


def liste() -> list[dict]:
    if not DOSSIER.is_dir():
        return []
    bancs = []
    for f in sorted(DOSSIER.glob("*/resultats.json"), reverse=True):
        r = _completer(json.loads(f.read_text(encoding="utf-8")))
        bancs.append({"id": r["id"], "tache": r["tache"], "date": r["date"], "revele": r["revele"],
                       "nb_workflows": len(r["workflows"]), "favori": r["favori"] if r["revele"] else None})
    return bancs


# Résultats d'un banc terminé : comparaison à l'aveugle, notes, favori.

LETTRES = "ABCDEFGH"
ORIGINE_SOURCES = SOURCES / "origine.json"


def _fichier(id_banc: str):
    fichier = (DOSSIER / id_banc / "resultats.json").resolve()
    if not fichier.is_relative_to(DOSSIER.resolve()) or not fichier.is_file():
        raise ErreurBanc(f"Banc inconnu : {id_banc}")
    return fichier


def _completer(r: dict) -> dict:
    """Ajoute l'ordre à l'aveugle aux bancs créés avant son introduction."""
    if "aveugle" not in r:
        r["aveugle"] = random.sample(list(r["workflows"]), len(r["workflows"]))
        r["revele"] = False
        (DOSSIER / r["id"] / "resultats.json").write_text(json.dumps(r, indent=4, ensure_ascii=False), encoding="utf-8")
    return r


def charger(id_banc: str) -> dict:
    return _completer(json.loads(_fichier(id_banc).read_text(encoding="utf-8")))


def _sauver(r: dict):
    _fichier(r["id"]).write_text(json.dumps(r, indent=4, ensure_ascii=False), encoding="utf-8")


def _workflow_de(r: dict, cle: str) -> str:
    """`cle` est une lettre tant que le banc n'est pas révélé, l'id du workflow ensuite."""
    if not r["revele"]:
        if len(cle) != 1 or LETTRES.find(cle) not in range(len(r["aveugle"])):
            raise ErreurBanc(f"Modèle inconnu : {cle}")
        return r["aveugle"][LETTRES.index(cle)]
    if cle not in r["workflows"]:
        raise ErreurBanc(f"Workflow inconnu : {cle}")
    return cle


def origine_sources() -> dict:
    if not ORIGINE_SOURCES.is_file():
        return {}
    return json.loads(ORIGINE_SOURCES.read_text(encoding="utf-8"))


def vue(id_banc: str) -> dict:
    """Résultats présentables : noms, chemins et mesures masqués tant que le banc n'est pas révélé."""
    r = charger(id_banc)
    sources = origine_sources()
    noms_sources = {p["source"] for p in jeu_de_prompts("retouche")} if r["tache"] == "texte_vers_image" else set()
    modeles = []
    for i, wid in enumerate(r["aveugle"]):
        w = r["workflows"][wid]
        cle = wid if r["revele"] else LETTRES[i]
        generations = []
        for g in w["generations"]:
            fichier = g["image"].split("/", 1)[1]
            source = f"{g['prompt']}.png"
            generations.append({
                "prompt": g["prompt"],
                "seed": g["seed"],
                "fichier": fichier,
                "url": f"/api/bancs/{id_banc}/images/{cle}/{fichier}",
                "note": r["notes"].get(g["image"]),
                "peut_servir_de_source": source in noms_sources,
                "est_la_source": sources.get(source) == {"banc": id_banc, "image": g["image"]},
                # Le texte du prompt trahirait le témoin (resté en français) : masqué jusqu'à la révélation.
                "texte": g.get("texte") if r["revele"] else None,
                "controles": g.get("controles") if r["revele"] else None,
            })
        modeles.append({
            "cle": cle,
            "nom": w["nom"] if r["revele"] else f"Modèle {LETTRES[i]}",
            "generations": generations,
            "resume": w["resume"] if r["revele"] else None,
            "liberation": w["liberation"] if r["revele"] else None,
        })
    notes = list(r["notes"].values())
    total = sum(len(w["generations"]) for w in r["workflows"].values())
    return {
        "id": r["id"], "tache": r["tache"], "date": r["date"], "seeds": r["seeds"], "prompts": r["prompts"],
        "vram_base_go": r["vram_base_go"], "revele": r["revele"],
        "favori": r["favori"] if r["revele"] else None,
        "notees": len(notes), "total": total,
        "modeles": modeles,
    }


def chemin_image(id_banc: str, cle: str, fichier: str):
    r = charger(id_banc)
    image = (DOSSIER / id_banc / _workflow_de(r, cle) / fichier).resolve()
    if not image.is_relative_to((DOSSIER / id_banc).resolve()) or not image.is_file():
        raise ErreurBanc("Image introuvable")
    return image


def noter(id_banc: str, cle: str, fichier: str, note: int):
    if note not in range(1, 6):
        raise ErreurBanc("La note va de 1 à 5")
    r = charger(id_banc)
    image = f"{_workflow_de(r, cle)}/{fichier}"
    if not any(g["image"] == image for w in r["workflows"].values() for g in w["generations"]):
        raise ErreurBanc("Image inconnue")
    r["notes"][image] = note
    _sauver(r)


def reveler(id_banc: str):
    r = charger(id_banc)
    r["revele"] = True
    _sauver(r)


def choisir_favori(id_banc: str, id_workflow: str):
    r = charger(id_banc)
    if not r["revele"]:
        raise ErreurBanc("Révèle d'abord les modèles")
    _workflow_de(r, id_workflow)
    r["favori"] = id_workflow
    _sauver(r)
    if r["tache"] == "reecriture":
        # Le témoin gagnant signifie « pas de LLM » : modèle vide.
        _ecrire_config("llama", "modele", r["workflows"][id_workflow]["fichier"] or "")
    else:
        _ecrire_config("generation", r["tache"], id_workflow)


def _ecrire_config(section: str, cle: str, valeur: str):
    """Met à jour <section>.<cle> dans config.yaml sans toucher au reste du fichier (ni aux commentaires)."""
    fichier = ROOT / "config.yaml"
    texte = fichier.read_text(encoding="utf-8")
    motif = re.compile(rf"(^{section}:\n(?:[ \t].*\n|\n)*?[ \t]+{cle}:)[^\n]*", re.MULTILINE)
    nouveau, n = motif.subn(lambda m: f'{m.group(1)} "{valeur}"', texte, count=1)
    if n != 1:
        raise ErreurBanc(f"Entrée {section}.{cle} introuvable dans config.yaml")
    fichier.write_text(nouveau, encoding="utf-8")
    CONFIG.setdefault(section, {})[cle] = valeur


def utiliser_comme_source(id_banc: str, cle: str, fichier: str) -> str:
    """Fige une image d'un banc texte vers image comme source du banc de retouche."""
    r = charger(id_banc)
    if r["tache"] != "texte_vers_image":
        raise ErreurBanc("Seules les images d'un banc texte vers image peuvent servir de source")
    image = chemin_image(id_banc, cle, fichier)
    prompt = next((g["prompt"] for w in r["workflows"].values() for g in w["generations"]
                   if g["image"] == f"{_workflow_de(r, cle)}/{fichier}"), None)
    source = f"{prompt}.png"
    if source not in {p["source"] for p in jeu_de_prompts("retouche")}:
        raise ErreurBanc(f"Aucune retouche n'utilise de source « {prompt} »")
    SOURCES.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(image, SOURCES / source)
    origine = origine_sources()
    origine[source] = {"banc": id_banc, "image": f"{_workflow_de(r, cle)}/{fichier}"}
    ORIGINE_SOURCES.write_text(json.dumps(origine, indent=4, ensure_ascii=False), encoding="utf-8")
    return source
