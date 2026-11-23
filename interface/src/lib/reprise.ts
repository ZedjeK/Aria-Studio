// La reprise d'une v2 installée pour un essai portable (aria_coeur::reprise) : ce que disent le bandeau du premier
// lancement et Réglages → Emplacements.

import type { EtatReprise } from './api';

/** Ce que le bandeau du premier lancement annonce ; rien s'il n'y a rien à annoncer. */
export function texteBandeau(etat: EtatReprise): { etiquette: string; texte: string } | null {
  if (etat.v2Lancee) {
    return {
      etiquette: 'Version 2 lancée',
      texte: 'Ferme la version 2, puis relance cet essai : ses modèles et ses moteurs seront repris, sans rien copier.',
    };
  }
  const bilan = etat.bilan;
  if (!bilan || bilan.annonce || bilan.repris.length === 0) return null;
  return {
    etiquette: 'Version 2 reprise',
    texte: `Modèles et moteurs de la v2 repris : ${bilan.repris.map((e) => e.nom).join(', ')}.`,
  };
}

/** Un élément repris vaut-il encore (ni changé ni retiré depuis) ? */
export function enVigueur(etat: EtatReprise, cle: string): boolean {
  return etat.bilan?.enVigueur.includes(cle) ?? false;
}
