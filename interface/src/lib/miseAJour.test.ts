import { describe, expect, it } from 'vitest';
import type { InfosMiseAJour } from './api';
import { texteRecherche } from './miseAJour';

describe('texteRecherche', () => {
  it('dit que l’appli est à jour quand rien n’est trouvé', () => {
    expect(texteRecherche(null, '3.0.0')).toBe('3.0.0 est à jour.');
  });

  it('dit la version publiée et celle en cours', () => {
    const trouvee: InfosMiseAJour = { nom: 'Aria Studio', actuelle: '3.0.0-essai.2', version: '3.0.0', taille: 1234 };
    expect(texteRecherche(trouvee, '3.0.0-essai.2')).toBe('La version 3.0.0 est publiée (en cours : 3.0.0-essai.2).');
  });
});
