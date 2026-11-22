import { describe, expect, it } from 'vitest';
import type { ReglagesFond } from './api';
import { bornes, dureePassage, modeFond, tirage } from './fond';

const fond = (r: Partial<ReglagesFond> = {}): ReglagesFond => ({
  actif: true,
  vitesse: 'normale',
  minimum: 2,
  maximum: 5,
  ...r,
});

describe('modeFond', () => {
  it('est coupé si le réglage le désactive', () => {
    expect(modeFond(fond({ actif: false }), true, true)).toBe('coupe');
  });
  it('est coupé si Windows demande moins d’animations', () => {
    expect(modeFond(fond(), true, false)).toBe('coupe');
  });
  it('est en pause quand la fenêtre est cachée', () => {
    expect(modeFond(fond(), false, true)).toBe('pause');
  });
  it('tourne sinon', () => {
    expect(modeFond(fond(), true, true)).toBe('actif');
  });
});

describe('dureePassage', () => {
  it('suit la vitesse', () => {
    expect(dureePassage('lente')).toBe(14_000);
    expect(dureePassage('normale')).toBe(7_000);
    expect(dureePassage('rapide')).toBe(3_500);
  });
});

describe('bornes et tirage', () => {
  it('ramène les bornes entre 1 et 12 et les remet dans l’ordre', () => {
    expect(bornes(fond({ minimum: 40, maximum: -3 }))).toEqual({ minimum: 1, maximum: 12 });
    expect(bornes(fond({ minimum: 8, maximum: 4 }))).toEqual({ minimum: 4, maximum: 8 });
  });
  it('tire entre minimum et maximum compris', () => {
    expect(tirage(fond(), () => 0)).toBe(2);
    expect(tirage(fond(), () => 0.999_999)).toBe(5);
    expect(tirage(fond({ minimum: 3, maximum: 3 }), () => 0.5)).toBe(3);
  });
});
