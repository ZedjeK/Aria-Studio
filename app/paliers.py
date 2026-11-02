"""Banc de palier : la VRAM qu'un modèle demande vraiment, mesurée en simulant des cartes plus petites.

Pour simuler une carte de P Go sur la carte de mesure, un « lest » (un petit processus PyTorch) occupe réellement la
VRAM en trop : ComfyUI, Llama et le pilote ne voient plus que P Go, bureau Windows compris, comme chez un utilisateur
qui aurait cette carte. (L'option --reserve-vram de ComfyUI ne changerait que ses propres comptes : le pilote pourrait
encore lui donner plus.) Sous Windows, le pilote déborde en mémoire partagée au lieu d'échouer : une petite carte se
voit donc surtout au temps.

Pour chaque modèle, du plus grand palier au plus petit, avec la même génération (un prompt du Casting ; la vidéo en
Rapide : 5 s, 0,4 MP, 8 steps) :
    minimum = le plus petit palier où la génération aboutit dans le temps imparti ;
    confort = le plus petit palier où elle prend au plus CONFORT fois le temps du plus grand palier.
Un palier qui échoue arrête les paliers plus petits.

Chaque palier commence par une génération de chauffe (seed SEED), puis mesure une autre génération (seed SEED + 1) :
le banc du 2026-10-26 était faux, la 2e génération identique sortait du cache de ComfyUI (0,1 s) et la 1re vidéo
mesurait surtout la lecture du disque (325 s à 16 Go, 74 s ensuite). Un LLM est lancé comme Aria le lance
(orchestrateur.lancement_llm : moteur, vision, contexte), son chargement compté à part de la réécriture.

Usage : python -m app.paliers [ids de workflows ou de GGUF…]   (sans argument : tout le catalogue)
        python -m app.paliers --ecrire [dossier de résultats]   (reporte les mesures dans les fiches)
"""

import asyncio
import json
import re
import subprocess
import sys
import time
from datetime import date, datetime
from pathlib import Path

import httpx
import psutil
import yaml

from app import fiches, mesures, prompt, reglages, workflows
from app.orchestrateur import lancement_llm
from app.casting import SOURCES, jeu_de_prompts
from app.comfy import ComfyUI, ErreurComfy
from app.config import CONFIG
from app.emplacements import DATA, moteur
from app.llm import ErreurLlama, Llama
from app.modeles import CHEMINS_COMFYUI, DOSSIER as MODELES

GO = 2**30
PALIERS = (16, 12, 8)
CONFORT = 1.5
DOSSIER = DATA / "bancs" / "paliers"
SEED = 42
VIDEO = {"duree": 5, "megapixels": 0.4, "steps": 8}  # Rapide : ce qu'un utilisateur d'une petite carte lancerait
LIMITE_MIN_S = 600  # une génération est abandonnée au-delà de max(LIMITE_MIN_S, 4 × le temps du plus grand palier)
OCCUPATION_MAX_GO = 3.5  # comme le garde-VRAM : au-delà, une autre appli tient la carte et les mesures seraient faussées
LEST = """
import sys, torch
morceaux = []
for ligne in sys.stdin:  # « + » : 128 Mo de plus, écrits (Windows ne réserve la mémoire qu'à l'usage)
    morceaux.append(torch.ones(128 << 20, dtype=torch.uint8, device="cuda"))
    torch.cuda.synchronize()
    print("ok", flush=True)
"""
_MEMOIRE = re.compile(r"out of memory|outofmemory|cuda error: out|allocation on device|not enough memory", re.I)


class ErreurPalier(RuntimeError):
    pass


def libre_vise(palier: int, totale: int, occupee: int) -> int:
    """VRAM libre que laisserait une carte de `palier` Go faisant tourner le même bureau : la carte de mesure
    (totale) ramenée à palier/16 de sa taille, moins ce que le bureau occupe déjà."""
    return int(totale * palier / 16) - occupee


