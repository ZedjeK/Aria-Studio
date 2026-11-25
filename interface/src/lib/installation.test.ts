import { describe, expect, it } from 'vitest';
import { libelleRaccourci, texteDemarrage, texteInstallee, type EtatInstallation } from './installation';

const base: EtatInstallation = {
  version: '3.0.0',
  versionInstallee: null,
  version2Installee: false,
  dossierV2: 'C:\\Users\\moi\\.Aria-Studio\\programme.precedent',
  raccourciExiste: false,
  demarrageActif: false,
};

describe('proposition d’installation', () => {
  it('ne dit rien quand rien n’est installé', () => {
    expect(texteInstallee(base)).toBeNull();
  });

  it('dit qu’une version 3 est remplacée et gardée dans les sauvegardes', () => {
    const texte = texteInstallee({ ...base, versionInstallee: '2.9.0' });
    expect(texte).toContain('La version 2.9.0 déjà installée sera remplacée par la 3.0.0');
    expect(texte).toContain('sauvegardes');
  });

  it('dit qu’une version 2 est gardée entière, où, et qu’il faut la fermer', () => {
    const texte = texteInstallee({ ...base, version2Installee: true });
    expect(texte).toContain("La version 2 d'Aria Studio");
    expect(texte).toContain(base.dossierV2);
    expect(texte).toContain('gardée entière');
    expect(texte).toContain('Ferme la version 2');
  });

  it('propose de mettre à jour un raccourci déjà là, sinon d’en créer un', () => {
    expect(libelleRaccourci(false)).toBe('Créer un raccourci sur le Bureau');
    expect(libelleRaccourci(true)).toContain('Mettre à jour');
    expect(libelleRaccourci(true)).toContain('existe déjà');
  });

  it('annonce que le démarrage avec Windows garde son nom et change de cible, seulement s’il est coché', () => {
    expect(texteDemarrage(base)).toBeNull();
    const texte = texteDemarrage({ ...base, demarrageActif: true });
    expect(texte).toContain('« Aria Studio »');
    expect(texte).toContain('visera');
  });
});
