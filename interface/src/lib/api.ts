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

/** La VRAM de la carte à l'instant, en Go (aria_coeur::carte::Vram) : tous les programmes y comptent. */
export interface Vram {
  utiliseeGo: number;
  totaleGo: number;
}

/** La VRAM utilisée et totale (NVML) ; null sans carte NVIDIA ou sans lecture : rien ne se suppose. */
export const lireVram = (): Promise<Vram | null> => invoke<Vram | null>('lire_vram');

/** « J'ai déjà ComfyUI » : ce dossier portable, vérifié par un lancement d'essai ; null : revenir à celui d'Aria. */
export const choisirComfyui = (chemin: string | null): Promise<void> => invoke<void>('choisir_comfyui', { chemin });

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
  /** Le nombre d'images de référence qu'il reçoit (S'inspirer ; 0 : aucune). */
  references: number;
  /** Il sait redessiner une image. */
  variation: boolean;
  /** Réservé à la Retouche. */
  reserveRetouche: boolean;
  /** Les LoRA de sa famille. */
  loras: LoraProposee[];
  /** Les rubriques que ce modèle n'a pas à se faire demander : le guide ne pose pas leur question. */
  rubriquesIgnorees: string[];
}

/** Une LoRA de la famille d'un workflow, avec ses mots et sa force retenus (aria_coeur::plateau::LoraProposee). */
export interface LoraProposee {
  fichier: string;
  nom: string;
  declencheurs: string;
  force: number;
  page: string | null;
}

/** Une LoRA cochée (aria_coeur::plateau::ChoixLora) : ses mots déclencheurs vont en tête du prompt. */
export interface ChoixLora {
  fichier: string;
  force: number;
  declencheurs: string;
}

/** Un LLM proposé pour réécrire (aria_coeur::plateau::ChoixLlm). */
export interface ChoixLlm {
  fichier: string;
  nom: string;
  favori: boolean;
  /** Entraîné pour un modèle cible : il ne réécrit que pour lui. */
  specialiste: boolean;
  /** Il voit les images (un projecteur de sa famille est installé, la vision n'est pas coupée). */
  vision: boolean;
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
  /** Le workflow de « S'inspirer » par défaut. */
  defautReferences: string;
  referencesMax: number;
  /** La force d'une LoRA, au plus. */
  loraForceMax: number;
}

/** Une demande de création (aria_coeur::plateau::Demande). */
export interface DemandeImage {
  /** L'idée à réécrire, ou le prompt tel quel. */
  prompt: string;
  reecrire: boolean;
  /** Le GGUF qui réécrit ; null : celui du workflow. */
  llm: string | null;
  /** Des rushes de départ : à mélanger (S'inspirer) ou à redessiner (un seul). */
  references: string[];
  modeReference: ModeReference | null;
  /** Redessiner : la liberté, de 0,05 (copie) à 1 (libre). */
  liberte: number;
  workflow: string | null;
  format: string;
  seed: number | null;
  nombre: number;
  modele: string | null;
  loras: ChoixLora[];
  /** Le dossier où ranger les résultats (« Ranger dans ») ; null : non rangés. */
  dossier?: string | null;
  /** « Vérifier les images à la fin » : le LLM qui voit compare chaque image à l'intention. */
  verifier?: boolean;
}

/** Partir de rushes : s'en inspirer, ou en redessiner un. */
export type ModeReference = 'inspirer' | 'variation';

/** Où en est une commande (aria_coeur::plateau::EtatCommande). */
export type EtatCommande = 'attente' | 'en_cours' | 'termine' | 'erreur' | 'annule';

/** Une commande du Plateau (aria_coeur::plateau::Commande). */
/** Ce que fait une commande du Plateau. */
export type GenreCommande = 'creation' | 'retouche' | 'agrandissement' | 'clip' | 'voix' | 'sequence' | 'casting';

