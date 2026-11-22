// Jauge d'une mesure. Tant que la mesure n'est pas faite, la jauge reste vide et dit « à tester » : jamais un
// chiffre supposé.

export const NON_MESURE = 'à tester';

/** Nombre de segments remplis : aucun sans mesure, au moins un dès qu'une mesure dépasse zéro. */
export function segmentsRemplis(fraction: number | null, segments: number): number {
  if (fraction === null || Number.isNaN(fraction) || segments <= 0) return 0;
  const bornee = Math.min(1, Math.max(0, fraction));
  return bornee === 0 ? 0 : Math.max(1, Math.round(bornee * segments));
}

/** Texte à côté de la jauge : « à tester » sans mesure ; sinon le texte de la mesure, ou à défaut son pourcentage. */
export function texteJauge(fraction: number | null, mesure?: string): string {
  if (fraction === null || Number.isNaN(fraction)) return NON_MESURE;
  const texte = mesure?.trim();
  return texte ? texte : `${Math.round(Math.min(1, Math.max(0, fraction)) * 100)} %`;
}

/** Une taille en octets, lisible : « 412,5 Go ». */
export function octetsLisibles(octets: number): string {
  const unites = ['o', 'Ko', 'Mo', 'Go', 'To'];
  let valeur = octets;
  let i = 0;
  while (valeur >= 1024 && i < unites.length - 1) {
    valeur /= 1024;
    i += 1;
  }
  const nombre = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: i === 0 ? 0 : 1 }).format(valeur);
  return `${nombre} ${unites[i]}`;
}