class Lest:
    """Occupe la VRAM pour simuler une carte de `palier` Go (rien à faire au palier de la carte de mesure)."""

    def __init__(self, palier: int):
        self.palier, self.process = palier, None

    async def __aenter__(self):
        totale, occupee = mesures.vram_totale(), mesures.vram_utilisee()
        if self.palier * GO >= totale * 0.98:  # la carte de mesure elle-même
            return self
        # Le lest ajoute 128 Mo à la demande jusqu'à ce que NVML (la mesure du garde-VRAM) voie la VRAM libre visée :
        # PyTorch et NVML ne comptent pas pareil sous Windows (1,1 Go d'écart mesuré).
        vise = libre_vise(self.palier, totale, occupee)
        python = moteur(CONFIG["comfyui"]["dossier"]) / "python_embeded" / "python.exe"
        self.process = subprocess.Popen([str(python), "-c", LEST], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        text=True, creationflags=subprocess.CREATE_NO_WINDOW)
        while totale - mesures.vram_utilisee() > vise + (64 << 20):
            self.process.stdin.write("+\n")
            self.process.stdin.flush()
            if not (await asyncio.to_thread(self.process.stdout.readline)).startswith("ok"):
                await self.__aexit__()
                raise ErreurPalier(f"Le lest n'a pas pu occuper la VRAM pour simuler {self.palier} Go")
        return self

    async def __aexit__(self, *_):
        if self.process and self.process.poll() is None:
            self.process.stdin.close()  # le lest attend la fin de son entrée pour s'arrêter
            try:
                await asyncio.to_thread(self.process.wait, 15)
            except subprocess.TimeoutExpired:
                self.process.kill()
        self.process = None


def etat_erreur(message: str) -> str:
    return "memoire" if _MEMOIRE.search(message or "") else "erreur"


def conclure(paliers: dict) -> tuple[int | None, int | None]:
    """(minimum, confort) d'après les mesures {palier: {"etat", "temps_s"}} ; None si le plus grand palier échoue."""
    reussis = {p: m["temps_s"] for p, m in paliers.items() if m["etat"] == "ok"}
    if not reussis or max(paliers) not in reussis:
        return None, None
    reference = reussis[max(paliers)]
    confort = [p for p, t in reussis.items() if t <= CONFORT * reference]
    return min(reussis), min(confort)


# Mesures

def _valeurs(wf: workflows.Workflow, seed: int) -> tuple[dict, str | None]:
    """La génération de référence : le premier prompt du Casting pour la tâche, Rapide pour la vidéo."""
    p = jeu_de_prompts(wf.fiche["tache"])[0]
    valeurs = {"prompt": p["prompt"], "seed": seed, "prefixe": "aria_paliers"}
    if wf.fiche["tache"].endswith("video"):
        valeurs.update(VIDEO)
    return valeurs, p.get("source")


async def _generer(comfy: ComfyUI, wf: workflows.Workflow, limite: float, seed: int) -> dict:
    valeurs, source = _valeurs(wf, seed)
    if source:
        valeurs["image"] = await comfy.televerser(SOURCES / source)
    api = wf.preparer(valeurs)
    if wf.fiche["tache"].endswith("video"):
        wf.ajouter_affiche(api, "aria_paliers")
    debut = time.perf_counter()
    try:
        sorties = await asyncio.wait_for(comfy.executer(api), limite)
    except asyncio.TimeoutError:
        async with httpx.AsyncClient(timeout=10) as client:
            await client.post(f"{comfy.url}/interrupt")
        return {"etat": "trop_lent", "temps_s": round(time.perf_counter() - debut, 1)}
    except ErreurComfy as e:
        return {"etat": etat_erreur(str(e)), "temps_s": round(time.perf_counter() - debut, 1), "detail": str(e)[:300]}
    if not sorties:
        return {"etat": "erreur", "temps_s": round(time.perf_counter() - debut, 1), "detail": "aucune sortie"}
    return {"etat": "ok", "temps_s": round(time.perf_counter() - debut, 1)}


async def mesurer_workflow(wf: workflows.Workflow, journal) -> dict:
    """La même génération à chaque palier, ComfyUI relancé sous le lest : une chauffe (modèles chargés, disque lu),
    puis la mesure avec une autre seed (le cache de ComfyUI ne la sert pas)."""
    comfy = ComfyUI(moteur(CONFIG["comfyui"]["dossier"]), CONFIG["comfyui"]["port"], CHEMINS_COMFYUI)
    resultats, reference = {}, None
    for palier in PALIERS:
        limite = max(LIMITE_MIN_S, 4 * reference) if reference else 3 * LIMITE_MIN_S
        async with Lest(palier):
            try:
                await comfy.demarrer()
                essais = [await _generer(comfy, wf, limite, SEED)]
                if essais[0]["etat"] == "ok":
                    essais.append(await _generer(comfy, wf, limite, SEED + 1))
            finally:
                await comfy.arreter()
        mesure = {**essais[-1], "essais": [e["temps_s"] for e in essais]}
        resultats[palier] = mesure
        journal(f"{wf.id} · {palier} Go : {mesure['etat']} en {mesure['temps_s']} s")
        if mesure["etat"] != "ok":
            break
        reference = reference or mesure["temps_s"]
    return resultats


