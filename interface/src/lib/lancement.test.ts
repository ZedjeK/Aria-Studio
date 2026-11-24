import { describe, expect, it } from 'vitest';
import { lancementDuScenario } from './plateau';

describe('Créer depuis le Scénario', () => {
  it('lance la création quand tout est prêt', () => {
    expect(lancementDuScenario(true, 1)).toEqual({ lancer: true, message: 'Prompt reçu : la création est lancée.' });
  });

  it('pose seulement le prompt quand il manque quelque chose', () => {
    const manque = lancementDuScenario(false, 1);
    expect(manque.lancer).toBe(false);
    expect(manque.message).toContain('la création attend');
  });

  it('demande un clic quand plusieurs images sont demandées', () => {
    const plusieurs = lancementDuScenario(true, 4);
    expect(plusieurs.lancer).toBe(false);
    expect(plusieurs.message).toContain('4 images');
  });
});
