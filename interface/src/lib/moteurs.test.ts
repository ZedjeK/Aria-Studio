import { describe, expect, it } from 'vitest';
import type { Carte, EtatMoteur, TacheMoteur } from './api';
import {
  detailCarte,
  fractionTache,
  libelleStatut,
  manquants,
  mesureTache,
  peutReinstaller,
  teintePastille,
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
    expect(libelleStatut('externe', 'llama.cpp officiel (CUDA 13.4)')).toBe('ton llama.cpp officiel');
  });

  it('propose d’installer seulement ce qui n’est pas à jour', () => {
    expect(texteInstaller(moteur())).toBe('Installer');
    expect(texteInstaller(moteur({ statut: 'different' }))).toBe('Installer la b11734');
    expect(texteInstaller(moteur({ statut: 'a_jour' }))).toBeNull();
    expect(texteInstaller(moteur({ statut: 'externe' }))).toBeNull();
  });

  it('lit la taille à télécharger sur le bouton d’installation', () => {
    expect(texteInstaller(moteur({ aTelecharger: 1_677_722 }))).toBe('Installer · 1,6 Mo');
    expect(texteInstaller(moteur({ statut: 'different', aTelecharger: 155144809 }))).toBe('Installer la b11734 · 148 Mo');
  });

  it('ne réinstalle que ce que le moteur d’Aria a déjà à jour', () => {
    expect(peutReinstaller(moteur({ statut: 'a_jour' }))).toBe(true);
    expect(peutReinstaller(moteur({ statut: 'a_jour', externe: true }))).toBe(false);
    expect(peutReinstaller(moteur({ statut: 'absent' }))).toBe(false);
  });

  it('teinte la pastille : cyan quand c’est prêt, ambre sinon', () => {
    expect(teintePastille('a_jour')).toBe('cyan');
    expect(teintePastille('externe')).toBe('cyan');
    expect(teintePastille('absent')).toBe('ambre');
    expect(teintePastille('different')).toBe('ambre');
  });

  it('annonce la taille du téléchargement, ou qu’il n’y en a pas', () => {
    expect(texteConfirmation(moteur({ aTelecharger: 155144809 }))).toContain('148 Mo');
    expect(texteConfirmation(moteur())).toContain('rien à télécharger');
  });

  it('décrit la carte, ou ce qui ne va pas', () => {
    expect(texteCarte(carte())).toBe('RTX 4080 · pilote 581.15 · 16 Go de VRAM · 64 Go de RAM');
    expect(texteCarte(carte({ ok: false, message: 'Pilote 566.36 trop ancien' }))).toBe('Pilote 566.36 trop ancien');
  });

  it('sépare le détail de la carte de son nom', () => {
    expect(detailCarte(carte())).toBe('pilote 581.15 · 16 Go de VRAM · 64 Go de RAM');
    expect(detailCarte(carte({ ok: false, message: 'Pilote trop ancien' }))).toBe('');
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
