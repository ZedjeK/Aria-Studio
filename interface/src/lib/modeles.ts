// La page Modèles : tâches des workflows, avancement des téléchargements.

import type { StepsDits, Telechargement } from './api';
import { octetsLisibles } from './jauge';

const TACHES: Record<string, string> = {
  texte_vers_image: 'Texte → image',
  retouche: 'Image → image',
  image_vers_video: 'Image → vidéo',
  texte_vers_video: 'Texte → vidéo',
};

/** Le nom d'une tâche de workflow. */
export function libelleTache(tache: string): string {
  return TACHES[tache] ?? tache;
}

/** L'ordre des tâches sur la page, comme les sous-onglets de la v2. */
export function ordreTache(tache: string): number {
  const ordre = Object.keys(TACHES).indexOf(tache);
  return ordre === -1 ? Object.keys(TACHES).length : ordre;
}

/** Vrai tant qu'un téléchargement peut encore avancer (il s'annule alors). */
export function telechargementActif(t: Telechargement): boolean {
  return t.etat === 'attente' || t.etat === 'en_cours' || t.etat === 'verification';
}

/** La part reçue, de 0 à 1 ; null avant le premier octet d'un fichier de taille inconnue. */
export function fractionTelechargement(t: Telechargement): number | null {
  if (t.taille <= 0) return null;
  return Math.min(1, t.recu / t.taille);
}

/** L'avancement d'un téléchargement, en une ligne. */
export function texteTelechargement(t: Telechargement): string {
  switch (t.etat) {
    case 'attente':
      return `en attente · ${octetsLisibles(t.taille)}`;
    case 'en_cours': {
      const debit = t.debit > 0 ? ` · ${octetsLisibles(t.debit)}/s` : '';
      return `${octetsLisibles(t.recu)} sur ${octetsLisibles(t.taille)}${debit}`;
    }
    case 'verification':
      return 'vérification de l’empreinte SHA-256';
    case 'termine':
      return `terminé · ${octetsLisibles(t.taille)}`;
    case 'annule':
      return 'annulé';
    case 'erreur':
      return t.erreur ?? 'erreur';
  }
}

const ORIGINES: Record<string, string> = {
  aria: 'téléchargé par Aria',
  retrouve: 'source retrouvée',
  main: 'déposé à la main',
};

/** D'où vient un fichier de la collection. */
export function libelleOrigine(origine: string): string {
  return ORIGINES[origine] ?? origine;
}

/** Les steps d'un checkpoint, au plus (comme le cœur). */
export const STEPS_MAX = 150;
/** Les mots déclencheurs d'une LoRA, au plus. */
export const LONGUEUR_DECLENCHEURS = 300;

/** Les steps à proposer pour un checkpoint à valider : la première valeur entière que dit sa fiche. */
export function stepsProposes(dits: StepsDits[] | null): number | null {
  for (const d of dits ?? []) {
    if (typeof d.valeur === 'number' && Number.isInteger(d.valeur) && d.valeur >= 1 && d.valeur <= STEPS_MAX) {
      return d.valeur;
    }
  }
  return null;
}

/** Ce qui empêche d'enregistrer des steps ; null s'ils conviennent. */
export function erreurSteps(steps: number): string | null {
  return Number.isInteger(steps) && steps >= 1 && steps <= STEPS_MAX ? null : `Steps : un entier entre 1 et ${STEPS_MAX}`;
}

/** Ce qui empêche d'enregistrer le réglage d'une LoRA ; null s'il convient. */
export function erreurLora(declencheurs: string, force: number): string | null {
  if (declencheurs.trim().split(/\s+/).join(' ').length > LONGUEUR_DECLENCHEURS) {
    return `Mots déclencheurs : ${LONGUEUR_DECLENCHEURS} caractères au plus`;
  }
  if (!Number.isFinite(force) || force < 0 || force > 2) return 'Force de LoRA entre 0 et 2';
  return null;
}

/** Une date de la corbeille (`2026-11-22T14:03:00`), lisible. */
export function dateLisible(date: string | null): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(date ?? '');
  return m ? `${m[3]}/${m[2]}/${m[1]} ${m[4]}:${m[5]}` : '';
}
