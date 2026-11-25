// Les points de couleur : bloc « État » du menu de l'icône, pastilles de l'état en direct.
import type { ActionMenu, EtatMoteur, LigneMenu, PageMenu, Ton, Vram } from './api';

/** Les pages du bloc « Aller à » du menu de l'icône, dans l'ordre du menu. */
export const pagesDuMenu: { page: PageMenu; titre: string }[] = [
  { page: 'plateau', titre: 'Plateau' },
  { page: 'montage', titre: 'Montage' },
  { page: 'modeles', titre: 'Modèles' },
  { page: 'reglages', titre: 'Réglages' },
  { page: 'parametres', titre: 'Paramètres' },
];

/** La page que demande l'événement « aller-a » : une page du menu, ou rien (« mise-a-jour » a son propre chemin). */
export function pageDuMenu(cible: string): PageMenu | null {
  return pagesDuMenu.find((p) => p.page === cible)?.page ?? null;
}

/** Une ligne du bloc « Agir sans ouvrir ». */
export interface LigneAction {
  action: ActionMenu;
  titre: string;
  /** Grisée quand elle n'a rien à faire. */
  active: boolean;
}

/** Le bloc « Agir sans ouvrir » : annuler la génération en cours (seulement si une tourne), libérer la carte. */
export function actionsSansOuvrir(generationEnCours: boolean): LigneAction[] {
  return [
    { action: 'annulerGeneration', titre: 'Annuler la génération en cours', active: generationEnCours },
    { action: 'libererCarte', titre: 'Libérer la carte graphique', active: true },
  ];
}

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

/** Une lecture de VRAM n'est utilisable que si les deux valeurs existent et que la carte a une mémoire. */
function vramLisible(vram: Vram | null): vram is Vram {
  return vram !== null && Number.isFinite(vram.utiliseeGo) && Number.isFinite(vram.totaleGo) && vram.totaleGo > 0 && vram.utiliseeGo >= 0;
}

/**
 * La part de VRAM utilisée, de 0 à 1 : le fond orangé des pastilles ComfyUI et LLM, dont le bloc entier représente
 * la VRAM totale. `null` sans lecture : pas de fond, jamais une part supposée.
 */
export function fractionVram(vram: Vram | null): number | null {
  if (!vramLisible(vram)) return null;
  return Math.min(1, vram.utiliseeGo / vram.totaleGo);
}

/** Des Go à la française, au dixième, sans « ,0 » inutile : 9,8 ou 16. */
function enGo(valeur: number): string {
  return valeur.toFixed(1).replace('.', ',').replace(/,0$/, '');
}

/** L'info-bulle des pastilles : « 9,8 Go sur 16 Go ». `null` sans lecture. */
export function texteVram(vram: Vram | null): string | null {
  if (!vramLisible(vram)) return null;
  return `${enGo(Math.min(vram.utiliseeGo, vram.totaleGo))} Go sur ${enGo(vram.totaleGo)} Go`;
}
