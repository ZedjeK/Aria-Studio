import { describe, expect, it } from 'vitest';
import type { StructureScenario, ValeurListe } from './api';
import {
  choixDansOnglet,
  choixDuBrouillon,
  contexteListes,
  formatDemande,
  incompatible,
  poserChoix,
  sousListeVisible,
  valeursProposees,
} from './scenario';

function valeur(nom: string, changes: Partial<ValeurListe> = {}): ValeurListe {
  return {
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
    ...changes,
  };
}

const structure: StructureScenario = {
  sujet: 'personnage',
  nom: 'Personnage',
  agePrecis: [3, 120],
  longueurs: { action: 500, replique: 600 },
  incompatibilites: [
    [
      ['personnage', 'Haut', 'Pull'],
      ['personnage', 'Bas', 'Robe'],
    ],
  ],
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
            { nom: 'Genre', note: '', adulte: false, condition: null, valeurs: [valeur('Femme'), valeur('Homme')] },
            {
              nom: 'Âge',
              note: '',
              adulte: false,
              condition: null,
              valeurs: [valeur('Ado (12-17 ans)', { mineur: true }), valeur('Adulte (26-35 ans)')],
            },
            {
              nom: 'Origine',
              note: '',
              adulte: false,
              condition: null,
              valeurs: [valeur('Japonaise', { familles: ['asie-est'] })],
            },
          ],
        },
        {
          nom: 'Tenue',
          onglet: 'sujet',
          sousListes: [
            {
              nom: 'Haut',
              note: '',
              adulte: false,
              condition: null,
              valeurs: [valeur('Pull'), valeur('Corset', { adulte: true, genre: 'femme' })],
            },
            { nom: 'Bas', note: '', adulte: false, condition: null, valeurs: [valeur('Robe', { remplace: ['Haut'] })] },
            {
              nom: 'Teint',
              note: '',
              adulte: false,
              condition: null,
              valeurs: [valeur('Clair', { origines: ['europe'] }), valeur('Doré', { origines: ['asie-est'] })],
            },
            {
              nom: 'Motif de tatouage',
              note: '',
              adulte: false,
              condition: { sousListe: 'Signe particulier', contient: 'tatou' },
              valeurs: [valeur('Dragon')],
            },
          ],
        },
      ],
    },
  ],
};

const sl = (nom: string) => structure.blocs[0]!.groupes.flatMap((g) => g.sousListes).find((s) => s.nom === nom)!;

describe('Scénario', () => {
  it('filtre les valeurs selon le genre, l’âge et l’origine', () => {
    const homme = contexteListes(structure, { personnage: { Genre: 'Homme' } }, null, false);
    expect(valeursProposees('personnage', sl('Haut'), homme).map((v) => v.nom)).toEqual(['Pull']);
    const ado = contexteListes(structure, { personnage: { Âge: 'Ado (12-17 ans)' } }, null, false);
    expect(ado.mineur).toBe(true);
    expect(valeursProposees('personnage', sl('Haut'), ado).map((v) => v.nom)).toEqual(['Pull']);
    expect(contexteListes(structure, {}, 15, false).mineur).toBe(true);
    const japonaise = contexteListes(structure, { personnage: { Origine: 'Japonaise' } }, null, false);
    expect(valeursProposees('personnage', sl('Teint'), japonaise).map((v) => v.nom)).toEqual(['Doré']);
    const toutes = contexteListes(structure, { personnage: { Origine: 'Japonaise' } }, null, true);
    expect(valeursProposees('personnage', sl('Teint'), toutes)).toHaveLength(2);
  });

  it('montre une sous-liste à condition seulement quand elle est permise', () => {
    const sans = contexteListes(structure, {}, null, false);
    expect(sousListeVisible('personnage', sl('Motif de tatouage'), sans)).toBe(false);
    const avec = contexteListes(structure, { personnage: { 'Signe particulier': 'Tatouage au bras' } }, null, false);
    expect(sousListeVisible('personnage', sl('Motif de tatouage'), avec)).toBe(true);
  });

  it('applique incompatibilités et remplacements', () => {
    const choix = { personnage: { Haut: 'Pull' } };
    expect(incompatible(structure, choix, 'personnage', 'Bas', 'Robe')).toBe(true);
    expect(incompatible(structure, {}, 'personnage', 'Bas', 'Robe')).toBe(false);
    const robe = poserChoix(structure, choix, 'personnage', 'Bas', 'Robe');
    expect(robe['personnage']).toEqual({ Bas: 'Robe' });
    expect(poserChoix(structure, robe, 'personnage', 'Bas', '')['personnage']).toEqual({});
    expect(choixDansOnglet(structure, robe, 'sujet')).toBe(1);
    expect(choixDansOnglet(structure, robe, 'decor')).toBe(0);
  });

  it('relit un brouillon sans lui faire confiance', () => {
    expect(choixDuBrouillon({ personnage: { personnage: { Genre: 'Femme', Âge: 3 } }, objet: 'x' })).toEqual({
      personnage: { personnage: { Genre: 'Femme' } },
    });
    expect(choixDuBrouillon(null)).toEqual({});
  });
});

describe('Format de l’atelier Image', () => {
  const formats = ['1:1 (Square)', '16:9 (Widescreen)', '9:16 (Portrait)'];

  it('envoie le format choisi, au texte comme aux références', () => {
    expect(formatDemande('image', null, '9:16 (Portrait)', formats)).toBe('9:16 (Portrait)');
    expect(formatDemande('image', 'inspirer', '16:9 (Widescreen)', formats)).toBe('16:9 (Widescreen)');
  });

  it('n’envoie rien quand le modèle cible décide, ou pour un format que le Plateau ne propose pas', () => {
    expect(formatDemande('image', null, '', formats)).toBeNull();
    expect(formatDemande('image', null, '5:4', formats)).toBeNull();
    expect(formatDemande('image', null, '1:1 (Square)', [])).toBeNull();
  });

  it('ne cadre ni Redessiner (le format est celui de l’image) ni les autres ateliers', () => {
    expect(formatDemande('image', 'variation', '1:1 (Square)', formats)).toBeNull();
    expect(formatDemande('clip', null, '1:1 (Square)', formats)).toBeNull();
    expect(formatDemande('retouche', null, '1:1 (Square)', formats)).toBeNull();
  });
});
