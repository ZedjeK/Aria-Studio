"""File de téléchargement : un fichier à la fois, reprise via Range, vérification SHA-256."""

import asyncio
import hashlib
import time
import uuid
from dataclasses import dataclass, field
from pathlib import Path

import httpx

from app import modeles
from app.sources import Fiche, entetes

BLOC = 1 << 20
MARGE_DISQUE = 1 << 30  # on garde au moins 1 Go libre
ESSAIS_RESEAU = 5


class ErreurTelechargement(RuntimeError):
    pass


class _Annule(Exception):
    pass


class _Pause(Exception):
    pass


@dataclass
class Tache:
    fiche: Fiche
    dossier: str
    id: str = field(default_factory=lambda: uuid.uuid4().hex[:12])
    etat: str = "attente"  # attente, en_cours, pause, verification, termine, erreur, annule
    recu: int = 0
    debit: float = 0.0  # octets par seconde
    erreur: str | None = None
    hacheur: "hashlib._Hash" = field(default_factory=hashlib.sha256, repr=False)

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "nom": self.fiche.nom,
            "dossier": self.dossier,
            "taille": self.fiche.taille,
            "etat": self.etat,
            "recu": self.recu,
            "debit": self.debit,
            "erreur": self.erreur,
        }


def _hacher(fichier: Path):
    h = hashlib.sha256()
    with fichier.open("rb") as f:
        while bloc := f.read(16 * BLOC):
            h.update(bloc)
    return h


class Telechargeur:
    def __init__(self):
        self.taches: dict[str, Tache] = {}
        self._file: asyncio.Queue[Tache] = asyncio.Queue()
        self._autorise = asyncio.Event()
        self._autorise.set()
        self._annulations: set[str] = set()
        self._boucle_tache: asyncio.Task | None = None

    def demarrer(self):
        self._boucle_tache = asyncio.create_task(self._boucle())

    async def arreter(self):
        if self._boucle_tache:
            self._boucle_tache.cancel()
            await asyncio.gather(self._boucle_tache, return_exceptions=True)

    def suspendre(self):
        """Met la file en pause (pendant un banc d'essai) ; le .part est conservé."""
        self._autorise.clear()

    def reprendre(self):
        self._autorise.set()

    def ajouter(self, fiche: Fiche, dossier: str) -> Tache:
        if fiche.jeton_requis:
            raise ErreurTelechargement("Ce modèle est protégé : ajoute ton jeton d'accès dans l'environnement")
        dest = modeles.destination(dossier, fiche.nom)
        if dest.exists():
            raise ErreurTelechargement(f"Déjà présent : {modeles.cle(dest)}")
        if any(t.etat in ("attente", "en_cours", "pause", "verification") and t.fiche.url == fiche.url for t in self.taches.values()):
            raise ErreurTelechargement("Ce fichier est déjà dans la file")
        part = dest.with_name(dest.name + ".part")
        deja = part.stat().st_size if part.exists() else 0
        if modeles.espace_libre() - (fiche.taille - deja) < MARGE_DISQUE:
            raise ErreurTelechargement("Espace disque insuffisant")
        tache = Tache(fiche, dossier)
        self.taches[tache.id] = tache
        self._file.put_nowait(tache)
        return tache

    def annuler(self, id_tache: str):
        tache = self.taches.get(id_tache)
        if tache is None:
            raise ErreurTelechargement("Téléchargement inconnu")
        if tache.etat == "attente":
            tache.etat = "annule"
        elif tache.etat in ("en_cours", "pause"):
            self._annulations.add(id_tache)

    async def _boucle(self):
        while True:
            tache = await self._file.get()
            if tache.etat == "annule":
                continue
            await self._autorise.wait()
            try:
                await self._telecharger(tache)
            except _Annule:
                tache.etat = "annule"
            except Exception as e:
                tache.etat = "erreur"
                tache.erreur = str(e)
            finally:
                self._annulations.discard(tache.id)
                tache.debit = 0.0

    async def _telecharger(self, tache: Tache):
        fiche = tache.fiche
        dest = modeles.destination(tache.dossier, fiche.nom)
        dest.parent.mkdir(parents=True, exist_ok=True)
        part = dest.with_name(dest.name + ".part")

        if part.exists():
            tache.etat = "verification"  # relecture du .part laissé par une session précédente
            tache.hacheur = await asyncio.to_thread(_hacher, part)
            tache.recu = part.stat().st_size

        essais = 0
        try:
            async with httpx.AsyncClient(follow_redirects=True, timeout=httpx.Timeout(30, read=60)) as client:
                while tache.recu < fiche.taille:
                    try:
                        await self._recevoir(client, tache, part)
                        essais = 0
                    except _Pause:
                        tache.etat = "pause"
                        tache.debit = 0.0
                        while not self._autorise.is_set():
                            if tache.id in self._annulations:
                                raise _Annule
                            await asyncio.sleep(0.5)
                    except httpx.TransportError as e:
                        essais += 1
                        if essais > ESSAIS_RESEAU:
                            raise ErreurTelechargement(f"Réseau : {e!r}")
                        tache.debit = 0.0
                        await asyncio.sleep(2 ** essais)
        except _Annule:
            part.unlink(missing_ok=True)
            raise

        tache.etat = "verification"
        if tache.recu != fiche.taille:
            raise ErreurTelechargement(f"Taille incorrecte : {tache.recu} octets au lieu de {fiche.taille}")
        sha256 = tache.hacheur.hexdigest()
        if fiche.sha256 and sha256 != fiche.sha256.lower():
            part.unlink(missing_ok=True)
            raise ErreurTelechargement("Empreinte SHA-256 incorrecte : fichier rejeté")
        part.replace(dest)
        modeles.enregistrer(dest, fiche.to_dict(), sha256)
        tache.etat = "termine"

    async def _recevoir(self, client: httpx.AsyncClient, tache: Tache, part: Path):
        en_tetes = entetes(tache.fiche.source)
        if tache.recu:
            en_tetes["Range"] = f"bytes={tache.recu}-"
        async with client.stream("GET", tache.fiche.url, headers=en_tetes) as r:
            if r.status_code in (401, 403):
                raise ErreurTelechargement("Accès refusé : jeton manquant, ou licence à accepter sur la page du modèle")
            if tache.recu and r.status_code == 200:
                # Le serveur ignore la reprise : on repart de zéro.
                tache.recu = 0
                tache.hacheur = hashlib.sha256()
            elif r.status_code not in (200, 206):
                raise ErreurTelechargement(f"Réponse HTTP {r.status_code}")

            tache.etat = "en_cours"
            fenetre_t, fenetre_octets = time.monotonic(), tache.recu
            with part.open("ab" if tache.recu else "wb") as f:
                async for bloc in r.aiter_bytes(BLOC):
                    if tache.id in self._annulations:
                        raise _Annule
                    if not self._autorise.is_set():
                        raise _Pause
                    f.write(bloc)
                    tache.hacheur.update(bloc)
                    tache.recu += len(bloc)
                    ecoule = time.monotonic() - fenetre_t
                    if ecoule >= 1:
                        tache.debit = (tache.recu - fenetre_octets) / ecoule
                        fenetre_t, fenetre_octets = time.monotonic(), tache.recu
