// Le Montage : l'estimation d'un tournage, comme en v2.

import type { EstimationTournage, QualiteVideo } from './api';

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

/** Des gigaoctets lisibles : 15,2 Go. */
const go = (n: number | null): string => `${String(n ?? 0).replace('.', ',')} Go`;

/**
 * Le texte de l'estimation, comme en v2 : le calcul du cœur se lit dans l'ordre où il se fait (VRAM libre → steps du
 * checkpoint → poids du checkpoint → résolution → durée → clips) ; sans VRAM lue, ou en attendant le calcul, le
 * tableau d'origine (`secondesTableau`).
 */
export function texteEstimation(
  calcul: EstimationTournage | null,
  secondesTableau: number,
  nomQualite: string,
  duree: number,
): string {
  const chaud = ' · à chaud, sans le chargement du modèle au premier clip';
  if (calcul?.source === 'calcul') {
    const n = calcul.nombre;
    const clips = `${n} clip${n > 1 ? 's' : ''}${n > 1 ? ` de ≈ ${dureeLisible(calcul.clipS)}` : ''}`;
    return (
      `≈ ${dureeLisible(calcul.totalS)} sur ta carte : ${go(calcul.vramLibreGo)} de VRAM libre → ${calcul.steps} steps` +
      ` → modèle de ${go(calcul.poidsGo)} → ${nomQualite} → ${duree} s → ${clips}${chaud}`
    );
  }
  const secondes = calcul ? calcul.totalS : secondesTableau;
  return `≈ ${dureeLisible(secondes)} sur ta carte (tableau d'origine, mesuré sur une RTX 4080 : VRAM non lue)${chaud}`;
}
