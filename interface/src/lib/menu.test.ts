import { describe, expect, it } from 'vitest';
import { couleurTon, ligneErreur } from './menu';

describe('menu de l’icône', () => {
  it('donne à chaque ton la couleur de la pastille', () => {
    expect(couleurTon('actif')).toBe('var(--cyan)');
    expect(couleurTon('attention')).toBe('var(--orange)');
    expect(couleurTon('information')).toBe('var(--bleu)');
    expect(couleurTon('repos')).toBe('#3a4d70');
  });

  it('montre une erreur comme une ligne à regarder, avec une majuscule', () => {
    expect(ligneErreur('registre inaccessible')).toEqual({ ton: 'attention', texte: 'Registre inaccessible' });
  });
});
