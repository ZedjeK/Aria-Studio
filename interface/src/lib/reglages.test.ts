import { describe, expect, it } from 'vitest';
import type { ParamReglage } from './api';
import { changements, libelleChoix, nomConsigne, valeurAffichee } from './reglages';

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
    expect(nomConsigne('autre')).toBe('autre');
  });
});
