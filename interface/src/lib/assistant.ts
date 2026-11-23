// L'assistant de premier lancement, comme en v2 (web/assistant.js) : les étapes, et ce qui permet de passer à la
// suivante, d'après ce que rend le cœur (aria_coeur::assistant).

import type { Pack, PlanAssistant, Telechargement } from './api';

/** Les six étapes, dans l'ordre. */
export const ETAPES = ['Ta machine', 'Moteurs', 'Ce que tu veux faire', 'Licences', 'Téléchargement', "C'est prêt"];

/** Les packs cochés à l'ouverture : les conseillés qui ne sont pas encore installés. */
export function choixInitial(packs: Pack[]): string[] {
  return packs.filter((p) => p.conseille && !p.installe).map((p) => p.id);
}

/** Des octets en gigaoctets ronds, comme la v2 : « 23 Go ». */
export function go(octets: number): string {
  return `${Math.round(octets / 2 ** 30)} Go`;
}

/** Ce que l'écran des packs dit du total, et si l'on peut passer aux licences. */
export function bilanPlan(
  packs: Pack[],
  choisis: string[],
  plan: PlanAssistant | null,
): { texte: string; ko: boolean; suivant: boolean; alerte: string } {
  if (choisis.length === 0 || !plan) {
    const tout = packs.length > 0 && packs.every((p) => p.installe);
    return {
      texte: tout ? 'Tout est déjà installé.' : 'Coche au moins un pack.',
      ko: false,
      suivant: packs.some((p) => p.installe),
      alerte: '',
    };
  }
  const sansMoteur = packs.some((p) => choisis.includes(p.id) && p.moteursAbsents.length > 0);
  const libre = plan.libre === null ? '' : ` · ${go(plan.libre)} libres`;
  return {
    texte:
      `À télécharger : ${go(plan.aTelecharger)} (fichiers communs comptés une fois)${libre}` +
      (plan.placeOk ? '' : ' · pas assez de place : décoche un pack ou change le dossier des modèles'),
    ko: !plan.placeOk,
    suivant: plan.placeOk && !sansMoteur,
    alerte: sansMoteur ? "Un pack coché demande un moteur absent : installe-le à l'étape Moteurs" : '',
  };
}

const FINIS: Telechargement['etat'][] = ['termine', 'erreur', 'annule'];

/** Le suivi des téléchargements lancés par l'assistant : reçus sur total, et s'ils sont tous finis. */
export function suiviTelechargements(taches: Telechargement[]): { texte: string; finis: boolean } {
  const recu = taches.reduce((s, t) => s + (t.etat === 'termine' ? t.taille : t.recu), 0);
  const tout = taches.reduce((s, t) => s + t.taille, 0);
  return {
    texte: tout > 0 ? `${go(recu)} sur ${go(tout)}` : '',
    finis: taches.every((t) => FINIS.includes(t.etat)),
  };
}

const ETATS: Record<Telechargement['etat'], string> = {
  attente: 'en attente',
  en_cours: 'en cours',
  verification: 'vérification',
  termine: '✓',
  erreur: 'erreur',
  annule: 'annulé',
};

/** L'état d'un téléchargement en mots, avec son avancement s'il est en cours. */
export function etatTelechargement(t: Telechargement): string {
  if (t.etat !== 'en_cours') return ETATS[t.etat];
  const pourcent = t.taille > 0 ? Math.floor((t.recu * 100) / t.taille) : 0;
  return `en cours · ${pourcent} %`;
}

/** Les packs à donner à « terminer » : ceux déjà installés et ceux qu'on vient de choisir, sans doublon. */
export function packsAFinir(packs: Pack[], choisis: string[]): string[] {
  return [...new Set([...packs.filter((p) => p.installe).map((p) => p.id), ...choisis])];
}
