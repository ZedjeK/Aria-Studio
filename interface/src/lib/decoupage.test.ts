import { describe, expect, it } from 'vitest';
import type { PlanEcrit, Sequence } from './api';
import {
  alertesJusqua,
  alertesRenumerotees,
  corrigerEnchainements,
  depuisBrouillon,
  deplacer,
  depuisSequence,
  etatPlan,
  etatVide,
  majAlertes,
  motsPourDuree,
  motsReplique,
  nouveauPlan,
  plansDemande,
  plansPourMontage,
  poserPlan,
  redimensionner,
  remplisAuDela,
  retirer,
  sansPrompt,
  traductionValide,
} from './decoupage';

describe('atelier Séquence', () => {
  it('suit l’état d’un plan', () => {
    const p = nouveauPlan(0, null);
    expect(p.enchainement).toBe('texte');
    expect(nouveauPlan(0, 'r1').enchainement).toBe('rush');
    expect(etatPlan(p)[0]).toBe('a-ecrire');
    const ecrit: PlanEcrit = { ...plansDemande([p])[0]!, prompt: 'She walks.', idee: 'Elle marche.' };
    const pose = poserPlan(p, ecrit);
    expect([pose.note, etatPlan(pose)[0]]).toEqual(['Elle marche.', 'pret']);
    expect(etatPlan({ ...pose, duree: 10 })[0]).toBe('perime');
  });

  it('n’envoie la réplique que si le dialogue est coché', () => {
    const p = { ...nouveauPlan(1, null), replique: 'Salut', off: true };
    expect(plansDemande([p])[0]).toMatchObject({ replique: '', off: false });
    expect(plansDemande([{ ...p, dialogue: true }])[0]).toMatchObject({ replique: 'Salut', off: true });
    expect(motsReplique({ ...p, replique: 'un deux trois quatre cinq six sept huit neuf dix onze douze treize' }).trop).toBe(
      true,
    );
  });

  it('compte les mots d’une réplique de clip selon sa durée', () => {
    const quatorze = 'un deux trois quatre cinq six sept huit neuf dix onze douze treize quatorze';
    expect(motsPourDuree(quatorze, 5)).toEqual({ mots: 14, permis: 12, trop: true });
    expect(motsPourDuree(quatorze, 8)).toEqual({ mots: 14, permis: 20, trop: false });
    expect(motsPourDuree('  ', 5)).toEqual({ mots: 0, permis: 12, trop: false });
  });

  it('garde les alertes des autres plans', () => {
    const avant = ['Plan 1 : a', 'Plan 2 : b'];
    expect(majAlertes(avant, ['Plan 2 : c'], 1)).toEqual(['Plan 1 : a', 'Plan 2 : c']);
    expect(majAlertes(avant, ['x'], null)).toEqual(['x']);
    expect(alertesJusqua(['Plan 3 : z', 'Réplique « a »', 'Plan 1 : y'], 2)).toEqual(['Réplique « a »', 'Plan 1 : y']);
  });

  it('change le nombre de plans', () => {
    const plans = [nouveauPlan(0, null), nouveauPlan(1, null), { ...nouveauPlan(2, null), note: 'x' }];
    expect(remplisAuDela(plans, 2)).toBe(1);
    expect(redimensionner(plans, 4, null).map((p) => p.enchainement)).toEqual(['texte', 'couper', 'couper', 'couper']);
    expect(sansPrompt(plans)).toBe(0);
  });

  it('passe au Montage et en revient', () => {
    const e = etatVide();
    e.reference = 'r1';
    e.plans = [
      { ...nouveauPlan(0, 'r1'), prompt: 'A', idMontage: 'p0' },
      { ...nouveauPlan(1, 'r1', 'transition'), prompt: 'B', rush: 'r2' },
    ];
    const montage = plansPourMontage(e.plans, e.reference, [{ id: 'p0', seed: 7, idee: '', prompt: 'A', duree: 5, enchainement: 'rush', rush: 'r1', ambiance: true, clip: 'c', a_jour: true }]);
    expect(montage[0]).toMatchObject({ id: 'p0', seed: 7, rush: 'r1', idee: 'Plan 1' });
    expect(montage[1]).toMatchObject({ rush: 'r2', enchainement: 'transition' });
    expect(montage[1]?.id).toBeUndefined();
    const s: Sequence = {
      id: 's1',
      titre: 'Forêt',
      qualite: 'rapide',
      format: '9:16 (Vertical)',
      llm: null,
      modele: null,
      loras: [],
      film: null,
      casting: { personnage: null, voix: 'auto', ambiance: '', contrainte: 'pas de texte', reference_partout: true },
      plans: [{ id: 'p0', idee: 'Elle part', prompt: 'She leaves.', duree: 8, enchainement: 'rush', rush: 'r1', ambiance: true, clip: null, a_jour: false }],
      contrainte_en: { fr: 'pas de texte', en: 'no text' },
    };
    const revenu = depuisSequence(s, e);
    expect([revenu.sequenceId, revenu.reference, revenu.titre, revenu.plans.length]).toEqual(['s1', 'r1', 'Forêt', 2]);
    expect(etatPlan(revenu.plans[0]!)[0]).toBe('pret');
    expect(traductionValide(revenu)).toBe(true);
  });

  it('relit un brouillon abîmé sans planter', () => {
    expect(depuisBrouillon(null).plans).toHaveLength(3);
    const b = depuisBrouillon({ synopsis: 4, plans: [{ enchainement: 'nimporte' }], titre: 'T' });
    expect([b.synopsis, b.plans.length, b.titre]).toEqual(['', 3, 'T']);
    expect(depuisBrouillon({ plans: [{}, { moment: 'fin', duree: 10 }] }).plans[1]).toMatchObject({ moment: 'fin', duree: 10 });
  });
});

