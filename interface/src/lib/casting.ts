// Le Casting, comme en v2 (web/casting.js) : les textes de la page, calculés d'après ce que rend le cœur.

import type { ModeleCasting, PreparationCasting, ResumeCasting, TacheCasting, VueCasting } from './api';

export const NOMS_TACHES: Record<TacheCasting, string> = {
  texte_vers_image: 'Texte vers image',
  retouche: 'Retouche',
  reecriture: 'Écriture (LLM)',
  image_vers_video: 'Image vers vidéo',
  texte_vers_video: 'Texte vers vidéo',
};

/** Les tâches dont chaque essai part d'une image de départ. */
export const AVEC_SOURCE: TacheCasting[] = ['retouche', 'image_vers_video'];

/** Les contrôles d'un prompt écrit par un LLM, et leur nom. */
export const CONTROLES: [string, string][] = [
  ['anglais', 'anglais'],
  ['longueur', 'longueur'],
  ['sans_preambule', 'sans préambule'],
  ['texte_conserve', 'texte gardé'],
];

/** « 1 modèle », « 3 modèles ». */
export function nombreDe(n: number, mot: string): string {
  return `${n} ${mot}${n > 1 ? 's' : ''}`;
}

/** Ce que produit un casting de cette tâche : des images, des clips, ou des étapes (réécriture puis rendu). */
export function unite(tache: TacheCasting): string {
  if (tache === 'reecriture') return 'étape';
  return tache.endsWith('video') ? 'clip' : 'image';
}

/** Où en sont les notes d'un casting : « à l'aveugle · 3 / 24 notées ». */
export function etatCasting(c: Pick<ResumeCasting, 'tache' | 'revele' | 'notees' | 'total'>): string {
  const notes = c.tache.endsWith('video') ? 'notés' : 'notées';
  return `${c.revele ? 'révélé' : "à l'aveugle"} · ${c.notees} / ${c.total} ${notes}`;
}

/** « 20 nov. · 10:15 », d'après la date d'un casting. */
export function dateCasting(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const jour = d.toLocaleDateString('fr-FR', { day: 'numeric', month: 'short' });
  const heure = d.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' });
  return `${jour} · ${heure}`;
}

/** Sous le nom d'une tâche : combien de candidats, et ce que le casting produirait ou ce qui l'en empêche. */
export function resumeTache(p: PreparationCasting): string {
  const n = p.candidats.length;
  const mot = p.tache === 'reecriture' ? 'candidat' : 'modèle';
  const incomplets = p.candidats.filter((c) => c.manquants > 0).length;
  if (!p.blocage) return `${nombreDe(n, mot)} · ${nombreDe(p.total, unite(p.tache))} à produire`;
  if (n === 0) return `aucun ${mot}`;
  if (n === 1) return `1 seul ${mot} · rien à comparer`;
  if (incomplets > 0) return `${nombreDe(n, mot)} · ${incomplets} incomplet${incomplets > 1 ? 's' : ''}`;
  return 'ne peut pas partir';
}

/** La note moyenne de chaque modèle (null sans note). */
export function moyennes(modeles: ModeleCasting[]): (number | null)[] {
  return modeles.map((m) => {
    const notes = m.generations.map((g) => g.note).filter((n): n is number => n !== null);
    return notes.length > 0 ? notes.reduce((a, n) => a + n, 0) / notes.length : null;
  });
}

/** Les générations d'un prompt, tous modèles confondus, et combien sont notées. */
export function avancementPrompt(c: VueCasting, prompt: string): { notees: number; total: number } {
  const gens = c.modeles.flatMap((m) => m.generations.filter((g) => g.prompt === prompt));
  return { notees: gens.filter((g) => g.note !== null).length, total: gens.length };
}

/** Un clip se lit, une image se regarde. */
export const estVideo = (fichier: string): boolean => fichier.toLowerCase().endsWith('.mp4');
