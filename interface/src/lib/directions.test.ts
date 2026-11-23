import { describe, expect, it } from 'vitest';
import type { Direction } from './api';
import { aEcarter, alertesDe, apresProposition, ECARTEES_MAX, etatVide, versIdee } from './directions';

const d = (titre: string, texte = `${titre}, en deux phrases.`): Direction => ({ titre, texte, difference: '' });

describe('directions', () => {
  it('garde les directions vues pour la proposition suivante', () => {
    const premiere = apresProposition(etatVide(), [d('Néons'), d('Brume'), d('Métro')], []);
    expect(premiere.ecartees).toEqual([]);
    const seconde = apresProposition(premiere, [d('Aube'), d('Marché'), d('Toits')], ['Contrôle qualité : x']);
    expect(seconde.ecartees.map((x) => x.titre)).toEqual(['Néons', 'Brume', 'Métro']);
    expect(seconde.alertes).toEqual(['Contrôle qualité : x']);
    expect(aEcarter(seconde).map((x) => x.titre)).toEqual(['Néons', 'Brume', 'Métro', 'Aube', 'Marché', 'Toits']);
  });

  it('écarte sans doublon et borne la liste', () => {
    const etat = { proposees: [d('néons')], ecartees: [d('Néons'), d('Brume')], alertes: [] };
    expect(aEcarter(etat).map((x) => x.titre)).toEqual(['Brume', 'néons']);
    const beaucoup = { proposees: [], ecartees: Array.from({ length: 20 }, (_, i) => d(`D${i}`)), alertes: [] };
    expect(aEcarter(beaucoup)).toHaveLength(ECARTEES_MAX);
    expect(aEcarter(beaucoup)[0]?.titre).toBe('D8');
  });

  it("pose une direction dans l'idée", () => {
    expect(versIdee(d('Pluie de néons.', 'Une femme attend sous un abribus.'))).toBe(
      'Pluie de néons : Une femme attend sous un abribus.',
    );
    expect(versIdee({ titre: '', texte: 'Seulement le texte.', difference: '' })).toBe('Seulement le texte.');
  });

  it('dit pourquoi la réponse a été redemandée', () => {
    expect(alertesDe('il faut exactement 3 directions', [])).toEqual([
      'Directions redemandées au LLM : il faut exactement 3 directions.',
    ]);
    expect(alertesDe(null, ['Contrôle qualité : y'])).toEqual(['Contrôle qualité : y']);
  });
});
