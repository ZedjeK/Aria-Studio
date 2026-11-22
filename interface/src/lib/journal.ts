// Le journal : affichage des dates et des résultats.

import type { Resultat } from './api';

/**
 * Une date du journal en « 21/11/2026 18:45:02 ». Elle s'écrit sans décalage (« 2026-11-14T10:12:03 », heure
 * locale) ou avec (RFC 3339) ; le texte reçu tel quel s'il ne se lit pas.
 */
export function dateLisible(date: string): string {
  const lue = new Date(date);
  if (Number.isNaN(lue.getTime())) return date;
  const jour = lue.toLocaleDateString('fr-FR');
  const heure = lue.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  return `${jour} ${heure}`;
}

const LIBELLES: Record<Resultat, string> = {
  ok: 'fait',
  erreur: 'erreur',
  partiel: 'en partie',
  annule: 'annulé',
};

/** Le résultat d'une opération, en mots. */
export function libelleResultat(resultat: Resultat): string {
  return LIBELLES[resultat];
}
