import { describe, expect, it } from 'vitest';
import type { Pack, PlanAssistant, Telechargement } from './api';
import { bilanPlan, choixInitial, etatTelechargement, go, packsAFinir, suiviTelechargements } from './assistant';

const GO = 2 ** 30;

function pack(id: string, changements: Partial<Pack> = {}): Pack {
  return {
    id,
    nom: id,
    usage: '',
    conseille: false,
    facultatif: false,
    testee: null,
    taille: 10 * GO,
    aTelecharger: 10 * GO,
    installe: false,
    licences: [],
    licenceAcceptee: null,
    moteursAbsents: [],
    ...changements,
  };
}

const plan = (changements: Partial<PlanAssistant> = {}): PlanAssistant => ({
  fichiers: [],
  aTelecharger: 23 * GO,
  libre: 100 * GO,
  placeOk: true,
  licencesAAccepter: [],
  ...changements,
});

function tache(etat: Telechargement['etat'], recu: number, taille = 100): Telechargement {
  return { id: etat, nom: etat, dossier: 'llm', taille, etat, recu, debit: 0, erreur: null };
}

describe("l'assistant de premier lancement", () => {
  it('coche les packs conseillés pas encore installés', () => {
    const packs = [pack('images', { conseille: true }), pack('video'), pack('llm', { conseille: true, installe: true })];
    expect(choixInitial(packs)).toEqual(['images']);
    expect(packsAFinir(packs, ['images', 'llm'])).toEqual(['llm', 'images']);
  });

  it('dit ce que coûte la sélection', () => {
    const packs = [pack('images'), pack('video', { moteursAbsents: ['ComfyUI'] })];
    expect(go(23.4 * GO)).toBe('23 Go');
    expect(bilanPlan(packs, ['images'], plan())).toEqual({
      texte: 'À télécharger : 23 Go (fichiers communs comptés une fois) · 100 Go libres',
      ko: false,
      suivant: true,
      alerte: '',
    });
    const sansPlace = bilanPlan(packs, ['images'], plan({ placeOk: false }));
    expect(sansPlace.ko && !sansPlace.suivant).toBe(true);
    expect(bilanPlan(packs, ['video'], plan()).suivant).toBe(false);
    expect(bilanPlan(packs, [], null)).toMatchObject({ texte: 'Coche au moins un pack.', suivant: false });
    expect(bilanPlan([pack('images', { installe: true })], [], null)).toMatchObject({
      texte: 'Tout est déjà installé.',
      suivant: true,
    });
  });

  it('suit les téléchargements', () => {
    const taches = [tache('termine', 0, GO), tache('en_cours', GO / 2, GO)];
    expect(suiviTelechargements(taches)).toEqual({ texte: '2 Go sur 2 Go', finis: false });
    expect(suiviTelechargements([tache('erreur', 0)]).finis).toBe(true);
    expect(etatTelechargement(tache('en_cours', 25))).toBe('en cours · 25 %');
    expect(etatTelechargement(tache('verification', 0))).toBe('vérification');
  });
});
