// Le LLM regarde ce qui est sorti (nouveauté de la v3) : badges des créations vérifiées ou retenues, textes des
// bilans, et ce qui se relance d'ici (comme aria_coeur::verification::relance).

import { CHOIX_MIN, IMAGES_MAX_VISION, type Choix, type Creation, type Verifications } from './api';

/** Le badge d'une création vérifiée : fidèle, ou avec des écarts. */
export function badgeVerification(c: Creation): string | null {
  if (!c.verification) return null;
  return c.verification.fidele ? '✓ Fidèle' : '⚠ Écarts';
}

/** Le badge d'une création retenue parmi les meilleures de son lot. */
export function badgeMeilleure(c: Creation): string | null {
  if (!c.meilleure) return null;
  return c.meilleure.rang === 1 ? '★ Meilleure' : `★ ${c.meilleure.rang}e`;
}

/** Un lot à comparer : de 2 à 8 images. */
export function peutChoisir(nombre: number): boolean {
  return nombre >= CHOIX_MIN && nombre <= IMAGES_MAX_VISION;
}

/** Le bilan d'une vérification, pour la ligne d'état. */
export function texteVerifications(v: Verifications): string {
  const fideles = v.resultats.filter((r) => r.verification?.fidele).length;
  const ecarts = v.resultats.filter((r) => r.verification && !r.verification.fidele).length;
  const erreurs = v.resultats.filter((r) => r.erreur);
  const morceaux = [`${fideles} fidèle${fideles > 1 ? 's' : ''}`];
  if (ecarts > 0) morceaux.push(`${ecarts} avec des écarts`);
  if (erreurs.length > 0) {
    const premiere = erreurs[0]?.erreur ?? '';
    morceaux.push(`${erreurs.length} non vérifiée${erreurs.length > 1 ? 's' : ''} (${premiere})`);
  }
  return `Vérification par ${v.llm} : ${morceaux.join(', ')}.`;
}

/** Le bilan d'un choix, pour la ligne d'état. */
export function texteChoix(c: Choix): string {
  const n = c.meilleures.length;
  return `${n} image${n > 1 ? 's' : ''} retenue${n > 1 ? 's' : ''} par ${c.llm} : badge ★ dans les Rushes.`;
}

/** Pourquoi une création ne se relance pas d'ici avec un prompt corrigé ; rien si elle se relance. */
export function pasRelancable(c: Creation): string | null {
  if (c.type === 'creation') return null;
  if (c.type === 'clip') {
    return c.mode_video === 'animer' || c.mode_video === 'texte'
      ? null
      : 'Un plan ou un film de séquence se retourne depuis sa séquence, au Montage.';
  }
  if (c.type === 'retouche' || c.type === 'agrandissement') {
    return 'Une retouche se relance depuis la page Retouche (le masque peint n’est pas gardé) : copie le prompt corrigé dans sa consigne.';
  }
  return 'Cette création ne se relance pas d’ici.';
}
