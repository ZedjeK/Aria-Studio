import { describe, expect, it } from 'vitest';
import type { PlanEcrit, Sequence } from './api';
import {
  alertesJusqua,
  depuisBrouillon,
  depuisSequence,
  etatPlan,
  etatVide,
  majAlertes,
  motsReplique,
  nouveauPlan,
  plansDemande,
  plansPourMontage,
  poserPlan,
  redimensionner,
  remplisAuDela,
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
