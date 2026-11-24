import { describe, expect, it } from 'vitest';
import type { LlmCatalogue } from './api';
import { ceQueRemplace, rangerLlm } from './llmFacultatifs';

function llm(fichier: string, changes: Partial<LlmCatalogue>): LlmCatalogue {
  return {
    fichier,
    nom: fichier,
    page: '',
    specialiste: null,
    vision: false,
    licence: { nom: 'Apache-2.0', url: '', resume: '', commercial: true },
    taille: 1,
    aTelecharger: 1,
    installe: false,
    ...changes,
  };
}

describe('rangerLlm', () => {
  const catalogue = [
    llm('bonsai', { installe: true }),
    llm('gemma', {}),
    llm('pe-creation', { specialiste: 'creation' }),
    llm('pe-retouche', { specialiste: 'retouche' }),
  ];

  it('sépare les généralistes des spécialistes, dans l’ordre du catalogue', () => {
    const r = rangerLlm(catalogue);
    expect(r.generalistes.map((l) => l.fichier)).toEqual(['bonsai', 'gemma']);
    expect(r.specialistes.map((l) => l.fichier)).toEqual(['pe-creation', 'pe-retouche']);
  });

  it('sait si un généraliste est installé', () => {
    expect(rangerLlm(catalogue).generalisteInstalle).toBe(true);
    expect(rangerLlm(catalogue.map((l) => ({ ...l, installe: false }))).generalisteInstalle).toBe(false);
    // Un spécialiste installé ne remplace pas un généraliste.
    expect(rangerLlm([llm('bonsai', {}), llm('pe', { specialiste: 'creation', installe: true })]).generalisteInstalle).toBe(false);
  });
});

describe('ceQueRemplace', () => {
  it('dit ce que remplace chaque spécialiste', () => {
    expect(ceQueRemplace('creation')).toContain('réécriture');
    expect(ceQueRemplace('retouche')).toContain('consigne de retouche');
  });
});
