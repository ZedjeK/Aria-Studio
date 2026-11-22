// Le Montage : l'estimation d'un tournage, comme en v2.

import type { QualiteVideo } from './api';

/** Les secondes estimées pour `nombre` clips de `duree` secondes : la mesure pour 5 s, en (D / 5)^exposant. */
export function estimationClips(qualite: QualiteVideo, duree: number, nombre: number, exposant: number): number {
  return qualite.estimationS * (duree / 5) ** exposant * nombre;
}

/** Une durée lisible : 45 s, 3 min, 1 h 05. */
export function dureeLisible(secondes: number): string {
  const s = Math.round(secondes);
  if (s < 90) return `${s} s`;
  const minutes = Math.round(s / 60);
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, '0')}`;
}
