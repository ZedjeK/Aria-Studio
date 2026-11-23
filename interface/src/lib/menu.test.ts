import { describe, expect, it } from 'vitest';
import type { EtatMoteur } from './api';
import { couleurTon, ligneErreur, pastillesMoteurs } from './menu';

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
});
