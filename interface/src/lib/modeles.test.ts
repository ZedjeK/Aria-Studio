import { describe, expect, it } from 'vitest';
import type { Telechargement } from './api';
import {
  actionLlm,
  actionWorkflow,
  dateLisible,
  enFile,
  etatFichier,
  erreurLora,
  erreurSteps,
  fractionTelechargement,
  libelleOrigine,
  libelleTache,
  messageMiseEnFile,
  ordreTache,
  roleFichier,
  texteBoutonTelechargements,
  fichiersAManquer,
  stepsProposes,
  telechargementActif,
  texteIdentification,
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

  it('suit la recherche des sources', () => {
    const base = { total: 3, fait: 0, fichier: null, phase: null, trouves: [], introuvables: [], erreurs: [] };
    expect(texteIdentification({ ...base, etat: 'repos' })).toBe('');
    expect(texteIdentification({ ...base, etat: 'en_cours', fait: 1, fichier: 'a.safetensors', phase: 'civitai' })).toBe(
      '2 sur 3 · a.safetensors · recherche sur Civitai',
    );
    expect(
      texteIdentification({
        ...base,
        etat: 'termine',
        trouves: [{ nom: 'a', ou: 'Civitai : A' }],
        introuvables: ['b', 'c'],
        erreurs: [],
      }),
    ).toBe('1 retrouvé, 2 introuvables');
  });

  it('dit ce que fait un fichier et compte ce qui manque', () => {
    expect(roleFichier('image/_base/vae')).toBe('VAE');
    expect(roleFichier('image/_base/text_encoders')).toBe('encodeur de texte');
    expect(roleFichier('image/_base/checkpoints')).toBe('checkpoint');
    expect(roleFichier('image/qwen/inconnu')).toBe('inconnu');
    expect(fichiersAManquer([{ manque: false }, { manque: true }, { manque: true }])).toBe(2);
  });

  it('écrit le bouton des téléchargements', () => {
    expect(texteBoutonTelechargements(0)).toBe('⬇ Téléchargements');
    expect(texteBoutonTelechargements(2)).toBe('⬇ Téléchargements · 2');
  });
});

describe('etatFichier', () => {
  it('un fichier présent est présent, exigé ou non', () => {
    expect(etatFichier({ present: true, manque: false })).toBe('present');
    expect(etatFichier({ present: true, manque: true })).toBe('present');
  });

  it('un fichier absent et exigé manque : le bouton de téléchargement s’affiche', () => {
    expect(etatFichier({ present: false, manque: true })).toBe('manque');
  });

  it('un fichier absent mais non exigé : le cœur ne le demande pas', () => {
    expect(etatFichier({ present: false, manque: false })).toBe('non_exige');
  });
});

/** Un fichier de workflow : posé, à télécharger, ou le checkpoint d'origine qu'un autre rend inutile. */
function fichier(nom: string, dossier: string, etat: 'present' | 'manque' | 'non_exige', taille = 1000) {
  return { nom, dossier, taille, present: etat === 'present', manque: etat === 'manque' };
}

const VAE = 'image/_base/vae';
const MODELES = 'image/Krea2/modeles';

