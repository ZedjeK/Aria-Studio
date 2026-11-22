// Les commandes Rust que l'interface peut appeler (appli\src\commandes.rs, demarrage_auto.rs, icone.rs, moteurs.rs,
// plateau.rs), avec
// leurs types. Une commande ajoutée ici doit aussi l'être dans appli\build.rs et dans la capacité de la fenêtre
// qui l'appelle (appli\capabilities\principale.json, menu.json ou installation.json).
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Vitesse = 'lente' | 'normale' | 'rapide';

export interface ReglagesFond {
  actif: boolean;
  vitesse: Vitesse;
  minimum: number;
  maximum: number;
}

export interface Reglages {
  fond: ReglagesFond;
  proposerInstallation: boolean;
  chercherMisesAJour: boolean;
}

export interface Identite {
  nom: string;
  version: string;
  promesse: string;
  enFond: boolean;
  installable: boolean;
  dossier: string;
}

/** Place libre et taille du disque, en octets. */
export interface Espace {
  libre: number;
  total: number;
}

export const identite = (): Promise<Identite> => invoke<Identite>('identite');

export const lireReglages = (): Promise<Reglages> => invoke<Reglages>('lire_reglages');

/** Renvoie les réglages vraiment enregistrés : les bornes peuvent avoir été ramenées. */
export const enregistrerReglages = (reglages: Reglages): Promise<Reglages> =>
  invoke<Reglages>('enregistrer_reglages', { reglages });

export const mesurerDisque = (): Promise<Espace> => invoke<Espace>('mesurer_disque');

/** Comment une opération s'est terminée (journal::Resultat). */
export type Resultat = 'ok' | 'erreur' | 'partiel' | 'annule';

/** Une opération du journal (journal.json), au format de la v2. */
export interface Operation {
  /** Heure locale à la seconde, sans décalage : « 2026-11-14T10:12:03 ». */
  date: string;
  /** Ce qui a été fait. */
  action: string;
  /** Sur quoi ; vide s'il n'y a rien à préciser. */
  objet: string;
  resultat: Resultat;
  /** Une précision, absente le plus souvent. */
  detail?: string;
}

/** L'état d'un moteur sur le disque (aria_coeur::moteurs::Statut). */
export type StatutMoteur = 'absent' | 'manuel' | 'different' | 'a_jour' | 'externe';

/** Un moteur dans l'onglet Moteurs (aria_coeur::moteurs::EtatMoteur). */
export interface EtatMoteur {
  id: string;
  nom: string;
  role: string;
  facultatif: boolean;
  dossier: string;
  /** La version épinglée par le catalogue. */
  version: string;
  versionInstallee: string | null;
  statut: StatutMoteur;
  chemin: string;
  externe: boolean;
  /** Une version précédente attend à côté : « Revenir » est possible. */
  precedente: boolean;
  /** Octets encore à télécharger. */
  aTelecharger: number;
  /** Un programme écoute sur son port. */
  occupe: boolean;
  /** Ce qui manque pour l'installer (7-Zip pour un 7z). */
  prerequis: string | null;
}

/** La carte graphique (aria_coeur::carte::Carte). */
export interface Carte {
  nvidia: boolean;
  nom: string | null;
  pilote: string | null;
  vramGo: number | null;
  ramGo: number;
  ok: boolean;
  message: string | null;
}

/** L'installation en cours ou la dernière. */
export interface TacheMoteur {
  id: string;
  nom: string;
  etape: string;
  fait: number;
  total: number;
  erreur: string | null;
  fini: boolean;
}

export interface EtatMoteurs {
  carte: Carte;
  moteurs: EtatMoteur[];
  tache: TacheMoteur | null;
}

export const lireMoteurs = (): Promise<EtatMoteurs> => invoke<EtatMoteurs>('lire_moteurs');

/** Lance l'installation en arrière-plan ; son avancement se lit avec lireMoteurs. */
export const installerMoteur = (id: string): Promise<void> => invoke<void>('installer_moteur', { id });

/** Annule un téléchargement en cours (la partie reçue est gardée pour reprendre). */
export const annulerInstallationMoteur = (): Promise<void> => invoke<void>('annuler_installation_moteur');

/** Revient à la version précédente ; rend la version qui revient (null : posée à la main). */
export const revenirMoteur = (id: string): Promise<string | null> => invoke<string | null>('revenir_moteur', { id });

/** ComfyUI vide sa VRAM, llama-server s'arrête. */
export const libererCarte = (): Promise<void> => invoke<void>('liberer_carte');

/** Un workflow texte → image prêt (aria_coeur::plateau::ChoixWorkflow). */
export interface ChoixWorkflow {
  id: string;
  nom: string;
  famille: string;
  origine: string | null;
  /** Les checkpoints prêts à tourner. */
  modeles: string[];
  modeleDefaut: string | null;
  format: string | null;
  /** Les checkpoints présents dont les steps attendent d'être validés. */
  aValider: string[];
  stepsSuggeres: number[];
}

