import { describe, expect, it } from 'vitest';
import type { SousListeScenario, StructureScenario, ValeurListe } from './api';
import {
  ajouterLignes,
  candidatesHasard,
  etapeCourante,
  etapesGuide,
  guideVide,
  lignesGuide,
  numeroQuestion,
  parcours,
  type EtatGuide,
  type OptionsGuide,
} from './guide';
import { aideReference, contexteListes } from './scenario';

const valeur = (nom: string, plus: Partial<ValeurListe> = {}): ValeurListe => ({
  nom,
  prompt: nom.toLowerCase(),
  remplace: [],
  adulte: false,
  genre: null,
  promptsGenre: null,
  mineur: false,
  bornes: null,
  familles: [],
  origines: [],
  description: '',
  avec: {},
  categorie: '',
  ...plus,
});

const liste = (nom: string, valeurs: ValeurListe[], plus: Partial<SousListeScenario> = {}): SousListeScenario => ({
  nom,
  note: '',
  adulte: false,
  condition: null,
  valeurs,
  ...plus,
});

const lieux = Array.from({ length: 10 }, (_, i) =>
  valeur(`Lieu ${i}`, { categorie: i < 6 ? 'Intérieur / Habitation' : 'Extérieur' }),
);

const structure: StructureScenario = {
  sujet: 'personnage',
  nom: 'Personnage',
  incompatibilites: [],
  agePrecis: [1, 120],
  longueurs: { action: 200, replique: 300 },
  blocs: [
    {
      id: 'personnage',
      titre: 'Personnage',
      video: false,
      groupes: [
        {
          nom: 'Identité',
          onglet: 'sujet',
          sousListes: [
            liste('Genre', [valeur('Femme'), valeur('Homme')]),
            liste('Tenue', [valeur('Robe', { remplace: ['Haut'] }), valeur('Jean')]),
            liste('Haut', [valeur('Chemise')]),
          ],
        },
      ],
    },
    {
      id: 'decor',
      titre: 'Décor',
      video: false,
      groupes: [{ nom: 'Lieu', onglet: 'decor', sousListes: [liste('Lieu', lieux)] }],
    },
    {
      id: 'plan',
      titre: 'Plan',
      video: true,
      groupes: [{ nom: 'Son', onglet: 'scene', sousListes: [liste('Musique', [valeur('Piano')])] }],
    },
  ],
};

const options: OptionsGuide = { sansSujet: false, video: false, image: true, ignorees: [] };
const ctx = (choix = {}) => contexteListes(structure, choix, null, false);

describe('guide du Scénario', () => {
  it('enchaîne le sujet, les listes, puis le texte dans l’image', () => {
    const ids = etapesGuide(structure, guideVide(), ctx(), options).map((e) => e.id);
    expect(ids).toEqual(['sujet', 'personnage/Genre', 'personnage/Tenue', 'personnage/Haut', 'decor/Lieu', 'texte']);
    // Un clip a le bloc Plan, pas de texte dans l'image ; une rubrique ignorée par le modèle ne se demande pas.
    const clip = etapesGuide(structure, guideVide(), ctx(), { ...options, video: true, image: false, ignorees: ['Lieu'] });
    expect(clip.map((e) => e.id)).toEqual(['sujet', 'personnage/Genre', 'personnage/Tenue', 'personnage/Haut', 'plan/Musique']);
  });

  it('saute le sujet quand il vient des images ou d’une idée libre', () => {
    expect(etapesGuide(structure, guideVide(), ctx(), { ...options, sansSujet: true }).map((e) => e.id)).toEqual([
      'decor/Lieu',
      'texte',
    ]);
    const libre: EtatGuide = {
      ...guideVide(),
      fait: [
        { id: 'sujet', onglet: 'sujet', titre: 'Sujet', groupe: '', bloc: null, sousListe: null, genre: 'libre', valeur: '', texte: 'Un dragon' },
      ],
    };
    const etapes = etapesGuide(structure, libre, ctx(), options);
    expect(etapeCourante(etapes, libre)?.id).toBe('decor/Lieu');
    expect(lignesGuide(libre, {}, [], false)).toEqual(['Sujet : Un dragon']);
  });

  it('une robe remplace le haut : la question du haut disparaît', () => {
    const ids = etapesGuide(structure, guideVide(), ctx({ personnage: { Tenue: 'Robe' } }), options).map((e) => e.id);
    expect(ids).not.toContain('personnage/Haut');
    // Et une valeur qui remplacerait une liste déjà choisie n'est pas proposée.
    const tenue = etapesGuide(structure, guideVide(), ctx({ personnage: { Haut: 'Chemise' } }), options).find(
      (e) => e.id === 'personnage/Tenue',
    );
    expect(tenue?.type === 'liste' ? tenue.valeurs.map((v) => v.nom) : []).toEqual(['Jean']);
  });

  it('parcourt une longue liste par catégories', () => {
    const g = guideVide();
    expect(parcours(lieux, 'decor/Lieu', g).prochaines).toEqual(['Intérieur', 'Extérieur']);
    g.chemin['decor/Lieu'] = ['Intérieur'];
    const p = parcours(lieux, 'decor/Lieu', g);
    expect(p.prochaines).toEqual(['Habitation']);
    g.chemin['decor/Lieu'] = ['Intérieur', 'Habitation'];
    expect(parcours(lieux, 'decor/Lieu', g).restantes).toHaveLength(6);
    // « Toute la liste » : plus de catégories.
    expect(parcours(lieux, 'decor/Lieu', { ...guideVide(), toute: { 'decor/Lieu': true } }).prochaines).toEqual([]);
  });

  it('le dé reste dans la catégorie choisie et change de valeur', () => {
    const g: EtatGuide = { ...guideVide(), chemin: { 'decor/Lieu': ['Extérieur'] } };
    const e = etapesGuide(structure, g, ctx(), options).find((x) => x.id === 'decor/Lieu');
    if (!e) throw new Error('étape absente');
    expect(candidatesHasard(e, g, { decor: { Lieu: 'Lieu 6' } })).toEqual(['Lieu 7', 'Lieu 8', 'Lieu 9']);
  });

  it('range les réponses dans la conversation sans doublon', () => {
    expect(ajouterLignes('Lieu : Lieu 1\n', ['Lieu : Lieu 1', 'Genre : Femme'])).toBe('Lieu : Lieu 1\nGenre : Femme');
  });

  it('explique le départ de chaque atelier', () => {
    expect(aideReference('image', 'inspirer', 4)).toContain("Jusqu'à 4 images");
    expect(aideReference('image', null, 0)).toContain("Pas d'image");
    expect(aideReference('clip', null, 1)).toContain('première image du clip');
  });
});

describe('Numéro de la question', () => {
  const sujet = { type: 'sujet', id: 'sujet', onglet: 'sujet', titre: 'Sujet ?', groupe: '' } as const;
  const texte = { type: 'texte', id: 'texte', onglet: 'scene', titre: 'Texte ?', groupe: '' } as const;

  it('donne le rang de l’étape et le total du déroulé', () => {
    expect(numeroQuestion([sujet, texte], sujet)).toEqual({ numero: 1, total: 2 });
    expect(numeroQuestion([sujet, texte], texte)).toEqual({ numero: 2, total: 2 });
  });

  it('ne dépasse jamais le total quand l’étape n’est pas (ou plus) dans le déroulé', () => {
    expect(numeroQuestion([sujet], texte)).toEqual({ numero: 2, total: 2 });
    expect(numeroQuestion([], sujet)).toEqual({ numero: 1, total: 1 });
  });
});
