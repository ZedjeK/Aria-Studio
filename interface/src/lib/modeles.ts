// La page Modèles : tâches des workflows, avancement des téléchargements.

import type { Telechargement } from './api';
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
