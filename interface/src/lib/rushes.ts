// Les Rushes du Plateau : onglets, actions proposées et textes, d'après les compteurs du cœur.

import type { ActionRushes, EtatRushes, VueRushes } from './api';

/** Un onglet : une vue, ou un dossier (vue « tous » restreinte à ce dossier). */
export interface Onglet {
  cle: string;
  titre: string;
  vue: VueRushes;
  dossier: string | null;
}

const PREFIXE_DOSSIER = 'dossier:';

/** Les onglets dans l'ordre de la v2 : Tous, Non rangés, les dossiers, Archives, Corbeille ; chacun son nombre. */
export function onglets(etat: EtatRushes): Onglet[] {
  return [
    { cle: 'tous', titre: `Tous (${etat.tous})`, vue: 'tous', dossier: null },
    { cle: 'non_ranges', titre: `Non rangés (${etat.nonRanges})`, vue: 'non_ranges', dossier: null },
    ...etat.dossiers.map((d) => ({
      cle: `${PREFIXE_DOSSIER}${d.nom}`,
      titre: `${d.nom} (${d.nombre})`,
      vue: 'tous' as const,
      dossier: d.nom,
    })),
    { cle: 'archives', titre: `Archives (${etat.archives})`, vue: 'archives', dossier: null },
    { cle: 'corbeille', titre: `Corbeille (${etat.corbeille})`, vue: 'corbeille', dossier: null },
  ];
}

/** L'onglet de cette clé, ou « Tous » s'il a disparu (dossier supprimé ou renommé). */
export function ongletDe(liste: Onglet[], cle: string): Onglet {
  return liste.find((o) => o.cle === cle) ?? { cle: 'tous', titre: 'Tous', vue: 'tous', dossier: null };
}

/** Les actions proposées sur une sélection, selon la vue : on restaure depuis la corbeille, on désarchive depuis les archives. */
export function actions(vue: VueRushes): ActionRushes[] {
  if (vue === 'corbeille') return ['restaurer'];
  if (vue === 'archives') return ['desarchiver', 'corbeille'];
  return ['archiver', 'corbeille', 'ranger'];
}

const LIBELLES: Record<ActionRushes, string> = {
  corbeille: 'Mettre à la corbeille',
  restaurer: 'Restaurer',
  archiver: 'Archiver',
  desarchiver: 'Désarchiver',
  ranger: 'Ranger',
};

export function libelleAction(action: ActionRushes): string {
  return LIBELLES[action];
}

/** « 1 image », « 3 images ». */
export function images(nombre: number): string {
  return `${nombre} image${nombre > 1 ? 's' : ''}`;
}

/** Ce que l'action a fait, pour la ligne d'état. */
export function compteRendu(action: ActionRushes, nombre: number, dossier: string | null): string {
  const quoi = images(nombre);
  switch (action) {
    case 'corbeille':
      return `${quoi} à la corbeille.`;
    case 'restaurer':
      return `${quoi} restaurée${nombre > 1 ? 's' : ''}.`;
    case 'archiver':
      return `${quoi} archivée${nombre > 1 ? 's' : ''}.`;
    case 'desarchiver':
      return `${quoi} sortie${nombre > 1 ? 's' : ''} des archives.`;
    case 'ranger':
      return dossier ? `${quoi} rangée${nombre > 1 ? 's' : ''} dans ${dossier}.` : `${quoi} dans Non rangés.`;
  }
}

/** La sélection après un clic : le rush est ajouté, ou retiré s'il y était. */
export function basculer(selection: string[], id: string): string[] {
  return selection.includes(id) ? selection.filter((s) => s !== id) : [...selection, id];
}
