// Règles du fond animé, communes à toutes les applis Aria. Le motif (FondAnime.svelte) ne fait que dessiner un
// passage ; ce fichier décide s'il tourne, à quelle vitesse et avec combien d'éléments.
import type { ReglagesFond, Vitesse } from './api';

export const ELEMENTS_MIN = 1;
export const ELEMENTS_MAX = 12;

/** Coupé : rien n'est dessiné. En pause : il reprend quand la fenêtre revient. Actif : il tourne. */
export type ModeFond = 'coupe' | 'pause' | 'actif';

/**
 * Coupé si le réglage fond.actif est faux ou si Windows demande moins d'animations (prefers-reduced-motion) ;
 * en pause si la fenêtre est cachée ou réduite ; sinon, il tourne.
 */
export function modeFond(reglages: ReglagesFond, fenetreVisible: boolean, animationsSysteme: boolean): ModeFond {
  if (!reglages.actif || !animationsSysteme) return 'coupe';
  return fenetreVisible ? 'actif' : 'pause';
}

/** Durée d'un passage selon fond.vitesse, en millisecondes : 14 s, 7 s ou 3,5 s. */
export function dureePassage(vitesse: Vitesse): number {
  switch (vitesse) {
    case 'lente':
      return 14_000;
    case 'rapide':
      return 3_500;
    default:
      return 7_000;
  }
}

/** Bornes ramenées entre 1 et 12, et remises dans l'ordre si besoin. */
export function bornes(reglages: ReglagesFond): { minimum: number; maximum: number } {
  const borner = (n: number) =>
    Math.min(ELEMENTS_MAX, Math.max(ELEMENTS_MIN, Number.isFinite(n) ? Math.round(n) : ELEMENTS_MIN));
  const bas = borner(reglages.minimum);
  const haut = borner(reglages.maximum);
  return { minimum: Math.min(bas, haut), maximum: Math.max(bas, haut) };
}

/** Nombre d'éléments d'un passage, tiré au hasard entre minimum et maximum compris. */
export function tirage(reglages: ReglagesFond, hasard: () => number = Math.random): number {
  const { minimum, maximum } = bornes(reglages);
  const n = minimum + Math.floor(hasard() * (maximum - minimum + 1));
  return Math.min(maximum, n);
}
