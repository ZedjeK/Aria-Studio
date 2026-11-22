import { describe, expect, it } from 'vitest';
import { ANCRAGES, pointsDuTrait, problemeTaille, tailleProposee, versImage } from './retouche';

describe('Retouche', () => {
  it('range les ancrages en grille 3 × 3', () => {
    expect(ANCRAGES).toHaveLength(9);
    expect(ANCRAGES[4]?.id).toBe('centre');
    expect(ANCRAGES.map((a) => a.id)).toContain('bas-droite');
  });

  it('contrôle la taille comme le cœur', () => {
    const source = { largeur: 1024, hauteur: 768 };
    expect(problemeTaille(source, 1536, 768)).toBeNull();
    expect(problemeTaille(source, 1024, 768)).toBe('La nouvelle taille doit être plus grande que 1024×768');
    expect(problemeTaille(source, 1000, 900)).toBe('La nouvelle taille doit être plus grande que 1024×768');
    expect(problemeTaille(source, 5000, 768)).toBe('Taille maximale : 4096 px de côté');
    expect(problemeTaille(source, 1536.5, 768)).toBe('La taille est un nombre entier de pixels.');
    expect(tailleProposee(source)).toEqual({ largeur: 1536, hauteur: 768 });
    expect(tailleProposee({ largeur: 4000, hauteur: 100 })).toEqual({ largeur: 4096, hauteur: 100 });
  });

  it('ramène le pointeur aux pixels de l’image', () => {
    const cadre = { left: 10, top: 20, width: 500, height: 250 };
    expect(versImage(260, 145, cadre, { largeur: 1000, hauteur: 500 })).toEqual({ x: 500, y: 250 });
  });

  it('trace un trait continu', () => {
    const points = pointsDuTrait({ x: 0, y: 0 }, { x: 100, y: 0 }, 20);
    expect(points).toHaveLength(10);
    expect(points.at(-1)).toEqual({ x: 100, y: 0 });
    expect(pointsDuTrait({ x: 5, y: 5 }, { x: 5, y: 5 }, 20)).toEqual([{ x: 5, y: 5 }]);
  });
});
