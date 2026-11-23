// « 💬 M'aider à préciser » : la conversation gardée par l'interface, et ce qui en repart vers le LLM.

import type { EchangeConversation, FormeConversation, TourConversation } from './api';

/** Le message qui demande d'écrire le prompt (comme le cœur). */
export const ECRIRE = 'ÉCRIS LE PROMPT';

/** Un message affiché : ce que tu as écrit, ou un tour du LLM. */
export type MessageConversation = { role: 'user'; texte: string } | { role: 'assistant'; tour: TourConversation };

/** Les échanges envoyés au LLM : tes messages tels quels, ses tours tels qu'il les a rendus (JSON). */
export function echanges(messages: MessageConversation[]): EchangeConversation[] {
  return messages.map((m) => (m.role === 'user' ? { role: 'user', content: m.texte } : { role: 'assistant', content: m.tour.brut }));
}

/** La réponse à une question à choix, telle qu'elle part. */
export function reponse(question: string, choix: string): string {
  return `${question} → ${choix}`;
}

/** Le dernier prompt écrit dans la conversation, s'il y en a un. */
export function dernierPrompt(messages: MessageConversation[]): string | null {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const m = messages[i];
    if (m?.role === 'assistant' && m.tour.prompt) return m.tour.prompt;
  }
  return null;
}

function estTour(v: unknown): v is TourConversation {
  if (typeof v !== 'object' || v === null) return false;
  return (
    'message' in v &&
    typeof v.message === 'string' &&
    'brut' in v &&
    typeof v.brut === 'string' &&
    'fiche' in v &&
    Array.isArray(v.fiche) &&
    'questions' in v &&
    Array.isArray(v.questions) &&
    'prompt' in v &&
    (typeof v.prompt === 'string' || v.prompt === null)
  );
}

/** Une conversation gardée, si elle a la bonne forme (sinon, rien). */
export function conversationGardee(v: unknown): MessageConversation[] {
  if (!Array.isArray(v)) return [];
  const sortie: MessageConversation[] = [];
  for (const m of v) {
    if (typeof m !== 'object' || m === null || !('role' in m)) return [];
    if (m.role === 'user' && 'texte' in m && typeof m.texte === 'string') {
      sortie.push({ role: 'user', texte: m.texte });
    } else if (m.role === 'assistant' && 'tour' in m && estTour(m.tour)) {
      // Un tour gardé avant les alertes n'en a pas.
      const brutes: unknown = 'alertes' in m.tour ? m.tour.alertes : undefined;
      const lues = Array.isArray(brutes) ? brutes.filter((a: unknown): a is string => typeof a === 'string') : [];
      sortie.push({ role: 'assistant', tour: { ...m.tour, alertes: lues } });
    } else {
      return [];
    }
  }
  return sortie;
}

/** Où la conversation d'un atelier est gardée : une par atelier, une par plan d'une séquence (l'image garde la clé
 * d'avant les autres ateliers). */
export function cleConversation(forme: FormeConversation, plan: number | null = null): string {
  if (forme === 'image') return 'aria-scenario-conversation';
  if (forme === 'plan') return `aria-scenario-conversation-plan-${plan ?? 0}`;
  return `aria-scenario-conversation-${forme}`;
}
