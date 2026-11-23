import { describe, expect, it } from 'vitest';
import { erreurDuJournal, libelleCertitude, libelleSource } from './expliquer';

describe('expliquer une erreur', () => {
  it('dit la certitude et la source', () => {
    expect(libelleCertitude('sure')).toBe('Cause sûre');
    expect(libelleCertitude('inconnue')).toBe('Cause inconnue');
    expect(libelleSource({ source: 'base', llm: null })).toBe('Cause connue d’Aria');
    expect(libelleSource({ source: 'llm', llm: 'Mistral-Small.gguf' })).toBe('Analyse du LLM Mistral-Small');
    expect(libelleSource({ source: 'aucune', llm: null })).toBe('Aucune cause connue');
  });

  it('prend le message d’une opération du Journal', () => {
    expect(erreurDuJournal({ action: 'Téléchargement', objet: 'x.gguf', detail: 'Espace disque insuffisant' })).toBe(
      'Espace disque insuffisant',
    );
    expect(erreurDuJournal({ action: 'Téléchargement', objet: 'x.gguf' })).toBe('Téléchargement · x.gguf');
  });
});
