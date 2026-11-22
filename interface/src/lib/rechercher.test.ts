import { describe, expect, it } from 'vitest';
import { compte, conseil, dossierDeFamille, libelleVram, sources, tris } from './rechercher';

describe('Rechercher', () => {
  it('propose les sources et les tris de chaque type', () => {
    expect(sources('llm')).toEqual(['huggingface']);
    expect(sources('lora')).toContain('civitai');
    expect(tris('civitai').map((t) => t.id)).not.toContain('tendances');
    expect(tris('huggingface').map((t) => t.id)).toContain('tendances');
  });

  it('range un fichier dans sa famille', () => {
    const krea = { famille: 'Krea2', nature: 'image', bases: ['Krea 2'] };
    expect(dossierDeFamille(krea, 'lora')).toBe('image/Krea2/loras');
    expect(dossierDeFamille(krea, 'image')).toBe('image/Krea2/modeles');
  });

  it('rend les résultats lisibles', () => {
    expect(libelleVram('deborde')).toBe('déborde de la VRAM');
    expect(libelleVram(null)).toBe('');
    expect(compte(950)).toBe('950');
    expect(compte(12_345)).toBe('12,3 k');
    expect(compte(2_000_000)).toBe('2 M');
    expect(conseil('Steps', { statut: 'unique', valeurs: [8] })).toBe('Steps conseillés par la page : 8');
    expect(conseil('Steps', { statut: 'ambigu', valeurs: [4, 8] })).toContain('4 ou 8');
    expect(conseil('CFG', { statut: 'aucun', valeurs: [] })).toBe('');
    expect(conseil('CFG', null)).toBe('');
  });
});
