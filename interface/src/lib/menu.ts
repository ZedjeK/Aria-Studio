// Les points de couleur : bloc « État » du menu de l'icône, pastilles de l'état en direct.
import type { LigneMenu, Ton } from './api';

/** La couleur du point d'une ligne « État » : la même palette que la pastille de l'icône. */
export function couleurTon(ton: Ton): string {
  switch (ton) {
    case 'actif':
      return 'var(--cyan)';
    case 'attention':
      return 'var(--orange)';
    case 'information':
      return 'var(--bleu)';
    case 'repos':
      return '#3a4d70';
  }
}

/** Une erreur montrée dans le bloc « État », à regarder. */
export function ligneErreur(message: string): LigneMenu {
  return { ton: 'attention', texte: message.charAt(0).toUpperCase() + message.slice(1) };
}

/** Une pastille de l'état en direct, en haut à droite des pages. */
export interface Pastille {
  nom: string;
  detail: string;
  ton: Ton;
}