describe('actionWorkflow : tant que quelque chose manque, il y a un bouton', () => {
  const complet = { pret: true, fichiers: [fichier('m.safetensors', MODELES, 'present'), fichier('v.safetensors', VAE, 'present')] };
  const incomplet = { pret: false, fichiers: [fichier('m.safetensors', MODELES, 'manque'), fichier('v.safetensors', VAE, 'present')] };
  // Un autre checkpoint est là : l'origine est absente mais non exigée, le vae manque.
  const autreCheckpoint = {
    pret: false,
    fichiers: [fichier('m.safetensors', MODELES, 'non_exige'), fichier('v.safetensors', VAE, 'manque')],
  };
  // Un autre checkpoint est là et rien d'autre ne manque : prêt, sans bouton.
  const autreCheckpointPret = {
    pret: true,
    fichiers: [fichier('m.safetensors', MODELES, 'non_exige'), fichier('v.safetensors', VAE, 'present')],
  };

  it('un workflow complet est prêt, sans bouton', () => {
    expect(actionWorkflow(complet, [])).toEqual({ genre: 'pret' });
    expect(actionWorkflow(autreCheckpointPret, [])).toEqual({ genre: 'pret' });
  });

  it('un workflow incomplet propose de télécharger', () => {
    expect(actionWorkflow(incomplet, [])).toEqual({ genre: 'telecharger' });
    expect(actionWorkflow(autreCheckpoint, [])).toEqual({ genre: 'telecharger' });
  });

  it('un fichier non exigé ne compte pas parmi ce qui manque ni parmi ce qui est en file', () => {
    const encours = tache({ nom: 'v.safetensors', dossier: VAE, etat: 'en_cours' });
    // Le seul fichier exigé est en file : un bouton ouvre la file.
    expect(actionWorkflow(autreCheckpoint, [encours])).toEqual({ genre: 'en_file', fichiers: 1 });
    expect(fichiersAManquer(autreCheckpoint.fichiers)).toBe(1);
  });

  it('une partie seulement de ce qui manque est en file : le bouton reste', () => {
    const deux = { pret: false, fichiers: [fichier('m.safetensors', MODELES, 'manque'), fichier('v.safetensors', VAE, 'manque')] };
    const encours = tache({ nom: 'v.safetensors', dossier: VAE, etat: 'en_cours' });
    expect(actionWorkflow(deux, [encours])).toEqual({ genre: 'telecharger' });
    const deuxEnFile = [encours, tache({ id: 't2', nom: 'm.safetensors', dossier: MODELES, etat: 'attente' })];
    expect(actionWorkflow(deux, deuxEnFile)).toEqual({ genre: 'en_file', fichiers: 2 });
  });

  it('un téléchargement terminé, annulé ou en erreur n’est plus en file : le bouton revient', () => {
    for (const etat of ['termine', 'annule', 'erreur'] as const) {
      const t = tache({ nom: 'm.safetensors', dossier: MODELES, etat });
      expect(actionWorkflow(incomplet, [t])).toEqual({ genre: 'telecharger' });
    }
  });

  it('un fichier du même nom dans un autre dossier n’est pas le même fichier', () => {
    const ailleurs = tache({ nom: 'm.safetensors', dossier: 'image/Autre/modeles', etat: 'en_cours' });
    expect(enFile([ailleurs], MODELES, 'm.safetensors')).toBe(false);
    expect(actionWorkflow(incomplet, [ailleurs])).toEqual({ genre: 'telecharger' });
  });

  it('incomplet sans fichier à compter : le bouton reste (rien ne manque sans bouton)', () => {
    expect(actionWorkflow({ pret: false, fichiers: [] }, [])).toEqual({ genre: 'telecharger' });
  });
});

describe('actionLlm', () => {
  const absent = { fichier: 'Bonsai.gguf', installe: false };

  it('un LLM installé n’a pas de bouton', () => {
    expect(actionLlm({ fichier: 'Bonsai.gguf', installe: true }, [])).toBe('installe');
  });

  it('un LLM absent du catalogue, généraliste ou spécialiste, propose de l’installer', () => {
    expect(actionLlm(absent, [])).toBe('installer');
    expect(actionLlm({ fichier: 'Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf', installe: false }, [])).toBe('installer');
  });

  it('un LLM dont le GGUF est dans la file ouvre la file', () => {
    expect(actionLlm(absent, [tache({ nom: 'Bonsai.gguf', dossier: 'llm', etat: 'en_cours' })])).toBe('en_file');
    expect(actionLlm(absent, [tache({ nom: 'Bonsai.gguf', dossier: 'llm', etat: 'termine' })])).toBe('installer');
    expect(actionLlm(absent, [tache({ nom: 'autre.gguf', dossier: 'llm', etat: 'en_cours' })])).toBe('installer');
  });

  it('un LLM installé à qui il manque encore son projecteur propose de l’installer', () => {
    // Le cœur dit « installé » seulement avec tout ce qui l’accompagne : sans projecteur, installe est faux.
    expect(actionLlm({ fichier: 'Bonsai.gguf', installe: false }, [])).toBe('installer');
  });
});

describe('messageMiseEnFile', () => {
  it('dit ce qui est parti, ou pourquoi rien ne part', () => {
    expect(messageMiseEnFile('Krea 2', 1)).toBe('1 fichier en file pour Krea 2 : suis-les dans « Téléchargements ».');
    expect(messageMiseEnFile('Krea 2', 3)).toContain('3 fichiers en file');
    expect(messageMiseEnFile('Krea 2', 0)).toContain('déjà dans « Téléchargements »');
  });
});
