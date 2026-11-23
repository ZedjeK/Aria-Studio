// Le Scénario : les règles des listes appliquées en direct pour guider le choix (le cœur fait foi à l'assemblage).
// Genre, âge (mineur), « Avec <sous-liste> », conditions, incompatibilités et remplacements sont des règles ; le
// filtre par origine (teint, yeux, nez) n'est qu'une aide, que « Toutes les options » lève.

import type { ChoixListes, OngletScenario, SousListeScenario, StructureScenario, ValeurListe } from './api';

export const ONGLETS: { id: OngletScenario; titre: string }[] = [
  { id: 'sujet', titre: 'Sujet' },
  { id: 'decor', titre: 'Décor et ambiance' },
  { id: 'scene', titre: 'Mise en scène' },
];

/** L'âge de la majorité (comme le filtre AS-18). */
export const AGE_MAJORITE = 18;

/** Ce que les choix en cours disent du sujet. */
export interface ContexteListes {
  choix: ChoixListes;
  genre: 'femme' | 'homme' | null;
  mineur: boolean;
  /** Les familles de l'origine choisie (aide au choix). */
  familles: string[];
  /** « Toutes les options » : le filtre par origine est levé. */
  toutes: boolean;
}

function valeurDe(structure: StructureScenario, bloc: string, sousListe: string, nom: string): ValeurListe | undefined {
  return structure.blocs
    .find((b) => b.id === bloc)
    ?.groupes.flatMap((g) => g.sousListes)
    .find((s) => s.nom === sousListe)
    ?.valeurs.find((v) => v.nom === nom);
}

/** Le contexte des règles : genre, mineur (tranche ou âge précis) et familles de l'origine. */
export function contexteListes(
  structure: StructureScenario,
  choix: ChoixListes,
  age: number | null,
  toutes: boolean,
): ContexteListes {
  const perso = choix['personnage'] ?? {};
  const genreChoisi = perso['Genre']?.toLowerCase();
  const genre = genreChoisi === 'femme' || genreChoisi === 'homme' ? genreChoisi : null;
  const tranche = perso['Âge'] ? valeurDe(structure, 'personnage', 'Âge', perso['Âge']) : undefined;
  const origine = perso['Origine'] ? valeurDe(structure, 'personnage', 'Origine', perso['Origine']) : undefined;
  return {
    choix,
    genre,
    mineur: (tranche?.mineur ?? false) || (age !== null && age < AGE_MAJORITE),
    familles: origine?.familles ?? [],
    toutes,
  };
}

/** Une sous-liste se montre-t-elle ? Une sous-liste toute adulte disparaît pour un mineur ; une sous-liste à
 *  condition attend la valeur qui la permet (un motif de tatouage attend un tatouage). */
export function sousListeVisible(bloc: string, sl: SousListeScenario, ctx: ContexteListes): boolean {
  if (sl.adulte && ctx.mineur) return false;
  if (sl.condition) {
    const requise = ctx.choix[bloc]?.[sl.condition.sousListe] ?? '';
    return requise.toLowerCase().includes(sl.condition.contient);
  }
  return true;
}

/** Les valeurs proposées d'une sous-liste, d'après le genre, l'âge, « Avec » et l'origine. */
export function valeursProposees(bloc: string, sl: SousListeScenario, ctx: ContexteListes): ValeurListe[] {
  return sl.valeurs.filter((v) => {
    if (v.genre && ctx.genre && v.genre !== ctx.genre) return false;
    if (v.adulte && ctx.mineur) return false;
    for (const [reference, noms] of Object.entries(v.avec)) {
      const choisie = ctx.choix[bloc]?.[reference];
      if (choisie && !noms.includes(choisie)) return false;
    }
    if (!ctx.toutes && ctx.familles.length > 0 && v.origines.length > 0) {
      return v.origines.some((f) => ctx.familles.includes(f));
    }
    return true;
  });
}

