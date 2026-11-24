// « Ce que le LLM a écrit » : la consigne d'origine, ce que le LLM en a fait, le prompt réellement envoyé au modèle,
// et ce qui s'est passé à la réécriture (la fiche de la création garde `idee`, `prompt_llm`, `prompt_envoye`,
// `reecriture` et `reecriture_motif`). Une fiche plus ancienne n'a que l'idée et le prompt : elle se lit encore.

import type { Commande, Creation } from './api';

/** Le bloc montré sous un résultat. */
export interface TextesLlm {
  /** Ce que l'utilisateur a écrit. */
  idee: string;
  /** Ce que le LLM a écrit au premier essai, avant le nettoyage et le contrôle ; null s'il manque (fiche plus ancienne,
   * ou LLM qui a manqué). */
  retravaille: string | null;
  /** Le prompt réellement parti au modèle ; l'idée elle-même si le LLM a manqué. */
  envoye: string;
  /** Le LLM qui l'a écrit ; null s'il a manqué. */
  llm: string | null;
  /** Ce qui s'est passé, en une ligne ; null quand tout s'est bien passé. */
  remarque: string | null;
}

/** Ce qui s'est passé à la réécriture, d'après la fiche (aria_coeur::plateau::Reecriture). */
export function remarqueReecriture(etat: string | null | undefined, motif: string | null | undefined): string | null {
  const raison = motif ?? 'raison inconnue';
  switch (etat) {
    case 'repli':
      return `Le LLM a manqué (${raison}) : ta consigne est partie telle quelle.`;
    case 'redemandee':
      return `Le prompt a été redemandé au LLM (${raison}).`;
    case 'imparfaite':
      return `Le prompt a été redemandé au LLM, toujours imparfait (${raison}) : le meilleur essai est parti.`;
    default:
      return null;
  }
}

/**
 * Les textes d'une retouche réécrite par le LLM ; null quand le LLM n'était pas demandé ou qu'il n'a encore rien
 * rendu. La fiche de la création, quand elle est là, fait foi sur la commande.
 */
export function textesDuLlm(commande: Commande, fiche: Creation | undefined): TextesLlm | null {
  if (!commande.reecrire) return null;
  const envoye = fiche?.prompt_envoye || fiche?.prompt || commande.prompt;
  if (!envoye) return null;
  return {
    idee: fiche?.idee || commande.idee,
    retravaille: fiche?.prompt_llm || null,
    envoye,
    llm: fiche ? (fiche.llm ?? null) : commande.llm,
    remarque: remarqueReecriture(fiche?.reecriture, fiche?.reecriture_motif),
  };
}
