import { describe, expect, it } from 'vitest';
import type { QualiteVideo } from './api';
import { dureeLisible, estimationClips } from './montage';

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

  it('écrit une durée', () => {
    expect(dureeLisible(45)).toBe('45 s');
    expect(dureeLisible(954)).toBe('16 min');
    expect(dureeLisible(3900)).toBe('1 h 05');
  });
});
