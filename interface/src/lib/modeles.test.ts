import { describe, expect, it } from 'vitest';
import type { Telechargement } from './api';
import {
  dateLisible,
  erreurLora,
  erreurSteps,
  fractionTelechargement,
  libelleOrigine,
  libelleTache,
  ordreTache,
  stepsProposes,
  telechargementActif,
  texteTelechargement,
} from './modeles';

function tache(changes: Partial<Telechargement>): Telechargement {
  return {
    id: 't1',
    nom: 'vae.safetensors',
    dossier: 'image/_base/vae',
    taille: 2_000_000_000,
    etat: 'attente',
    recu: 0,
    debit: 0,
    erreur: null,
    ...changes,
  };
}

describe('Modèles', () => {
  it('nomme et range les tâches', () => {
    expect(libelleTache('retouche')).toBe('Image → image');
    expect(libelleTache('autre')).toBe('autre');
    expect(ordreTache('texte_vers_image')).toBeLessThan(ordreTache('texte_vers_video'));
    expect(ordreTache('autre')).toBe(4);
  });

  it('suit un téléchargement', () => {
    expect(telechargementActif(tache({}))).toBe(true);
    expect(telechargementActif(tache({ etat: 'verification' }))).toBe(true);
    expect(telechargementActif(tache({ etat: 'termine' }))).toBe(false);
    expect(fractionTelechargement(tache({ recu: 500_000_000 }))).toBe(0.25);
    expect(fractionTelechargement(tache({ taille: 0 }))).toBeNull();
    expect(texteTelechargement(tache({ etat: 'erreur', erreur: 'Espace disque insuffisant' }))).toBe('Espace disque insuffisant');
    expect(texteTelechargement(tache({ etat: 'verification' }))).toBe('vérification de l’empreinte SHA-256');
    expect(texteTelechargement(tache({ etat: 'en_cours', recu: 1, debit: 0 }))).not.toContain('/s');
  });

  it('aide à régler la collection', () => {
    expect(libelleOrigine('main')).toBe('déposé à la main');
    expect(libelleOrigine('autre')).toBe('autre');
    expect(
      stepsProposes([
        { valeur: 'beaucoup', origine: 'x', source: null },
        { valeur: 25, origine: 'défaut du template du workflow', source: null },
        { valeur: 40, origine: 'recommandé', source: 'https://exemple.org' },
      ]),
    ).toBe(25);
    expect(stepsProposes(null)).toBeNull();
    expect(stepsProposes([{ valeur: 500, origine: 'x', source: null }])).toBeNull();
    expect(erreurSteps(25)).toBeNull();
    expect(erreurSteps(0)).not.toBeNull();
    expect(erreurSteps(2.5)).not.toBeNull();
    expect(erreurLora('  neon   glow ', 0.8)).toBeNull();
    expect(erreurLora('a'.repeat(301), 1)).toContain('300');
    expect(erreurLora('neon', 2.5)).toBe('Force de LoRA entre 0 et 2');
    expect(dateLisible('2026-11-22T14:03:00')).toBe('22/11/2026 14:03');
    expect(dateLisible(null)).toBe('');
  });
});
