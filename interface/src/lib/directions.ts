// « Proposer des directions » au Scénario : trois directions proposées par le LLM à partir d'une envie vague, gardées
// tant qu'on reste dans l'atelier ; « Proposer d'autres directions » donne au LLM celles déjà vues pour qu'il s'en
// écarte (aria_coeur::directions).

import type { Direction } from './api';

/** Les directions déjà vues, envoyées au LLM, au plus (comme ECARTEES_MAX du cœur). */
export const ECARTEES_MAX = 12;

/** Ce qu'un atelier garde : les directions proposées en dernier, et celles vues avant. */
export interface EtatDirections {
  proposees: Direction[];
  ecartees: Direction[];
  /** La réponse a été redemandée au LLM, ou le contrôle qualité reproche encore quelque chose. */
  alertes: string[];
}

export function etatVide(): EtatDirections {
  return { proposees: [], ecartees: [], alertes: [] };
}

function cle(d: Direction): string {
  return d.titre.trim().toLowerCase();
}

/** Les directions à éviter pour la prochaine proposition : celles vues avant et celles proposées en dernier, sans
 * doublon, les plus récentes gardées. */
export function aEcarter(etat: EtatDirections): Direction[] {
  const vues = new Map<string, Direction>();
  for (const d of [...etat.ecartees, ...etat.proposees]) {
    vues.delete(cle(d));
    vues.set(cle(d), d);
  }
  return [...vues.values()].slice(-ECARTEES_MAX);
}

/** L'état après une nouvelle proposition : les précédentes passent dans les directions vues. */
export function apresProposition(etat: EtatDirections, nouvelles: Direction[], alertes: string[]): EtatDirections {
  return { proposees: nouvelles, ecartees: aEcarter(etat), alertes };
}

/** Le texte qu'une direction pose dans l'idée (ou le fil de l'histoire) : son titre, puis ses phrases. */
export function versIdee(d: Direction): string {
  const titre = d.titre.trim().replace(/[.:]+$/, '');
  const texte = d.texte.trim();
  if (titre === '') return texte;
  if (texte === '') return titre;
  return `${titre} : ${texte}`;
}

/** Les alertes à montrer : pourquoi la réponse a été redemandée, puis ce qui reste. */
export function alertesDe(redemandee: string | null, alertes: string[]): string[] {
  return [...(redemandee ? [`Directions redemandées au LLM : ${redemandee}.`] : []), ...alertes];
}
