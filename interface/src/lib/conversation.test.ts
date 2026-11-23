import { describe, expect, it } from 'vitest';
import type { TourConversation } from './api';
import {
  ECRIRE,
  cleConversation,
  conversationGardee,
  dernierPrompt,
  echanges,
  comprisDernier,
  demandeRetrait,
  pourModifier,
  pourReessayer,
  reponse,
  texteACopier,
  unTourEnArriere,
  type MessageConversation,
} from './conversation';

const tour = (prompt: string | null): TourConversation => ({
  message: 'Compris.',
  fiche: [{ nom: 'Sujet', valeur: 'un renard' }],
  questions: [],
  prompt,
  brut: '{"message":"Compris."}',
  alertes: [],
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

  it('revient en arrière, réessaie et modifie', () => {
    const messages: MessageConversation[] = [
      { role: 'user', texte: 'un renard' },
      { role: 'assistant', tour: tour(null) },
      { role: 'user', texte: 'dans la neige' },
      { role: 'assistant', tour: tour('A fox') },
    ];
    expect(unTourEnArriere(messages)).toEqual(messages.slice(0, 2));
    expect(unTourEnArriere(messages.slice(0, 3))).toEqual(messages.slice(0, 2));
    expect(unTourEnArriere(messages.slice(0, 2))).toEqual([]);
    expect(unTourEnArriere([])).toEqual([]);

    expect(pourReessayer(messages)).toEqual({ avant: messages.slice(0, 2), texte: 'dans la neige' });
    expect(pourReessayer(messages.slice(0, 3))).toBeNull();
    expect(pourReessayer([])).toBeNull();

    expect(pourModifier(messages, 2)).toEqual({ avant: messages.slice(0, 2), texte: 'dans la neige' });
    expect(pourModifier(messages, 1)).toBeNull();
    expect(pourModifier([{ role: 'user', texte: ECRIRE }], 0)).toBeNull();
  });

  it('montre ce que le LLM a compris et sait le lui faire oublier', () => {
    const messages: MessageConversation[] = [
      { role: 'user', texte: 'un renard' },
      { role: 'assistant', tour: tour(null) },
    ];
    expect(comprisDernier(messages)).toEqual([{ nom: 'Sujet', valeur: 'un renard' }]);
    expect(comprisDernier(messages.slice(0, 1))).toEqual([]);
    expect(demandeRetrait('Sujet', 'un renard')).toBe('Retire ça de ce que tu as compris : Sujet : un renard');
  });

  it('copie le prompt écrit, sinon le message', () => {
    expect(texteACopier({ role: 'user', texte: 'bonjour' })).toBe('bonjour');
    expect(texteACopier({ role: 'assistant', tour: tour('A fox') })).toBe('A fox');
    expect(texteACopier({ role: 'assistant', tour: tour(null) })).toBe('Compris.');
  });

  it('relit une conversation gardée sans lui faire confiance', () => {
    const gardee = [{ role: 'user', texte: 'x' }, { role: 'assistant', tour: tour('A fox') }];
    expect(conversationGardee(gardee)).toHaveLength(2);
    expect(conversationGardee([{ role: 'user', texte: 3 }])).toEqual([]);
    expect(conversationGardee('n’importe quoi')).toEqual([]);
  });

  it('garde une conversation par atelier et par plan', () => {
    expect(cleConversation('image')).toBe('aria-scenario-conversation');
    expect(cleConversation('retouche')).toBe('aria-scenario-conversation-retouche');
    expect(cleConversation('plan', 2)).toBe('aria-scenario-conversation-plan-2');
    // Un tour gardé avant les alertes en reçoit une liste vide.
    const { alertes: _sans, ...ancien } = tour('A fox');
    const relue = conversationGardee([{ role: 'assistant', tour: ancien }]);
    expect(relue[0]).toEqual({ role: 'assistant', tour: tour('A fox') });
  });
});
