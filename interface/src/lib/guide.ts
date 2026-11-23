// Scénario, « 🧭 Me guider » (ateliers Image et Clip), comme en v2 (web/guide.js) : un déroulé fixe, sans LLM. Les
// questions s'enchaînent (le sujet, puis le décor et l'ambiance, puis la mise en scène, une question par liste) et les
// réponses vont dans les mêmes choix que les compléments. Les questions se recalculent à chaque réponse : le genre,
// l'âge, l'origine ou une tenue qui en remplace une autre changent les listes proposées.

import type { ChoixListes, OngletScenario, SousListeScenario, StructureScenario, SujetScenario, ValeurListe } from './api';
import { incompatible, sousListeVisible, valeursProposees, type ContexteListes } from './scenario';

/** Les premières valeurs en puces ; le reste dans une liste déroulante. */
export const PUCES_MAX = 8;

export const SECTIONS: Record<OngletScenario, string> = {
  sujet: 'Sujet',
  decor: 'Décor et ambiance',
  scene: 'Mise en scène',
};

const RANG: Record<OngletScenario, number> = { sujet: 0, decor: 1, scene: 2 };

/** Ce que le guide dit en tête, selon l'atelier et le départ de l'image. */
export const AIDES_GUIDE = {
  texte: "Choisis ce que tu veux imposer : le sujet, le décor et l'ambiance, la mise en scène. Rien n'est obligatoire.",
  inspirer: "Le sujet vient de tes images. Choisis ici le décor, l'ambiance et la mise en scène (pose et action comprises).",
  variation:
    "Le sujet et la composition viennent de ton image : ce que tu choisis ici sert à la modifier (décor, ambiance, mise en scène).",
  clip: 'Choisis ce que tu veux imposer au clip : le sujet, le décor, la mise en scène, le dialogue et la musique.',
} as const;

export type GenreReponse = 'valeur' | 'libre' | 'passe' | 'hasard';

/** Une réponse donnée, dans l'ordre. */
export interface ReponseGuide {
  id: string;
  onglet: OngletScenario;
  titre: string;
  groupe: string;
  bloc: string | null;
  sousListe: string | null;
  genre: GenreReponse;
  valeur: string;
  texte: string;
}

/** Où en est le guide d'un atelier : les réponses, et le parcours des listes rangées par catégorie. */
export interface EtatGuide {
  fait: ReponseGuide[];
  /** Les catégories choisies pour une étape (« Intérieur », puis « Habitation »…). */
  chemin: Record<string, string[]>;
  /** L'étape montre toute sa liste, sans passer par les catégories. */
  toute: Record<string, boolean>;
}

export const guideVide = (): EtatGuide => ({ fait: [], chemin: {}, toute: {} });

export type EtapeGuide =
  | { type: 'sujet'; id: 'sujet'; onglet: 'sujet'; titre: string; groupe: '' }
  | { type: 'texte'; id: 'texte'; onglet: 'scene'; titre: string; groupe: '' }
  | {
      type: 'liste';
      id: string;
      onglet: OngletScenario;
      titre: string;
      groupe: string;
      bloc: string;
      sousListe: SousListeScenario;
      valeurs: ValeurListe[];
    };

/** Les réglages du guide pour l'atelier en cours. */
export interface OptionsGuide {
  /** Le sujet vient des images (S'inspirer, Redessiner) : pas de question du Sujet. */
  sansSujet: boolean;
  /** Le modèle cible est vidéo : le bloc Plan (dialogue, musique) est proposé. */
  video: boolean;
  /** Atelier Image : une question pour le texte dans l'image. */
  image: boolean;
  /** Les rubriques que le modèle cible ignore. */
  ignorees: string[];
}

export const cleEtape = (bloc: string, sousListe: string): string => `${bloc}/${sousListe}`;

/** Les sous-listes remplacées par une valeur choisie (une robe remplace le haut et le bas). */
export function remplacees(structure: StructureScenario, choix: ChoixListes): Set<string> {
  const sortie = new Set<string>();
  for (const b of structure.blocs) {
    for (const g of b.groupes) {
      for (const sl of g.sousListes) {
        const choisie = choix[b.id]?.[sl.nom];
        if (!choisie) continue;
        for (const cible of sl.valeurs.find((v) => v.nom === choisie)?.remplace ?? []) sortie.add(cleEtape(b.id, cible));
      }
    }
  }
  return sortie;
}

/** Les valeurs qu'une question propose : permises (genre, âge, Avec, origine), qui ont un prompt, sans conflit avec
 *  les réponses données, et qui ne remplaceraient pas une liste déjà choisie. */
export function valeursOffertes(
  structure: StructureScenario,
  bloc: string,
  sl: SousListeScenario,
  ctx: ContexteListes,
): ValeurListe[] {
  return valeursProposees(bloc, sl, ctx).filter(
    (v) =>
      (v.prompt !== '' || v.promptsGenre !== null) &&
      !incompatible(structure, ctx.choix, bloc, sl.nom, v.nom) &&
      !v.remplace.some((cible) => ctx.choix[bloc]?.[cible]),
  );
}

