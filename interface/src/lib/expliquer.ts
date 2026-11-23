// « Expliquer une erreur » : la cause connue d'Aria (aussitôt, sans LLM), sinon l'analyse du LLM, et les remèdes
// concrets dans l'appli (aria_coeur::expliquer).

import type { Certitude, Explication } from './api';

/** La certitude, telle qu'on la lit. */
export function libelleCertitude(certitude: Certitude): string {
  const libelles: Record<Certitude, string> = {
    sure: 'Cause sûre',
    probable: 'Cause probable',
    possible: 'Cause possible',
    inconnue: 'Cause inconnue',
  };
  return libelles[certitude];
}

/** D'où vient l'explication : une cause connue d'Aria, l'analyse d'un LLM (sans « .gguf »), ou rien. */
export function libelleSource(e: Pick<Explication, 'source' | 'llm'>): string {
  if (e.source === 'base') return 'Cause connue d’Aria';
  if (e.source === 'llm') return e.llm ? `Analyse du LLM ${e.llm.replace(/\.gguf$/i, '')}` : 'Analyse du LLM';
  return 'Aucune cause connue';
}

/** Le message d'une opération du Journal à expliquer : sa précision, sinon ce sur quoi elle portait. */
export function erreurDuJournal(op: { action: string; objet: string; detail?: string }): string {
  const detail = op.detail?.trim();
  return detail ? detail : `${op.action} · ${op.objet}`;
}
