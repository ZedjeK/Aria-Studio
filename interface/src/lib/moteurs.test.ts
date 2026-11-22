import { describe, expect, it } from 'vitest';
import type { Carte, EtatMoteur, TacheMoteur } from './api';
import {
  fractionTache,
  libelleStatut,
  manquants,
  mesureTache,
  texteCarte,
  texteConfirmation,
  texteInstaller,
} from './moteurs';

const moteur = (m: Partial<EtatMoteur> = {}): EtatMoteur => ({
  id: 'llama',
  nom: 'llama.cpp officiel',
  role: 'Les GGUF',
  facultatif: false,
  dossier: 'llama',
  version: 'b11734',
  versionInstallee: null,
  statut: 'absent',
  chemin: 'C:\\moteurs\\llama',
  externe: false,
  precedente: false,
  aTelecharger: 0,
  occupe: false,
  prerequis: null,
  ...m,
});

const carte = (c: Partial<Carte> = {}): Carte => ({
  nvidia: true,
  nom: 'RTX 4080',
  pilote: '581.15',
  vramGo: 16,
  ramGo: 64,
  ok: true,
  message: null,
  ...c,
});

describe('moteurs', () => {
  it('dit chaque statut en mots', () => {
    expect(libelleStatut('a_jour')).toBe('à jour');
    expect(libelleStatut('externe')).toBe('ton ComfyUI');
  });

  it('propose d’installer seulement ce qui n’est pas à jour', () => {
    expect(texteInstaller(moteur())).toBe('Installer');
    expect(texteInstaller(moteur({ statut: 'different' }))).toBe('Installer la b11734');
    expect(texteInstaller(moteur({ statut: 'a_jour' }))).toBeNull();
    expect(texteInstaller(moteur({ statut: 'externe' }))).toBeNull();
  });

  it('annonce la taille du téléchargement, ou qu’il n’y en a pas', () => {
    expect(texteConfirmation(moteur({ aTelecharger: 155144809 }))).toContain('148 Mo');
    expect(texteConfirmation(moteur())).toContain('rien à télécharger');
  });

  it('décrit la carte, ou ce qui ne va pas', () => {
    expect(texteCarte(carte())).toBe('RTX 4080 · pilote 581.15 · 16 Go de VRAM · 64 Go de RAM');
    expect(texteCarte(carte({ ok: false, message: 'Pilote 566.36 trop ancien' }))).toBe('Pilote 566.36 trop ancien');
  });

  it('suit l’avancement seulement quand il a un total', () => {
    const tache: TacheMoteur = {
      id: 'llama',
      nom: 'llama',
      etape: 'Téléchargement',
      fait: 512,
      total: 1024,
      erreur: null,
      fini: false,
    };
    expect(fractionTache(tache)).toBe(0.5);
    expect(mesureTache(tache)).toBe('512 o sur 1 Ko');
    expect(fractionTache({ ...tache, total: 0 })).toBeNull();
    expect(mesureTache({ ...tache, total: 0 })).toBeUndefined();
  });

  it('signale les moteurs obligatoires absents', () => {
    const liste = [moteur(), moteur({ id: 'prism', facultatif: true }), moteur({ id: 'comfyui', statut: 'a_jour' })];
    expect(manquants(liste).map((m) => m.id)).toEqual(['llama']);
  });
});