/** Les questions encore possibles, dans l'ordre des compléments. */
export function etapesGuide(
  structure: StructureScenario | null,
  guide: EtatGuide,
  ctx: ContexteListes | null,
  o: OptionsGuide,
): EtapeGuide[] {
  const etapes: EtapeGuide[] = [];
  if (!o.sansSujet) {
    etapes.push({ type: 'sujet', id: 'sujet', onglet: 'sujet', titre: "Qu'est-ce que tu veux créer ?", groupe: '' });
  }
  if (!structure || !ctx) return etapes;
  // Un sujet décrit en toutes lettres : pas de questions du Sujet.
  const sujetLibre = guide.fait.some((f) => f.id === 'sujet' && f.genre === 'libre');
  const sansListesSujet = o.sansSujet || sujetLibre;
  const parRemplacement = remplacees(structure, ctx.choix);
  const candidates: { bloc: string; groupe: string; onglet: OngletScenario; sl: SousListeScenario }[] = [];
  for (const b of structure.blocs) {
    if (b.video && !o.video) continue;
    for (const g of b.groupes) {
      if (sansListesSujet && g.onglet === 'sujet') continue;
      for (const sl of g.sousListes) {
        if (!o.ignorees.includes(sl.nom)) candidates.push({ bloc: b.id, groupe: g.nom, onglet: g.onglet, sl });
      }
    }
  }
  candidates.sort((x, y) => RANG[x.onglet] - RANG[y.onglet]); // le tri garde l'ordre des groupes
  for (const { bloc, groupe, onglet, sl } of candidates) {
    if (!sousListeVisible(bloc, sl, ctx) || parRemplacement.has(cleEtape(bloc, sl.nom))) continue;
    const valeurs = valeursOffertes(structure, bloc, sl, ctx);
    if (valeurs.length > 0) {
      etapes.push({ type: 'liste', id: cleEtape(bloc, sl.nom), onglet, titre: sl.nom, groupe, bloc, sousListe: sl, valeurs });
    }
  }
  if (o.image) {
    etapes.push({ type: 'texte', id: 'texte', onglet: 'scene', titre: "Un texte à écrire dans l'image ?", groupe: '' });
  }
  return etapes;
}

/** La première question sans réponse ; aucune : le récapitulatif. */
export function etapeCourante(etapes: EtapeGuide[], guide: EtatGuide): EtapeGuide | undefined {
  return etapes.find((e) => !guide.fait.some((f) => f.id === e.id));
}

/** « Question 2 sur 6 » : le rang de l'étape dans le déroulé (le total change si une réponse écarte des questions). */
export function numeroQuestion(etapes: EtapeGuide[], etape: EtapeGuide): { numero: number; total: number } {
  const rang = etapes.findIndex((e) => e.id === etape.id);
  const numero = rang < 0 ? etapes.length + 1 : rang + 1;
  return { numero, total: Math.max(etapes.length, numero) };
}

const niveaux = (v: ValeurListe): string[] => (v.categorie ? v.categorie.split(' / ') : []);

/** Le parcours d'une longue liste rangée par catégories : la catégorie, puis le niveau suivant, puis les valeurs. */
export function parcours(
  valeurs: ValeurListe[],
  id: string,
  guide: EtatGuide,
): { chemin: string[]; restantes: ValeurListe[]; prochaines: string[] } {
  const chemin = guide.chemin[id] ?? [];
  const restantes = valeurs.filter((v) => chemin.every((c, i) => niveaux(v)[i] === c));
  const cascade = valeurs.some((v) => v.categorie) && valeurs.length > PUCES_MAX && !guide.toute[id];
  const prochaines = cascade
    ? [...new Set(restantes.map((v) => niveaux(v)[chemin.length]).filter((c): c is string => Boolean(c)))]
    : [];
  return { chemin, restantes, prochaines };
}

/** Le nombre de valeurs d'une catégorie, au niveau où en est le parcours. */
export function nombreDans(restantes: ValeurListe[], niveau: number, categorie: string): number {
  return restantes.filter((v) => niveaux(v)[niveau] === categorie).length;
}

/** Le nom d'un sujet. */
export function nomSujet(sujets: SujetScenario[], id: string): string {
  return sujets.find((s) => s.id === id)?.nom ?? id;
}

/** Ce que montre la bulle d'une réponse. */
export function texteReponse(f: ReponseGuide, sujets: SujetScenario[]): string {
  if (f.genre === 'passe') return 'Passer';
  if (f.genre === 'libre') return f.texte;
  if (f.genre === 'hasard') return `🎲 ${f.valeur}`;
  return f.id === 'sujet' ? nomSujet(sujets, f.valeur) : f.valeur;
}

/** Les réponses, une ligne « liste : valeur » chacune, pour la conversation. */
export function lignesGuide(guide: EtatGuide, choix: ChoixListes, sujets: SujetScenario[], sansSujet: boolean): string[] {
  return guide.fait.flatMap((f) => {
    if (f.genre === 'passe' || (sansSujet && f.onglet === 'sujet')) return [];
    if (f.id === 'sujet') return [`Sujet : ${f.genre === 'libre' ? f.texte : nomSujet(sujets, f.valeur)}`];
    if (f.id === 'texte') return [`Texte dans l'image : ${f.texte}`];
    if (f.genre === 'libre') return [`${f.titre} : ${f.texte}`];
    const valeur = f.bloc && f.sousListe ? choix[f.bloc]?.[f.sousListe] : undefined;
    return valeur ? [`${f.titre} : ${valeur}`] : [];
  });
}

/** Les valeurs que le dé peut tirer : offertes, dans la catégorie choisie, autres que l'actuelle. */
export function candidatesHasard(e: EtapeGuide, guide: EtatGuide, choix: ChoixListes): string[] {
  if (e.type !== 'liste') return [];
  const actuelle = choix[e.bloc]?.[e.sousListe.nom];
  const dansLeParcours = new Set(parcours(e.valeurs, e.id, guide).restantes.map((v) => v.nom));
  return e.valeurs.map((v) => v.nom).filter((nom) => nom !== actuelle && dansLeParcours.has(nom));
}

/** Ajoute les lignes au texte de la conversation, sans doublon. */
export function ajouterLignes(saisie: string, lignes: string[]): string {
  const deja = saisie
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '');
  return [...deja, ...lignes.filter((l) => !deja.includes(l))].join('\n');
}
