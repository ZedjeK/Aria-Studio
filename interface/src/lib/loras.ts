// Les LoRA cochées au Plateau et à la Retouche : celles de la famille du workflow choisi, avec leur force et leurs
// mots déclencheurs (retenus par Aria d'une fois sur l'autre).

import type { ChoixLora, LoraProposee } from './api';

/** Une ligne du sélecteur. */
export interface LigneLora {
  actif: boolean;
  force: number;
  declencheurs: string;
}

/** Les lignes d'un workflow : ce qui était coché ou réglé est gardé, le reste part des valeurs retenues. */
export function lignesPour(proposees: LoraProposee[], avant: Record<string, LigneLora>): Record<string, LigneLora> {
  return Object.fromEntries(
    proposees.map((l) => [
      l.fichier,
      avant[l.fichier] ?? { actif: false, force: l.force, declencheurs: l.declencheurs },
    ]),
  );
}

/** Les LoRA cochées, à envoyer avec la demande. */
export function lorasChoisies(proposees: LoraProposee[], lignes: Record<string, LigneLora>): ChoixLora[] {
  return proposees.flatMap((l) => {
    const ligne = lignes[l.fichier];
    return ligne?.actif ? [{ fichier: l.fichier, force: ligne.force, declencheurs: ligne.declencheurs }] : [];
  });
}

/** Le nombre de LoRA cochées. */
export function actives(lignes: Record<string, LigneLora>): number {
  return Object.values(lignes).filter((l) => l.actif).length;
}

/** Une force lisible : 0,75. */
export function forceLisible(force: number): string {
  return String(Math.round(force * 100) / 100).replace('.', ',');
}
