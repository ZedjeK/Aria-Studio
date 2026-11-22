import { describe, expect, it } from 'vitest';
import type { Telechargement } from './api';
import { fractionTelechargement, libelleTache, ordreTache, telechargementActif, texteTelechargement } from './modeles';

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
});
