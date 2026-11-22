import { describe, expect, it } from 'vitest';
import { dateLisible } from './journal';

describe('journal', () => {
  it('écrit une date du journal à la française', () => {
    const texte = dateLisible('2026-11-21T18:45:02+01:00');
    expect(texte).toMatch(/^\d{2}\/\d{2}\/\d{4} \d{2}:\d{2}:\d{2}$/);
  });

  it('rend tel quel un texte qui n’est pas une date', () => {
    expect(dateLisible('hier')).toBe('hier');
  });
});
