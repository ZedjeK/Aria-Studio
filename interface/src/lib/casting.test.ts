import { describe, expect, it } from 'vitest';
import type { PreparationCasting, VueCasting } from './api';
import { avancementPrompt, estVideo, etatCasting, moyennes, nombreDe, resumeTache, unite } from './casting';

const preparation = (changements: Partial<PreparationCasting>): PreparationCasting => ({
  tache: 'texte_vers_image',
  candidats: [],
  rendu: null,
  sources: [],
  essais: [],
  seeds: [42, 1234],
  total: 0,
  blocage: null,
  ...changements,
});

const generation = (prompt: string, note: number | null) => ({
  prompt,
  seed: 42,
  fichier: `${prompt}_42.png`,
  note,
  peut_servir_de_source: false,
  est_la_source: false,
  texte: null,
  controles: null,
});

describe('casting', () => {
  it('dit ce que produirait une tâche, ou ce qui l’en empêche', () => {
    const deux = [
      { id: 'a', nom: 'A', manquants: 0 },
      { id: 'b', nom: 'B', manquants: 2 },
    ];
    expect(resumeTache(preparation({ candidats: deux, total: 24 }))).toBe('2 modèles · 24 images à produire');
    expect(resumeTache(preparation({ candidats: deux, blocage: 'B pas complet' }))).toBe('2 modèles · 1 incomplet');
    expect(resumeTache(preparation({ candidats: [], blocage: 'aucun' }))).toBe('aucun modèle');
    expect(resumeTache(preparation({ tache: 'reecriture', candidats: deux.slice(0, 1), blocage: 'x' }))).toBe(
      '1 seul candidat · rien à comparer',
    );
    expect(unite('image_vers_video')).toBe('clip');
    expect(nombreDe(1, 'clip')).toBe('1 clip');
  });

  it('suit les notes', () => {
    expect(etatCasting({ tache: 'texte_vers_video', revele: false, notees: 3, total: 8 })).toBe("à l'aveugle · 3 / 8 notés");
    const c: VueCasting = {
      id: 'x',
      tache: 'texte_vers_image',
      date: '2026-10-19T22:22:30',
      seeds: [42],
      prompts: [],
      vram_base_go: 0.4,
      revele: false,
      favori: null,
      par_defaut: null,
      notees: 2,
      total: 4,
      modeles: [
        { cle: 'A', nom: 'Modèle A', generations: [generation('portrait', 4), generation('neon', 2)], resume: null, liberation: null },
        { cle: 'B', nom: 'Modèle B', generations: [generation('portrait', null), generation('neon', null)], resume: null, liberation: null },
      ],
    };
    expect(moyennes(c.modeles)).toEqual([3, null]);
    expect(avancementPrompt(c, 'portrait')).toEqual({ notees: 1, total: 2 });
    expect(estVideo('portrait_42.MP4')).toBe(true);
  });
});
