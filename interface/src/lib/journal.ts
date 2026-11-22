// Le journal : affichage des dates.

/** Une date du journal (RFC 3339) en « 21/11/2026 18:45:02 » ; le texte reçu tel quel s'il ne se lit pas. */
export function dateLisible(date: string): string {
  const lue = new Date(date);
  if (Number.isNaN(lue.getTime())) return date;
  const jour = lue.toLocaleDateString('fr-FR');
  const heure = lue.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  return `${jour} ${heure}`;
}
