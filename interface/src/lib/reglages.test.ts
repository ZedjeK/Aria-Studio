import { describe, expect, it } from 'vitest';
import type { ParamReglage } from './api';
import {
  changements,
  filtrerReglages,
  groupesLlm,
  libelleChoix,
  nombreModifies,
  nomConsigne,
  texteChangements,
  valeurAffichee,
} from './reglages';

function param(changes: Partial<ParamReglage>): ParamReglage {
  return {
    cle: 'cfg',
    libelle: 'CFG',
    type: 'reel',
    defaut: 1,
    valeur: 1,
    modifie: false,
    min: 0,
    max: 30,
    pas: 0.1,
    choix: [],
    aide: '',
    libelles: {},
    ...changes,
  };
}

describe('Réglages', () => {
  it('ne garde que ce qui change', () => {
    const cfg = param({});
    const steps = param({ cle: 'steps', type: 'entier', defaut: 8, valeur: 12, modifie: true });
    expect(changements([cfg, steps], {})).toEqual({});
    expect(changements([cfg, steps], { cfg: 1 })).toEqual({});
    expect(changements([cfg, steps], { cfg: 2.5 })).toEqual({ cfg: 2.5 });
    // Revenir à l'origine : null, seulement si la valeur était modifiée.
    expect(changements([cfg, steps], { cfg: null, steps: null })).toEqual({ steps: null });
  });

  it('affiche la saisie, l’origine ou la valeur en vigueur', () => {
    const steps = param({ cle: 'steps', defaut: 8, valeur: 12, modifie: true });
    expect(valeurAffichee(steps, {})).toBe(12);
    expect(valeurAffichee(steps, { steps: 20 })).toBe(20);
    expect(valeurAffichee(steps, { steps: null })).toBe(8);
  });

  it('nomme les choix et les consignes', () => {
    const moteur = param({ cle: 'moteur', type: 'choix', libelles: { auto: 'Automatique' } });
    expect(libelleChoix(moteur, 'auto')).toBe('Automatique');
    expect(libelleChoix(moteur, 'llama')).toBe('llama');
    expect(nomConsigne('retouche')).toBe('Consigne de retouche');
    expect(nomConsigne('decoupage')).toBe('Découpage d’une séquence');
    expect(nomConsigne('directions')).toBe('Directions proposées');
    expect(nomConsigne('verification')).toBe('Vérification d’une image');
    expect(nomConsigne('choix')).toBe('Choix des meilleures images');
    expect(nomConsigne('explication')).toBe('Explication d’une erreur');
    expect(nomConsigne('video')).toBe('Prompt d’un clip');
    expect(nomConsigne('precisions')).toBe('Traduction des précisions');
    expect(nomConsigne('autre')).toBe('autre');
  });

  it('range les réglages du LLM en trois groupes, sans groupe vide', () => {
    const cles = [
      'temperature_reecriture',
      'top_p',
      'presence_penalty',
      'temperature_reflexion',
      'top_k_reflexion',
      'max_tokens',
      'contexte',
      'reflexion',
      'moteur',
      'vision',
    ];
    const groupes = groupesLlm(cles.map((cle) => param({ cle })));
    expect(groupes.map((g) => g.titre)).toEqual(['Écriture sans réflexion', 'Écriture avec réflexion', 'Capacité et moteur']);
    expect(groupes[0]?.params.map((p) => p.cle)).toEqual(['temperature_reecriture', 'top_p', 'presence_penalty']);
    expect(groupes[1]?.params.map((p) => p.cle)).toEqual(['temperature_reflexion', 'top_k_reflexion']);
    expect(groupes[2]?.params.map((p) => p.cle)).toEqual(['max_tokens', 'contexte', 'reflexion', 'moteur', 'vision']);
    expect(groupesLlm([param({ cle: 'moteur' })]).map((g) => g.titre)).toEqual(['Capacité et moteur']);
    expect(groupesLlm([])).toEqual([]);
  });

  it('compte les changements non enregistrés', () => {
    expect(texteChangements(0)).toBe('0 changement non enregistré');
    expect(texteChangements(1)).toBe('1 changement non enregistré');
    expect(texteChangements(2)).toBe('2 changements non enregistrés');
  });

  it('cherche un réglage sans accent ni majuscule, mot par mot', () => {
    const params = [
      param({ cle: 'temperature_reflexion', libelle: 'Température · écriture avec réflexion' }),
      param({ cle: 'top_k', libelle: 'Top-k · sans réflexion', aide: 'Nombre de mots candidats' }),
    ];
    expect(filtrerReglages(params, {}, '', false)).toHaveLength(2);
    expect(filtrerReglages(params, {}, 'TEMPERATURE', false).map((p) => p.cle)).toEqual(['temperature_reflexion']);
    expect(filtrerReglages(params, {}, 'reflexion sans', false).map((p) => p.cle)).toEqual(['top_k']);
    expect(filtrerReglages(params, {}, 'candidats', false).map((p) => p.cle)).toEqual(['top_k']);
    expect(filtrerReglages(params, {}, 'absent', false)).toEqual([]);
  });

  it('ne garde que les réglages modifiés, enregistrés ou en cours de saisie', () => {
    const params = [
      param({ cle: 'a', defaut: 1, valeur: 2, modifie: true }),
      param({ cle: 'b', defaut: 1, valeur: 1 }),
      param({ cle: 'c', defaut: 1, valeur: 1 }),
    ];
    expect(filtrerReglages(params, {}, '', true).map((p) => p.cle)).toEqual(['a']);
    expect(filtrerReglages(params, { c: 5, a: null }, '', true).map((p) => p.cle)).toEqual(['c']);
    expect(nombreModifies(params, {})).toBe(1);
    expect(nombreModifies(params, { b: 3 })).toBe(2);
  });
});
