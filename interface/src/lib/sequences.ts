// Les séquences du Montage, comme en v2 (web/sequence.js) : plans à tourner et estimation calculés comme le cœur
// (aria_coeur::sequences::a_tourner, estimation), et les règles d'enchaînement des plans.

import type { Enchainement, OptionsSequence, PlanSequence, QualiteVideo, Sequence } from './api';

/** Le premier plan démarre ; les suivants s'enchaînent. Les mêmes mots qu'au Scénario. */
export const DEPARTS: [Enchainement, string][] = [
  ['rush', "Départ d'une image"],
  ['texte', 'Départ du texte'],
];
export const SUITES: [Enchainement, string][] = [
  ['prolonger', '↪ Prolonger'],
  ['couper', '✂ Couper'],
  ['transition', 'Transition'],
];
export const AIDE_ENCHAINEMENT: Record<Enchainement, string> = {
  rush: 'Le plan anime ce rush.',
  texte: 'Le plan naît du texte seul.',
  prolonger: 'Même plan, sans coupure : la fin du plan précédent et son son continuent ici.',
  couper: 'Nouveau plan : le personnage et la voix de référence gardent la ressemblance.',
  transition: 'Le plan part de la dernière image du précédent et finit exactement sur ce rush.',
};

/** Une réplique : la balise `<d>` de H3, ou des guillemets (comme aria_coeur::h3::parle). */
const REPLIQUE = /<d>|[«"“]/i;

export const parlePlan = (p: PlanSequence): boolean => REPLIQUE.test(`${p.idee} ${p.prompt ?? ''}`);

/** Le plan reçoit un rush : celui de départ, ou celui d'arrivée d'une transition. */
export const avecRush = (e: Enchainement): boolean => e === 'rush' || e === 'transition';

export function nouveauPlan(enchainement: Enchainement): PlanSequence {
  return { idee: '', prompt: '', duree: 5, enchainement, rush: null, ambiance: true, clip: null, a_jour: false };
}

/** Une séquence neuve. Rapide par défaut, comme en v2 : les plans qui parlent montent seuls aux steps de la parole. */
export function sequenceVide(modele: string | null): Sequence {
  return {
    id: null,
    titre: '',
    qualite: 'rapide',
    format: '16:9 (Widescreen)',
    llm: null,
    modele,
    loras: [],
    film: null,
    casting: { personnage: null, voix: 'auto', ambiance: '', contrainte: '', reference_partout: true },
    plans: [nouveauPlan('rush')],
  };
}

/** Les plans à tourner : ceux demandés ou pas à jour, plus ceux qui partent du plan d'avant. */
export function aTourner(plans: PlanSequence[], demandes: number[] = []): number[] {
  const choisis = new Set(demandes);
  plans.forEach((p, k) => {
    if (!(p.clip && p.a_jour)) choisis.add(k);
  });
  for (let k = 1; k < plans.length; k++) {
    const e = plans[k]?.enchainement;
    if (choisis.has(k - 1) && (e === 'prolonger' || e === 'transition')) choisis.add(k);
  }
  return [...choisis].sort((a, b) => a - b);
}

/** Les steps du checkpoint de la séquence ; 0 s'il n'est pas connu. */
export function stepsSequence(s: Sequence, options: OptionsSequence): number {
  const modele = s.modele ?? options.modeleDefaut;
  return modele ? (options.steps[modele] ?? 0) : 0;
}

/** Les secondes de calcul, assemblage compris : la mesure de la résolution, au prorata des steps du checkpoint. */
export function estimationSequence(
  s: Sequence,
  indices: number[],
  qualite: QualiteVideo,
  options: OptionsSequence,
): number {
  if (indices.length === 0) return 0;
  const base = stepsSequence(s, options) || qualite.steps;
  const cout = (p: PlanSequence) =>
    qualite.estimationS *
    (base / qualite.steps) *
    options.surcout *
    (p.duree / 5) ** options.exposantDuree *
    (parlePlan(p) && base < options.stepsParole ? options.surcoutParole : 1);
  return indices.reduce((total, k) => {
    const p = s.plans[k];
    return p ? total + cout(p) : total;
  }, 5);
}

/** Après un déplacement ou une suppression : le premier plan démarre, les suivants s'enchaînent. */
export function normaliserEnchainements(plans: PlanSequence[]): PlanSequence[] {
  return plans.map((p, k) => {
    const depart = p.enchainement === 'rush' || p.enchainement === 'texte';
    if (k === 0 && !depart) return { ...p, enchainement: p.rush ? 'rush' : 'texte', a_jour: false };
    if (k > 0 && depart) return { ...p, enchainement: 'couper', rush: null, a_jour: false };
    return p;
  });
}

/** Échange deux plans voisins ; les deux sont à retourner. */
export function deplacer(plans: PlanSequence[], k: number, vers: number): PlanSequence[] {
  const a = plans[k];
  const b = plans[vers];
  if (!a || !b) return plans;
  const copie = [...plans];
  copie[k] = { ...b, a_jour: false };
  copie[vers] = { ...a, a_jour: false };
  return normaliserEnchainements(copie);
}

/** Retire un plan (son clip reste dans les Bobines). */
export function retirer(plans: PlanSequence[], k: number): PlanSequence[] {
  return normaliserEnchainements(plans.filter((_, i) => i !== k));
}
