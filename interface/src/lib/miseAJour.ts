// « Vérifier maintenant » (Paramètres › Mises à jour) : ce que dit la ligne sous le bouton.

import type { InfosMiseAJour } from './api';

/** Ce que la recherche à la demande a trouvé : une version publiée, ou rien quand l'appli est à jour. */
export function texteRecherche(trouvee: InfosMiseAJour | null, version: string): string {
  return trouvee
    ? `La version ${trouvee.version} est publiée (en cours : ${trouvee.actuelle}).`
    : `${version} est à jour.`;
}
