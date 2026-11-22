import { describe, expect, it } from 'vitest';
import type { TourConversation } from './api';
import { ECRIRE, conversationGardee, dernierPrompt, echanges, reponse, type MessageConversation } from './conversation';

const tour = (prompt: string | null): TourConversation => ({
  message: 'Compris.',
  fiche: [{ nom: 'Sujet', valeur: 'un renard' }],
  questions: [],
  prompt,
  brut: '{"message":"Compris."}',
  modele: 'mistral.gguf',
  dureeS: 1.2,
});

describe('Conversation', () => {
  it('rend les échanges tels que le LLM les attend', () => {
    const messages: MessageConversation[] = [
      { role: 'user', texte: 'un renard' },
      { role: 'assistant', tour: tour(null) },
      { role: 'user', texte: ECRIRE },
    ];
    expect(echanges(messages)).toEqual([
      { role: 'user', content: 'un renard' },
      { role: 'assistant', content: '{"message":"Compris."}' },
      { role: 'user', content: 'ÉCRIS LE PROMPT' },
    ]);
    expect(dernierPrompt(messages)).toBeNull();
    expect(dernierPrompt([...messages, { role: 'assistant', tour: tour('A fox') }])).toBe('A fox');
    expect(reponse('Quel décor ?', 'Forêt')).toBe('Quel décor ? → Forêt');
  });

  it('relit une conversation gardée sans lui faire confiance', () => {
    const gardee = [{ role: 'user', texte: 'x' }, { role: 'assistant', tour: tour('A fox') }];
    expect(conversationGardee(gardee)).toHaveLength(2);
    expect(conversationGardee([{ role: 'user', texte: 3 }])).toEqual([]);
    expect(conversationGardee('n’importe quoi')).toEqual([]);
  });
});
