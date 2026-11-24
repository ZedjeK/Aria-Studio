// Les LLM du catalogue, rangés pour la page Modèles : les généralistes d'abord, puis les spécialistes de Qwen-Image 2.1,
// qui sont facultatifs (le généraliste en place sait déjà écrire pour ce modèle).

import type { LlmCatalogue } from './api';

/** Les LLM du catalogue en deux groupes, et si un généraliste est déjà installé. */
export interface LlmRanges {
  generalistes: LlmCatalogue[];
  specialistes: LlmCatalogue[];
  generalisteInstalle: boolean;
}

export function rangerLlm(llm: LlmCatalogue[]): LlmRanges {
  const generalistes = llm.filter((l) => l.specialiste === null);
  return {
    generalistes,
    specialistes: llm.filter((l) => l.specialiste !== null),
    generalisteInstalle: generalistes.some((l) => l.installe),
  };
}

/** Ce qu'un spécialiste remplace, et seulement cela. */
export function ceQueRemplace(specialiste: 'creation' | 'retouche'): string {
  return specialiste === 'creation'
    ? "la réécriture d'une idée (pas les références, ni la vidéo)"
    : "la traduction d'une consigne de retouche (il voit l'image source)";
}
