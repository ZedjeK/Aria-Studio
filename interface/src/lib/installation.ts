// Les phrases de la proposition d'installation : ce qui sera remplacé, gardé ou visé.
import type { InfosInstallation } from './api';

/** Ce que la proposition sait de l'installation en place et de ce qui l'entoure (sous-ensemble d'`InfosInstallation`). */
export type EtatInstallation = Pick<
  InfosInstallation,
  'version' | 'versionInstallee' | 'version2Installee' | 'dossierV2' | 'raccourciExiste' | 'demarrageActif'
>;

/**
 * Ce qui arrive à ce qui occupe déjà le dossier d'installation : une version 3 est remplacée et gardée dans les
 * sauvegardes ; une version 2 est gardée entière à côté (son Python, son interface : on peut la remettre). `null` :
 * rien n'est installé.
 */
export function texteInstallee(infos: EtatInstallation): string | null {
  if (infos.version2Installee) {
    return (
      `La version 2 d'Aria Studio est installée ici : elle est gardée entière dans ${infos.dossierV2}, ` +
      `et la version ${infos.version} prend sa place. Ferme la version 2 avant d'installer (icône de la barre des tâches, « Quitter »).`
    );
  }
  if (infos.versionInstallee) {
    return `La version ${infos.versionInstallee} déjà installée sera remplacée par la ${infos.version}, et gardée dans les sauvegardes.`;
  }
  return null;
}

/** La case du raccourci : un raccourci déjà sur le Bureau est proposé en mise à jour, jamais remplacé sans elle. */
export function libelleRaccourci(existe: boolean): string {
  return existe ? 'Mettre à jour le raccourci du Bureau (il existe déjà)' : 'Créer un raccourci sur le Bureau';
}

/** La ligne sur « Démarrer avec Windows » quand il est coché : même entrée, nouvelle cible. `null` : décoché. */
export function texteDemarrage(infos: EtatInstallation): string | null {
  return infos.demarrageActif
    ? `Le démarrage avec Windows est coché : son entrée « Aria Studio » gardera son nom et visera l'exe installé.`
    : null;
}
