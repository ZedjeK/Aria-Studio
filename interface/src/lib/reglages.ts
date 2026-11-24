// Réglages → LLM et workflows : ce qui a changé dans les champs, et comment l'afficher.

import type { Changements, ParamReglage } from './api';

/** Une saisie en cours : la valeur tapée, ou null pour revenir à celle d'origine. */
export type Saisies = Record<string, number | boolean | string | null>;

/** Les saisies qui diffèrent de ce qui est en vigueur : ce qu'il faut enregistrer. */
export function changements(params: ParamReglage[], saisies: Saisies): Changements {
  const sortie: Changements = {};
  for (const p of params) {
    if (!(p.cle in saisies)) continue;
    const saisie = saisies[p.cle];
    if (saisie === undefined) continue;
    if (saisie === null) {
      if (p.modifie) sortie[p.cle] = null;
    } else if (saisie !== p.valeur) {
      sortie[p.cle] = saisie;
    }
  }
  return sortie;
}

/** La valeur à montrer dans un champ : la saisie en cours, ou celle d'origine si on y revient, ou celle en vigueur. */
export function valeurAffichee(p: ParamReglage, saisies: Saisies): number | boolean | string {
  if (!(p.cle in saisies)) return p.valeur;
  const saisie = saisies[p.cle];
  return saisie === null || saisie === undefined ? p.defaut : saisie;
}

/** Le libellé d'un choix : celui que le cœur donne, sinon la valeur elle-même. */
export function libelleChoix(p: ParamReglage, valeur: string | number): string {
  return p.libelles[String(valeur)] ?? String(valeur);
}

/** Le nom d'une consigne d'usage. */
export function nomConsigne(cle: string): string {
  const noms: Record<string, string> = {
    reecriture: 'Réécriture d’une idée',
    retouche: 'Consigne de retouche',
    precisions: 'Traduction des précisions',
    references: 'Images de référence',
    video: 'Prompt d’un clip',
    decoupage: 'Découpage d’une séquence',
    directions: 'Directions proposées',
    verification: 'Vérification d’une image',
    choix: 'Choix des meilleures images',
    explication: 'Explication d’une erreur',
  };
  return noms[cle] ?? cle;
}

/** Un groupe de champs du LLM : son titre, et les réglages qui y vont. */
export interface GroupeReglages {
  titre: string;
  params: ParamReglage[];
}

/**
 * Les réglages d'un LLM rangés comme dans la maquette : l'écriture sans réflexion, l'écriture avec réflexion, puis la
 * capacité et le moteur. Un réglage inconnu du classement va avec la capacité ; un groupe vide n'est pas rendu.
 */
export function groupesLlm(params: ParamReglage[]): GroupeReglages[] {
  const sans: ParamReglage[] = [];
  const avec: ParamReglage[] = [];
  const capacite: ParamReglage[] = [];
  for (const p of params) {
    if (p.cle.endsWith('_reflexion')) avec.push(p);
    else if (/^(temperature_|top_p$|top_k$|min_p$|presence_penalty$)/.test(p.cle)) sans.push(p);
    else capacite.push(p);
  }
  return [
    { titre: 'Écriture sans réflexion', params: sans },
    { titre: 'Écriture avec réflexion', params: avec },
    { titre: 'Capacité et moteur', params: capacite },
  ].filter((g) => g.params.length > 0);
}

/** « 2 changements non enregistrés », « 1 changement non enregistré », « 0 changement non enregistré ». */
export function texteChangements(nombre: number): string {
  return `${nombre} changement${nombre > 1 ? 's' : ''} non enregistré${nombre > 1 ? 's' : ''}`;
}

/** Un texte sans accents ni majuscules, pour chercher sans se soucier de l'écriture. */
export function simplifie(texte: string): string {
  return texte.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase();
}

/** Un réglage dont la valeur montrée n'est pas celle d'origine (enregistrée ou en cours de saisie). */
export function estModifie(p: ParamReglage, saisies: Saisies): boolean {
  return valeurAffichee(p, saisies) !== p.defaut;
}

/** Le nombre de réglages modifiés dans une liste. */
export function nombreModifies(params: ParamReglage[], saisies: Saisies): number {
  return params.filter((p) => estModifie(p, saisies)).length;
}

/**
 * Les réglages à montrer pour « Chercher un réglage » et « Modifiés seulement ». Chaque mot tapé doit se lire dans le
 * libellé, la clé ou l'aide du réglage ; sans accent ni majuscule. Côté interface seulement : rien n'est enregistré.
 */
export function filtrerReglages(
  params: ParamReglage[],
  saisies: Saisies,
  recherche: string,
  modifiesSeulement: boolean,
): ParamReglage[] {
  const mots = simplifie(recherche).split(/\s+/).filter((m) => m !== '');
  return params.filter((p) => {
    if (modifiesSeulement && !estModifie(p, saisies)) return false;
    const texte = simplifie(`${p.libelle} ${p.cle} ${p.aide}`);
    return mots.every((m) => texte.includes(m));
  });
}
