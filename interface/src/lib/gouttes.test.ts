import { describe, expect, it } from 'vitest';
import type { ReglagesFond } from './api';
import { avancement, eclat, nouveauPassage, type Goutte } from './gouttes';

const fond = (r: Partial<ReglagesFond> = {}): ReglagesFond => ({
  actif: true,
  vitesse: 'normale',
  minimum: 2,
  maximum: 5,
  ...r,
});

/** Un « hasard » qui rend les valeurs d'une liste, en boucle. */
function suite(valeurs: number[]): () => number {
  let i = 0;
  return () => {
    const v = valeurs[i % valeurs.length] ?? 0;
    i += 1;
    return v;
  };
}

const goutte = (g: Partial<Goutte> = {}): Goutte => ({ x: 0.5, y: 0.5, retard: 0.2, teinte: 'cyan', rayon: 200, ...g });

describe('nouveauPassage', () => {
  it('tire autant de gouttes que les réglages le permettent', () => {
    for (const hasard of [0, 0.5, 0.999]) {
      const n = nouveauPassage(fond(), () => hasard).length;
      expect(n).toBeGreaterThanOrEqual(2);
      expect(n).toBeLessThanOrEqual(5);
    }
  });

  it('respecte un minimum et un maximum égaux', () => {
    expect(nouveauPassage(fond({ minimum: 3, maximum: 3 })).length).toBe(3);
  });

  it('place chaque goutte dans la zone, avec un retard et un rayon bornés', () => {
    for (const g of nouveauPassage(fond({ minimum: 12, maximum: 12 }))) {
      expect(g.x).toBeGreaterThanOrEqual(0);
      expect(g.x).toBeLessThan(1);
      expect(g.y).toBeGreaterThanOrEqual(0);
      expect(g.y).toBeLessThan(1);
      expect(g.retard).toBeGreaterThanOrEqual(0);
      expect(g.retard).toBeLessThan(0.45);
      expect(g.rayon).toBeGreaterThanOrEqual(190);
      expect(g.rayon).toBeLessThan(340);
    }
  });

  it('choisit les trois couleurs de la palette, le cyan plus souvent', () => {
    const teintes = [0, 0.3, 0.6, 0.9].map((v) => {
      // Dans l'ordre des appels : nombre de gouttes, x, y, retard, couleur, rayon.
      const hasard = suite([0.5, 0.5, 0.5, 0.5, v, 0.5]);
      return nouveauPassage(fond({ minimum: 1, maximum: 1 }), hasard)[0]?.teinte;
    });
    expect(new Set(teintes)).toEqual(new Set(['cyan', 'bleu', 'orange']));
    expect(teintes.filter((t) => t === 'cyan').length).toBe(2);
  });
});

describe('avancement', () => {
  it('ne commence qu\'après le retard', () => {
    expect(avancement(goutte(), 0.1)).toBeNull();
    expect(avancement(goutte(), 0.2)).toBeNull();
    expect(avancement(goutte(), 0.4)).toBeGreaterThan(0);
  });

  it('grandit jusqu\'à 1, puis la goutte est finie', () => {
    const a = avancement(goutte(), 0.5);
    const b = avancement(goutte(), 0.7);
    expect(a).not.toBeNull();
    expect(b).not.toBeNull();
    expect(b ?? 0).toBeGreaterThan(a ?? 1);
    expect(avancement(goutte(), 0.9)).toBeNull();
    expect(avancement(goutte(), 1)).toBeNull();
  });
});

describe('eclat', () => {
  it('est maximal sur l\'anneau, au départ de la goutte', () => {
    expect(eclat(100, 100, 44, 0)).toBe(1);
  });

  it('décroît quand le point s\'éloigne de l\'anneau', () => {
    const proche = eclat(110, 100, 44, 0.2);
    const loin = eclat(130, 100, 44, 0.2);
    expect(proche).toBeGreaterThan(loin);
    expect(loin).toBeGreaterThan(0);
  });

  it('est nul hors de l\'anneau', () => {
    expect(eclat(200, 100, 44, 0.2)).toBe(0);
    expect(eclat(56, 100, 44, 0.2)).toBe(0);
  });

  it('s\'éteint avec l\'avancement de la goutte', () => {
    expect(eclat(100, 100, 44, 0.9)).toBeLessThan(eclat(100, 100, 44, 0.1));
  });
});
