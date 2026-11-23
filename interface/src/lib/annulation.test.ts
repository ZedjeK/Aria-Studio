import { describe, expect, it } from 'vitest';
import { estAnnulation } from './api';

describe('✕ Annuler', () => {
  it("reconnaît l'annulation, et seulement elle", () => {
    expect(estAnnulation('Annulé')).toBe(true);
    expect(estAnnulation(new Error('Annulé'))).toBe(true);
    expect(estAnnulation('llama-server ne répond pas')).toBe(false);
    expect(estAnnulation(null)).toBe(false);
  });
});