describe('plans déplacés, retirés, renumérotés', () => {
  it('avance, recule et retire sans rien perdre', () => {
    expect(deplacer(['a', 'b', 'c'], 1, -1)).toEqual(['b', 'a', 'c']);
    expect(deplacer(['a', 'b', 'c'], 1, 1)).toEqual(['a', 'c', 'b']);
    expect(deplacer(['a', 'b'], 0, -1)).toEqual(['a', 'b']);
    expect(deplacer(['a', 'b'], 1, 1)).toEqual(['a', 'b']);
    expect(retirer(['a', 'b', 'c'], 1)).toEqual(['a', 'c']);
  });

  it('fait suivre les alertes au plan qui change de rang', () => {
    const alertes = ['Plan 1 : trop long', 'Plan 2 : réplique inventée', 'Pas de plan'];
    // Le plan 2 passe en premier, le plan 1 en deuxième.
    const echange = (r: number) => (r === 0 ? 1 : r === 1 ? 0 : r);
    expect(alertesRenumerotees(alertes, echange)).toEqual(['Plan 2 : trop long', 'Plan 1 : réplique inventée', 'Pas de plan']);
    // Le plan 1 disparaît : le plan 2 devient le premier.
    const sans = (r: number) => (r === 0 ? null : r - 1);
    expect(alertesRenumerotees(alertes, sans)).toEqual(['Plan 1 : réplique inventée', 'Pas de plan']);
  });

  it('garde un départ au premier plan et des enchaînements aux autres', () => {
    const [a, b] = [nouveauPlan(0, null), nouveauPlan(1, null, 'prolonger')];
    // Un plan qui prolonge ne peut pas ouvrir la séquence ; celui du texte ne peut pas suivre.
    const inverse = corrigerEnchainements([b, a], null);
    expect(inverse.map((p) => p.enchainement)).toEqual(['texte', 'couper']);
    expect(corrigerEnchainements([b, a], 'r1')[0]?.enchainement).toBe('rush');
    // Rien ne change quand tout est en ordre (les plans sont rendus tels quels).
    const ordre = corrigerEnchainements([a, b], null);
    expect(ordre[0]).toBe(a);
    expect(ordre[1]).toBe(b);
    // Le rush d'arrivée ne survit qu'à une transition.
    const transition = { ...nouveauPlan(1, null, 'transition'), rush: 'r2' };
    expect(corrigerEnchainements([a, transition], null)[1]?.rush).toBe('r2');
    expect(corrigerEnchainements([{ ...transition, enchainement: 'couper' }], null)[0]?.rush).toBeNull();
  });
});
