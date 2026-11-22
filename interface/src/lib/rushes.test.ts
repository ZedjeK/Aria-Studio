import { describe, expect, it } from 'vitest';
import type { EtatRushes } from './api';
import { actions, basculer, compteRendu, images, libelleAction, ongletDe, onglets } from './rushes';

const etat: EtatRushes = {
  tous: 12,
  nonRanges: 4,
  archives: 2,
  corbeille: 1,
  dossiers: [
    { nom: 'Renards', nombre: 5 },
    { nom: 'Été 2026', nombre: 3 },
  ],
};

describe('Rushes', () => {
  it('range les onglets comme la v2', () => {
    const liste = onglets(etat);
    expect(liste.map((o) => o.titre)).toEqual([
      'Tous (12)',
      'Non rangés (4)',
      'Renards (5)',
      'Été 2026 (3)',
      'Archives (2)',
      'Corbeille (1)',
    ]);
    expect(ongletDe(liste, 'dossier:Renards')).toMatchObject({ vue: 'tous', dossier: 'Renards' });
    expect(ongletDe(liste, 'corbeille')).toMatchObject({ vue: 'corbeille', dossier: null });
    // Un dossier supprimé ou renommé : retour à Tous.
    expect(ongletDe(liste, 'dossier:Absent')).toMatchObject({ cle: 'tous', vue: 'tous', dossier: null });
  });

  it('propose les actions de la vue', () => {
    expect(actions('corbeille')).toEqual(['restaurer']);
    expect(actions('archives')).toEqual(['desarchiver', 'corbeille']);
    expect(actions('tous')).toEqual(['archiver', 'corbeille', 'ranger']);
    expect(actions('non_ranges')).toEqual(['archiver', 'corbeille', 'ranger']);
    expect(libelleAction('corbeille')).toBe('Mettre à la corbeille');
  });

  it('dit ce qui a été fait', () => {
    expect(images(1)).toBe('1 image');
    expect(images(0)).toBe('0 image');
    expect(compteRendu('corbeille', 3, null)).toBe('3 images à la corbeille.');
    expect(compteRendu('restaurer', 1, null)).toBe('1 image restaurée.');
    expect(compteRendu('archiver', 2, null)).toBe('2 images archivées.');
    expect(compteRendu('desarchiver', 2, null)).toBe('2 images sorties des archives.');
    expect(compteRendu('ranger', 2, 'Renards')).toBe('2 images rangées dans Renards.');
    expect(compteRendu('ranger', 1, null)).toBe('1 image dans Non rangés.');
  });

  it('bascule la sélection', () => {
    expect(basculer([], 'a')).toEqual(['a']);
    expect(basculer(['a', 'b'], 'a')).toEqual(['b']);
  });
});