export interface Commande {
  id: string;
  type: GenreCommande;
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
  /** L'image de départ d'une retouche ou d'un agrandissement. */
  parent: string | null;
  masque: boolean;
  fondu: number;
  largeur: number | null;
  hauteur: number | null;
  ancrage: string | null;
  cadrage: string | null;
  references: string[];
  modeReference: ModeReference | null;
  liberte: number;
  loras: ChoixLora[];
  /** Montage : animer (un rush) ou texte, la résolution et la durée. */
  modeVideo: 'animer' | 'texte' | null;
  qualite: string | null;
  duree: number | null;
  /** Essai de voix : qui parle, et comment. */
  description: string;
  /** Séquence du Montage : son identifiant, et les plans à tourner. */
  sequence: string | null;
  plans: number[];
  /** Le dossier où ranger les résultats ; null : non rangés. */
  dossier: string | null;
  /** La réécriture (prompt redemandé, meilleur essai imparfait, repli sur l'idée), puis le bilan de la vérification. */
  note: string | null;
  /** Les images seront vérifiées par le LLM qui voit, après la génération. */
  verifier: boolean;
  /** Casting : son identifiant, et les générations faites (sur `nombre`). */
  banc: string | null;
  fait: number;
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
  /** Ce que le LLM a écrit au premier essai, avant le nettoyage et le contrôle qualité ; absent des fiches de la v2. */
  prompt_llm?: string | null;
  /** Le prompt réellement envoyé au modèle (mots déclencheurs des LoRA compris) ; absent des fiches plus anciennes. */
  prompt_envoye?: string | null;
  llm?: string | null;
  /** Ce qui s'est passé à la réécriture de l'idée : `faite`, `redemandee`, `imparfaite` ou `repli`, et pourquoi. */
  reecriture?: string | null;
  reecriture_motif?: string | null;
  workflow?: string | null;
  modele?: string | null;
  format?: string | null;
  seed?: number | null;
  loras?: ChoixLora[];
  /** Un clip du Montage : `animer`, `texte`, un plan de `sequence` ou le `film` assemblé. */
  mode_video?: string | null;
  /** La séquence d'un plan ou d'un film. */
  sequence?: string | null;
  /** La source d'une retouche, ou le rush animé d'un clip. */
  parent?: string | null;
  /** Un clip : sa résolution, sa durée en secondes, et le format auquel son rush a été recadré. */
  qualite?: string | null;
  duree?: number | null;
  cadrage?: string | null;
  duree_s?: number;
  /** Rangement des Rushes : dossier, archive, date de mise à la corbeille. */
  dossier?: string;
  archive?: boolean;
  corbeille?: string;
  /** Vérifiée par le LLM qui voit (Vérifier l'image). */
  verification?: Verification | null;
  /** Retenue parmi les meilleures d'un lot (Choisir les meilleures). */
  meilleure?: Meilleure | null;
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

/** « Celui de l'image » : la source d'une retouche garde son format (aria_coeur::retouche::FORMAT_IMAGE). */
export const FORMAT_IMAGE = 'image source';

/** Une demande de retouche (aria_coeur::retouche::DemandeRetouche). */
export interface DemandeRetouche {
  source: string;
  instruction: string;
  reecrire: boolean;
  llm: string | null;
  workflow: string | null;
  /** Le masque peint, en data URL PNG ; null : toute l'image. */
  masque: string | null;
  fondu: number;
  seed: number | null;
  modele: string | null;
  nombre: number;
  /** Un format, qui recadre la source au centre ; null : celui de l'image. */
  format: string | null;
  loras: ChoixLora[];
  /** Le dossier où ranger les résultats (« Ranger dans ») ; null : à côté de la source. */
  dossier?: string | null;
}

/** Une demande d'agrandissement (aria_coeur::retouche::DemandeAgrandissement). */
export interface DemandeAgrandissement {
  source: string;
  largeur: number;
  hauteur: number;
  ancrage: string;
  precision: string;
  reecrire: boolean;
  llm: string | null;
  workflow: string | null;
  fondu: number;
  seed: number | null;
  modele: string | null;
  nombre: number;
  loras: ChoixLora[];
  /** Le dossier où ranger les résultats (« Ranger dans ») ; null : à côté de la source. */
  dossier?: string | null;
}

/** Les workflows de retouche, les formats et les LLM. */
export const optionsRetouche = (): Promise<OptionsPlateau> => invoke<OptionsPlateau>('options_retouche');

export const commanderRetouche = (demande: DemandeRetouche): Promise<Commande> =>
  invoke<Commande>('commander_retouche', { demande });

export const commanderAgrandissement = (demande: DemandeAgrandissement): Promise<Commande> =>
  invoke<Commande>('commander_agrandissement', { demande });

/** Ajoute une image personnelle (data URL) aux créations ; rend sa fiche. */
export const importerImage = (image: string, nom: string, dossier: string | null = null): Promise<Creation> =>
  invoke<Creation>('importer_image', { image, nom, dossier });

/** La licence d'un modèle (aria_coeur::installations::Licence). */
export interface LicenceModele {
  nom: string;
  url: string;
  resume: string;
  commercial: boolean;
}

export interface FichierModele {
  nom: string;
  dossier: string;
  taille: number;
  present: boolean;
  /** Absent et exigé : il reste à télécharger (le checkpoint d'origine n'est pas exigé si la famille en a un autre). */
  manque: boolean;
}

/** Un workflow livré et ce qui lui manque (aria_coeur::installations::EtatWorkflow). */
export interface WorkflowModeles {
  id: string;
  nom: string;
  tache: string;
  famille: string;
  licence: LicenceModele;
  fichiers: FichierModele[];
  aTelecharger: number;
  pret: boolean;
}

/** Un LLM du catalogue (aria_coeur::installations::EtatLlm). */
export interface LlmCatalogue {
  fichier: string;
  nom: string;
  page: string;
  specialiste: 'creation' | 'retouche' | null;
  vision: boolean;
  licence: LicenceModele;
  taille: number;
  aTelecharger: number;
  installe: boolean;
}

/** Un téléchargement (aria_coeur::telechargement::Tache). */
export interface Telechargement {
  id: string;
  nom: string;
  dossier: string;
  taille: number;
  etat: 'attente' | 'en_cours' | 'verification' | 'termine' | 'erreur' | 'annule';
  recu: number;
  /** Octets par seconde. */
  debit: number;
  erreur: string | null;
}

export interface EtatModeles {
  workflows: WorkflowModeles[];
  llm: LlmCatalogue[];
  taches: Telechargement[];
}

export const etatModeles = (): Promise<EtatModeles> => invoke<EtatModeles>('etat_modeles');

export const lireTelechargements = (): Promise<Telechargement[]> => invoke<Telechargement[]>('lire_telechargements');

/** Met en file ce qui manque à un workflow ; rend le nombre de fichiers ajoutés. */
export const telechargerWorkflow = (id: string): Promise<number> => invoke<number>('telecharger_workflow', { id });

/** Met en file un LLM du catalogue ; rend le nombre de fichiers ajoutés. */
export const installerLlm = (fichier: string): Promise<number> => invoke<number>('installer_llm', { fichier });

export const annulerTelechargement = (id: string): Promise<Telechargement> =>
  invoke<Telechargement>('annuler_telechargement', { id });

// --- Modèles : Ma collection et la corbeille ---

export interface ReglageLora {
  declencheurs: string;
  force: number;
  regle: boolean;
  declencheursSource: string;
  civitai: boolean;
}

export interface StepsDits {
  valeur: unknown;
  origine: string;
  source: string | null;
}

export interface ElementCollection {
  cle: string;
  nom: string;
  dossier: string;
  famille: string | null;
  taille: number;
  origine: 'aria' | 'retrouve' | 'main';
  source: string | null;
  page: string | null;
  licence: string | null;
  declencheurs: string[];
  utilisePar: string[];
  steps: number | null;
  aValider: StepsDits[] | null;
  lora: ReglageLora | null;
}

export interface GroupeCollection {
  id: string;
  nom: string;
  elements: ElementCollection[];
  taille: number;
}

export interface Collection {
  groupes: GroupeCollection[];
  orphelins: number;
}

export interface ModeleALaCorbeille {
  cle: string;
  nom: string;
  taille: number;
  present: boolean;
  date: string | null;
  dossierModeles: string;
  ailleurs: boolean;
}

export interface CorbeilleModeles {
  modeles: ModeleALaCorbeille[];
  taille: number;
}

export const collectionModeles = (): Promise<Collection> => invoke<Collection>('collection_modeles');

export const validerStepsModele = (cle: string, steps: number): Promise<void> =>
  invoke<void>('valider_steps_modele', { cle, steps });

export const reglerLora = (cle: string, declencheurs: string, force: number): Promise<ReglageLora> =>
  invoke<ReglageLora>('regler_lora', { cle, declencheurs, force });

export const oublierReglageLora = (cle: string): Promise<ReglageLora> =>
  invoke<ReglageLora>('oublier_reglage_lora', { cle });

/** Relit sur Civitai les mots déclarés et la force d'une LoRA qui en vient. */
export const relireLora = (cle: string): Promise<ReglageLora> => invoke<ReglageLora>('relire_lora', { cle });

export const jeterModele = (cle: string): Promise<void> => invoke<void>('jeter_modele', { cle });

export const corbeilleModeles = (): Promise<CorbeilleModeles> => invoke<CorbeilleModeles>('corbeille_modeles');

export const restaurerModele = (cle: string): Promise<void> => invoke<void>('restaurer_modele', { cle });

export const viderCorbeilleModeles = (): Promise<{ effaces: number; liberes: number }> =>
  invoke<{ effaces: number; liberes: number }>('vider_corbeille_modeles');

// --- Modèles : Rechercher (aria_coeur::decouvrir) ---

export type TypeRecherche = 'image' | 'video' | 'lora' | 'llm';
export type SourceRecherche = 'civitai' | 'huggingface';

export interface FamilleDecouvrir {
  famille: string;
  nature: string;
  bases: string[];
}

export interface DemandeRecherche {
  source: SourceRecherche;
  type: TypeRecherche;
  texte: string;
  famille: string | null;
  tri: string;
  curseur: string | null;
  adulte: boolean;
}

export interface ApercuModele {
  url: string;
  nsfw: boolean;
}

export interface FichierTrouve {
  nom: string;
  taille: number;
  principal: boolean;
  installe: boolean;
  lien: string | null;
  dossier: string | null;
  vram: 'tient' | 'juste' | 'deborde' | null;
}

export interface VersionTrouvee {
  id: number;
  nom: string | null;
  base: string | null;
  famille: string | null;
  motsDeclencheurs: string[];
  fichiers: FichierTrouve[];
  lien: string;
  dossier: string;
  apercu: ApercuModele | null;
}

export interface ModeleTrouve {
  source: SourceRecherche;
  id: string;
  nom: string;
  auteur: string | null;
  type: TypeRecherche;
  famille: string | null;
  installe: boolean;
  telechargements: number;
  appreciations: number;
  date: string | null;
  page: string;
  protege: boolean;
  apercu: ApercuModele | null;
  versions: VersionTrouvee[];
}

export interface Recherche {
  resultats: ModeleTrouve[];
  curseur: string | null;
}

export interface FichiersDepot {
  fichiers: FichierTrouve[];
  vramLibreGo: number | null;
  vramTotaleGo: number | null;
}

/** La fiche d'un fichier distant (aria_coeur::telechargement::Fiche, champs de la v2). */
export interface FicheModele {
  source: string;
  nom: string;
  taille: number;
  sha256: string | null;
  url: string;
  page: string;
  licence: string | null;
  mots_declencheurs: string[];
  base: string | null;
  force_auteur: number | null;
  images_force: number;
  prompt_exemple: string | null;
  steps_suggeres: { statut: string; valeurs: number[] } | null;
  cfg_suggere: { statut: string; valeurs: number[] } | null;
}

export interface PropositionLien {
  fiche: FicheModele;
  dossier: string | null;
  jetonRequis: boolean;
}

export const famillesDecouvrir = (genre: TypeRecherche): Promise<FamilleDecouvrir[]> =>
  invoke<FamilleDecouvrir[]>('familles_decouvrir', { genre });

export const chercherModeles = (demande: DemandeRecherche): Promise<Recherche> =>
  invoke<Recherche>('chercher_modeles', { demande });

export const fichiersDepot = (depot: string, genre: TypeRecherche, famille: string | null): Promise<FichiersDepot> =>
  invoke<FichiersDepot>('fichiers_depot', { depot, genre, famille });

export const preparerLien = (lien: string): Promise<PropositionLien> =>
  invoke<PropositionLien>('preparer_lien', { lien });

/** Le clic vaut confirmation : la fiche est relue à la source, puis mise en file dans ce dossier. */
export const telechargerLien = (lien: string, dossier: string): Promise<Telechargement> =>
  invoke<Telechargement>('telecharger_lien', { lien, dossier });

/** Une vignette de Civitai, en octets (l'interface ne charge rien d'ailleurs). */
export const vignetteModele = (url: string): Promise<ArrayBuffer> => invoke<ArrayBuffer>('vignette_modele', { url });

/** Où en est « Retrouver les sources » (aria_coeur::retrouver::Identification). */
export interface IdentificationSources {
  etat: 'repos' | 'en_cours' | 'termine';
  total: number;
  fait: number;
  fichier: string | null;
  phase: 'empreinte' | 'civitai' | null;
  trouves: { nom: string; ou: string }[];
  introuvables: string[];
  erreurs: { nom: string; erreur: string }[];
}

export const retrouverSources = (): Promise<IdentificationSources> =>
  invoke<IdentificationSources>('retrouver_sources');

export const etatSources = (): Promise<IdentificationSources> => invoke<IdentificationSources>('etat_sources');

// --- Modèles : onglets d'usage (aria_coeur::usages) ---

export interface SpecialisteUsage {
  fichier: string;
  nom: string;
  page: string;
  usage: string;
  taille: number;
  aTelecharger: number;
  installe: boolean;
  choisi: boolean;
  vision: boolean;
  licence: LicenceModele;
}

export interface ReecritureUsage {
  llm: string | null;
  effectif: string;
  style: string;
  langue: string;
  motsMax: number | null;
  regles: number;
  specialistes: SpecialisteUsage[];
  aVenir: { nom: string; page: string; raison: string }[];
  ailleurs: string[] | null;
}

export interface WorkflowUsage {
  id: string;
  nom: string;
  tache: string;
  famille: string;
  dossierModeles: string;
  dossierLoras: string;
  licence: LicenceModele;
  vramEstimeeGo: number | null;
  vramMinGo: number | null;
  taille: number;
  pret: boolean;
  manquants: { dossier: string; nom: string }[];
  aTelecharger: number;
  modeles: string[];
  aValider: string[];
  origine: string | null;
  modeleDefaut: string | null;
  loras: { nom: string; declencheurs: string; force: number }[];
  possibles: string[];
  favoriPour: string[];
  references: number;
  variation: boolean;
  reecriture: ReecritureUsage;
}

export interface UsageModeles {
  id: string;
  nom: string;
  workflows: WorkflowUsage[];
  favoris: { cle: string; libelle: string; valeur: string | null }[];
  variation: string[];
}

export interface Usages {
  llmDefaut: string;
  llmChoix: ChoixLlm[];
  usages: UsageModeles[];
}

export const usagesModeles = (): Promise<Usages> => invoke<Usages>('usages_modeles');

export const definirFavori = (cle: string, workflow: string): Promise<void> =>
  invoke<void>('definir_favori', { cle, workflow });

/** Le LLM qui réécrit pour ce workflow ; null : le LLM par défaut. */
export const definirLlmWorkflow = (workflow: string, llm: string | null): Promise<void> =>
  invoke<void>('definir_llm_workflow', { workflow, llm });

/** Un dossier d'Aria (aria_coeur::emplacements::DossierConnu). */
export interface DossierConnu {
  cle: string;
  nom: string;
  chemin: string;
  existe: boolean;
}

/** Réglages → Emplacements : les dossiers, et le choix du dossier des modèles. */
export interface EtatLieux {
  portable: boolean;
  dossiers: DossierConnu[];
  modelesChoisi: string | null;
  modelesDefaut: string;
  /** Le dossier des modèles du prochain lancement. */
  modelesProchain: string;
  /** Un choix est fait, pris en compte au prochain lancement. */
  redemarrage: boolean;
  /** La place libre sur le disque de chaque dossier, en octets. */
  libres: Record<string, number | null>;
}

/** D'où vient la clé d'une source : jamais la clé elle-même. */
export interface EtatCle {
  source: string;
  nom: string;
  variable: string;
  aide: string;
  origine: 'aria' | 'variable' | null;
}

export interface TestCle {
  ok: boolean;
  message: string;
}

export const etatEmplacements = (): Promise<EtatLieux> => invoke<EtatLieux>('etat_emplacements');

/** Un dossier d'une v2 repris pour un essai portable (aria_coeur::reprise::Element). */
export interface ElementRepris {
  /** `modeles`, `comfyui`, `llama` ou `llama-prism`. */
  cle: string;
  nom: string;
  chemin: string;
}

/** Ce qui a été repris d'une v2 installée (aria_coeur::reprise::Bilan). */
export interface Bilan {
  depuis: string;
  date: string;
  repris: ElementRepris[];
  /** Le bandeau du premier lancement a été vu. */
  annonce: boolean;
  /** Les clés reprises qui valent encore (ni changées ni retirées depuis). */
  enVigueur: string[];
}

export interface EtatReprise {
  bilan: Bilan | null;
  /** Une v2 installée tournait au lancement : rien n'a été repris. */
  v2Lancee: boolean;
}

export const etatRepriseV2 = (): Promise<EtatReprise> => invoke<EtatReprise>('etat_reprise_v2');

/** Le bandeau de la reprise a été vu : il ne revient plus. */
export const repriseV2Vue = (): Promise<void> => invoke<void>('reprise_v2_vue');

/** Ne plus utiliser les dossiers de la v2 (au prochain lancement) ; rend les clés retirées. */
export const annulerRepriseV2 = (): Promise<string[]> => invoke<string[]>('annuler_reprise_v2');

/** Ouvre un dossier connu dans l'Explorateur (`sous` : un sous-dossier connu des modèles). */
export const ouvrirDossier = (cle: string, sous: string | null = null): Promise<void> =>
  invoke<void>('ouvrir_dossier', { cle, sous });

/** Le sélecteur de dossier natif ; null : l'utilisateur a renoncé. */
export const parcourirDossier = (titre: string, depart: string | null): Promise<string | null> =>
  invoke<string | null>('parcourir_dossier', { titre, depart });

/** Le dossier des modèles du prochain lancement ; null : celui par défaut. */
export const choisirDossierModeles = (chemin: string | null): Promise<EtatLieux> =>
  invoke<EtatLieux>('choisir_dossier_modeles', { chemin });

export const etatCles = (): Promise<EtatCle[]> => invoke<EtatCle[]>('etat_cles');

export const enregistrerCle = (sourceId: string, valeur: string): Promise<EtatCle[]> =>
  invoke<EtatCle[]>('enregistrer_cle', { sourceId, valeur });

export const supprimerCle = (sourceId: string): Promise<EtatCle[]> => invoke<EtatCle[]>('supprimer_cle', { sourceId });

/** Présente la clé à sa source, qui dit à quel compte elle appartient. */
export const testerCle = (sourceId: string): Promise<TestCle> => invoke<TestCle>('tester_cle', { sourceId });

/** Un réglage fin (aria_coeur::reglages_fins::Param). */
export interface ParamReglage {
  cle: string;
  libelle: string;
  type: 'entier' | 'reel' | 'booleen' | 'choix';
  defaut: number | boolean | string;
  valeur: number | boolean | string;
  modifie: boolean;
  min: number | null;
  max: number | null;
  pas: number | null;
  choix: (string | number)[];
  aide: string;
  /** Les libellés des choix qui ne parlent pas d'eux-mêmes. */
  libelles: Record<string, string>;
}

/** Une consigne d'usage d'un LLM : celle d'origine, celle en vigueur. */
export interface ConsigneLlm {
  cle: string;
  defaut: string;
  valeur: string;
  modifie: boolean;
}

export interface EtatLlm {
  fichier: string;
  nom: string;
  params: ParamReglage[];
  consignes: ConsigneLlm[];
}

export interface EtatVariante {
  fichier: string;
  origine: boolean;
  params: ParamReglage[];
}

export interface EtatWorkflow {
  id: string;
  nom: string;
  tache: string;
  famille: string;
  dossierModeles: string;
  params: ParamReglage[];
  variantes: EtatVariante[];
}

/** Réglages → LLM et workflows (aria_coeur::reglages_fins::EtatReglages). */
export interface EtatReglages {
  llm: EtatLlm[];
  defautLlm: string;
  noteDefaut: string | null;
  clauseMineurs: string;
  workflows: EtatWorkflow[];
  samplersDeComfyui: boolean;
}

/** Une valeur à enregistrer ; null revient à celle d'origine. */
export type Changements = Record<string, number | boolean | string | null>;

export const etatReglages = (): Promise<EtatReglages> => invoke<EtatReglages>('etat_reglages');

export const modifierReglagesLlm = (
  fichier: string,
  changements: Changements,
  consignes: Record<string, string | null>,
): Promise<EtatLlm> => invoke<EtatLlm>('modifier_reglages_llm', { fichier, changements, consignes });

export const modifierReglagesWorkflow = (id: string, changements: Changements): Promise<EtatWorkflow> =>
  invoke<EtatWorkflow>('modifier_reglages_workflow', { id, changements });

export const modifierReglagesVariante = (id: string, fichier: string, changements: Changements): Promise<EtatWorkflow> =>
  invoke<EtatWorkflow>('modifier_reglages_variante', { id, fichier, changements });

/** Un réglage qui changerait (aria_coeur::optimiseur::Proposition) : rien n'est appliqué sans l'accord de l'utilisateur. */
export interface Proposition {
  type: string;
  /** Le GGUF ou le workflow. */
  cible: string;
  nom: string;
  cle: string;
  libelle: string;
  actuel: number | boolean | string | null;
  propose: number | boolean | string | null;
  raison: string;
  source: string | null;
}

/** La machine, telle que l'onglet Réglages la résume (aria_coeur::optimiseur::Materiel). */
export interface Materiel {
  gpu: string;
  vramTotaleGo: number;
  vramAutresGo: number;
  vramLibreGo: number;
  ramGo: number;
  coeurs: number | null;
  threads: number | null;
}

/** « Recommander » pour un modèle d'image ou de vidéo. */
export interface Recommandations {
  workflow: string;
  nom: string;
  propositions: Proposition[];
  alertes: string[];
  sansRecommandation: boolean;
}

/** « ⚡ Adapter à ma machine » pour les LLM. */
export interface Adaptation {
  materiel: Materiel;
  propositions: Proposition[];
  alertes: string[];
  sansRecommandation: string[];
}

export const adapterAMaMachine = (): Promise<Adaptation> => invoke<Adaptation>('adapter_a_ma_machine');

export const recommanderWorkflow = (id: string): Promise<Recommandations> =>
  invoke<Recommandations>('recommander_workflow', { id });

/** Un terme qui a fait bloquer, et d'où il vient (aria_coeur::securite::Terme). */
export interface TermeBloque {
  terme: string;
  liste: string;
  /** `socle`, `ajouts`, ou `age` pour un âge reconnu. */
  etage: string;
}

/** Une phrase témoin rejouée (aria_coeur::securite::EtatTemoin). */
export interface TemoinSecurite {
  texte: string;
  attendu: 'bloque' | 'passe';
  /** Livrée avec Aria (sinon, ajoutée par l'utilisateur). */
  livre: boolean;
  obtenu: 'bloque' | 'passe';
  ok: boolean;
  termes: TermeBloque[];
}

/** Une liste du filtre AS-18 (aria_coeur::securite_reglages::ListeSecurite). */
export interface ListeSecurite {
  cle: string;
  nom: string;
  socle: string[];
  ajouts: string[];
}

/** Réglages → Sécurité (aria_coeur::securite_reglages::EtatSecurite). */
export interface EtatSecurite {
  fichierAjouts: string;
  erreur: string | null;
  listes: ListeSecurite[];
  temoins: TemoinSecurite[];
  longueurMin: number;
}

export interface EssaiSecurite {
  bloque: boolean;
  message: string | null;
  termes: TermeBloque[];
}

/** Un ajout fait (`etat`), ou à confirmer parce qu'il ferait bloquer des phrases témoins qui doivent passer. */
export interface AjoutSecurite {
  etat: EtatSecurite | null;
  aConfirmer: string | null;
}

export const etatSecurite = (): Promise<EtatSecurite> => invoke<EtatSecurite>('etat_securite');

export const essayerSecurite = (texte: string): Promise<EssaiSecurite> =>
  invoke<EssaiSecurite>('essayer_securite', { texte });

export const ajouterTermeSecurite = (liste: string, terme: string, forcer: boolean): Promise<AjoutSecurite> =>
  invoke<AjoutSecurite>('ajouter_terme_securite', { liste, terme, forcer });

export const retirerTermeSecurite = (liste: string, terme: string): Promise<EtatSecurite> =>
  invoke<EtatSecurite>('retirer_terme_securite', { liste, terme });

export const ajouterTemoinSecurite = (texte: string, attendu: 'bloque' | 'passe'): Promise<EtatSecurite> =>
  invoke<EtatSecurite>('ajouter_temoin_securite', { texte, attendu });

export const retirerTemoinSecurite = (texte: string): Promise<EtatSecurite> =>
  invoke<EtatSecurite>('retirer_temoin_securite', { texte });

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

/** Rushes du Plateau (images) ou Bobines du Montage (clips) : chacun ses dossiers (aria_coeur::rushes::GENRES). */
export type GenreRushes = 'images' | 'clips';

/** Ce qu'on fait d'un clip des Bobines : le retourner (Variante), reprendre ses réglages, ou ouvrir sa séquence. */
export type ActionClip = 'variante' | 'reutiliser' | 'sequence';

/** Les rushes d'une vue, ou d'un dossier (vue « tous »), les plus récents d'abord. */
export const lireRushes = (vue: VueRushes, dossier: string | null, genre: GenreRushes = 'images'): Promise<Creation[]> =>
  invoke<Creation[]>('lire_rushes', { vue, dossier, genre });

export const etatRushes = (genre: GenreRushes = 'images'): Promise<EtatRushes> =>
  invoke<EtatRushes>('etat_rushes', { genre });

/** Applique une action à une sélection ; rend le nombre de rushes modifiés. `dossier` : pour « ranger » (null : Non rangés). */
export const agirRushes = (ids: string[], action: ActionRushes, dossier: string | null): Promise<number> =>
  invoke<number>('agir_rushes', { ids, action, dossier });

/** Supprime pour de bon les rushes de la corbeille ; rend leur nombre. */
export const viderCorbeille = (genre: GenreRushes = 'images'): Promise<number> =>
  invoke<number>('vider_corbeille', { genre });

export const creerDossier = (nom: string, genre: GenreRushes = 'images'): Promise<string> =>
  invoke<string>('creer_dossier', { nom, genre });

export const renommerDossier = (ancien: string, nouveau: string, genre: GenreRushes = 'images'): Promise<string> =>
  invoke<string>('renommer_dossier', { ancien, nouveau, genre });

/** Le dossier disparaît, pas ses images : elles retournent dans « Non rangés ». Rend leur nombre. */
export const supprimerDossier = (nom: string, genre: GenreRushes = 'images'): Promise<number> =>
  invoke<number>('supprimer_dossier', { nom, genre });

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

/**
 * « Vérifier maintenant » : une seule requête anonyme vers GitHub (la dernière release du dépôt de l'appli), à la
 * demande. Rend la version trouvée, ou null quand l'appli est à jour ; l'erreur dit ce qui a manqué. Rien n'est
 * téléchargé ni installé.
 */
export const chercherMiseAJour = (): Promise<InfosMiseAJour | null> =>
  invoke<InfosMiseAJour | null>('chercher_mise_a_jour');

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

/** Le message d'une écriture ou d'une génération arrêtée par ✕ Annuler (aria_coeur::comfy_serveur::ANNULEE). */
export const ANNULEE = 'Annulé';

/** L'erreur vient de ✕ Annuler : ce n'est pas une panne. */
export function estAnnulation(erreur: unknown): boolean {
  return messageErreur(erreur) === ANNULEE;
}

/** ✕ Annuler : l'écriture en cours du Scénario s'arrête (le LLM cesse d'écrire). */
export const annulerEcriture = (): Promise<void> => invoke<void>('annuler_ecriture');

// --- Scénario : listes guidées et assemblage (aria_coeur::listes) ---

export interface SujetScenario {
  id: string;
  nom: string;
}

export interface ValeurListe {
  nom: string;
  prompt: string;
  remplace: string[];
  adulte: boolean;
  genre: string | null;
  promptsGenre: Record<string, string> | null;
  mineur: boolean;
  bornes: [number, number] | null;
  familles: string[];
  origines: string[];
  description: string;
  /** Sous-liste → valeurs avec lesquelles celle-ci reste cohérente. */
  avec: Record<string, string[]>;
  categorie: string;
}

export interface SousListeScenario {
  nom: string;
  note: string;
  /** Toutes ses valeurs sont adultes : elle disparaît pour un mineur. */
  adulte: boolean;
  condition: { sousListe: string; contient: string } | null;
  valeurs: ValeurListe[];
}

export type OngletScenario = 'sujet' | 'decor' | 'scene';

export interface GroupeScenario {
  nom: string;
  onglet: OngletScenario;
  sousListes: SousListeScenario[];
}

export interface BlocScenario {
  id: string;
  titre: string;
  /** Le bloc Plan : seulement pour un modèle vidéo. */
  video: boolean;
  groupes: GroupeScenario[];
}

/** [bloc, sous-liste, valeur] de chaque côté. */
export type Incompatibilite = [[string, string, string], [string, string, string]];

export interface StructureScenario {
  sujet: string;
  nom: string;
  blocs: BlocScenario[];
  incompatibilites: Incompatibilite[];
  agePrecis: [number, number];
  longueurs: { action: number; replique: number };
}

/** {bloc: {sous-liste: valeur}}. */
export type ChoixListes = Record<string, Record<string, string>>;

export interface DemandeAssemblage {
  sujet: string;
  choix: ChoixListes;
  age: number | null;
  texte: string;
  precisions: string;
  workflow: string | null;
  action: string;
  replique: string;
  dialogue: boolean | null;
  musique: string;
  avecMusique: boolean | null;
  sansSujet: boolean;
}

export interface Assemblage {
  prompt: string;
  mots: number;
  motsMax: number | null;
  alertes: string[];
  mineur: boolean;
  tranche: string | null;
  aTraduire: boolean;
  video: boolean;
}

export const sujetsScenario = (): Promise<SujetScenario[]> => invoke<SujetScenario[]>('sujets_scenario');

export const structureScenario = (sujet: string): Promise<StructureScenario> =>
  invoke<StructureScenario>('structure_scenario', { sujet });

/** Contrôle les règles des listes et le filtre AS-18, puis assemble le prompt. */
export const assemblerPrompt = (demande: DemandeAssemblage): Promise<Assemblage> =>
  invoke<Assemblage>('assembler_prompt', { demande });

/** « ✨ Écrire le prompt » (aria_coeur::scenario::DemandeEcriture). */
/** Les ateliers du Scénario où ✨ écrit un prompt (aria_coeur::scenario::ATELIERS). */
export type AtelierEcriture = 'image' | 'retouche' | 'clip';

export interface DemandeEcriture {
  atelier: AtelierEcriture;
  idee: string;
  /** Ce que les compléments imposent (le prompt assemblé des listes), déjà fixé. */
  complements: string;
  workflow: string | null;
  llm: string | null;
  /** L'image à retoucher, ou le rush à animer. */
  images?: string[];
  /** `animer` pour un clip qui anime son rush. */
  role?: string | null;
  /** La durée d'un clip, en secondes. */
  duree?: number | null;
}

export interface PromptEcrit {
  prompt: string;
  mots: number;
  dureeS: number;
  llm: string;
  /** Clip : les heures de coupe hors du clip ou dans le désordre. */
  alertes: string[];
}

/** Un prompt écrit au Scénario, en route vers le Plateau : son modèle cible et, pour s'inspirer ou redessiner, ses
 * images de départ. */
export interface EnvoiPlateau {
  prompt: string;
  workflow: string | null;
  references: string[];
  modeReference: ModeReference | null;
}

/** Une consigne écrite au Scénario, en route vers la page Retouche avec son image. */
export interface EnvoiRetouche {
  source: string;
  instruction: string;
  workflow: string | null;
}

/** Le prompt d'un clip écrit au Scénario, en route vers le Montage. */
export interface EnvoiClip {
  prompt: string;
  mode: 'animer' | 'texte';
  source: string | null;
  duree: number;
}

/** Le LLM écrit le prompt pour le modèle cible (il prend la carte, comme au Plateau). */
export const ecrirePromptScenario = (demande: DemandeEcriture): Promise<PromptEcrit> =>
  invoke<PromptEcrit>('ecrire_prompt_scenario', { demande });

/** Un atelier qui propose des directions (aria_coeur::directions::ATELIERS). */
export type AtelierDirections = 'image' | 'retouche' | 'clip' | 'sequence';

/** Une direction proposée par le LLM : un titre court, deux ou trois phrases, et ce qui la distingue des autres. */
export interface Direction {
  titre: string;
  texte: string;
  difference: string;
}

/** Une demande de directions (aria_coeur::directions::DemandeDirections). */
export interface DemandeDirections {
  atelier: AtelierDirections;
  /** L'envie, souvent vague. */
  envie: string;
  /** Ce que les compléments imposent déjà. */
  complements: string;
  workflow: string | null;
  llm: string | null;
  images: string[];
  role: string | null;
  duree: number | null;
  /** Les directions déjà proposées, dont le LLM doit s'écarter. */
  ecartees: Direction[];
}

/** Les directions proposées (aria_coeur::directions::Directions). */
export interface Directions {
  directions: Direction[];
  /** La réponse a été redemandée au LLM, et pourquoi. */
  redemandee: string | null;
  /** Ce que le contrôle qualité reproche encore à la réponse gardée. */
  alertes: string[];
  llm: string;
  dureeS: number;
}

/** « Proposer des directions » : trois directions distinctes à partir d'une envie, avant d'écrire le prompt. */
export const proposerDirections = (demande: DemandeDirections): Promise<Directions> =>
  invoke<Directions>('proposer_directions', { demande });

// --- Expliquer une erreur (aria_coeur::expliquer) ---

export type Certitude = 'sure' | 'probable' | 'possible' | 'inconnue';

/** Là où une erreur peut être expliquée (aria_coeur::expliquer::ORIGINES). */
export type OrigineErreur =
  | 'plateau'
  | 'retouche'
  | 'montage'
  | 'moteur'
  | 'telechargement'
  | 'installation'
  | 'journal'
  | 'autre';

export interface DemandeExplication {
  erreur: string;
  origine: OrigineErreur;
  /** L'étape, l'objet du Journal, le fichier téléchargé… */
  contexte?: string;
  /** La commande du Plateau, de la Retouche ou du Montage en erreur. */
  commande?: string | null;
  /** Le moteur en cause (`comfyui`, `llama`), pour lire la fin de son journal. */
  moteur?: string | null;
  llm?: string | null;
  /** Faire lire au LLM une cause connue mais large, avec le journal du moteur. */
  approfondir?: boolean;
}

export interface Explication {
  cause: string;
  certitude: Certitude;
  remedes: string[];
  /** `base` : une cause connue d'Aria ; `llm` : l'analyse du LLM ; `aucune` : rien ne correspond. */
  source: 'base' | 'llm' | 'aucune';
  cle: string | null;
  /** Une cause connue mais large, qu'un LLM disponible peut approfondir. */
  approfondir: boolean;
  llm: string | null;
  alertes: string[];
  dureeS: number;
}

/** La cause d'une erreur et ses remèdes : connue d'Aria (aussitôt), sinon analysée par le LLM ; notée au Journal. */
export const expliquerErreur = (demande: DemandeExplication): Promise<Explication> =>
  invoke<Explication>('expliquer_erreur', { demande });

/** Traduit les précisions en anglais. */
export const traduirePrecisions = (texte: string, llm: string | null): Promise<string> =>
  invoke<string>('traduire_precisions', { texte, llm });

/** Un personnage enregistré (aria_coeur::personnages::Personnage) : ses choix du bloc « personnage » et son âge. */
export interface Personnage {
  nom: string;
  choix: Record<string, string>;
  age: number | null;
}

export const lirePersonnages = (): Promise<Personnage[]> => invoke<Personnage[]>('lire_personnages');

export const enregistrerPersonnage = (nom: string, choix: Record<string, string>, age: number | null): Promise<Personnage> =>
  invoke<Personnage>('enregistrer_personnage', { nom, choix, age });

export const supprimerPersonnage = (nom: string): Promise<void> => invoke<void>('supprimer_personnage', { nom });

// --- Scénario : « 💬 M'aider à préciser » (aria_coeur::conversation) ---

export interface EchangeConversation {
  role: 'user' | 'assistant';
  content: string;
}

export interface QuestionConversation {
  question: string;
  choix: string[];
  /** Une rubrique du guide : sa liste complète peut s'ouvrir. */
  liste?: string;
}

export interface TourConversation {
  message: string;
  /** « Ce que j'ai compris ». */
  fiche: { nom: string; valeur: string }[];
  questions: QuestionConversation[];
  prompt: string | null;
  /** Le tour tel qu'il repart au LLM. */
  brut: string;
  /** Clip : heures de coupe hors du clip ; plan : répliques inventées retirées, réplique trop longue. */
  alertes: string[];
  modele: string;
  dureeS: number;
}

/** L'atelier d'une conversation : un plan est celui d'une séquence, `sequence` en écrit le fil de l'histoire. */
export type FormeConversation = 'image' | 'retouche' | 'clip' | 'sequence' | 'plan';

export interface DemandeConversation {
  echanges: EchangeConversation[];
  forme: FormeConversation;
  workflow: string | null;
  contexte: string;
  llm: string | null;
  format: string | null;
  sujet: string | null;
  /** L'image à retoucher, le rush à animer, ou les images de référence. */
  images: string[];
  /** Leur rôle : inspirer, variation, retouche ou animer (null : celui de l'atelier). */
  role: string | null;
  /** La durée d'un clip ; un plan prend la sienne. */
  duree: number | null;
  /** S'inspirer, Redessiner : le sujet vient des images. */
  sansSujet: boolean;
  /** 💬 d'un plan : la demande du découpage, `plan` étant son numéro. */
  sequence: DemandeDecoupage | null;
}

export const conversationScenario = (demande: DemandeConversation): Promise<TourConversation> =>
  invoke<TourConversation>('conversation_scenario', { demande });

// --- Montage : tourner un clip (aria_coeur::montage) ---

export interface QualiteVideo {
  cle: string;
  nom: string;
  aide: string;
  megapixels: number;
  steps: number;
  /** Secondes mesurées pour 5 s, à ces steps (RTX 4080 de l'auteur de la v2). */
  estimationS: number;
}

export interface OptionsMontage {
  qualites: QualiteVideo[];
  defautQualite: string;
  durees: number[];
  nombreMax: number;
  exposantDuree: number;
  formats: string[];
  animer: OptionsPlateau;
  texte: OptionsPlateau;
}

export interface DemandeClip {
  idee: string;
  prompt: string;
  mode: 'animer' | 'texte';
  source: string | null;
  qualite: string;
  duree: number;
  format: string;
  seed: number | null;
  nombre: number;
  modele: string | null;
  loras: ChoixLora[];
  ecritPar: string | null;
  /** Le dossier où ranger les résultats (« Ranger dans ») ; null : non rangés (un dossier des Bobines). */
  dossier?: string | null;
}

export const optionsMontage = (): Promise<OptionsMontage> => invoke<OptionsMontage>('options_montage');

/** Le temps estimé d'un tournage (aria_coeur::estimation::Estimation). */
export interface EstimationTournage {
  /** `calcul` (VRAM lue) ou `tableau_d_origine` (VRAM non lue). */
  source: 'calcul' | 'tableau_d_origine';
  clipS: number;
  totalS: number;
  nombre: number;
  checkpoint: string;
  poidsGo: number;
  steps: number;
  aValider: boolean;
  vramLibreGo: number | null;
  facteurVram: number;
}

/** Le temps d'un tournage du Montage d'après la VRAM libre, les steps et le poids du checkpoint (comme en v2). */
export const estimerTournage = (
  mode: 'animer' | 'texte',
  qualite: string,
  duree: number,
  nombre: number,
  modele: string | null,
): Promise<EstimationTournage> => invoke<EstimationTournage>('estimer_tournage', { mode, qualite, duree, nombre, modele });

/** Contrôle la demande (AS-18 compris) et la met en file. */
export const commanderClip = (demande: DemandeClip): Promise<Commande> => invoke<Commande>('commander_clip', { demande });

/** La vidéo d'un clip, en octets. */
export const videoCreation = (id: string): Promise<ArrayBuffer> => invoke<ArrayBuffer>('video_creation', { id });

// --- Montage : voix de référence (aria_coeur::voix, au format de la v2) ---

export interface VoixBibliotheque {
  id: string;
  date: string;
  nom: string | null;
  gardee: boolean;
  source: 'essai' | 'fichier';
  fichier: string;
  duree_son: number | null;
  phrase: string | null;
  description: string | null;
  personnage: string | null;
}

export interface OptionsVoix {
  phraseDefaut: string;
  longueurPhrase: number;
  motsMax: number;
  longueurDescription: number;
  longueurNom: number;
  formats: string[];
  dureeMin: number;
  dureeMax: number;
  estimationS: number;
}

export interface DemandeEssaiVoix {
  phrase: string;
  description: string;
  personnage: string | null;
  seed: number | null;
}

/** Les voix (brouillons compris si demandé) et ce que l'interface doit savoir d'elles. */
export const lireVoix = (brouillons: boolean): Promise<[VoixBibliotheque[], OptionsVoix]> =>
  invoke<[VoixBibliotheque[], OptionsVoix]>('lire_voix', { brouillons });

export const essayerVoix = (demande: DemandeEssaiVoix): Promise<Commande> => invoke<Commande>('essayer_voix', { demande });

export const garderVoix = (id: string, nom: string): Promise<VoixBibliotheque> =>
  invoke<VoixBibliotheque>('garder_voix', { id, nom });

export const supprimerVoix = (id: string): Promise<void> => invoke<void>('supprimer_voix', { id });

/** Un fichier audio (data URL), gardé aussitôt dans la bibliothèque. */
export const importerVoix = (son: string, nom: string, nomOrigine: string): Promise<VoixBibliotheque> =>
  invoke<VoixBibliotheque>('importer_voix', { son, nom, nomOrigine });

/** Le son d'une voix, en octets. */
export const sonVoix = (id: string): Promise<ArrayBuffer> => invoke<ArrayBuffer>('son_voix', { id });

// --- Montage : séquences (aria_coeur::sequences, data\sequences\ au format de la v2) ---

export type Enchainement = 'rush' | 'texte' | 'prolonger' | 'couper' | 'transition';

export interface PlanSequence {
  /** Absent pour un plan pas encore enregistré. */
  id?: string;
  idee: string;
  prompt: string | null;
  duree: number;
  enchainement: Enchainement;
  rush: string | null;
  seed?: number | null;
  ambiance: boolean;
  clip: string | null;
  prompt_final?: string | null;
  a_jour: boolean;
}

export interface CastingSequence {
  personnage: string | null;
  /** `auto`, `voix:<id>` (bibliothèque), l'identifiant d'un clip, ou rien. */
  voix: string | null;
  ambiance: string;
  contrainte: string;
  reference_partout: boolean;
}

export interface Sequence {
  /** Absent tant qu'elle n'est pas enregistrée. */
  id: string | null;
  titre: string;
  qualite: string;
  format: string;
  llm: string | null;
  modele: string | null;
  loras: ChoixLora[];
  casting: CastingSequence;
  plans: PlanSequence[];
  film: string | null;
  date?: string;
  dossier?: string;
  /** Par exemple : sa voix de la bibliothèque a été supprimée. */
  alerte?: string;
  contrainte_en?: { fr: string; en: string };
  /** Calculés par le cœur. */
  a_tourner?: number[];
  estimation_s?: number;
  duree_totale?: number;
}

export interface SequenceCorbeille {
  id: string;
  titre: string;
  plans: number;
  date: string | null;
}

export interface OptionsSequence {
  plansMax: number;
  longueurTitre: number;
  longueurIdee: number;
  longueurAmbiance: number;
  longueurContrainte: number;
  surcout: number;
  stepsParole: number;
  surcoutParole: number;
  exposantDuree: number;
  /** Les steps de chaque checkpoint prêt. */
  steps: Record<string, number>;
  modeleDefaut: string | null;
}

export interface EtatSequences {
  sequences: Sequence[];
  corbeille: SequenceCorbeille[];
  enTournage: string[];
  options: OptionsSequence;
}

export const lireSequences = (): Promise<EtatSequences> => invoke<EtatSequences>('lire_sequences');

export const creerSequence = (donnees: Sequence): Promise<Sequence> => invoke<Sequence>('creer_sequence', { donnees });

export const modifierSequence = (id: string, donnees: Sequence): Promise<Sequence> =>
  invoke<Sequence>('modifier_sequence', { id, donnees });

export const supprimerSequence = (id: string): Promise<void> => invoke<void>('supprimer_sequence', { id });

export const restaurerSequence = (id: string): Promise<Sequence> => invoke<Sequence>('restaurer_sequence', { id });

/** Efface pour de bon les séquences de la corbeille (leurs clips restent dans les Bobines) ; rend leur nombre. */
export const viderCorbeilleSequences = (): Promise<number> => invoke<number>('vider_corbeille_sequences');

/** Tourne les plans demandés (tous ceux qui ne sont pas à jour si la liste est vide), puis assemble le film. */
export const tournerSequence = (id: string, plans: number[]): Promise<Commande> =>
  invoke<Commande>('tourner_sequence', { id, plans });

// --- Scénario : atelier Séquence (aria_coeur::decoupage) ---

/** Le moment de la réplique dans le plan. */
export type MomentReplique = 'debut' | 'pendant' | 'fin';

/** La contrainte et sa traduction anglaise, reprise tant qu'elle vaut. */
export interface TraductionContrainte {
  fr: string;
  en: string;
}

/** Un plan tel que l'atelier l'envoie au LLM. */
export interface PlanDemandeDecoupage {
  enchainement: Enchainement;
  duree: number;
  note: string;
  rush: string | null;
  replique: string;
  moment: MomentReplique;
  off: boolean;
  /** Le prompt déjà écrit : le contexte des plans voisins. */
  prompt: string;
}

/** ✨ Tout écrire (`plan` : null) ou ✨ d'un plan (son numéro, 0 pour le premier). */
export interface DemandeDecoupage {
  synopsis: string;
  sujet: string;
  choix: ChoixListes;
  age: number | null;
  plans: PlanDemandeDecoupage[];
  reference: string | null;
  contrainte: string;
  llm: string | null;
  /** La musique d'ambiance ; null : pas de musique de fond. */
  musique: string | null;
  plan: number | null;
  contrainteEn: TraductionContrainte | null;
  /** Le format de la séquence, si le premier plan part du texte. */
  format: string | null;
}

/** Un plan écrit : le plan contrôlé, son prompt et son résumé en français. */
export interface PlanEcrit extends PlanDemandeDecoupage {
  idee: string;
}

export interface Decoupe {
  plans: PlanEcrit[];
  cible: number | null;
  alertes: string[];
  dureeS: number;
  reference: string | null;
  contrainte: string;
  contrainteEn: TraductionContrainte | null;
  /** Le GGUF qui a écrit. */
  llm: string;
}

/** Le LLM écrit les prompts des plans ; les garde-fous du cœur les contrôlent. */
export const decouperSequence = (demande: DemandeDecoupage): Promise<Decoupe> =>
  invoke<Decoupe>('decouper_sequence', { demande });

// --- Casting (aria_coeur::casting, data\bancs\ au format de la v2) ---

export type TacheCasting = 'texte_vers_image' | 'retouche' | 'reecriture' | 'image_vers_video' | 'texte_vers_video';

export interface EssaiCasting {
  id: string;
  teste: string;
  prompt: string;
  source?: string;
}

export interface CandidatCasting {
  id: string;
  nom: string;
  /** Les fichiers qui lui manquent. */
  manquants: number;
}

export interface PreparationCasting {
  tache: TacheCasting;
  candidats: CandidatCasting[];
  /** Casting LLM : le modèle qui rend les prompts. */
  rendu: CandidatCasting | null;
  sources: { nom: string; presente: boolean }[];
  essais: EssaiCasting[];
  seeds: number[];
  total: number;
  /** Ce qui empêche le casting de partir ; null s'il peut partir. */
  blocage: string | null;
}

/** Un casting fait (snake_case : format de la v2). */
export interface ResumeCasting {
  id: string;
  tache: TacheCasting;
  date: string;
  revele: boolean;
  nb_workflows: number;
  favori: string | null;
  notees: number;
  total: number;
}

export interface EtatCasting {
  /** Le dernier casting lancé dans la file du Plateau (`commande` : pour ✕ Annuler). */
  actif: {
    id: string;
    commande: string;
    tache: TacheCasting;
    etat: EtatCommande;
    erreur: string | null;
    fait: number;
    total: number;
    courant: string;
  } | null;
  castings: ResumeCasting[];
  preparation: { vramMaxOccupeeGo: number; taches: PreparationCasting[] };
}

export interface GenerationCasting {
  prompt: string;
  seed: number;
  fichier: string;
  note: number | null;
  peut_servir_de_source: boolean;
  est_la_source: boolean;
  /** Révélé seulement : le prompt écrit par le LLM, et ses contrôles. */
  texte: string | null;
  controles: Record<string, boolean | number | null> | null;
}

export interface ModeleCasting {
  /** Une lettre à l'aveugle, l'identifiant ensuite. */
  cle: string;
  nom: string;
  generations: GenerationCasting[];
  resume: Record<string, number | string | null> | null;
  liberation: { vram_avant_go?: number; vram_apres_go?: number; duree_s?: number } | null;
}

export interface VueCasting {
  id: string;
  tache: TacheCasting;
  date: string;
  seeds: number[];
  prompts: EssaiCasting[];
  vram_base_go: number | null;
  revele: boolean;
  favori: string | null;
  par_defaut: string | null;
  notees: number;
  total: number;
  modeles: ModeleCasting[];
}

export const lireCastings = (): Promise<EtatCasting> => invoke<EtatCasting>('lire_castings');

export const lireCasting = (id: string): Promise<VueCasting> => invoke<VueCasting>('lire_casting', { id });

/** Lance un casting dans la file du Plateau ; `workflows` : les modèles choisis, sinon tous ceux de la tâche. */
export const lancerCasting = (tache: TacheCasting, workflows: string[] | null = null): Promise<Commande> =>
  invoke<Commande>('lancer_casting', { tache, workflows });

/** Une image (ou un clip) d'un casting, en octets. */
export const imageCasting = (id: string, cle: string, fichier: string): Promise<ArrayBuffer> =>
  invoke<ArrayBuffer>('image_casting', { id, cle, fichier });

/** Une image de départ des castings de retouche et d'image vers vidéo, en octets. */
export const sourceCasting = (nom: string): Promise<ArrayBuffer> => invoke<ArrayBuffer>('source_casting', { nom });

export const noterCasting = (id: string, cle: string, fichier: string, note: number): Promise<VueCasting> =>
  invoke<VueCasting>('noter_casting', { id, cle, fichier, note });

/** Révèle les modèles : ça ne se défait pas. */
export const revelerCasting = (id: string): Promise<VueCasting> => invoke<VueCasting>('reveler_casting', { id });

export const favoriCasting = (id: string, workflow: string): Promise<VueCasting> =>
  invoke<VueCasting>('favori_casting', { id, workflow });

/** Fige l'image comme image de départ des castings de retouche et d'image vers vidéo. */
export const sourceDepuisCasting = (id: string, cle: string, fichier: string): Promise<VueCasting> =>
  invoke<VueCasting>('source_depuis_casting', { id, cle, fichier });

/** Pose un fichier (data URL) comme image de départ `nom` des castings ; rend l'état du Casting. */
export const importerSourceCasting = (nom: string, image: string, nomOrigine: string): Promise<EtatCasting> =>
  invoke<EtatCasting>('importer_source_casting', { nom, image, nomOrigine });

/** Pose une image des Rushes comme image de départ `nom` des castings ; rend l'état du Casting. */
export const sourceCastingDepuisRush = (nom: string, rush: string): Promise<EtatCasting> =>
  invoke<EtatCasting>('source_casting_depuis_rush', { nom, rush });

// --- Assistant de premier lancement (aria_coeur::assistant, catalogue/packs.yaml) ---

/** Une licence d'un pack, avec ce qu'il faut savoir avant de l'accepter. */
export interface LicencePack {
  nom: string;
  url: string;
  resume: string;
  commercial: boolean | null;
  territoiresExclus: string[];
  /** « fichier : licence » des composants qui ont la leur. */
  composants: string[];
}

/** Un pack, tel que l'écran « Ce que tu veux faire » le montre. */
export interface Pack {
  id: string;
  nom: string;
  usage: string;
  conseille: boolean;
  facultatif: boolean;
  testee: string | null;
  /** Octets de tous ses fichiers, et de ceux qui manquent. */
  taille: number;
  aTelecharger: number;
  installe: boolean;
  licences: LicencePack[];
  /** La date où sa licence a été acceptée. */
  licenceAcceptee: string | null;
  moteursAbsents: string[];
}

/** Ce que coûte une sélection de packs. */
export interface PlanAssistant {
  fichiers: { dossier: string; nom: string; taille: number }[];
  aTelecharger: number;
  /** La place libre sur le disque des modèles ; null : inconnue. */
  libre: number | null;
  placeOk: boolean;
  licencesAAccepter: string[];
}

export interface EtatAssistant {
  /** S'ouvrir au lancement : pas encore terminé, et il manque de quoi générer. */
  ouvrir: boolean;
  termine: string | null;
  packs: Pack[];
}

export interface EnvoiAssistant {
  /** Les téléchargements ajoutés à la file. */
  ajoutes: string[];
  /** Les fichiers que la file avait déjà. */
  dejaEnFile: string[];
}

/** Un usage du récapitulatif de fin et son modèle. */
export interface RecapAssistant {
  usage: string;
  nom: string;
}

export interface FinAssistant {
  /** « section.clé » → valeur. */
  favoris: Record<string, string>;
  regles: RecapAssistant[];
  manquants: RecapAssistant[];
  essai: boolean;
}

export const etatAssistant = (): Promise<EtatAssistant> => invoke<EtatAssistant>('etat_assistant');

export const planAssistant = (packs: string[]): Promise<PlanAssistant> =>
  invoke<PlanAssistant>('plan_assistant', { packs });

/** « J'ai lu et j'accepte » pour chaque pack coché ; rend le plan à jour. */
export const accepterLicencesAssistant = (packs: string[]): Promise<PlanAssistant> =>
  invoke<PlanAssistant>('accepter_licences_assistant', { packs });

export const telechargerAssistant = (packs: string[]): Promise<EnvoiAssistant> =>
  invoke<EnvoiAssistant>('telecharger_assistant', { packs });

/** Règle les favoris sur ce qui est installé et note la fin de l'assistant. */
export const terminerAssistant = (packs: string[]): Promise<FinAssistant> =>
  invoke<FinAssistant>('terminer_assistant', { packs });

/** Une image rapide (peu de steps) avec le favori « Créer une image » ; suivie par la file du Plateau. */
export const essaiAssistant = (): Promise<Commande> => invoke<Commande>('essai_assistant');

// --- Réglages → Claude (MCP) (appli/src/mcp.rs) ---

/** De quoi déclarer le serveur MCP d'Aria dans Claude Code : la commande, et le serveur pour « mcpServers ». */
export interface DeclarationMcp {
  nom: string;
  exe: string;
  commande: string;
  serveur: { type: string; command: string; args: string[] };
}

export const commandeMcp = (): Promise<DeclarationMcp> => invoke<DeclarationMcp>('commande_mcp');

// --- Le LLM regarde ce qui est sorti (aria_coeur::verification, nouveauté de la v3) ---

/** La vérification d'une création, gardée sur sa fiche (clés en snake_case, comme la fiche). */
export interface Verification {
  fidele: boolean;
  verdict: string;
  ecarts: string[];
  /** Le prompt corrigé, si l'image n'est pas fidèle. */
  prompt: string | null;
  alertes: string[];
  llm: string;
  date: string;
}

/** Une image retenue parmi les meilleures d'un lot, gardée sur sa fiche. */
export interface Meilleure {
  rang: number;
  raison: string;
  lot: string[];
  llm: string;
  date: string;
}

/** Le LLM qui regarderait, ou pourquoi il n'y en a pas (et quoi installer). */
export interface EtatVision {
  llm: string | null;
  explication: string | null;
}

export interface Verifiee {
  id: string;
  verification: Verification | null;
  erreur: string | null;
}

export interface Verifications {
  resultats: Verifiee[];
  llm: string;
  dureeS: number;
}

export interface Retenue {
  id: string;
  rang: number;
  raison: string;
}

export interface Choix {
  meilleures: Retenue[];
  redemandee: string | null;
  llm: string;
  dureeS: number;
}

/** Les images d'un lot, au plus (aria_coeur::verification::IMAGES_MAX), et le minimum pour en choisir les meilleures. */
export const IMAGES_MAX_VISION = 8;
export const CHOIX_MIN = 2;

export const etatVision = (workflow: string | null = null): Promise<EtatVision> =>
  invoke<EtatVision>('etat_vision', { workflow });

/** Le LLM qui voit compare chaque image à l'intention (✕ Annuler l'arrête). */
export const verifierImages = (ids: string[], llm: string | null = null): Promise<Verifications> =>
  invoke<Verifications>('verifier_images', { ids, llm });

/** Le LLM qui voit retient, dans un lot de 2 à 8 images, celles qui respectent le mieux l'intention. */
export const choisirMeilleures = (ids: string[], llm: string | null = null): Promise<Choix> =>
  invoke<Choix>('choisir_meilleures', { ids, llm });

/** La même demande que la création, avec ce prompt, remise en file. */
export const relancerAvecPrompt = (id: string, prompt: string): Promise<Commande> =>
  invoke<Commande>('relancer_avec_prompt', { id, prompt });