/** Un LLM proposé pour réécrire (aria_coeur::plateau::ChoixLlm). */
export interface ChoixLlm {
  fichier: string;
  nom: string;
  favori: boolean;
  /** Entraîné pour un modèle cible : il ne réécrit que pour lui. */
  specialiste: boolean;
}

/** Ce que le Plateau propose (aria_coeur::plateau::Options). */
export interface OptionsPlateau {
  formats: string[];
  nombreMax: number;
  defaut: string;
  workflows: ChoixWorkflow[];
  /** Les workflows auxquels il manque des fichiers, par leur nom. */
  incomplets: string[];
  /** Les LLM installés, les favoris d'abord. */
  llm: ChoixLlm[];
  /** Le LLM de chaque workflow prêt. */
  llmParWorkflow: Record<string, string>;
}

/** Une demande de création (aria_coeur::plateau::Demande). */
export interface DemandeImage {
  /** L'idée à réécrire, ou le prompt tel quel. */
  prompt: string;
  reecrire: boolean;
  /** Le GGUF qui réécrit ; null : celui du workflow. */
  llm: string | null;
  workflow: string | null;
  format: string;
  seed: number | null;
  nombre: number;
  modele: string | null;
}

/** Où en est une commande (aria_coeur::plateau::EtatCommande). */
export type EtatCommande = 'attente' | 'en_cours' | 'termine' | 'erreur' | 'annule';

/** Une commande du Plateau (aria_coeur::plateau::Commande). */
export interface Commande {
  id: string;
  /** Ce que l'utilisateur a écrit. */
  idee: string;
  /** Le prompt envoyé au modèle ; vide tant que le LLM ne l'a pas réécrit. */
  prompt: string;
  reecrire: boolean;
  llm: string | null;
  workflow: string;
  format: string;
  seed: number | null;
  nombre: number;
  modele: string | null;
  etat: EtatCommande;
  annulee: boolean;
  etape: string;
  /** Avancement de l'image en cours, de 0 à 1. */
  progression: number;
  /** Les créations enregistrées. */
  images: string[];
  erreur: string | null;
}

/** La fiche d'une création (data\creations\<id>.json, au format de la v2 : clés en snake_case). */
export interface Creation {
  id: string;
  /** Heure locale à la seconde : « 2026-11-22T14:03:05 ». */
  date: string;
  largeur: number;
  hauteur: number;
  type?: string;
  idee?: string | null;
  prompt?: string | null;
  llm?: string | null;
  workflow?: string | null;
  modele?: string | null;
  format?: string | null;
  seed?: number | null;
  duree_s?: number;
  /** Rangement des Rushes : dossier, archive, date de mise à la corbeille. */
  dossier?: string;
  archive?: boolean;
  corbeille?: string;
}

export const optionsPlateau = (): Promise<OptionsPlateau> => invoke<OptionsPlateau>('options_plateau');

/** Contrôle la demande (AS-18 compris) et la met en file. */
export const commanderImage = (demande: DemandeImage): Promise<Commande> =>
  invoke<Commande>('commander_image', { demande });

/** Les commandes de ce lancement, la plus récente en premier. */
export const lireCommandes = (): Promise<Commande[]> => invoke<Commande[]>('lire_commandes');

export const annulerCommande = (id: string): Promise<Commande> => invoke<Commande>('annuler_commande', { id });

/** Valide les steps d'un checkpoint : il devient utilisable dans les workflows de sa famille. */
export const validerCheckpoint = (workflow: string, checkpoint: string, steps: number): Promise<void> =>
  invoke<void>('valider_checkpoint', { workflow, checkpoint, steps });

/** Une vue des Rushes (aria_coeur::rushes::VUES). */
export type VueRushes = 'tous' | 'non_ranges' | 'archives' | 'corbeille';

/** Une action sur une sélection de rushes (aria_coeur::rushes::ACTIONS). */
export type ActionRushes = 'corbeille' | 'restaurer' | 'archiver' | 'desarchiver' | 'ranger';

/** Les compteurs des Rushes (aria_coeur::rushes::EtatRushes). */
export interface EtatRushes {
  tous: number;
  nonRanges: number;
  archives: number;
  corbeille: number;
  dossiers: { nom: string; nombre: number }[];
}

/** Les rushes d'une vue, ou d'un dossier (vue « tous »), les plus récents d'abord. */
export const lireRushes = (vue: VueRushes, dossier: string | null): Promise<Creation[]> =>
  invoke<Creation[]>('lire_rushes', { vue, dossier });

export const etatRushes = (): Promise<EtatRushes> => invoke<EtatRushes>('etat_rushes');

/** Applique une action à une sélection ; rend le nombre de rushes modifiés. `dossier` : pour « ranger » (null : Non rangés). */
export const agirRushes = (ids: string[], action: ActionRushes, dossier: string | null): Promise<number> =>
  invoke<number>('agir_rushes', { ids, action, dossier });