/** Une valeur est-elle incompatible avec un choix déjà fait ? */
export function incompatible(
  structure: StructureScenario,
  choix: ChoixListes,
  bloc: string,
  sousListe: string,
  valeur: string,
): boolean {
  return structure.incompatibilites.some(([g, d]) => {
    const [gb, gs, gv] = g;
    const [db, ds, dv] = d;
    if (gb === bloc && gs === sousListe && gv === valeur) return choix[db]?.[ds] === dv;
    if (db === bloc && ds === sousListe && dv === valeur) return choix[gb]?.[gs] === gv;
    return false;
  });
}

/** Pose un choix (vide : le retire) ; une valeur qui en remplace d'autres (une robe, le Bas) les retire. */
export function poserChoix(
  structure: StructureScenario,
  choix: ChoixListes,
  bloc: string,
  sousListe: string,
  valeur: string,
): ChoixListes {
  const suivant: ChoixListes = { ...choix, [bloc]: { ...(choix[bloc] ?? {}) } };
  const dansBloc = suivant[bloc] ?? {};
  if (valeur === '') {
    delete dansBloc[sousListe];
  } else {
    dansBloc[sousListe] = valeur;
    for (const cible of valeurDe(structure, bloc, sousListe, valeur)?.remplace ?? []) delete dansBloc[cible];
  }
  suivant[bloc] = dansBloc;
  return suivant;
}

/** Le nombre de choix faits dans un onglet. */
export function choixDansOnglet(structure: StructureScenario, choix: ChoixListes, onglet: OngletScenario): number {
  let n = 0;
  for (const b of structure.blocs) {
    for (const g of b.groupes) {
      if (g.onglet !== onglet) continue;
      for (const s of g.sousListes) if (choix[b.id]?.[s.nom]) n += 1;
    }
  }
  return n;
}

/** Les choix gardés en brouillon, s'ils ont la bonne forme : {sujet: {bloc: {sous-liste: valeur}}}. */
export function choixDuBrouillon(v: unknown): Record<string, ChoixListes> {
  const sortie: Record<string, ChoixListes> = {};
  if (typeof v !== 'object' || v === null) return sortie;
  for (const [sujet, blocs] of Object.entries(v)) {
    if (typeof blocs !== 'object' || blocs === null) continue;
    const parBloc: ChoixListes = {};
    for (const [bloc, listes] of Object.entries(blocs)) {
      if (typeof listes !== 'object' || listes === null) continue;
      const valeurs: Record<string, string> = {};
      for (const [sl, valeur] of Object.entries(listes)) if (typeof valeur === 'string') valeurs[sl] = valeur;
      parBloc[bloc] = valeurs;
    }
    sortie[sujet] = parBloc;
  }
  return sortie;
}

/** L'aide du départ d'un atelier (« 1 · Départ » de la v2) : ce que font ses images. */
export function aideReference(
  atelier: 'image' | 'retouche' | 'clip' | 'sequence',
  depart: 'inspirer' | 'variation' | null,
  max: number,
): string {
  if (atelier === 'retouche') {
    return "L'image à retoucher : le LLM la regarde pour écrire une consigne qui lui correspond. Le masque se dessine ensuite dans la page Retouche.";
  }
  if (atelier === 'clip') return "Le rush à animer : il devient la première image du clip. Sans lui, le clip part du texte seul.";
  if (atelier === 'sequence') {
    return 'Le personnage de référence de la séquence (facultatif) : son visage et sa tenue tiennent sur les plans.';
  }
  if (depart === 'inspirer') {
    return (
      `Tes images sont des ingrédients : le modèle d'édition les regarde et compose une nouvelle scène (format libre). ` +
      `Jusqu'à ${max} images. Cite-les par numéro dans ton idée (« la femme de l'image 1 sur le dragon de l'image 2 »). ` +
      "Le LLM écrit le prompt en gardant ce que chaque image montre, sauf si tu demandes de le changer."
    );
  }
  if (depart === 'variation') {
    return (
      "Ton image est le point de départ : le modèle la redessine en partie. Composition, sujets et style sont gardés, " +
      "ton idée dit ce qui change. La Liberté du Plateau décide de la force : plus elle monte, plus l'image change."
    );
  }
  return "Pas d'image : le modèle crée tout à partir de ton idée et de tes compléments (sujet, décor, mise en scène).";
}
