import { describe, expect, it } from 'vitest';
import { NON_MESURE, octetsLisibles, segmentsRemplis, texteJauge } from './jauge';

describe('jauge', () => {
  it('reste vide et dit « à tester » sans mesure', () => {
    expect(segmentsRemplis(null, 20)).toBe(0);
    expect(texteJauge(null)).toBe(NON_MESURE);
  });
  it('remplit au moins un segment dès qu’une mesure dépasse zéro', () => {
    expect(segmentsRemplis(0.001, 20)).toBe(1);
    expect(segmentsRemplis(0, 20)).toBe(0);
    expect(segmentsRemplis(1.7, 20)).toBe(20);
  });
  it('donne le texte de la mesure, ou son pourcentage', () => {
    expect(texteJauge(0.42, ' 42 Go libres ')).toBe('42 Go libres');
    expect(texteJauge(0.42)).toBe('42 %');
  });
  it('écrit une taille lisible', () => {
    expect(octetsLisibles(512)).toBe('512 o');
    expect(octetsLisibles(1536)).toBe('1,5 Ko');
  });
});