/** Supprime pour de bon les rushes de la corbeille ; rend leur nombre. */
export const viderCorbeille = (): Promise<number> => invoke<number>('vider_corbeille');

export const creerDossier = (nom: string): Promise<string> => invoke<string>('creer_dossier', { nom });

export const renommerDossier = (ancien: string, nouveau: string): Promise<string> =>
  invoke<string>('renommer_dossier', { ancien, nouveau });

/** Le dossier disparaît, pas ses images : elles retournent dans « Non rangés ». Rend leur nombre. */
export const supprimerDossier = (nom: string): Promise<number> => invoke<number>('supprimer_dossier', { nom });

/** Le PNG d'une création, en octets. */
export const imageCreation = (id: string): Promise<ArrayBuffer> => invoke<ArrayBuffer>('image_creation', { id });

/** Les opérations du journal, la plus récente d'abord. */
export const lireJournal = (): Promise<Operation[]> => invoke<Operation[]>('lire_journal');

/** Quitte l'appli, icône de notification comprise. */
export const quitter = (): Promise<void> => invoke<void>('quitter');

/** « Démarrer avec Windows », relu dans le registre à chaque appel. */
export const lireDemarrageAuto = (): Promise<boolean> => invoke<boolean>('lire_demarrage_auto');

/** Coche ou décoche « Démarrer avec Windows » ; renvoie l'état relu ensuite. */
export const reglerDemarrageAuto = (actif: boolean): Promise<boolean> =>
  invoke<boolean>('regler_demarrage_auto', { actif });

/** Couleur du point d'une ligne « État » du menu de l'icône. */
export type Ton = 'repos' | 'actif' | 'attention' | 'information';

export interface LigneMenu {
  ton: Ton;
  texte: string;
}

export interface EtatMenu {
  nom: string;
  lignes: LigneMenu[];
  demarrageAuto: boolean;
  /** La version disponible : « Voir la mise à jour » n'apparaît qu'avec elle. */
  miseAJour: string | null;
}

export type ActionMenu = 'ouvrir' | 'parametres' | 'miseAJour' | 'quitter';

export const etatMenu = (): Promise<EtatMenu> => invoke<EtatMenu>('etat_menu');

/** Le menu se range, puis l'action se fait. */
export const actionMenu = (action: ActionMenu): Promise<void> => invoke<void>('action_menu', { action });

/** La fenêtre du menu prend la hauteur de son contenu. */
export const tailleMenu = (hauteur: number): Promise<void> => invoke<void>('taille_menu', { hauteur });

export interface InfosInstallation {
  nom: string;
  version: string;
  /** Le dossier programme\ du dossier de données. */
  dossier: string;
  versionInstallee: string | null;
  /** Le raccourci du Bureau qui serait créé ; null si le Bureau est introuvable. */
  raccourci: string | null;
}

export const infosInstallation = (): Promise<InfosInstallation> => invoke<InfosInstallation>('infos_installation');

/** Installe, puis l'appli se relance depuis son dossier et ce lancement se ferme : seul un échec revient ici. */
export const installer = (raccourci: boolean): Promise<void> => invoke<void>('installer', { raccourci });

export const lancerSansInstaller = (nePlusProposer: boolean): Promise<void> =>
  invoke<void>('lancer_sans_installer', { nePlusProposer });

export interface InfosMiseAJour {
  nom: string;
  actuelle: string;
  version: string;
  /** Taille du zip, en octets. */
  taille: number;
}

/** La mise à jour trouvée au lancement ; null s'il n'y en a pas (ou pas encore). */
export const miseAJourDisponible = (): Promise<InfosMiseAJour | null> =>
  invoke<InfosMiseAJour | null>('mise_a_jour_disponible');

/** Ouvre la page de la release dans le navigateur. */
export const voirNotes = (): Promise<void> => invoke<void>('voir_notes');

/** Télécharge, vérifie et installe, puis l'appli se relance : seul un échec revient ici. */
export const installerMiseAJour = (): Promise<void> => invoke<void>('installer_mise_a_jour');

/**
 * Événements envoyés par l'appli : « etat-icone » (le menu relit son état), « aller-a » (page ou dialogue à
 * montrer : « parametres », « mise-a-jour »), « mise-a-jour » (version disponible).
 */
export const ecouter = <T>(
  evenement: 'etat-icone' | 'aller-a' | 'mise-a-jour',
  rappel: (charge: T) => void,
): Promise<UnlistenFn> =>
  listen<T>(evenement, (e) => rappel(e.payload));

/** Le message d'une erreur renvoyée par une commande Rust (du texte) ou par le navigateur. */
export function messageErreur(erreur: unknown): string {
  if (typeof erreur === 'string') return erreur;
  if (erreur instanceof Error) return erreur.message;
  return 'erreur inconnue';
}
