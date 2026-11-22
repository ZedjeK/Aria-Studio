// Modèles → onglets d'usage : ce que chaque workflow attend du LLM et ce qu'on dit de lui en une ligne.

import type { ChoixLlm, ReecritureUsage, WorkflowUsage } from './api';
import { octetsLisibles } from './jauge';

const STYLES: Record<string, string> = {
  prose: 'de la prose descriptive',
  instruction: "une consigne d'édition",
  brief_video: 'un brief vidéo',
};

/** Ce que le modèle attend du LLM : style, langue, longueur, règles propres. */
export function attentes(r: ReecritureUsage): string {
  const langue = r.langue === 'en' ? 'anglais' : r.langue === 'fr' ? 'français' : r.langue;
  return [
    `${STYLES[r.style] ?? r.style} en ${langue}`,
    r.motsMax ? `${r.motsMax} mots au plus` : '',
    r.regles ? `${r.regles} règle${r.regles > 1 ? 's' : ''} propre${r.regles > 1 ? 's' : ''}` : '',
  ]
    .filter((t) => t !== '')
    .join(' · ');
}

/** Le nom d'un LLM d'après son fichier. */
export function nomLlm(fichier: string, choix: ChoixLlm[]): string {
  return choix.find((c) => c.fichier === fichier)?.nom ?? fichier;
}

/** Les puces d'un workflow : famille, taille, VRAM, références, redessin. */
export function puces(wf: WorkflowUsage): string[] {
  return [
    `Famille ${wf.famille}`,
    octetsLisibles(wf.taille),
    wf.vramMinGo ? `VRAM mesurée ≥ ${wf.vramMinGo} Go` : wf.vramEstimeeGo ? `VRAM estimée ${wf.vramEstimeeGo} Go` : '',
    wf.references ? `${wf.references} image${wf.references > 1 ? 's' : ''} de référence au plus` : '',
    wf.variation ? 'redessiner une image' : '',
  ].filter((t) => t !== '');
}

/** Le type cherché dans Rechercher pour les checkpoints d'un workflow : image ou vidéo selon sa tâche. */
export function natureDe(wf: WorkflowUsage): 'image' | 'video' {
  return wf.tache.endsWith('video') ? 'video' : 'image';
}