async def mesurer_llm(fichier: str, journal) -> dict:
    """Une réécriture (la première idée du Casting LLM) à chaque palier, le LLM lancé comme Aria le lance (moteur,
    vision, contexte, réflexion) ; llama.cpp place lui-même les couches (--fit). temps_s : la réécriture seule."""
    fiche = fiches.fiche_llm(fichier)
    lancement = lancement_llm(fichier)
    idee = jeu_de_prompts("reecriture")[0]["prompt"]
    resultats, reference = {}, None
    for palier in PALIERS:
        llama = Llama(lancement["serveur"], CONFIG["llama"]["port"], CONFIG["llama"]["contexte"],
                      CONFIG["llama"]["reflexion"])
        async with Lest(palier):
            debut = time.perf_counter()
            try:
                regle = reglages.valeurs_llm(fichier)
                await llama.demarrer(MODELES / "llm" / fichier, **lancement)
                chargement = time.perf_counter() - debut
                debut = time.perf_counter()
                reponse = await llama.generer(prompt.messages(idee, fichier), seed=SEED, temperature=0.0,
                                              max_tokens=regle["max_tokens"], **reglages.echantillonnage(fichier))
                mesure = {"etat": "ok", "temps_s": round(time.perf_counter() - debut, 1),
                          "chargement_s": round(chargement, 1), "jetons_s": reponse.ecriture_tps}
            except ErreurLlama as e:
                mesure = {"etat": etat_erreur(str(e)), "temps_s": round(time.perf_counter() - debut, 1),
                          "detail": str(e)[:300]}
            finally:
                await llama.arreter()
        resultats[palier] = mesure
        journal(f"{fiche['id'] if fiche else fichier} · {palier} Go : {mesure['etat']} en {mesure['temps_s']} s")
        if mesure["etat"] != "ok":
            break
        reference = reference or mesure["temps_s"]
    return resultats


async def banc(ids: list[str] | None = None) -> dict:
    """Mesure les workflows et les LLM demandés (tout le catalogue par défaut) ; écrit data/bancs/paliers/<date>/."""
    if mesures.vram_utilisee() > OCCUPATION_MAX_GO * GO:
        raise ErreurPalier(f"La carte est déjà occupée ({mesures.vram_utilisee() / GO:.1f} Go) : ferme les autres "
                           "applis, les mesures seraient faussées")
    comfy = ComfyUI(moteur(CONFIG["comfyui"]["dossier"]), CONFIG["comfyui"]["port"], CHEMINS_COMFYUI)
    if await comfy.est_pret():
        raise ErreurPalier("Un ComfyUI tourne déjà sur le port d'Aria : arrête le serveur d'Aria pendant le banc")
    if await Llama(Path("llama-server.exe"), CONFIG["llama"]["port"], 0, False).est_pret():
        raise ErreurPalier("Un llama-server tourne déjà sur le port d'Aria : arrête le serveur d'Aria pendant le banc")
    cibles_wf = sorted((wf for wf in workflows.catalogue() if not ids or wf.id in ids),
                       key=lambda wf: wf.fiche["tache"].endswith("video"))  # les images d'abord : plus courtes
    cibles_llm = [f["id"] for f in fiches.catalogue_llm() if (not ids or f["id"] in ids)
                  and (MODELES / "llm" / f["id"]).is_file()]
    dossier = DOSSIER / f"{datetime.now():%Y-%m-%d_%H%M%S}"
    dossier.mkdir(parents=True, exist_ok=True)
    resultats = {"date": date.today().isoformat(), "carte": f"{mesures.nom_gpu()} "
                 f"({mesures.vram_totale() / GO:.0f} Go), {psutil.virtual_memory().total / GO:.0f} Go de RAM",
                 "paliers": list(PALIERS), "confort": CONFORT, "workflows": {}, "llm": {}}

    def journal(ligne):
        print(f"{datetime.now():%H:%M:%S} {ligne}", flush=True)
        (dossier / "resultats.json").write_text(json.dumps(resultats, indent=4, ensure_ascii=False), encoding="utf-8")

    for wf in cibles_wf:
        if wf.manquants():
            journal(f"{wf.id} : fichiers manquants, pas mesuré")
            continue
        resultats["workflows"][wf.id] = await mesurer_workflow(wf, journal)
        journal(f"{wf.id} : minimum, confort = {conclure(resultats['workflows'][wf.id])}")
    for fichier in cibles_llm:
        resultats["llm"][fichier] = await mesurer_llm(fichier, journal)
        journal(f"{fichier} : minimum, confort = {conclure(resultats['llm'][fichier])}")
    return resultats


