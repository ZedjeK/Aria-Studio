import { describe, expect, it } from 'vitest';
import { dateLisible, libelleResultat } from './journal';

describe('journal', () => {
  it('écrit une date du journal à la française', () => {
    const texte = dateLisible('2026-11-21T18:45:02+01:00');
    expect(texte).toMatch(/^\d{2}\/\d{2}\/\d{4} \d{2}:\d{2}:\d{2}$/);
  });

  it('lit une date de la v2, sans décalage, en heure locale', () => {
    expect(dateLisible('2026-11-14T10:12:03')).toBe('14/11/2026 10:12:03');
  });

  it('rend tel quel un texte qui n’est pas une date', () => {
    expect(dateLisible('hier')).toBe('hier');
  });

  it('dit chaque résultat en mots', () => {
    expect(libelleResultat('ok')).toBe('fait');
    expect(libelleResultat('erreur')).toBe('erreur');
    expect(libelleResultat('partiel')).toBe('en partie');
    expect(libelleResultat('annule')).toBe('annulé');
  });
});
