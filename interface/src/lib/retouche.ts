// La page Retouche : ancrages, tailles d'agrandissement, pinceau du masque.

/** Les ancrages d'un agrandissement, dans l'ordre de la grille 3 × 3, avec leur flèche (comme en v2). */
export const ANCRAGES: { id: string; fleche: string; titre: string }[] = [
  { id: 'haut-gauche', fleche: '↖', titre: 'En haut à gauche' },
  { id: 'haut', fleche: '↑', titre: 'En haut' },
  { id: 'haut-droite', fleche: '↗', titre: 'En haut à droite' },
  { id: 'gauche', fleche: '←', titre: 'À gauche' },
  { id: 'centre', fleche: '•', titre: 'Au centre' },
  { id: 'droite', fleche: '→', titre: 'À droite' },
  { id: 'bas-gauche', fleche: '↙', titre: 'En bas à gauche' },
  { id: 'bas', fleche: '↓', titre: 'En bas' },
  { id: 'bas-droite', fleche: '↘', titre: 'En bas à droite' },
];

/** La plus grande image acceptée, en pixels de côté (aria_coeur::images::TAILLE_MAX). */
export const TAILLE_MAX = 4096;
/** Le fondu le plus large, en pixels (aria_coeur::retouche::FONDU_MAX). */
export const FONDU_MAX = 128;
/** Les fondus d'origine de la v2. */
export const FONDU_RETOUCHE = 24;
export const FONDU_AGRANDISSEMENT = 96;

/** Ce qui ne va pas dans la taille d'un agrandissement, ou null : les mêmes règles que le cœur. */
export function problemeTaille(source: { largeur: number; hauteur: number }, largeur: number, hauteur: number): string | null {
  if (!Number.isInteger(largeur) || !Number.isInteger(hauteur)) return 'La taille est un nombre entier de pixels.';
  if (largeur < source.largeur || hauteur < source.hauteur || (largeur === source.largeur && hauteur === source.hauteur)) {
    return `La nouvelle taille doit être plus grande que ${source.largeur}×${source.hauteur}`;
  }
  if (Math.max(largeur, hauteur) > TAILLE_MAX) return `Taille maximale : ${TAILLE_MAX} px de côté`;
  return null;
}

/** Une taille d'agrandissement proposée : la source avec 50 % de largeur en plus, sans dépasser la taille maximale. */
export function tailleProposee(source: { largeur: number; hauteur: number }): { largeur: number; hauteur: number } {
  return { largeur: Math.min(TAILLE_MAX, Math.round(source.largeur * 1.5)), hauteur: source.hauteur };
}

/** Le point de l'image sous le pointeur : le canevas est affiché à une autre taille que l'image. */
export function versImage(
  x: number,
  y: number,
  cadre: { left: number; top: number; width: number; height: number },
  image: { largeur: number; hauteur: number },
): { x: number; y: number } {
  return {
    x: ((x - cadre.left) / cadre.width) * image.largeur,
    y: ((y - cadre.top) / cadre.height) * image.hauteur,
  };
}

/** Les points où poser le pinceau entre deux positions, assez serrés pour un trait continu. */
export function pointsDuTrait(
  depart: { x: number; y: number },
  arrivee: { x: number; y: number },
  rayon: number,
): { x: number; y: number }[] {
  const distance = Math.hypot(arrivee.x - depart.x, arrivee.y - depart.y);
  const pas = Math.max(1, rayon / 2);
  const nombre = Math.max(1, Math.ceil(distance / pas));
  return Array.from({ length: nombre }, (_, i) => {
    const t = (i + 1) / nombre;
    return { x: depart.x + (arrivee.x - depart.x) * t, y: depart.y + (arrivee.y - depart.y) * t };
  });
}
