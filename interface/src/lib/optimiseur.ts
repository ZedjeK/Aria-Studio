// Réglages : « Recommander » (modèle d'image ou de vidéo) et « ⚡ Adapter à ma machine » (LLM), comme en v2. Le cœur
// propose ; ici, on choisit ce qui change vraiment et on le regroupe par modèle pour l'enregistrer.

import type { Changements, Materiel, ParamReglage, Proposition } from './api';
import { valeurAffichee, type Saisies } from './reglages';

/** Une proposition et la valeur qu'elle remplacerait dans le formulaire. */
export interface Ecart {
  proposition: Proposition;
  avant: number | boolean | string | null;
}

/**
 * Les recommandations d'un workflow qui changent quelque chose : on compare à ce que montrent les champs (réglages
 * enregistrés ou saisie en cours), pas seulement à ce qui est enregistré. Les autres sont déjà aux bonnes valeurs.
 */
export function ecarts(
  propositions: Proposition[],
  params: ParamReglage[],
  saisies: Saisies,
): { aChanger: Ecart[]; dejaBon: Proposition[] } {
  const aChanger: Ecart[] = [];
  const dejaBon: Proposition[] = [];
  for (const p of propositions) {
    const param = params.find((x) => x.cle === p.cle);
    if (!param) continue;
    const avant = valeurAffichee(param, saisies);
    if (avant === p.propose) dejaBon.push(p);
    else aChanger.push({ proposition: p, avant });
  }
  return { aChanger, dejaBon };
}

/** Les propositions choisies, une demande par modèle (LLM ou workflow) avec toutes ses valeurs. */
export function parCible(choisies: Proposition[]): { type: string; cible: string; changements: Changements }[] {
  const groupes = new Map<string, { type: string; cible: string; changements: Changements }>();
  for (const p of choisies) {
    const cle = `${p.type}|${p.cible}`;
    const groupe = groupes.get(cle) ?? { type: p.type, cible: p.cible, changements: {} };
    if (p.propose !== null && typeof p.propose !== 'object') groupe.changements[p.cle] = p.propose;
    groupes.set(cle, groupe);
  }
  return [...groupes.values()];
}

/** La machine en une ligne, comme en v2. */
export function texteMateriel(m: Materiel): string {
  const nombre = (v: number) => String(v).replace('.', ',');
  const processeur =
    m.coeurs !== null && m.threads !== null
      ? ` · ${m.coeurs} cœurs / ${m.threads} threads`
      : m.threads !== null
        ? ` · ${m.threads} threads`
        : '';
  return (
    `${m.gpu} · ${nombre(m.vramTotaleGo)} Go de VRAM (dont ${nombre(m.vramAutresGo)} Go pris par tes autres applis) · ` +
    `${nombre(m.ramGo)} Go de RAM${processeur}`
  );
}

/** Une valeur de réglage lisible : oui ou non, nombre à virgule, texte. */
export function valeurLisible(v: unknown): string {
  if (typeof v === 'boolean') return v ? 'oui' : 'non';
  if (typeof v === 'number') return String(v).replace('.', ',');
  if (v === null || v === undefined) return '—';
  return String(v);
}
