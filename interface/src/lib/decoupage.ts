// Scénario, atelier Séquence, comme en v2 (web/decoupage.js) : les plans de l'atelier, leur état (à écrire, prompt
// prêt, changé depuis l'écriture), ce qui part au LLM et ce qui part au Montage. Le cœur contrôle tout à nouveau.

import type {
  Enchainement,
  MomentReplique,
  PlanDemandeDecoupage,
  PlanEcrit,
  PlanSequence,
  Sequence,
  TraductionContrainte,
} from './api';

/** Les plans d'une séquence, au plus (aria_coeur::sequences::PLANS_MAX) ; l'atelier en veut deux au moins. */
export const PLANS_MAX = 12;
export const PLANS_MIN = 2;
/** Débit de parole confortable (aria_coeur::decoupage::MOTS_PAR_SECONDE). */
export const MOTS_PAR_SECONDE = 2.5;

export const TYPES_DEPART: [Enchainement, string][] = [
  ['texte', '✍️ Départ du texte'],
  ['rush', '🖼️ Départ de la référence'],
];
export const TYPES_SUITE: [Enchainement, string][] = [
  ['prolonger', '↪ Prolonger'],
  ['couper', '✂ Couper'],
  ['transition', '🌀 Transition'],
];
export const NOTES_TYPES: Record<Enchainement, string> = {
  texte: 'Ce qui se passe : elle entre dans le hangar, casque sous le bras…',
  rush: 'Ce qui bouge dans la référence : elle tourne la tête vers la porte…',
  prolonger: "La suite du même plan : elle pose son casque sur l'établi…",
  couper: 'Le nouveau moment ou angle : gros plan sur ses mains…',
  transition: 'Où la caméra arrive : la rue sous la pluie…',
};
export const MOMENTS: [MomentReplique, string][] = [
  ['debut', 'au début'],
  ['pendant', "pendant l'action"],
  ['fin', 'à la fin'],
];

/** Un plan de l'atelier. */
export interface PlanDecoupage {
  enchainement: Enchainement;
  duree: number;
  note: string;
  /** Le rush d'arrivée d'une transition. */
  rush: string | null;
  dialogue: boolean;
  replique: string;
  moment: MomentReplique;
  off: boolean;
  prompt: string;
  /** Le résumé en français écrit par le LLM. */
  idee: string;
  /** Ce que le prompt a pris en compte (`signature`) quand il a été écrit. */
  ecrit: string | null;
  /** Le plan correspondant dans la séquence du Montage (une mise à jour garde son clip). */
  idMontage: string | null;
}

/** Tout l'atelier, gardé en brouillon. */
export interface EtatDecoupage {
  synopsis: string;
  reference: string | null;
  contrainte: string;
  contrainteEn: TraductionContrainte | null;
  avecMusique: boolean;
  musique: string;
  /** Le cadre de la séquence (seulement si le plan 1 part du texte). */
  format: string;
  plans: PlanDecoupage[];
  alertes: string[];
  /** La séquence du Montage qu'un nouvel envoi met à jour. */
  sequenceId: string | null;
  titre: string;
}

const ENCHAINEMENTS: Enchainement[] = ['texte', 'rush', 'prolonger', 'couper', 'transition'];
const MOMENTS_VALIDES: MomentReplique[] = ['debut', 'pendant', 'fin'];

function texte(v: unknown): string {
  return typeof v === 'string' ? v : '';
}

function texteOuNull(v: unknown): string | null {
  return typeof v === 'string' && v !== '' ? v : null;
}

/** Un plan complet, quel que soit le brouillon d'où il vient. */
export function normaliserPlan(p: unknown): PlanDecoupage {
  const o: Record<string, unknown> = typeof p === 'object' && p !== null ? { ...p } : {};
  const enchainement = ENCHAINEMENTS.find((e) => e === o['enchainement']) ?? 'couper';
  const moment = MOMENTS_VALIDES.find((m) => m === o['moment']) ?? 'pendant';
  return {
    enchainement,
    duree: typeof o['duree'] === 'number' ? o['duree'] : 5,
    note: texte(o['note']),
    rush: texteOuNull(o['rush']),
    dialogue: o['dialogue'] === true,
    replique: texte(o['replique']),
    moment,
    off: o['off'] === true,
    prompt: texte(o['prompt']),
    idee: texte(o['idee']),
    ecrit: texteOuNull(o['ecrit']),
    idMontage: texteOuNull(o['idMontage']),
  };
}

/** Un nouveau plan : le premier part de la référence s'il y en a une, sinon du texte ; les suivants coupent. */
export function nouveauPlan(k: number, reference: string | null, enchainement?: Enchainement): PlanDecoupage {
  return normaliserPlan({ enchainement: enchainement ?? (k === 0 ? (reference ? 'rush' : 'texte') : 'couper') });
}

