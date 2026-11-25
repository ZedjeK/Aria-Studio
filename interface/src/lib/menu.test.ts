import { describe, expect, it } from 'vitest';
import type { EtatMoteur } from './api';
import { actionsSansOuvrir, couleurTon, fractionVram, ligneErreur, pageDuMenu, pagesDuMenu, pastillesMoteurs, texteVram } from './menu';

describe('menu de l’icône', () => {
  it('donne à chaque ton la couleur de la pastille', () => {
    expect(couleurTon('actif')).toBe('var(--cyan)');
    expect(couleurTon('attention')).toBe('var(--orange)');
    expect(couleurTon('information')).toBe('var(--bleu)');
    expect(couleurTon('repos')).toBe('#3a4d70');
  });

  it('montre une erreur comme une ligne à regarder, avec une majuscule', () => {
    expect(ligneErreur('registre inaccessible')).toEqual({ ton: 'attention', texte: 'Registre inaccessible' });
  });

  describe('pastilles des moteurs', () => {
    const moteur = (id: string, statut: EtatMoteur['statut'], occupe = false): EtatMoteur => ({
      id,
      nom: id,
      role: '',
      facultatif: false,
      dossier: '',
      version: '1',
      versionInstallee: statut === 'absent' ? null : '1',
      statut,
      chemin: '',
      externe: false,
      precedente: false,
      aTelecharger: 0,
      occupe,
      prerequis: null,
    });

    it('attend la lecture avant de conclure', () => {
      expect(pastillesMoteurs(null)).toEqual([
        { nom: 'ComfyUI', detail: 'lecture…', ton: 'repos' },
        { nom: 'LLM', detail: 'lecture…', ton: 'repos' },
      ]);
    });

    it('montre un moteur installé au repos, ou en marche quand un programme écoute', () => {
      const repos = pastillesMoteurs([moteur('comfyui', 'a_jour'), moteur('llama', 'a_jour'), moteur('llama-prism', 'a_jour')]);
      expect(repos.map((p) => [p.detail, p.ton])).toEqual([
        ['au repos', 'repos'],
        ['au repos', 'repos'],
      ]);
      const marche = pastillesMoteurs([moteur('comfyui', 'a_jour', true), moteur('llama', 'a_jour'), moteur('llama-prism', 'a_jour', true)]);
      expect(marche.map((p) => [p.detail, p.ton])).toEqual([
        ['en marche', 'actif'],
        ['en marche', 'actif'],
      ]);
    });

    it('signale un moteur absent, et garde le LLM au repos si une seule variante est installée', () => {
      const p = pastillesMoteurs([moteur('comfyui', 'absent'), moteur('llama', 'absent'), moteur('llama-prism', 'a_jour')]);
      expect(p[0]).toEqual({ nom: 'ComfyUI', detail: 'à installer', ton: 'attention' });
      expect(p[1]).toEqual({ nom: 'LLM', detail: 'au repos', ton: 'repos' });
      expect(pastillesMoteurs([])[0]?.ton).toBe('attention');
    });

    it('prend le ComfyUI de l’utilisateur pour un moteur installé', () => {
      expect(pastillesMoteurs([moteur('comfyui', 'externe')])[0]).toEqual({ nom: 'ComfyUI', detail: 'au repos', ton: 'repos' });
    });
  });

  describe('VRAM en fond des pastilles', () => {
    it('remplit la part utilisée de la VRAM totale', () => {
      expect(fractionVram({ utiliseeGo: 8, totaleGo: 16 })).toBe(0.5);
      expect(fractionVram({ utiliseeGo: 0, totaleGo: 16 })).toBe(0);
    });

    it('ne dépasse jamais le bloc entier', () => {
      expect(fractionVram({ utiliseeGo: 20, totaleGo: 16 })).toBe(1);
      expect(texteVram({ utiliseeGo: 20, totaleGo: 16 })).toBe('16 Go sur 16 Go');
    });

    it('dit « 9,8 Go sur 16 Go » dans l’info-bulle, sans « ,0 » inutile', () => {
      expect(texteVram({ utiliseeGo: 9.8, totaleGo: 16 })).toBe('9,8 Go sur 16 Go');
      expect(texteVram({ utiliseeGo: 9.8, totaleGo: 15.99 })).toBe('9,8 Go sur 16 Go');
      expect(texteVram({ utiliseeGo: 0.04, totaleGo: 24 })).toBe('0 Go sur 24 Go');
    });

    it('ne montre rien sans lecture, jamais un chiffre supposé', () => {
      expect(fractionVram(null)).toBeNull();
      expect(texteVram(null)).toBeNull();
      expect(fractionVram({ utiliseeGo: 3, totaleGo: 0 })).toBeNull();
      expect(texteVram({ utiliseeGo: 3, totaleGo: 0 })).toBeNull();
      expect(fractionVram({ utiliseeGo: Number.NaN, totaleGo: 16 })).toBeNull();
      expect(texteVram({ utiliseeGo: -1, totaleGo: 16 })).toBeNull();
    });
  });

  describe('blocs « Agir sans ouvrir » et « Aller à »', () => {
    it('propose l’annulation seulement quand une génération tourne, et toujours de libérer la carte', () => {
      expect(actionsSansOuvrir(false)).toEqual([
        { action: 'annulerGeneration', titre: 'Annuler la génération en cours', active: false },
        { action: 'libererCarte', titre: 'Libérer la carte graphique', active: true },
      ]);
      expect(actionsSansOuvrir(true)[0]?.active).toBe(true);
    });

    it('liste les pages du menu dans l’ordre : Plateau, Montage, Modèles, Réglages, Paramètres', () => {
      expect(pagesDuMenu.map((p) => p.titre)).toEqual(['Plateau', 'Montage', 'Modèles', 'Réglages', 'Paramètres']);
    });

    it('ne reconnaît que les pages du menu dans l’événement « aller-a »', () => {
      expect(pageDuMenu('montage')).toBe('montage');
      expect(pageDuMenu('parametres')).toBe('parametres');
      expect(pageDuMenu('mise-a-jour')).toBeNull();
      expect(pageDuMenu('journal')).toBeNull();
    });
  });
});
