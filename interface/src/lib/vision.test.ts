import { describe, expect, it } from 'vitest';
import type { Creation, Verification } from './api';
import { badgeMeilleure, badgeVerification, pasRelancable, peutChoisir, texteChoix, texteVerifications } from './vision';

const creation = (changements: Partial<Creation>): Creation => ({
  id: '20261123-101500-abcdef',
  date: '2026-11-23T10:15:00',
  largeur: 1024,
  hauteur: 1024,
  type: 'creation',
  ...changements,
});

const verification = (fidele: boolean): Verification => ({
  fidele,
  verdict: fidele ? 'Conforme.' : 'Le renard est gris.',
  ecarts: fidele ? [] : ['renard gris'],
  prompt: fidele ? null : 'A red fox.',
  alertes: [],
  llm: 'mistral.gguf',
  date: '2026-11-23T10:16:00',
});

describe('vision', () => {
  it('donne les badges', () => {
    expect(badgeVerification(creation({}))).toBeNull();
    expect(badgeVerification(creation({ verification: verification(true) }))).toBe('✓ Fidèle');
    expect(badgeVerification(creation({ verification: verification(false) }))).toBe('⚠ Écarts');
    const meilleure = { rang: 1, raison: '', lot: [], llm: 'x', date: '' };
    expect(badgeMeilleure(creation({ meilleure }))).toBe('★ Meilleure');
    expect(badgeMeilleure(creation({ meilleure: { ...meilleure, rang: 2 } }))).toBe('★ 2e');
  });

  it('compte un lot à comparer', () => {
    expect([1, 2, 8, 9].map(peutChoisir)).toEqual([false, true, true, false]);
  });

  it('résume les bilans', () => {
    expect(
      texteVerifications({
        resultats: [
          { id: 'a', verification: verification(true), erreur: null },
          { id: 'b', verification: verification(false), erreur: null },
          { id: 'c', verification: null, erreur: 'Image importée : pas d’intention à comparer' },
        ],
        llm: 'mistral.gguf',
        dureeS: 12,
      }),
    ).toBe(
      'Vérification par mistral.gguf : 1 fidèle, 1 avec des écarts, 1 non vérifiée (Image importée : pas d’intention à comparer).',
    );
    expect(texteChoix({ meilleures: [{ id: 'a', rang: 1, raison: '' }], redemandee: null, llm: 'm', dureeS: 3 })).toBe(
      '1 image retenue par m : badge ★ dans les Rushes.',
    );
  });

  it('dit ce qui se relance d’ici', () => {
    expect(pasRelancable(creation({}))).toBeNull();
    expect(pasRelancable(creation({ type: 'clip', mode_video: 'animer' }))).toBeNull();
    expect(pasRelancable(creation({ type: 'clip', mode_video: 'sequence' }))).toContain('séquence');
    expect(pasRelancable(creation({ type: 'retouche' }))).toContain('Retouche');
    expect(pasRelancable(creation({ type: 'import' }))).toContain('ne se relance pas');
  });
});
