// La page Modèles : tâches des workflows, avancement des téléchargements.

import type { IdentificationSources, StepsDits, Telechargement } from './api';
import { octetsLisibles } from './jauge';

const TACHES: Record<string, string> = {
  texte_vers_image: 'Texte → image',
  retouche: 'Image → image',
  image_vers_video: 'Image → vidéo',
  texte_vers_video: 'Texte → vidéo',
};

/** Le nom d'une tâche de workflow. */
export function libelleTache(tache: string): string {
  return TACHES[tache] ?? tache;
}

/** L'ordre des tâches sur la page, comme les sous-onglets de la v2. */
export function ordreTache(tache: string): number {
  const ordre = Object.keys(TACHES).indexOf(tache);
  return ordre === -1 ? Object.keys(TACHES).length : ordre;
}

/**
 * Un fichier d'un workflow : présent, manquant, ou « non exigé ». Le cœur n'exige pas le checkpoint d'origine quand la
 * famille en a déjà un : le fichier est absent du disque, mais il ne manque pas, et rien n'est à télécharger pour lui.
 * C'est le cœur qui le dit (`manque`) : l'interface ne le devine pas d'après l'état du workflow.
 */
export type EtatFichier = 'present' | 'manque' | 'non_exige';

export function etatFichier(f: { present: boolean; manque: boolean }): EtatFichier {
  if (f.present) return 'present';
  return f.manque ? 'manque' : 'non_exige';
}

export function libelleEtatFichier(etat: EtatFichier): string {
  switch (etat) {
    case 'present':
      return 'présent';
    case 'manque':
      return 'manque';
    case 'non_exige':
      return 'non exigé : un autre checkpoint de la famille est déjà là';
  }
}

/** Vrai tant qu'un téléchargement peut encore avancer (il s'annule alors). */
export function telechargementActif(t: Telechargement): boolean {
  return t.etat === 'attente' || t.etat === 'en_cours' || t.etat === 'verification';
}

/** La part reçue, de 0 à 1 ; null avant le premier octet d'un fichier de taille inconnue. */
export function fractionTelechargement(t: Telechargement): number | null {
  if (t.taille <= 0) return null;
  return Math.min(1, t.recu / t.taille);
}

/** L'avancement d'un téléchargement, en une ligne. */
export function texteTelechargement(t: Telechargement): string {
  switch (t.etat) {
    case 'attente':
      return `en attente · ${octetsLisibles(t.taille)}`;
    case 'en_cours': {
      const debit = t.debit > 0 ? ` · ${octetsLisibles(t.debit)}/s` : '';
      return `${octetsLisibles(t.recu)} sur ${octetsLisibles(t.taille)}${debit}`;
    }
    case 'verification':
      return 'vérification de l’empreinte SHA-256';
    case 'termine':
      return `terminé · ${octetsLisibles(t.taille)}`;
    case 'annule':
      return 'annulé';
    case 'erreur':
      return t.erreur ?? 'erreur';
  }
}

const ORIGINES: Record<string, string> = {
  aria: 'téléchargé par Aria',
  retrouve: 'source retrouvée',
  main: 'déposé à la main',
};

/** D'où vient un fichier de la collection. */
export function libelleOrigine(origine: string): string {
  return ORIGINES[origine] ?? origine;
}

/** Les steps d'un checkpoint, au plus (comme le cœur). */
export const STEPS_MAX = 150;
/** Les mots déclencheurs d'une LoRA, au plus. */
export const LONGUEUR_DECLENCHEURS = 300;

/** Les steps à proposer pour un checkpoint à valider : la première valeur entière que dit sa fiche. */
export function stepsProposes(dits: StepsDits[] | null): number | null {
  for (const d of dits ?? []) {
    if (typeof d.valeur === 'number' && Number.isInteger(d.valeur) && d.valeur >= 1 && d.valeur <= STEPS_MAX) {
      return d.valeur;
    }
  }
  return null;
}

/** Ce qui empêche d'enregistrer des steps ; null s'ils conviennent. */
export function erreurSteps(steps: number): string | null {
  return Number.isInteger(steps) && steps >= 1 && steps <= STEPS_MAX ? null : `Steps : un entier entre 1 et ${STEPS_MAX}`;
}

/** Ce qui empêche d'enregistrer le réglage d'une LoRA ; null s'il convient. */
export function erreurLora(declencheurs: string, force: number): string | null {
  if (declencheurs.trim().split(/\s+/).join(' ').length > LONGUEUR_DECLENCHEURS) {
    return `Mots déclencheurs : ${LONGUEUR_DECLENCHEURS} caractères au plus`;
  }
  if (!Number.isFinite(force) || force < 0 || force > 2) return 'Force de LoRA entre 0 et 2';
  return null;
}

