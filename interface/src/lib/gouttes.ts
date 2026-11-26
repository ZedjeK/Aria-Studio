// Le motif du fond animé d'Aria Studio : une grille de points que des gouttes, des cercles qui grandissent, allument au
// hasard en cyan, en bleu et en orange. Ce fichier ne contient que le calcul (testé) ; FondGouttes.svelte le dessine.
// Les règles communes (coupé, en pause, durée d'un passage, nombre d'éléments) restent dans fond.ts.
import type { ReglagesFond } from './api';
import { tirage } from './fond';

/** Les trois couleurs d'une goutte, celles de la palette (--cyan, --bleu, --orange). */
export type TeinteGoutte = 'cyan' | 'bleu' | 'orange';

/** Une goutte d'un passage : où elle tombe (0 à 1 de la zone), quand elle part, jusqu'où elle s'étend. */
export interface Goutte {
  x: number;
  y: number;
  /** Part du passage qui s'écoule avant son départ (0 à 0,45). */
  retard: number;
  teinte: TeinteGoutte;
  /** Rayon final, en pixels CSS. */
  rayon: number;
}

/** Cyan une fois sur deux, bleu et orange chacun une fois sur quatre. */
const TEINTES: readonly TeinteGoutte[] = ['cyan', 'cyan', 'bleu', 'orange'];

/** Écart entre deux points de la grille, en pixels CSS. */
export const PAS_GRILLE = 34;
/** Largeur de l'anneau qui allume les points, en pixels CSS. */
export const LARGEUR_ANNEAU = 44;
/** Part de la fin du passage où les gouttes ont fini de s'étendre. */
const MARGE_FIN = 0.15;

/** Les gouttes d'un nouveau passage : leur nombre est tiré entre le minimum et le maximum des réglages. */
export function nouveauPassage(reglages: ReglagesFond, hasard: () => number = Math.random): Goutte[] {
  return Array.from({ length: tirage(reglages, hasard) }, () => ({
    x: hasard(),
    y: hasard(),
    retard: hasard() * 0.45,
    teinte: TEINTES[Math.floor(hasard() * TEINTES.length)] ?? 'cyan',
    rayon: 190 + hasard() * 150,
  }));
}

/**
 * Où en est une goutte à l'instant `t` du passage (0 à 1) : entre 0 et 1 pendant qu'elle s'étend, `null` avant son
 * départ et une fois finie.
 */
export function avancement(goutte: Goutte, t: number): number | null {
  const p = (t - goutte.retard) / (1 - goutte.retard - MARGE_FIN);
  return p > 0 && p < 1 ? p : null;
}

/**
 * L'éclat qu'une goutte donne à un point de la grille, de 0 (rien) à 1 (sur l'anneau, au départ) : il décroît avec
 * l'écart entre la distance du point et le rayon courant, et avec l'avancement de la goutte.
 */
export function eclat(distance: number, rayonCourant: number, largeur: number, avance: number): number {
  const ecart = Math.abs(distance - rayonCourant);
  if (ecart >= largeur) return 0;
  return (1 - ecart / largeur) * (1 - avance * 0.7);
}
