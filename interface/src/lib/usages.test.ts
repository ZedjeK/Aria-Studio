import { describe, expect, it } from 'vitest';
import type { ReecritureUsage, WorkflowUsage } from './api';
import { attentes, natureDe, nomLlm, puces } from './usages';

const reecriture: ReecritureUsage = {
  llm: null,
  effectif: 'mistral.gguf',
  style: 'prose',
  langue: 'en',
  motsMax: 150,
  regles: 1,
  specialistes: [],
  aVenir: [],
  ailleurs: null,
};

function workflow(changes: Partial<WorkflowUsage>): WorkflowUsage {
  return {
    id: 'krea2_turbo',
    nom: 'Krea 2',
    tache: 'texte_vers_image',
    famille: 'Krea2',
    dossierModeles: 'image/Krea2/modeles',
    dossierLoras: 'image/Krea2/loras',
    licence: { nom: 'Krea', url: '', resume: '', commercial: false },
    vramEstimeeGo: 12.6,
    vramMinGo: null,
    taille: 1024,
    pret: true,
    manquants: [],
    aTelecharger: 0,
    modeles: [],
    aValider: [],
    origine: null,
    modeleDefaut: null,
    loras: [],
    possibles: [],
    favoriPour: [],
    references: 0,
    variation: true,
    reecriture,
    ...changes,
  };
}

describe('Usages', () => {
  it('dit ce que le modèle attend du LLM', () => {
    expect(attentes(reecriture)).toBe('de la prose descriptive en anglais · 150 mots au plus · 1 règle propre');
    expect(attentes({ ...reecriture, style: 'instruction', motsMax: null, regles: 0 })).toBe(
      "une consigne d'édition en anglais",
    );
  });

  it('résume un workflow', () => {
    expect(puces(workflow({}))).toEqual(['Famille Krea2', '1 Ko', 'VRAM estimée 12.6 Go', 'redessiner une image']);
    expect(puces(workflow({ vramMinGo: 10, references: 3, variation: false }))).toContain('3 images de référence au plus');
    expect(natureDe(workflow({ tache: 'image_vers_video' }))).toBe('video');
    expect(natureDe(workflow({ tache: 'retouche' }))).toBe('image');
    expect(nomLlm('a.gguf', [{ fichier: 'a.gguf', nom: 'A', favori: true, specialiste: false }])).toBe('A');
    expect(nomLlm('b.gguf', [])).toBe('b.gguf');
  });
});