/** Une date de la corbeille (`2026-11-22T14:03:00`), lisible. */
export function dateLisible(date: string | null): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(date ?? '');
  return m ? `${m[3]}/${m[2]}/${m[1]} ${m[4]}:${m[5]}` : '';
}

/** L'avancement de « Retrouver les sources », en une ligne. */
export function texteIdentification(i: IdentificationSources): string {
  if (i.etat === 'en_cours') {
    const phase = i.phase === 'civitai' ? 'recherche sur Civitai' : 'calcul de l’empreinte';
    return `${i.fait + 1} sur ${i.total}${i.fichier ? ` · ${i.fichier}` : ''} · ${phase}`;
  }
  if (i.etat === 'termine') {
    const erreurs = i.erreurs.length > 0 ? `, ${i.erreurs.length} erreur${i.erreurs.length > 1 ? 's' : ''}` : '';
    return `${i.trouves.length} retrouvé${i.trouves.length > 1 ? 's' : ''}, ${i.introuvables.length} introuvable${i.introuvables.length > 1 ? 's' : ''}${erreurs}`;
  }
  return '';
}

const ROLES: Record<string, string> = {
  checkpoints: 'checkpoint',
  modeles: 'checkpoint',
  diffusion_models: 'modèle de diffusion',
  vae: 'VAE',
  text_encoders: 'encodeur de texte',
  clip: 'encodeur de texte',
  loras: 'LoRA',
};

/** Ce que fait un fichier d'un workflow, d'après son dossier (`image/_base/vae` : VAE). */
export function roleFichier(dossier: string): string {
  const dernier = dossier.split('/').filter((s) => s !== '').pop() ?? dossier;
  return ROLES[dernier] ?? dernier;
}

/** Le texte du bouton de la file : le nombre de téléchargements encore en cours, s'il y en a. */
export function texteBoutonTelechargements(enCours: number): string {
  return enCours > 0 ? `⬇ Téléchargements · ${enCours}` : '⬇ Téléchargements';
}

/** Combien de fichiers un workflow doit encore recevoir (un fichier « non exigé » n'en fait pas partie). */
export function fichiersAManquer(fichiers: { manque: boolean }[]): number {
  return fichiers.filter((f) => f.manque).length;
}

/** Ce fichier est-il dans la file, en attente ou en route ? */
export function enFile(taches: Telechargement[], dossier: string, nom: string): boolean {
  return taches.some((t) => telechargementActif(t) && t.dossier === dossier && t.nom === nom);
}

/**
 * Ce que montre la carte d'un workflow. Tant que quelque chose manque, il y a un bouton : « Télécharger ce qui
 * manque », ou, quand tout ce qui manque est déjà dans la file, un bouton qui l'ouvre.
 */
export type ActionWorkflow = { genre: 'pret' } | { genre: 'telecharger' } | { genre: 'en_file'; fichiers: number };

export function actionWorkflow(
  w: { pret: boolean; fichiers: { nom: string; dossier: string; manque: boolean }[] },
  taches: Telechargement[],
): ActionWorkflow {
  if (w.pret) return { genre: 'pret' };
  const manquants = w.fichiers.filter((f) => f.manque);
  const dansLaFile = manquants.filter((f) => enFile(taches, f.dossier, f.nom));
  if (manquants.length > 0 && dansLaFile.length === manquants.length) {
    return { genre: 'en_file', fichiers: dansLaFile.length };
  }
  return { genre: 'telecharger' };
}

/** Ce que montre la ligne d'un LLM du catalogue : installé, « Installer », ou son GGUF est déjà dans la file. */
export type ActionLlm = 'installe' | 'installer' | 'en_file';

export function actionLlm(l: { fichier: string; installe: boolean }, taches: Telechargement[]): ActionLlm {
  if (l.installe) return 'installe';
  return enFile(taches, 'llm', l.fichier) ? 'en_file' : 'installer';
}

/** Le message d'après un clic sur « Télécharger » : ce qui est parti, ou pourquoi rien n'est parti. */
export function messageMiseEnFile(nom: string, ajoutes: number): string {
  if (ajoutes > 0) {
    return `${ajoutes} fichier${ajoutes > 1 ? 's' : ''} en file pour ${nom} : suis-les dans « Téléchargements ».`;
  }
  return `Rien de plus à mettre en file pour ${nom} : ce qui manque est déjà dans « Téléchargements ».`;
}
