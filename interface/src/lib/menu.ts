// Les points de couleur : bloc « État » du menu de l'icône, pastilles de l'état en direct.
import type { EtatMoteur, LigneMenu, Ton } from './api';

/** La couleur du point d'une ligne « État » : la même palette que la pastille de l'icône. */
export function couleurTon(ton: Ton): string {
  switch (ton) {
    case 'actif':
      return 'var(--cyan)';
    case 'attention':
      return 'var(--orange)';
    case 'information':
      return 'var(--bleu)';
    case 'repos':
      return '#3a4d70';
  }
}

/** Une erreur montrée dans le bloc « État », à regarder. */
export function ligneErreur(message: string): LigneMenu {
  return { ton: 'attention', texte: message.charAt(0).toUpperCase() + message.slice(1) };
}

/** Une pastille de l'état en direct, en haut à droite des pages. */
export interface Pastille {
  nom: string;
  detail: string;
  ton: Ton;
}

/**
 * Les pastilles de l'état en direct d'Aria Studio : ComfyUI et le LLM (llama.cpp, officiel ou PrismML). Un moteur
 * en marche est « actif », un moteur installé qui attend est « au repos » (Aria le lance au besoin), un moteur
 * absent est « à regarder ». `null` : l'état n'est pas encore lu.
 */
export function pastillesMoteurs(moteurs: EtatMoteur[] | null): Pastille[] {
  const pastille = (nom: string, parmi: EtatMoteur[] | undefined): Pastille => {
    if (!parmi) return { nom, detail: 'lecture…', ton: 'repos' };
    if (parmi.some((m) => m.occupe)) return { nom, detail: 'en marche', ton: 'actif' };
    if (parmi.length === 0 || parmi.every((m) => m.statut === 'absent')) {
      return { nom, detail: 'à installer', ton: 'attention' };
    }
    return { nom, detail: 'au repos', ton: 'repos' };
  };
  const comfy = moteurs?.filter((m) => m.id === 'comfyui');
  const llm = moteurs?.filter((m) => m.id.startsWith('llama'));
  return [pastille('ComfyUI', comfy), pastille('LLM', llm)];
}
