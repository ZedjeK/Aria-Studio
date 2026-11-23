// L'onglet Moteurs : les textes, d'après l'état que rend le cœur.

import type { Carte, EtatMoteur, StatutMoteur, TacheMoteur } from './api';
import { octetsLisibles } from './jauge';

const LIBELLES: Record<StatutMoteur, string> = {
  absent: 'absent',
  manuel: 'installé à la main',
  different: 'autre version',
  a_jour: 'à jour',
  externe: 'ton ComfyUI',
};

/** Le statut d'un moteur, en mots. Un moteur pris ailleurs que chez Aria : « ton ComfyUI », « ton llama.cpp »… */
export function libelleStatut(statut: StatutMoteur, nom?: string): string {
  if (statut === 'externe' && nom) return `ton ${nom.split(' (')[0]}`;
  return LIBELLES[statut];
}

/** Le bouton d'installation d'un moteur, ou rien s'il est à jour ou si c'est le ComfyUI de l'utilisateur. */
export function texteInstaller(moteur: EtatMoteur): string | null {
  if (moteur.statut === 'absent') return 'Installer';
  if (moteur.statut === 'manuel' || moteur.statut === 'different') return `Installer la ${moteur.version}`;
  return null;
}

/** Ce que l'installation va faire, pour la confirmer. */
export function texteConfirmation(moteur: EtatMoteur): string {
  const source =
    moteur.aTelecharger > 0
      ? `Téléchargement : ${octetsLisibles(moteur.aTelecharger)} depuis sa release GitHub officielle, empreinte SHA-256 vérifiée.`
      : 'Les archives sont déjà là : rien à télécharger.';
  return `${source} Le moteur est arrêté le temps de l'échange ; la version actuelle reste à côté (Revenir).`;
}

/** La carte graphique en une ligne, ou ce qui ne va pas. */
export function texteCarte(carte: Carte): string {
  if (!carte.nvidia || !carte.ok) return carte.message ?? 'Carte graphique non vérifiée.';
  const morceaux: string[] = [];
  if (carte.nom) morceaux.push(carte.nom);
  if (carte.pilote) morceaux.push(`pilote ${carte.pilote}`);
  if (carte.vramGo !== null) morceaux.push(`${carte.vramGo} Go de VRAM`);
  if (carte.ramGo > 0) morceaux.push(`${carte.ramGo} Go de RAM`);
  return morceaux.join(' · ');
}

/** L'avancement d'une installation, de 0 à 1 ; rien quand l'étape n'a pas de total (extraction, mise en place). */
export function fractionTache(tache: TacheMoteur): number | null {
  return tache.total > 0 ? Math.min(1, tache.fait / tache.total) : null;
}

/** Le texte à côté de la jauge : « 412,5 Mo sur 1,9 Go ». */
export function mesureTache(tache: TacheMoteur): string | undefined {
  return tache.total > 0 ? `${octetsLisibles(tache.fait)} sur ${octetsLisibles(tache.total)}` : undefined;
}

/** Les moteurs obligatoires qui manquent : sans eux, Aria ne peut pas générer. */
export function manquants(moteurs: EtatMoteur[]): EtatMoteur[] {
  return moteurs.filter((m) => m.statut === 'absent' && !m.facultatif);
}
