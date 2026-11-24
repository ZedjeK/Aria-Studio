import { describe, expect, it } from 'vitest';
import type { Commande, Creation } from './api';
import { remarqueReecriture, textesDuLlm } from './reecriture';

function commande(changes: Partial<Commande>): Commande {
  return { id: 'c1', idee: 'rends-la rousse', prompt: '', reecrire: true, llm: 'bonsai', ...changes } as Commande;
}

function fiche(changes: Partial<Creation>): Creation {
  return { id: 'r1', date: '2026-11-24T10:00:00', largeur: 1024, hauteur: 1024, ...changes };
}

describe('remarqueReecriture', () => {
  it('ne dit rien quand la réécriture a réussi', () => {
    expect(remarqueReecriture('faite', null)).toBeNull();
    expect(remarqueReecriture(undefined, undefined)).toBeNull();
  });

  it('dit que le LLM a manqué et que la consigne est partie telle quelle', () => {
    expect(remarqueReecriture('repli', 'délai dépassé')).toBe(
      'Le LLM a manqué (délai dépassé) : ta consigne est partie telle quelle.',
    );
  });

  it('dit le prompt redemandé, parfait ou non', () => {
    expect(remarqueReecriture('redemandee', 'trop court')).toContain('redemandé au LLM (trop court)');
    expect(remarqueReecriture('imparfaite', 'trop court')).toContain('le meilleur essai est parti');
  });
});

describe('textesDuLlm', () => {
  it('rien sans le LLM, ni avant qu’il ait écrit', () => {
    expect(textesDuLlm(commande({ reecrire: false, prompt: 'x' }), undefined)).toBeNull();
    expect(textesDuLlm(commande({ prompt: '' }), undefined)).toBeNull();
  });

  it('montre l’idée, le prompt et le LLM de la commande', () => {
    expect(textesDuLlm(commande({ prompt: 'a red-haired woman' }), undefined)).toEqual({
      idee: 'rends-la rousse',
      retravaille: null,
      envoye: 'a red-haired woman',
      llm: 'bonsai',
      remarque: null,
    });
  });

  it('une fiche à trois textes les montre tous les trois', () => {
    const t = textesDuLlm(
      commande({ prompt: 'a red-haired woman' }),
      fiche({
        idee: 'rends-la rousse',
        prompt_llm: '**Make her hair red**',
        prompt: 'Make her hair red',
        prompt_envoye: 'neon glow, Make her hair red',
        llm: 'bonsai',
        reecriture: 'faite',
      }),
    );
    expect(t).toEqual({
      idee: 'rends-la rousse',
      retravaille: '**Make her hair red**',
      envoye: 'neon glow, Make her hair red',
      llm: 'bonsai',
      remarque: null,
    });
  });

  it('une fiche à deux textes (v2, ou écrite avant) se lit encore', () => {
    const t = textesDuLlm(
      commande({ prompt: 'a red-haired woman' }),
      fiche({ idee: 'rends-la rousse', prompt: 'Make her hair red', llm: 'bonsai', reecriture: 'faite' }),
    );
    expect(t).toEqual({
      idee: 'rends-la rousse',
      retravaille: null,
      envoye: 'Make her hair red',
      llm: 'bonsai',
      remarque: null,
    });
  });

  it('la fiche fait foi : LLM manquant et remarque de repli', () => {
    const t = textesDuLlm(
      commande({ prompt: 'rends-la rousse' }),
      fiche({ idee: 'rends-la rousse', prompt: 'rends-la rousse', llm: null, reecriture: 'repli', reecriture_motif: 'LLM éteint' }),
    );
    expect(t?.llm).toBeNull();
    expect(t?.remarque).toContain('LLM éteint');
  });
});
