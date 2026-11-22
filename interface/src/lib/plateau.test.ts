import { describe, expect, it } from 'vitest';
import type { ChoixWorkflow, Commande, OptionsPlateau } from './api';
import {
  estActive,
  formatInitial,
  llmInitial,
  rushesMax,
  workflowsPour,
  libelleEtat,
  lireSeed,
  modeleInitial,
  nomCheckpoint,
  texteAvancement,
  texteCommande,
  texteStepsSuggeres,
} from './plateau';

const krea: ChoixWorkflow = {
  id: 'krea2_turbo',
  nom: 'Krea 2',
  famille: 'Krea2',
  origine: 'krea2_turbo.safetensors',
  modeles: ['krea2_turbo.safetensors', 'autre.safetensors'],
  modeleDefaut: 'autre.safetensors',
  format: '16:9 (Widescreen)',
  aValider: [],
  stepsSuggeres: [8],
  references: 0,
  variation: true,
  reserveRetouche: false,
};

const edit: ChoixWorkflow = {
  ...krea,
  id: 'qwen_image_2_1_edit',
  nom: 'Qwen-Image 2.1 Edit',
  references: 10,
  variation: false,
};

const options: OptionsPlateau = {
  formats: ['1:1 (Square)', '16:9 (Widescreen)'],
  nombreMax: 50,
  defaut: 'qwen_image_2_1',
  workflows: [krea],
  incomplets: [],
  llm: [
    { fichier: 'Ternary-Bonsai-2-27B-PTQ1_0.gguf', nom: 'Bonsai 2 27B', favori: true, specialiste: false },
    { fichier: 'mistral.gguf', nom: 'mistral', favori: false, specialiste: false },
  ],
  llmParWorkflow: { krea2_turbo: 'mistral.gguf', qwen_image_2_1: 'retire.gguf' },
  defautReferences: 'qwen_image_2_1_edit',
  referencesMax: 10,
};

function commande(changements: Partial<Commande>): Commande {
  return {
    id: 'a1',
    type: 'creation',
    idee: 'un renard',
    prompt: 'a fox',
    reecrire: false,
    llm: null,
    workflow: 'krea2_turbo',
    format: '1:1 (Square)',
    seed: null,
    nombre: 1,
    modele: null,
    etat: 'attente',
    annulee: false,
    etape: '',
    progression: 0,
    images: [],
    erreur: null,
    references: [],
    modeReference: null,
    liberte: 0.6,
    parent: null,
    masque: false,
    fondu: 0,
    largeur: null,
    hauteur: null,
    ancrage: null,
    cadrage: null,
    ...changements,
  };
}

describe('Plateau', () => {
  it('dit l’état d’une commande', () => {
    expect(libelleEtat('en_cours')).toBe('en cours');
    expect(libelleEtat('annule')).toBe('annulée');
    expect(estActive(commande({ etat: 'attente' }))).toBe(true);
    expect(estActive(commande({ etat: 'en_cours' }))).toBe(true);
    expect(estActive(commande({ etat: 'termine' }))).toBe(false);
    expect(estActive(commande({ etat: 'erreur' }))).toBe(false);
  });

  it('lit la seed tapée', () => {
    expect(lireSeed('')).toBeNull();
    expect(lireSeed('   ')).toBeNull();
    expect(lireSeed(' 42 ')).toBe(42);
    expect(lireSeed('0')).toBe(0);
    expect(lireSeed('-1')).toBeUndefined();
    expect(lireSeed('4.2')).toBeUndefined();
    expect(lireSeed('1e3')).toBeUndefined();
    expect(lireSeed('abc')).toBeUndefined();
    expect(lireSeed('9007199254740992')).toBeUndefined();
    expect(lireSeed('9007199254740991')).toBe(9007199254740991);
  });

  it('propose le format et le checkpoint des réglages', () => {
    expect(formatInitial(options, krea)).toBe('16:9 (Widescreen)');
    expect(formatInitial(options, { ...krea, format: '5:4' })).toBe('1:1 (Square)');
    expect(formatInitial(options, undefined)).toBe('1:1 (Square)');
    expect(modeleInitial(krea)).toBe('autre.safetensors');
    expect(modeleInitial({ ...krea, modeleDefaut: 'retire.safetensors' })).toBe('krea2_turbo.safetensors');
    expect(modeleInitial({ ...krea, modeles: [] })).toBe('');
    expect(modeleInitial(undefined)).toBe('');
  });

  it('propose les workflows selon le départ', () => {
    const retouche = { ...options, workflows: [edit, { ...edit, id: 'reserve', reserveRetouche: true }] };
    expect(workflowsPour(null, options, retouche).map((w) => w.id)).toEqual(['krea2_turbo']);
    expect(workflowsPour('inspirer', options, retouche).map((w) => w.id)).toEqual(['qwen_image_2_1_edit']);
    expect(workflowsPour('inspirer', options, null)).toEqual([]);
    expect(workflowsPour('variation', options, retouche).map((w) => w.id)).toEqual(['krea2_turbo']);
    expect(rushesMax('variation', krea)).toBe(1);
    expect(rushesMax('inspirer', edit)).toBe(10);
    expect(rushesMax(null, krea)).toBe(0);
  });

  it('propose le LLM du workflow', () => {
    expect(llmInitial(options, 'krea2_turbo')).toBe('mistral.gguf');
    expect(llmInitial(options, 'qwen_image_2_1')).toBe('Ternary-Bonsai-2-27B-PTQ1_0.gguf');
    expect(llmInitial({ ...options, llm: [] }, 'krea2_turbo')).toBe('');
    expect(texteCommande(commande({ prompt: '', idee: 'un renard' }))).toBe('un renard');
    expect(texteCommande(commande({}))).toBe('a fox');
  });

  it('écrit les checkpoints et les steps', () => {
    expect(nomCheckpoint('qwen_image_2.1_bf16.safetensors')).toBe('qwen_image_2.1_bf16');
    expect(texteStepsSuggeres([])).toBe('La fiche ne dit rien de ses steps : essaie, puis ajuste.');
    expect(texteStepsSuggeres([8])).toBe('La fiche indique 8 steps.');
    expect(texteStepsSuggeres([25, 40])).toBe('La fiche indique 25 steps par défaut, 40 recommandé.');
  });

  it('résume l’avancement', () => {
    expect(texteAvancement(commande({}))).toBe('en attente');
    expect(texteAvancement(commande({ etat: 'en_cours', etape: 'Génération 2/3', progression: 0.456, nombre: 3, images: ['x'] }))).toBe(
      'Génération 2/3 · 46 % · 1/3 images',
    );
    expect(texteAvancement(commande({ etat: 'en_cours' }))).toBe('Préparation · 0 %');
    expect(texteAvancement(commande({ etat: 'erreur', erreur: 'ComfyUI ne répond pas' }))).toBe('ComfyUI ne répond pas');
    expect(texteAvancement(commande({ etat: 'termine', nombre: 2, images: ['a', 'b'] }))).toBe('terminée · 2/2 images');
  });
});