export function etatVide(): EtatDecoupage {
  return {
    synopsis: '',
    reference: null,
    contrainte: '',
    contrainteEn: null,
    avecMusique: false,
    musique: '',
    format: '16:9 (Widescreen)',
    plans: [nouveauPlan(0, null), nouveauPlan(1, null, 'prolonger'), nouveauPlan(2, null)],
    alertes: [],
    sequenceId: null,
    titre: '',
  };
}

/** L'atelier d'un brouillon, contrôlé : un brouillon abîmé repart de zéro, champ par champ. */
export function depuisBrouillon(d: unknown): EtatDecoupage {
  const vide = etatVide();
  if (typeof d !== 'object' || d === null) return vide;
  const o: Record<string, unknown> = { ...d };
  const plans = Array.isArray(o['plans']) ? o['plans'].map(normaliserPlan).slice(0, PLANS_MAX) : vide.plans;
  const en = o['contrainteEn'];
  return {
    synopsis: texte(o['synopsis']),
    reference: texteOuNull(o['reference']),
    contrainte: texte(o['contrainte']),
    contrainteEn:
      typeof en === 'object' && en !== null && 'fr' in en && 'en' in en && typeof en.fr === 'string' && typeof en.en === 'string'
        ? { fr: en.fr, en: en.en }
        : null,
    avecMusique: o['avecMusique'] === true,
    musique: texte(o['musique']),
    format: texte(o['format']) || vide.format,
    plans: plans.length >= PLANS_MIN ? plans : vide.plans,
    alertes: Array.isArray(o['alertes']) ? o['alertes'].filter((a): a is string => typeof a === 'string') : [],
    sequenceId: texteOuNull(o['sequenceId']),
    titre: texte(o['titre']),
  };
}

/** Ce que le prompt d'un plan a pris en compte : s'il change ensuite, le plan est à réécrire. */
export function signature(p: PlanDecoupage): string {
  return JSON.stringify([
    p.enchainement,
    p.duree,
    p.note.trim(),
    p.dialogue ? p.replique.trim() : '',
    p.moment,
    p.off,
    p.rush,
  ]);
}

export type EtatPlan = 'a-ecrire' | 'perime' | 'pret';

export function etatPlan(p: PlanDecoupage): [EtatPlan, string] {
  if (!p.prompt.trim()) return ['a-ecrire', '✍️ à écrire'];
  if (p.ecrit && p.ecrit !== signature(p)) return ['perime', '⚠ changé : ✨ pour réécrire'];
  return ['pret', '✅ prompt prêt'];
}

export function compterMots(t: string): number {
  return t.split(/\s+/).filter((m) => m !== '').length;
}

/** Une réplique trop longue pour son plan : le cœur prévient aussi. */
export function motsReplique(p: PlanDecoupage): { mots: number; permis: number; trop: boolean } {
  const mots = compterMots(p.replique);
  const permis = Math.floor(MOTS_PAR_SECONDE * p.duree);
  return { mots, permis, trop: mots > permis };
}

/** Le format ne compte que si le premier plan part du texte : avec un rush, la taille est celle du rush. */
export function formatCompte(plans: PlanDecoupage[]): boolean {
  return (plans[0]?.enchainement ?? 'texte') === 'texte';
}

/** Les plans tels que le LLM les reçoit. */
export function plansDemande(plans: PlanDecoupage[]): PlanDemandeDecoupage[] {
  return plans.map((p) => ({
    enchainement: p.enchainement,
    duree: p.duree,
    note: p.note,
    rush: p.rush,
    replique: p.dialogue ? p.replique : '',
    moment: p.moment,
    off: p.dialogue && p.off,
    prompt: p.prompt,
  }));
}

/** Un plan écrit par le LLM : son prompt, son résumé ; « ce qui se passe » vide est rempli. */
export function poserPlan(p: PlanDecoupage, ecrit: PlanEcrit): PlanDecoupage {
  const pose: PlanDecoupage = { ...p, prompt: ecrit.prompt };
  if (ecrit.idee) pose.idee = ecrit.idee;
  if (!pose.note.trim() && ecrit.idee) pose.note = ecrit.idee;
  pose.ecrit = signature(pose);
  return pose;
}

/** Les alertes d'un plan réécrit remplacent les siennes ; celles des autres restent. */
export function majAlertes(avant: string[], nouvelles: string[], k: number | null): string[] {
  if (k === null) return nouvelles;
  return [...avant.filter((a) => !a.startsWith(`Plan ${k + 1} :`)), ...nouvelles];
}

/** Les plans remplis qu'un passage à `n` plans supprimerait. */
export function remplisAuDela(plans: PlanDecoupage[], n: number): number {
  return plans.slice(n).filter((p) => p.note.trim() || p.replique.trim() || p.prompt.trim()).length;
}

/** Passe à `n` plans : les derniers partent, des plans neufs complètent. */
export function redimensionner(plans: PlanDecoupage[], n: number, reference: string | null): PlanDecoupage[] {
  const garde = plans.slice(0, n);
  while (garde.length < n) garde.push(nouveauPlan(garde.length, reference));
  return garde;
}

