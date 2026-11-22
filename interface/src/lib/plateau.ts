// Le Plateau : textes et saisies, d'après ce que rend le cœur.

import type { ChoixWorkflow, Commande, EtatCommande, OptionsPlateau } from './api';

const LIBELLES: Record<EtatCommande, string> = {
  attente: 'en attente',
  en_cours: 'en cours',
  termine: 'terminée',
  erreur: 'en erreur',
  annule: 'annulée',
};

/** L'état d'une commande, en mots. */
export function libelleEtat(etat: EtatCommande): string {
  return LIBELLES[etat];
}

/** Vrai tant que la commande peut encore produire une image (elle s'annule alors). */
export function estActive(commande: Commande): boolean {
  return commande.etat === 'attente' || commande.etat === 'en_cours';
}

/** La plus grande seed proposée : un entier que JavaScript garde exact. */
export const SEED_MAX = Number.MAX_SAFE_INTEGER;

/**
 * La seed tapée : `null` si le champ est vide (tirée au hasard), un entier de 0 à SEED_MAX, ou `undefined` si la
 * saisie n'en est pas un.
 */
export function lireSeed(texte: string): number | null | undefined {
  const propre = texte.trim();
  if (propre === '') return null;
  if (!/^\d+$/.test(propre)) return undefined;
  const seed = Number(propre);
  return Number.isSafeInteger(seed) && seed <= SEED_MAX ? seed : undefined;
}

/** Le format proposé pour ce workflow : celui de ses réglages s'il est encore proposé, sinon le premier. */
export function formatInitial(options: OptionsPlateau, workflow: ChoixWorkflow | undefined): string {
  const regle = workflow?.format;
  if (regle && options.formats.includes(regle)) return regle;
  return options.formats[0] ?? '';
}

/** Le checkpoint proposé pour ce workflow : celui de ses réglages s'il est prêt, sinon le premier prêt. */
export function modeleInitial(workflow: ChoixWorkflow | undefined): string {
  if (!workflow) return '';
  const regle = workflow.modeleDefaut;
  if (regle && workflow.modeles.includes(regle)) return regle;
  return workflow.modeles[0] ?? '';
}

/** Le LLM proposé pour ce workflow : le sien s'il est installé, sinon le premier proposé. */
export function llmInitial(options: OptionsPlateau, workflowId: string): string {
  const sien = options.llmParWorkflow[workflowId];
  if (sien && options.llm.some((l) => l.fichier === sien)) return sien;
  return options.llm[0]?.fichier ?? '';
}

/** Ce qu'une commande montre : le prompt, ou l'idée tant que le LLM ne l'a pas réécrite. */
export function texteCommande(commande: Commande): string {
  return commande.prompt || commande.idee;
}

/** Le nom d'un checkpoint sans son extension. */
export function nomCheckpoint(fichier: string): string {
  return fichier.replace(/\.safetensors$/i, '');
}

/** Les steps que dit la fiche, à proposer avant de valider un checkpoint. */
export function texteStepsSuggeres(steps: number[]): string {
  if (steps.length === 0) return 'La fiche ne dit rien de ses steps : essaie, puis ajuste.';
  const [defaut, ...autres] = steps;
  if (autres.length === 0) return `La fiche indique ${defaut} steps.`;
  return `La fiche indique ${defaut} steps par défaut, ${autres.join(' ou ')} recommandé.`;
}

/** L'avancement d'une commande, en une ligne. */
export function texteAvancement(commande: Commande): string {
  const faites = commande.images.length;
  const total = commande.nombre;
  const compte = total > 1 ? ` · ${faites}/${total} images` : '';
  if (commande.etat === 'en_cours') {
    const etape = commande.etape || 'Préparation';
    return `${etape} · ${Math.round(commande.progression * 100)} %${compte}`;
  }
  if (commande.etat === 'erreur') return commande.erreur ?? 'Erreur inconnue';
  return `${libelleEtat(commande.etat)}${compte}`;
}
