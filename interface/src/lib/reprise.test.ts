import { describe, expect, it } from 'vitest';
import type { Bilan, EtatReprise } from './api';
import { enVigueur, texteBandeau } from './reprise';

const bilan: Bilan = {
  depuis: 'C:\\Users\\moi\\.Aria-Studio',
  date: '2026-11-23T10:00:00',
  annonce: false,
  repris: [
    { cle: 'modeles', nom: 'Modèles (image, vidéo, LLM)', chemin: 'C:\\Users\\moi\\.Aria-Studio\\modeles' },
    { cle: 'comfyui', nom: 'ComfyUI', chemin: 'C:\\Users\\moi\\.Aria-Studio\\moteurs\\comfyui' },
  ],
  enVigueur: ['modeles'],
};
const repris: EtatReprise = { v2Lancee: false, bilan };

describe('reprise de la v2', () => {
  it('annonce une fois ce qui a été repris', () => {
    expect(texteBandeau(repris)?.texte).toBe('Modèles et moteurs de la v2 repris : Modèles (image, vidéo, LLM), ComfyUI.');
    expect(texteBandeau({ ...repris, bilan: { ...bilan, annonce: true } })).toBeNull();
    expect(texteBandeau({ v2Lancee: false, bilan: null })).toBeNull();
  });

  it('dit de fermer une v2 lancée', () => {
    expect(texteBandeau({ v2Lancee: true, bilan: null })?.etiquette).toBe('Version 2 lancée');
  });

  it('sait ce qui vaut encore', () => {
    expect(enVigueur(repris, 'modeles')).toBe(true);
    expect(enVigueur(repris, 'comfyui')).toBe(false);
  });
});
