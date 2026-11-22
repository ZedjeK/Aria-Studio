import { describe, expect, it } from 'vitest';
import type { LoraProposee } from './api';
import { actives, forceLisible, lignesPour, lorasChoisies } from './loras';

const neon: LoraProposee = { fichier: 'neon.safetensors', nom: 'neon', declencheurs: 'neon', force: 0.8, page: null };
const pluie: LoraProposee = { fichier: 'pluie.safetensors', nom: 'pluie', declencheurs: '', force: 1, page: null };

describe('LoRA', () => {
  it('garde ce qui était coché quand la liste change', () => {
    const lignes = lignesPour([neon, pluie], {});
    expect(lignes['neon.safetensors']).toEqual({ actif: false, force: 0.8, declencheurs: 'neon' });
    expect(actives(lignes)).toBe(0);
    const cochee = { ...lignes, 'neon.safetensors': { actif: true, force: 0.6, declencheurs: 'neon glow' } };
    const apres = lignesPour([neon], cochee);
    expect(Object.keys(apres)).toEqual(['neon.safetensors']);
    expect(lorasChoisies([neon], apres)).toEqual([{ fichier: 'neon.safetensors', force: 0.6, declencheurs: 'neon glow' }]);
    expect(actives(apres)).toBe(1);
  });

  it('écrit une force', () => {
    expect(forceLisible(0.75)).toBe('0,75');
    expect(forceLisible(1)).toBe('1');
  });
});