# Report dans les fiches

def bloc_mesure(paliers: dict, carte: str, jour: str, indentation: str) -> str:
    """Le bloc `mesure` d'une fiche, en YAML, à l'indentation du bloc `materiel`."""
    detail = ", ".join(f"{p} Go : {m['temps_s']} s" if m["etat"] == "ok" else f"{p} Go : {m['etat']}"
                       for p, m in sorted(paliers.items(), key=lambda x: -int(x[0])))
    methode = (f"banc de palier (app/paliers.py) : VRAM réellement occupée par un lest pour simuler "
               f"{', '.join(map(str, PALIERS))} Go ; confort = au plus {CONFORT:g} × le temps du plus grand palier")
    lignes = [f"mesure:", f"    carte: {carte}", f"    date: {jour}", f"    methode: \"{methode}\"",
              f"    paliers: \"{detail}\""]
    return ("\n" + indentation).join(lignes)


def reporter(texte: str, ancre: str, paliers: dict, carte: str, jour: str) -> str:
    """Remplace vram_min_go, vram_confort_go et mesure dans le bloc `materiel` qui suit `ancre` dans le texte d'une
    fiche (les commentaires et le reste restent tels quels)."""
    debut = texte.index(ancre)
    m = re.compile(r"(?m)^( *)vram_min_go: .*\n\1vram_confort_go: .*\n\1mesure: null.*$").search(texte, debut)
    if not m:
        raise ErreurPalier(f"Bloc materiel introuvable après « {ancre} »")
    minimum, confort = conclure({int(p): v for p, v in paliers.items()})
    ind = m.group(1)
    nouveau = (f"{ind}vram_min_go: {minimum if minimum else 'null'}  # mesurée (banc de palier)\n"
               f"{ind}vram_confort_go: {confort if confort else 'null'}\n"
               f"{ind}{bloc_mesure(paliers, carte, jour, ind)}")
    return texte[:m.start()] + nouveau + texte[m.end():]


def ecrire(resultats: dict):
    """Reporte un banc dans les fiches de workflow et le catalogue des LLM ; chaque fiche est revalidée."""
    for id_, paliers in resultats["workflows"].items():
        f = workflows.DOSSIER / id_ / "fiche.yaml"
        texte = reporter(f.read_text(encoding="utf-8"), "materiel:", paliers, resultats["carte"], resultats["date"])
        if p := fiches.problemes_workflow(yaml.safe_load(texte)):
            raise ErreurPalier(f"{id_} : " + " ; ".join(p))
        f.write_text(texte, encoding="utf-8")
    if resultats["llm"]:
        texte = fiches.CATALOGUE_LLM.read_text(encoding="utf-8")
        for fichier, paliers in resultats["llm"].items():
            texte = reporter(texte, f"id: {fichier}", paliers, resultats["carte"], resultats["date"])
        fiches.CATALOGUE_LLM.write_text(texte, encoding="utf-8")
        fiches.catalogue_llm()  # revalide


if __name__ == "__main__":
    if "--ecrire" in sys.argv:
        args = [a for a in sys.argv[1:] if a != "--ecrire"]
        dossier = DOSSIER / args[0] if args else max(DOSSIER.iterdir())
        ecrire(json.loads((dossier / "resultats.json").read_text(encoding="utf-8")))
        print(f"Fiches mises à jour depuis {dossier.name}")
    else:
        asyncio.run(banc(sys.argv[1:] or None))
