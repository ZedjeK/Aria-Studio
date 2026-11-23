import { describe, expect, it } from 'vitest';
import type { EstimationTournage, QualiteVideo } from './api';
import { dureeLisible, estimationClips, texteEstimation } from './montage';

const compromis: QualiteVideo = {
  cle: 'compromis',
  nom: '0,6 MP',
  aide: 'compromis',
  megapixels: 0.6,
  steps: 12,
  estimationS: 170,
};

describe('Montage', () => {
  it('estime un tournage', () => {
    expect(estimationClips(compromis, 5, 1, 1.57)).toBe(170);
    expect(estimationClips(compromis, 5, 3, 1.57)).toBe(510);
    expect(Math.round(estimationClips(compromis, 15, 1, 1.57))).toBe(954);
  });

  it('dit le calcul dans son ordre', () => {
    const calcul: EstimationTournage = {
      source: 'calcul',
      clipS: 126,
      totalS: 378,
      nombre: 3,
      checkpoint: 'h3.safetensors',
      poidsGo: 19.5,
      steps: 12,
      aValider: false,
      vramLibreGo: 15.2,
      facteurVram: 0.74,
    };
    expect(texteEstimation(calcul, 510, '0,6 MP', 5)).toBe(
      '≈ 6 min sur ta carte : 15,2 Go de VRAM libre → 12 steps → modèle de 19,5 Go → 0,6 MP → 5 s → 3 clips de ≈ 2 min' +
        ' · à chaud, sans le chargement du modèle au premier clip',
    );
    expect(texteEstimation(null, 170, '0,6 MP', 5)).toContain("≈ 3 min sur ta carte (tableau d'origine");
    // Sans VRAM lue, le cœur a quand même tenu compte des steps du checkpoint.
    expect(texteEstimation({ ...calcul, source: 'tableau_d_origine', totalS: 340 }, 510, '0,6 MP', 5)).toContain('≈ 6 min');
  });

  it('écrit une durée', () => {
    expect(dureeLisible(45)).toBe('45 s');
    expect(dureeLisible(954)).toBe('16 min');
    expect(dureeLisible(3900)).toBe('1 h 05');
  });
});