/** Les alertes qui ne parlent plus d'un plan supprimé. */
export function alertesJusqua(alertes: string[], n: number): string[] {
  return alertes.filter((a) => {
    const m = /^Plan (\d+) :/.exec(a);
    return !m || Number(m[1]) <= n;
  });
}

/** Une liste où l'élément `k` a avancé (delta -1) ou reculé (delta 1) d'un rang ; rien ne bouge au bord. */
export function deplacer<T>(liste: T[], k: number, delta: -1 | 1): T[] {
  const vers = k + delta;
  const a = liste[k];
  const b = liste[vers];
  if (a === undefined || b === undefined) return liste;
  const copie = [...liste];
  copie[k] = b;
  copie[vers] = a;
  return copie;
}

/** Une liste sans l'élément `k`. */
export function retirer<T>(liste: T[], k: number): T[] {
  return liste.filter((_, i) => i !== k);
}

/** Les alertes d'un plan (« Plan 2 : … ») suivent le plan quand il change de rang ; `renumeroter` rend son nouveau
 * rang (0 = le premier), ou null s'il disparaît. */
export function alertesRenumerotees(alertes: string[], renumeroter: (rang: number) => number | null): string[] {
  return alertes.flatMap((a) => {
    const m = /^Plan (\d+) :(.*)$/s.exec(a);
    if (!m) return [a];
    const nouveau = renumeroter(Number(m[1]) - 1);
    return nouveau === null ? [] : [`Plan ${nouveau + 1} :${m[2]}`];
  });
}

/** Un plan qui change de rang change de type possible : seul le premier part du texte ou de la référence, les autres
 * enchaînent. Le rush d'arrivée ne vaut que pour une transition. */
export function corrigerEnchainements(plans: PlanDecoupage[], reference: string | null): PlanDecoupage[] {
  return plans.map((p, k) => {
    const depart = p.enchainement === 'texte' || p.enchainement === 'rush';
    const enchainement: Enchainement =
      k === 0 ? (depart ? p.enchainement : reference ? 'rush' : 'texte') : depart ? 'couper' : p.enchainement;
    return enchainement === p.enchainement && (enchainement === 'transition' || p.rush === null)
      ? p
      : { ...p, enchainement, rush: enchainement === 'transition' ? p.rush : null };
  });
}

/** Ce que le Montage reçoit : un plan déjà dans la séquence garde son identifiant (et son clip) et sa seed. */
export function plansPourMontage(
  plans: PlanDecoupage[],
  reference: string | null,
  avant: PlanSequence[],
): PlanSequence[] {
  return plans.map((p, k) => {
    const ancien = avant.find((a) => a.id !== undefined && a.id === p.idMontage);
    const plan: PlanSequence = {
      idee: p.idee || p.note || `Plan ${k + 1}`,
      prompt: p.prompt,
      duree: p.duree,
      enchainement: p.enchainement,
      // Départ de la référence : l'image du personnage est le rush du plan 1 (le cœur l'exige).
      rush: p.enchainement === 'rush' ? reference : p.rush,
      ambiance: true,
      clip: null,
      a_jour: false,
    };
    if (ancien?.id) {
      plan.id = ancien.id;
      plan.seed = ancien.seed ?? null;
    }
    return plan;
  });
}

/** Une séquence du Montage revient à l'atelier : ses plans, leurs prompts (prêts tels quels), la référence et la
 * contrainte ; les répliques sont dans les prompts. Le prochain envoi la met à jour. */
export function depuisSequence(s: Sequence, avant: EtatDecoupage): EtatDecoupage {
  const premier = s.plans[0];
  const reference = s.casting.personnage ?? (premier?.enchainement === 'rush' ? premier.rush : null);
  const plans = s.plans.map((p, k) => {
    const plan = normaliserPlan({
      enchainement: k === 0 && p.enchainement === 'rush' && !reference ? 'texte' : p.enchainement,
      duree: p.duree,
      note: p.idee,
      idee: p.idee,
      prompt: p.prompt ?? '',
      idMontage: p.id ?? null,
      rush: p.enchainement === 'transition' ? p.rush : null,
    });
    plan.ecrit = signature(plan);
    return plan;
  });
  // Le Montage accepte un seul plan, l'atelier en veut deux : un plan vide complète.
  while (plans.length < PLANS_MIN) plans.push(nouveauPlan(plans.length, reference));
  return {
    ...avant,
    sequenceId: s.id,
    reference,
    contrainte: s.casting.contrainte,
    contrainteEn: s.contrainte_en ?? null,
    format: s.format || avant.format,
    plans,
    alertes: [],
    titre: s.titre,
  };
}

/** Le premier plan qui n'a pas encore de prompt, ou -1. */
export function sansPrompt(plans: PlanDecoupage[]): number {
  return plans.findIndex((p) => !p.prompt.trim());
}

/** La contrainte traduite vaut-elle encore ? */
export function traductionValide(e: EtatDecoupage): boolean {
  return e.contrainteEn !== null && e.contrainteEn.fr === e.contrainte.trim().replace(/\s+/g, ' ');
}
