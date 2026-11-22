import { describe, expect, it } from 'vitest';
import type { OptionsSequence, PlanSequence, QualiteVideo } from './api';
import {
  aTourner,
  deplacer,
  estimationSequence,
  nouveauPlan,
  parlePlan,
  retirer,
  sequenceVide,
  stepsSequence,
} from './sequences';

const plan = (changements: Partial<PlanSequence>): PlanSequence => ({ ...nouveauPlan('couper'), ...changements });

const options: OptionsSequence = {
  plansMax: 12,
  longueurTitre: 40,
  longueurIdee: 1000,
  longueurAmbiance: 300,
  longueurContrainte: 300,
  surcout: 1.4,
  stepsParole: 12,
  surcoutParole: 1.17,
  exposantDuree: 1.25,
  steps: { 'h3.safetensors': 8 },
  modeleDefaut: 'h3.safetensors',
};

const rapide: QualiteVideo = { cle: 'rapide', nom: '0,4 MP', aide: 'rapide', megapixels: 0.4, steps: 8, estimationS: 80 };

describe('séquences', () => {
  it('reconnaît une réplique', () => {
    expect(parlePlan(plan({ idee: 'Elle dit « Bonjour »' }))).toBe(true);
    expect(parlePlan(plan({ prompt: 'She says <d>[French] Salut.</d>' }))).toBe(true);
    expect(parlePlan(plan({ idee: 'La pluie tombe' }))).toBe(false);
  });

  it('retourne les plans qui partent du précédent', () => {
    const plans = [
      plan({ enchainement: 'rush', clip: 'c0', a_jour: true }),
      plan({ clip: 'c1', a_jour: true }),
      plan({ enchainement: 'prolonger', clip: 'c2', a_jour: true }),
      plan({ enchainement: 'couper', clip: null }),
    ];
    expect(aTourner(plans)).toEqual([3]);
    expect(aTourner(plans, [1])).toEqual([1, 2, 3]);
  });

  it('estime comme le cœur', () => {
    const s = sequenceVide(null);
    s.plans = [plan({ enchainement: 'rush', duree: 5 }), plan({ duree: 10, idee: '« Salut »' })];
    expect(stepsSequence(s, options)).toBe(8);
    const attendu = 5 + 80 * 1.4 + 80 * 1.4 * 2 ** 1.25 * 1.17;
    expect(estimationSequence(s, [0, 1], rapide, options)).toBeCloseTo(attendu, 6);
    expect(estimationSequence(s, [], rapide, options)).toBe(0);
  });

  it('garde un départ en tête après un déplacement ou une suppression', () => {
    const plans = [plan({ enchainement: 'rush', rush: 'r0', a_jour: true }), plan({ enchainement: 'couper' })];
    const deplaces = deplacer(plans, 1, 0);
    expect(deplaces.map((p) => p.enchainement)).toEqual(['texte', 'couper']);
    expect(deplaces.every((p) => !p.a_jour)).toBe(true);
    expect(retirer(plans, 0).map((p) => p.enchainement)).toEqual(['texte']);
  });
});
