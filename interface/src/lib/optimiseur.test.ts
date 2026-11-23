import { describe, expect, it } from 'vitest';
import type { ParamReglage, Proposition } from './api';
import { ecarts, parCible, texteMateriel, valeurLisible } from './optimiseur';

const param = (cle: string, valeur: number | string | boolean): ParamReglage => ({
  cle,
  libelle: cle,
  type: typeof valeur === 'number' ? 'reel' : typeof valeur === 'boolean' ? 'booleen' : 'choix',
  defaut: valeur,
  valeur,
  modifie: false,
  min: null,
  max: null,
  pas: null,
  choix: [],
  aide: '',
  libelles: {},
});

const proposition = (cle: string, propose: number | string, cible = 'krea2_turbo', type = 'workflow'): Proposition => ({
  type,
  cible,
  nom: cible,
  cle,
  libelle: cle,
  actuel: null,
  propose,
  raison: 'Recommandation officielle',
  source: null,
});

describe('optimiseur', () => {
  it('compare aux champs, saisie en cours comprise', () => {
    const params = [param('cfg', 3.5), param('sampler', 'euler')];
    const r = ecarts([proposition('cfg', 1), proposition('sampler', 'euler'), proposition('steps', 8)], params, {
      cfg: 1.5,
    });
    expect(r.aChanger.map((e) => [e.proposition.cle, e.avant])).toEqual([['cfg', 1.5]]);
    expect(r.dejaBon.map((p) => p.cle)).toEqual(['sampler']);
  });

  it('regroupe par modèle', () => {
    const g = parCible([
      proposition('top_k', 20, 'a.gguf', 'llm'),
      proposition('contexte', 16384, 'a.gguf', 'llm'),
      proposition('top_p', 0.8, 'b.gguf', 'llm'),
    ]);
    expect(g).toEqual([
      { type: 'llm', cible: 'a.gguf', changements: { top_k: 20, contexte: 16384 } },
      { type: 'llm', cible: 'b.gguf', changements: { top_p: 0.8 } },
    ]);
  });

  it('écrit la machine et les valeurs comme en v2', () => {
    expect(
      texteMateriel({
        gpu: 'RTX 4080',
        vramTotaleGo: 16,
        vramAutresGo: 1.2,
        vramLibreGo: 14.8,
        ramGo: 63.9,
        coeurs: 8,
        threads: 16,
      }),
    ).toBe('RTX 4080 · 16 Go de VRAM (dont 1,2 Go pris par tes autres applis) · 63,9 Go de RAM · 8 cœurs / 16 threads');
    expect([valeurLisible(true), valeurLisible(0.8), valeurLisible('euler'), valeurLisible(null)]).toEqual([
      'oui',
      '0,8',
      'euler',
      '—',
    ]);
  });
});
