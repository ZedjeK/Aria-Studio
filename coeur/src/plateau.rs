//! Le Plateau : la file des créations, comme en v2 (`app/plateau.py`), réduite pour l'instant à la création d'une
//! image depuis une idée réécrite par le LLM, ou depuis un prompt écrit tel quel (« Sans réécriture »). Les
//! références, la retouche, l'agrandissement, les LoRA et la vidéo viendront ensuite.
//!
//! Une commande est contrôlée avant d'entrer dans la file (workflow prêt, modèle, LLM installé, format, nombre,
//! filtre AS-18), puis un fil unique les exécute une par une : réécriture par le LLM s'il y a lieu, graphe préparé,
//! ComfyUI, image enregistrée dans `data\creations\` avec la fiche de la v2. Le filtre AS-18 relit l'idée et le
//! prompt au moment de générer, et `Workflow::preparer` relit encore le prompt.
//!
//! ✕ Annuler : une commande en attente ne part pas ; celle en cours s'arrête (ComfyUI interrompt la sienne ; une
//! réécriture en cours va jusqu'au bout, puis rien ne part vers ComfyUI). Les images déjà enregistrées restent.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::comfy_serveur::{ANNULEE, Fichier};
use crate::llama::{self, Lancement};
use crate::llm::{self, Lieux};
use crate::orchestrateur::Orchestrateur;
use crate::retouche::{self, DemandeAgrandissement, DemandeRetouche};
use crate::workflows::{self, FORMATS, ReglagesWorkflow, Workflow};
use crate::{Dossier, collection, creations, fiches, modeles, prompt, reglages, securite};

/// Images d'une même commande, au plus (contre une faute de frappe, comme en v2).
pub const NOMBRE_MAX: u32 = 50;
/// Le workflow texte → image de la v2 (`config.yaml`, `generation.texte_vers_image`).
pub const WORKFLOW_DEFAUT: &str = "qwen_image_2_1";
/// Le workflow de « S'inspirer » de la v2 (`config.yaml`, `generation.references`).
pub const WORKFLOW_REFERENCES: &str = "qwen_image_2_1_edit";
/// Les modes d'une création à partir de rushes : s'en inspirer, ou en redessiner un.
pub const MODES_REFERENCE: [&str; 2] = ["inspirer", "variation"];
/// La liberté d'une variation par défaut (denoise), comme en v2.
pub const LIBERTE_DEFAUT: f64 = 0.6;
/// Le préfixe des fichiers que ComfyUI écrit pour le Plateau.
pub const PREFIXE: &str = "aria_plateau";
/// La tâche des workflows du Plateau.
const TACHE: &str = "texte_vers_image";
/// Une seed tirée au hasard est prise sous 2⁴⁸, comme en v2.
const SEED_MAX: u64 = 1 << 48;

/// Où en est une commande. Dans l'interface : « attente », « en_cours », « termine », « erreur », « annule ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EtatCommande {
    Attente,
    EnCours,
    Termine,
    Erreur,
    Annule,
}

/// Ce que l'interface demande.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Demande {
    /// L'idée à réécrire, ou le prompt tel quel.
    pub prompt: String,
    /// Faire réécrire l'idée par le LLM (absent : non).
    #[serde(default)]
    pub reecrire: bool,
    /// Le GGUF qui réécrit ; absent : celui du workflow, sinon le LLM par défaut.
    #[serde(default)]
    pub llm: Option<String>,
    /// Absent : le favori de l'utilisateur, sinon celui de la v2.
    pub workflow: Option<String>,
    pub format: String,
    /// Absente : tirée au hasard ; les images suivantes prennent la seed suivante.
    pub seed: Option<u64>,
    pub nombre: u32,
    /// Un checkpoint de la famille ; absent : celui des réglages.
    pub modele: Option<String>,
    /// Des rushes de départ : à mélanger (S'inspirer, de 1 à 10 selon le modèle) ou à redessiner (un seul).
    #[serde(default)]
    pub references: Vec<String>,
    /// `inspirer` ou `variation`, s'il y a des rushes de départ.
    #[serde(default)]
    pub mode_reference: Option<String>,
    /// Redessiner : la liberté (denoise), de 0,05 (copie) à 1 (libre).
    #[serde(default = "liberte_defaut")]
    pub liberte: f64,
    /// Les LoRA de la famille du workflow, cochées au Plateau.
    #[serde(default)]
    pub loras: Vec<ChoixLora>,
    /// Le dossier où ranger les résultats (« Ranger dans ») ; absent : non rangés.
    #[serde(default)]
    pub dossier: Option<String>,
}

const fn liberte_defaut() -> f64 {
    LIBERTE_DEFAUT
}

/// Une LoRA choisie : son fichier dans le dossier des LoRA de la famille, sa force et ses mots déclencheurs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoixLora {
    pub fichier: String,
    #[serde(default = "force_lora_defaut")]
    pub force: f64,
    #[serde(default)]
    pub declencheurs: String,
}

const fn force_lora_defaut() -> f64 {
    1.0
}

/// Une LoRA proposée pour un workflow : celles de sa famille, avec les mots et la force retenus.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoraProposee {
    pub fichier: String,
    pub nom: String,
    pub declencheurs: String,
    pub force: f64,
    pub page: Option<String>,
}

/// Une commande du Plateau et son avancement.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commande {
    pub id: String,
    /// `creation`, `retouche` ou `agrandissement`.
    #[serde(rename = "type")]
    pub genre: String,
    /// Ce que l'utilisateur a écrit.
    pub idee: String,
    /// Le prompt envoyé au modèle : l'idée telle quelle, ou vide tant que le LLM ne l'a pas réécrite.
    pub prompt: String,
    pub reecrire: bool,
    /// Le GGUF qui réécrit.
    pub llm: Option<String>,
    pub workflow: String,
    pub format: String,
    pub seed: Option<u64>,
    pub nombre: u32,
    pub modele: Option<String>,
    pub etat: EtatCommande,
    pub annulee: bool,
    pub etape: String,
    /// Avancement de l'image en cours, de 0 à 1.
    pub progression: f64,
    /// Les créations enregistrées.
    pub images: Vec<String>,
    pub erreur: Option<String>,
    /// L'image de départ d'une retouche ou d'un agrandissement.
    pub parent: Option<String>,
    /// Le masque peint (PNG) d'une retouche ; il ne part pas vers l'interface.
    #[serde(skip)]
    pub masque_png: Option<Vec<u8>>,
    /// Un masque a été peint.
    pub masque: bool,
    /// Le fondu du recollage, en pixels.
    pub fondu: u32,
    /// La taille visée d'un agrandissement.
    pub largeur: Option<u32>,
    pub hauteur: Option<u32>,
    pub ancrage: Option<String>,
    /// Le format auquel la source d'une retouche est recadrée au centre.
    pub cadrage: Option<String>,
    /// Les rushes de départ d'une création.
    pub references: Vec<String>,
    pub mode_reference: Option<String>,
    pub liberte: f64,
    /// Les LoRA branchées, et leurs mots déclencheurs en tête du prompt.
    pub loras: Vec<ChoixLora>,
    /// Montage : `animer` (un rush) ou `texte`, la résolution et la durée en secondes.
    pub mode_video: Option<String>,
    pub qualite: Option<String>,
    pub duree: Option<u32>,
    /// Essai de voix : qui parle, et comment (la phrase est dans `idee`, le personnage dans `parent`).
    pub description: String,
    /// Séquence du Montage : son identifiant, et les plans à tourner (indices, dans l'ordre).
    pub sequence: Option<String>,
    pub plans: Vec<usize>,
    /// Le dossier des Rushes ou des Bobines où ranger les résultats ; absent : non rangés (une retouche reste à côté
    /// de sa source).
    pub dossier: Option<String>,
    /// Casting : l'identifiant du casting (`data\bancs\<id>`), et les générations faites (sur `nombre`).
    pub banc: Option<String>,
    pub fait: u32,
}

impl Commande {
    /// Une commande en attente, sans rien de réglé.
    pub(crate) fn nouvelle(id: String, genre: &str, idee: String, workflow: String, format: &str) -> Self {
        Self {
            id,
            genre: genre.to_owned(),
            idee,
            prompt: String::new(),
            reecrire: false,
            llm: None,
            workflow,
            format: format.to_owned(),
            seed: None,
            nombre: 1,
            modele: None,
            etat: EtatCommande::Attente,
            annulee: false,
            etape: String::new(),
            progression: 0.0,
            images: Vec::new(),
            erreur: None,
            parent: None,
            masque_png: None,
            masque: false,
            fondu: 0,
            largeur: None,
            hauteur: None,
            ancrage: None,
            cadrage: None,
            references: Vec::new(),
            mode_reference: None,
            liberte: LIBERTE_DEFAUT,
            loras: Vec::new(),
            mode_video: None,
            qualite: None,
            duree: None,
            description: String::new(),
            sequence: None,
            plans: Vec::new(),
            dossier: None,
            banc: None,
            fait: 0,
        }
    }
}

/// Un workflow proposé par le Plateau.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoixWorkflow {
    pub id: String,
    pub nom: String,
    pub famille: String,
    pub origine: Option<String>,
    /// Les checkpoints prêts à tourner.
    pub modeles: Vec<String>,
    /// Celui des réglages.
    pub modele_defaut: Option<String>,
    /// Le format des réglages.
    pub format: Option<String>,
    /// Les checkpoints présents dont les steps attendent d'être validés.
    pub a_valider: Vec<String>,
    /// Les steps que dit la fiche, à proposer pour les valider.
    pub steps_suggeres: Vec<u64>,
    /// Le nombre d'images de référence qu'il reçoit (S'inspirer ; 0 : aucune).
    pub references: usize,
    /// Il sait redessiner une image (variation).
    pub variation: bool,
    /// Réservé à la Retouche : S'inspirer ne le propose pas.
    pub reserve_retouche: bool,
    /// Les LoRA de sa famille.
    pub loras: Vec<LoraProposee>,
}

/// Un LLM proposé pour réécrire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoixLlm {
    pub fichier: String,
    pub nom: String,
    /// Proposé au choix par Aria (gagnant du Casting).
    pub favori: bool,
    /// Entraîné pour un modèle cible : il ne réécrit que pour lui.
    pub specialiste: bool,
}

/// Ce que le Plateau propose : relu à chaque appel, un modèle déposé y apparaît aussitôt.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub formats: Vec<String>,
    pub nombre_max: u32,
    pub defaut: String,
    /// Les workflows prêts (rien ne manque sur le disque).
    pub workflows: Vec<ChoixWorkflow>,
    /// Ceux auxquels il manque des fichiers, par leur nom.
    pub incomplets: Vec<String>,
    /// Les LLM installés, les favoris d'abord.
    pub llm: Vec<ChoixLlm>,
    /// Le LLM de chaque workflow prêt : celui choisi pour lui, sinon le LLM par défaut.
    pub llm_par_workflow: Map<String, Value>,
    /// Le workflow de « S'inspirer » par défaut.
    pub defaut_references: String,
    /// Le plus d'images de référence qu'Aria envoie.
    pub references_max: usize,
    /// La force d'une LoRA, au plus.
    pub lora_force_max: f64,
}

/// Les dossiers dont le Plateau a besoin.
#[derive(Debug, Clone)]
pub struct Contexte {
    pub dossier: Dossier,
    pub data: PathBuf,
    pub modeles: PathBuf,
    pub moteurs: PathBuf,
    /// Le port de llama-server.
    pub port_llama: u16,
}

impl Contexte {
    /// Où vivent les LLM et leur moteur.
    pub fn lieux(&self) -> Lieux {
        Lieux {
            dossier: self.dossier.clone(),
            modeles: self.modeles.clone(),
            moteurs: self.moteurs.clone(),
            port: self.port_llama,
        }
    }
}

/// Ce qui écrit et ce qui génère : llama-server et ComfyUI par l'orchestrateur, un faux dans les tests.
pub trait Generateur: Send + 'static {
    /// Fait écrire le LLM, lancé avec ces réglages.
    ///
    /// # Errors
    /// llama-server ne démarre pas, refuse la demande, ou rend une réponse vide.
    fn ecrire(&mut self, lancement: Lancement, demande: &llama::Demande) -> Result<llama::Reponse, String>;

    /// Téléverse une image dans ComfyUI ; rend le nom sous lequel le graphe la retrouve.
    ///
    /// # Errors
    /// ComfyUI ne démarre pas ou refuse l'image.
    fn televerser(&mut self, nom: &str, octets: &[u8]) -> Result<String, String>;

    /// Exécute le graphe et rend ses fichiers de sortie.
    ///
    /// # Errors
    /// ComfyUI indisponible, graphe refusé, erreur de nœud, ou annulation ([`ANNULEE`]).
    fn generer(
        &mut self,
        api: &Value,
        progression: &mut dyn FnMut(u64, u64),
        annule: &AtomicBool,
    ) -> Result<Vec<Fichier>, String>;

    /// Vide la VRAM du moteur en marche (ComfyUI décharge ses modèles sans s'arrêter) ; rien à faire par défaut.
    ///
    /// # Errors
    /// Le moteur ne répond pas.
    fn liberer(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// ComfyUI, démarré au besoin par l'orchestrateur. L'orchestrateur reste pris pendant la génération : un autre
/// usage de la carte attend la fin de l'image.
impl Generateur for Arc<Mutex<Orchestrateur>> {
    fn ecrire(&mut self, lancement: Lancement, demande: &llama::Demande) -> Result<llama::Reponse, String> {
        let mut orchestrateur = self
            .lock()
            .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?;
        orchestrateur.llm(lancement)?.generer(demande)
    }

    fn televerser(&mut self, nom: &str, octets: &[u8]) -> Result<String, String> {
        let mut orchestrateur = self
            .lock()
            .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?;
        orchestrateur.comfyui()?.televerser(nom, octets)
    }

    fn generer(
        &mut self,
        api: &Value,
        progression: &mut dyn FnMut(u64, u64),
        annule: &AtomicBool,
    ) -> Result<Vec<Fichier>, String> {
        let mut orchestrateur = self
            .lock()
            .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?;
        orchestrateur
            .comfyui()?
            .executer(api, &mut |valeur, maximum| progression(valeur, maximum), annule)
    }

    fn liberer(&mut self) -> Result<(), String> {
        self.lock()
            .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?
            .liberer()
    }
}

/// L'écriture du Scénario, que ✕ Annuler arrête : le drapeau est lu avant de prendre la carte, après l'avoir prise,
/// et à chaque morceau de la réponse. Il vit hors du verrou de l'orchestrateur, qu'une écriture garde tout du long.
pub struct EcritureAnnulable {
    pub orchestrateur: Arc<Mutex<Orchestrateur>>,
    pub annule: Arc<AtomicBool>,
}

impl EcritureAnnulable {
    fn verifier(&self) -> Result<(), String> {
        if self.annule.load(Ordering::Relaxed) {
            Err(ANNULEE.to_owned())
        } else {
            Ok(())
        }
    }
}

impl Generateur for EcritureAnnulable {
    fn ecrire(&mut self, lancement: Lancement, demande: &llama::Demande) -> Result<llama::Reponse, String> {
        self.verifier()?;
        let mut orchestrateur = self
            .orchestrateur
            .lock()
            .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?;
        self.verifier()?;
        let serveur = orchestrateur.llm(lancement)?;
        self.verifier()?;
        serveur.generer_annulable(demande, &self.annule)
    }

    fn televerser(&mut self, nom: &str, octets: &[u8]) -> Result<String, String> {
        self.orchestrateur.televerser(nom, octets)
    }

    fn generer(
        &mut self,
        api: &Value,
        progression: &mut dyn FnMut(u64, u64),
        annule: &AtomicBool,
    ) -> Result<Vec<Fichier>, String> {
        self.orchestrateur.generer(api, progression, annule)
    }

    fn liberer(&mut self) -> Result<(), String> {
        self.orchestrateur.liberer()
    }
}

/// Les réglages de l'utilisateur pour ce workflow (`reglages.json`, `workflows.<id>`).
fn reglages_workflow(dossier: &Dossier, id: &str) -> Option<ReglagesWorkflow> {
    reglages::section(dossier, "workflows")?
        .remove(id)
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
}

/// Le workflow texte → image par défaut : le favori de l'utilisateur (`favoris.generation.texte_vers_image`) s'il
/// est encore livré, sinon celui de la v2.
pub fn workflow_defaut(dossier: &Dossier) -> String {
    reglages::section(dossier, "favoris")
        .and_then(|f| f.get("generation")?.get(TACHE)?.as_str().map(str::to_owned))
        .filter(|id| workflows::obtenir(id).is_ok())
        .unwrap_or_else(|| WORKFLOW_DEFAUT.to_owned())
}

/// Ce que le Plateau propose.
pub fn options(contexte: &Contexte) -> Options {
    options_pour(contexte, TACHE, workflow_defaut(&contexte.dossier))
}

/// Les workflows d'une tâche (prêts, et incomplets par leur nom), les formats et les LLM, avec ce défaut.
pub fn options_pour(contexte: &Contexte, tache: &str, defaut: String) -> Options {
    let mut prets = Vec::new();
    let mut incomplets = Vec::new();
    for wf in workflows::catalogue()
        .0
        .into_iter()
        .filter(|w| w.tache() == tache)
    {
        let nom = nom_de(&wf);
        if !wf.manquants(&contexte.modeles).is_empty() {
            incomplets.push(nom);
            continue;
        }
        let reglages = reglages_workflow(&contexte.dossier, &wf.id);
        let regle = wf.regle(&contexte.modeles, reglages.as_ref());
        let modeles = wf.variantes(&contexte.modeles, reglages.as_ref());
        let a_valider = wf
            .presents(&contexte.modeles)
            .into_iter()
            .filter(|m| !modeles.contains(m))
            .collect();
        prets.push(ChoixWorkflow {
            nom,
            famille: wf.famille().to_owned(),
            origine: wf.modele_origine(),
            modele_defaut: regle.get("modele").and_then(Value::as_str).map(str::to_owned),
            format: regle.get("ratio").and_then(Value::as_str).map(str::to_owned),
            a_valider,
            steps_suggeres: wf.steps_suggeres(),
            references: wf.accepte_references(),
            variation: wf.accepte_variation(),
            reserve_retouche: wf.reserve_retouche(),
            loras: loras_proposees(contexte, &wf),
            modeles,
            id: wf.id,
        });
    }
    let llm_par_workflow = prets
        .iter()
        .map(|w| {
            let choisi = llm::de_workflow(&contexte.dossier, &contexte.modeles, &w.id);
            (w.id.clone(), Value::String(choisi))
        })
        .collect();
    Options {
        formats: FORMATS.iter().map(|f| (*f).to_owned()).collect(),
        nombre_max: NOMBRE_MAX,
        defaut,
        workflows: prets,
        incomplets,
        llm: llm_au_choix(&contexte.modeles),
        llm_par_workflow,
        defaut_references: workflow_references(&contexte.dossier),
        references_max: workflows::REFERENCES_MAX,
        lora_force_max: workflows::FORCE_LORA_MAX,
    }
}

/// Les LoRA de la famille d'un workflow, avec leurs mots et leur force retenus.
fn loras_proposees(contexte: &Contexte, wf: &Workflow) -> Vec<LoraProposee> {
    let registre = crate::telechargement::registre(&contexte.data);
    let dossier = wf.dossier_loras();
    collection::loras(&contexte.dossier, &contexte.data, &contexte.modeles, &dossier)
        .into_iter()
        .map(|(fichier, r)| LoraProposee {
            nom: fichier
                .rsplit_once('.')
                .map_or(fichier.as_str(), |(n, _)| n)
                .to_owned(),
            page: registre
                .get(&format!("{dossier}/{fichier}"))
                .and_then(|e| e.get("page"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            declencheurs: r.declencheurs,
            force: r.force,
            fichier,
        })
        .collect()
}

/// Les LoRA d'une demande, contrôlées comme en v2 : de la famille du workflow, force de 0 à 2, mots déclencheurs
/// sans espaces en trop. Le filtre AS-18 les lit avec la demande (voir [`textes_loras`]).
///
/// # Errors
/// LoRA absente, force hors bornes, mots trop longs.
pub fn valider_loras(
    contexte: &Contexte,
    wf: &Workflow,
    loras: &[ChoixLora],
) -> Result<Vec<ChoixLora>, String> {
    if loras.is_empty() {
        return Ok(Vec::new());
    }
    let dossier = wf.dossier_loras();
    let disponibles = modeles::fichiers(&contexte.modeles, &dossier, &modeles::EXTENSIONS_IMAGE);
    loras
        .iter()
        .map(|l| {
            if !disponibles.contains(&l.fichier) {
                return Err(format!("LoRA absente de modeles/{dossier} : {}", l.fichier));
            }
            if !(0.0..=workflows::FORCE_LORA_MAX).contains(&l.force) {
                return Err(format!("Force de LoRA entre 0 et 2 : {}", l.force));
            }
            let declencheurs = l.declencheurs.split_whitespace().collect::<Vec<_>>().join(" ");
            if declencheurs.chars().count() > collection::LONGUEUR_DECLENCHEURS {
                return Err(format!(
                    "Mots déclencheurs de {} : {} caractères au plus",
                    l.fichier,
                    collection::LONGUEUR_DECLENCHEURS
                ));
            }
            Ok(ChoixLora {
                fichier: l.fichier.clone(),
                force: (l.force * 100.0).round() / 100.0,
                declencheurs,
            })
        })
        .collect()
}

/// Les mots déclencheurs des LoRA, pour le filtre AS-18.
pub fn textes_loras(loras: &[ChoixLora]) -> impl Iterator<Item = &str> {
    loras
        .iter()
        .map(|l| l.declencheurs.as_str())
        .filter(|d| !d.is_empty())
}

/// Retient les mots et la force des LoRA choisies pour la prochaine fois (`reglages.json`, section `loras`).
///
/// # Errors
/// `reglages.json` ne s'écrit pas.
pub fn memoriser_loras(contexte: &Contexte, wf: &Workflow, loras: &[ChoixLora]) -> Result<(), String> {
    if loras.is_empty() {
        return Ok(());
    }
    let dossier = wf.dossier_loras();
    reglages::modifier_section(&contexte.dossier, "loras", |s| {
        for l in loras {
            s.insert(
                format!("{dossier}/{}", l.fichier),
                json!({"declencheurs": l.declencheurs, "force": l.force}),
            );
        }
    })
    .map_err(|e| e.to_string())
}

/// Le prompt envoyé à ComfyUI : les mots déclencheurs des LoRA en tête, comme en v2.
pub fn avec_declencheurs(prompt: &str, loras: &[ChoixLora]) -> String {
    let declencheurs: Vec<&str> = textes_loras(loras).collect();
    if declencheurs.is_empty() {
        prompt.to_owned()
    } else {
        format!("{}, {prompt}", declencheurs.join(", "))
    }
}

/// Le workflow de « S'inspirer » par défaut : le favori de l'utilisateur (`favoris.generation.references`) s'il est
/// encore livré, sinon celui de la v2.
pub fn workflow_references(dossier: &Dossier) -> String {
    reglages::section(dossier, "favoris")
        .and_then(|f| {
            f.get("generation")?
                .get("references")?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|id| workflows::obtenir(id).is_ok())
        .unwrap_or_else(|| WORKFLOW_REFERENCES.to_owned())
}

/// Vérifie les rushes de départ et rend le workflow qui les recevra, comme la v2.
fn valider_references(contexte: &Contexte, demande: &Demande) -> Result<String, String> {
    let mode = demande.mode_reference.as_deref().unwrap_or_default();
    if !MODES_REFERENCE.contains(&mode) {
        return Err(format!("Mode de référence inconnu : {mode}"));
    }
    for id in &demande.references {
        creations::fiche(&contexte.data, id)?;
    }
    if mode == "variation" {
        if demande.references.len() != 1 {
            return Err("Redessiner une image part d'une seule image de référence".to_owned());
        }
        if !(workflows::LIBERTE.0..=workflows::LIBERTE.1).contains(&demande.liberte) {
            return Err(format!(
                "Liberté entre {} et {}",
                workflows::LIBERTE.0,
                workflows::LIBERTE.1
            )
            .replace('.', ","));
        }
        let id = demande
            .workflow
            .clone()
            .unwrap_or_else(|| workflow_defaut(&contexte.dossier));
        let wf = workflows::obtenir(&id)?;
        if wf.tache() != TACHE || !wf.accepte_variation() {
            return Err(format!("{} ne permet pas de redessiner une image", nom_de(&wf)));
        }
        return Ok(id);
    }
    let id = demande
        .workflow
        .clone()
        .unwrap_or_else(|| workflow_references(&contexte.dossier));
    let wf = workflows::obtenir(&id)?;
    if wf.reserve_retouche() {
        return Err(format!(
            "{} est réservé à la retouche : Mélanger des images passe par un autre modèle",
            nom_de(&wf)
        ));
    }
    let max = wf.accepte_references();
    if !(1..=max).contains(&demande.references.len()) {
        return Err(format!(
            "{} accepte de 1 à {max} images de référence",
            nom_de(&wf)
        ));
    }
    Ok(id)
}

/// Les LLM installés : les favoris d'abord, avec leur nom ; les autres avec le nom de leur fiche, sinon celui du
/// fichier.
pub fn llm_au_choix(modeles: &std::path::Path) -> Vec<ChoixLlm> {
    let presents = llm::disponibles(modeles);
    let mut choix: Vec<ChoixLlm> = llm::FAVORIS
        .iter()
        .filter(|(f, _)| presents.iter().any(|p| p == f))
        .map(|(f, nom)| ChoixLlm {
            fichier: (*f).to_owned(),
            nom: (*nom).to_owned(),
            favori: true,
            specialiste: false,
        })
        .collect();
    for f in presents {
        if choix.iter().any(|c| c.fichier == f) {
            continue;
        }
        let fiche = fiches::fiche_llm(&f);
        let nom = fiche
            .as_ref()
            .and_then(|x| x.get("nom"))
            .and_then(Value::as_str)
            .map_or_else(|| f.trim_end_matches(".gguf").to_owned(), str::to_owned);
        choix.push(ChoixLlm {
            specialiste: fiche.as_ref().is_some_and(|x| x.get("specialiste").is_some()),
            fichier: f,
            nom,
            favori: false,
        });
    }
    choix
}

fn nom_de(wf: &Workflow) -> String {
    wf.fiche
        .get("nom")
        .and_then(Value::as_str)
        .unwrap_or(&wf.id)
        .to_owned()
}

/// Ce qu'une demande contrôlée devient.
#[derive(Debug, Clone, PartialEq)]
pub struct Validee {
    pub workflow: String,
    /// L'idée ou le prompt, sans les espaces autour.
    pub texte: String,
    /// Le GGUF qui réécrira, s'il faut réécrire.
    pub llm: Option<String>,
    /// Les LoRA contrôlées.
    pub loras: Vec<ChoixLora>,
}

/// Contrôle une demande, comme la v2 avant de la mettre en file.
///
/// # Errors
/// Le message de la v2 : idée vide, workflow indisponible, modèle absent, LLM absent, format inconnu, nombre hors
/// bornes, ou refus du filtre AS-18.
pub fn valider(contexte: &Contexte, demande: &Demande) -> Result<Validee, String> {
    let prompt = demande.prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err("Écris ton idée".to_owned());
    }
    let avec_references = !demande.references.is_empty();
    let id = if avec_references {
        valider_references(contexte, demande)?
    } else {
        demande
            .workflow
            .clone()
            .unwrap_or_else(|| workflow_defaut(&contexte.dossier))
    };
    let indisponible = || format!("Workflow indisponible pour créer : {id}");
    let wf = workflows::obtenir(&id).map_err(|_| indisponible())?;
    if (!avec_references && wf.tache() != TACHE) || !wf.manquants(&contexte.modeles).is_empty() {
        return Err(indisponible());
    }
    let reglages = reglages_workflow(&contexte.dossier, &id);
    let prets = wf.variantes(&contexte.modeles, reglages.as_ref());
    match &demande.modele {
        Some(modele) if !prets.contains(modele) => {
            return Err(format!(
                "Modèle absent de modeles/{} : {modele}",
                wf.dossier_modeles()
            ));
        }
        None if prets.is_empty() => {
            let checkpoint = wf
                .regle(&contexte.modeles, reglages.as_ref())
                .get("modele")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_default();
            return Err(format!(
                "Checkpoint à valider : {checkpoint}. Ses steps ne sont pas connus : valide-les avant de t'en servir"
            ));
        }
        _ => {}
    }
    let image_source = demande.mode_reference.as_deref() == Some("variation")
        && demande.format == crate::retouche::FORMAT_IMAGE;
    if !FORMATS.contains(&demande.format.as_str()) && !image_source {
        return Err(format!("Format inconnu : {}", demande.format));
    }
    if !(1..=NOMBRE_MAX).contains(&demande.nombre) {
        return Err(format!("Nombre d'images entre 1 et {NOMBRE_MAX}"));
    }
    let llm = if demande.reecrire {
        let choisi = demande
            .llm
            .clone()
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| llm::de_workflow(&contexte.dossier, &contexte.modeles, &id));
        if !llm::disponibles(&contexte.modeles).contains(&choisi) {
            return Err(format!("GGUF absent de modeles/llm : {choisi}"));
        }
        Some(choisi)
    } else {
        None
    };
    // Le filtre lit aussi les idées et prompts des rushes de départ et de leurs ancêtres : il ne lit pas les images.
    let loras = valider_loras(contexte, &wf, &demande.loras)?;
    let origine = creations::textes_origine(&contexte.data, &demande.references)?;
    let mut textes: Vec<&str> = vec![prompt.as_str()];
    textes.extend(origine.iter().map(String::as_str));
    textes.extend(textes_loras(&loras));
    securite::verifier(&contexte.dossier, &textes).map_err(|e| e.to_string())?;
    Ok(Validee {
        workflow: id,
        texte: prompt,
        llm,
        loras,
    })
}

/// Un checkpoint déposé qui n'est pas de la bonne architecture fait échouer ComfyUI au chargement : on le dit
/// clairement plutôt que de montrer l'erreur brute (comme en v2).
pub fn expliquer_erreur(wf: &Workflow, modele: Option<&str>, variantes: &[String], message: &str) -> String {
    let origine = wf.modele_origine();
    let Some(modele) = modele.filter(|m| Some(*m) != origine.as_deref()) else {
        return message.to_owned();
    };
    let chargement_rate = RegexBuilder::new(
        r"UNETLoader|CheckpointLoader|state_dict|size mismatch|unexpected key|missing key|deserializ|could not detect model type|header too large",
    )
    .case_insensitive(true)
    .build()
    .is_ok_and(|motif| motif.is_match(message));
    if !chargement_rate {
        return message.to_owned();
    }
    let conseil = match origine {
        Some(o) if variantes.contains(&o) => format!("Reviens au modèle d'origine ({o})"),
        _ => format!("Choisis un autre checkpoint de la famille {}", wf.famille()),
    };
    let detail: String = message.chars().take(200).collect();
    format!(
        "Le checkpoint « {modele} » ne se charge pas dans {} : il n'est sans doute pas de la même architecture. \
         {conseil}. Détail : {detail}",
        nom_de(wf)
    )
}

fn aleatoire(octets: usize) -> Result<u64, String> {
    let mut tampon = [0u8; 8];
    let part = tampon
        .get_mut(..octets.min(8))
        .ok_or_else(|| "aléa demandé trop long".to_owned())?;
    getrandom::fill(part).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    Ok(u64::from_le_bytes(tampon))
}

fn nouvel_id() -> Result<String, String> {
    Ok(format!("{:012x}", aleatoire(6)?))
}

/// Une seed tirée au hasard, sous 2⁴⁸ comme en v2.
///
/// # Errors
/// Le système ne donne pas d'aléa.
pub(crate) fn seed_au_hasard() -> Result<u64, String> {
    Ok(aleatoire(6)? % SEED_MAX)
}

/// L'état partagé entre les commandes de l'interface et le fil du Plateau.
#[derive(Debug, Default)]
struct File {
    commandes: Vec<Commande>,
    /// La commande en cours.
    courante: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Partage {
    file: Mutex<File>,
    /// ✕ Annuler sur la commande en cours.
    annule: AtomicBool,
}

impl Partage {
    fn file(&self) -> MutexGuard<'_, File> {
        // Une commande qui a paniqué ne doit pas bloquer la file : l'état reste lisible.
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn modifier(&self, id: &str, modification: impl FnOnce(&mut Commande)) {
        if let Some(c) = self.file().commandes.iter_mut().find(|c| c.id == id) {
            modification(c);
        }
    }

    /// Pose une commande dans la file, sans la lancer (pour les tests des modules qui exécutent).
    #[cfg(test)]
    pub(crate) fn ajouter(&self, commande: Commande) {
        self.file().commandes.push(commande);
    }

    /// Une copie d'une commande de la file (pour les tests).
    #[cfg(test)]
    pub(crate) fn commande(&self, id: &str) -> Option<Commande> {
        self.file().commandes.iter().find(|c| c.id == id).cloned()
    }
}

/// Le suivi d'une commande en cours, pour ce qui l'exécute : étape, avancement, prompt écrit, images enregistrées,
/// annulation.
pub struct Suivi<'a> {
    partage: &'a Partage,
    id: &'a str,
}

#[cfg(test)]
impl<'a> Suivi<'a> {
    /// Le suivi de la commande `id` (pour les tests des modules qui exécutent).
    pub(crate) fn de(partage: &'a Partage, id: &'a str) -> Self {
        Self { partage, id }
    }
}

impl Suivi<'_> {
    pub fn etape(&self, etape: &str) {
        self.partage.modifier(self.id, |c| etape.clone_into(&mut c.etape));
    }

    pub fn prompt(&self, prompt: &str) {
        self.partage
            .modifier(self.id, |c| prompt.clone_into(&mut c.prompt));
    }

    pub fn progression(&self, part: f64) {
        self.partage
            .modifier(self.id, |c| c.progression = part.clamp(0.0, 1.0));
    }

    /// L'avancement donné par ComfyUI (valeur sur maximum).
    pub fn progression_de(&self, valeur: u64, maximum: u64) {
        if maximum > 0 {
            #[allow(clippy::cast_precision_loss)] // une barre de progression
            let part = valeur as f64 / maximum as f64;
            self.progression(part);
        }
    }

    pub fn image(&self, id: String) {
        self.partage.modifier(self.id, |c| {
            c.images.push(id);
            c.progression = 1.0;
        });
    }

    /// Casting : une génération de plus est faite.
    pub fn fait(&self, fait: u32) {
        self.partage.modifier(self.id, |c| {
            c.fait = fait;
            c.progression = 1.0;
        });
    }

    /// ✕ Annuler a été demandé.
    pub fn annulee(&self) -> bool {
        self.partage.annule.load(Ordering::Relaxed)
    }

    /// Le drapeau d'annulation, que ComfyUI surveille.
    pub fn drapeau(&self) -> &AtomicBool {
        &self.partage.annule
    }
}

/// La file du Plateau et son fil d'exécution.
pub struct Plateau {
    contexte: Contexte,
    partage: Arc<Partage>,
    envoi: Sender<String>,
}

impl Plateau {
    /// Le Plateau, son fil d'exécution lancé avec ce générateur.
    pub fn demarrer(contexte: Contexte, generateur: impl Generateur) -> Self {
        let partage = Arc::new(Partage::default());
        let (envoi, reception) = mpsc::channel();
        let fil = Arc::clone(&partage);
        let contexte_fil = contexte.clone();
        std::thread::spawn(move || boucle(&contexte_fil, generateur, &fil, &reception));
        Self {
            contexte,
            partage,
            envoi,
        }
    }

    /// Les dossiers du Plateau.
    pub fn contexte(&self) -> &Contexte {
        &self.contexte
    }

    /// Ce que le Plateau propose.
    pub fn options(&self) -> Options {
        options(&self.contexte)
    }

    /// Valide les steps d'un checkpoint de la famille de ce workflow (voir [`workflows::valider_checkpoint`]).
    ///
    /// # Errors
    /// Voir [`workflows::valider_checkpoint`].
    pub fn valider_checkpoint(&self, workflow: &str, checkpoint: &str, steps: u64) -> Result<(), String> {
        workflows::valider_checkpoint(
            &self.contexte.dossier,
            &self.contexte.modeles,
            workflow,
            checkpoint,
            steps,
        )
    }

    /// Contrôle la demande et la met en file.
    ///
    /// # Errors
    /// Voir [`valider`] ; ou le fil du Plateau est arrêté.
    pub fn commander(&self, demande: &Demande) -> Result<Commande, String> {
        let validee = valider(&self.contexte, demande)?;
        let wf = workflows::obtenir(&validee.workflow)?;
        memoriser_loras(&self.contexte, &wf, &validee.loras)?;
        let mut commande = Commande::nouvelle(
            nouvel_id()?,
            "creation",
            validee.texte.clone(),
            validee.workflow,
            &demande.format,
        );
        if !demande.reecrire {
            commande.prompt = validee.texte;
        }
        commande.reecrire = demande.reecrire;
        commande.llm = validee.llm;
        commande.seed = demande.seed;
        commande.nombre = demande.nombre;
        commande.modele.clone_from(&demande.modele);
        commande.loras = validee.loras;
        commande.dossier =
            crate::rushes::valider_dossier(&self.contexte.data, demande.dossier.as_deref(), "images")?;
        if !demande.references.is_empty() {
            commande.references.clone_from(&demande.references);
            commande.mode_reference.clone_from(&demande.mode_reference);
            commande.liberte = demande.liberte;
        }
        self.mettre_en_file(commande)
    }

    /// Contrôle une demande de retouche et la met en file.
    ///
    /// # Errors
    /// Voir [`retouche::commande_retouche`] ; ou le fil du Plateau est arrêté.
    pub fn commander_retouche(&self, demande: &DemandeRetouche) -> Result<Commande, String> {
        let commande = retouche::commande_retouche(&self.contexte, demande, nouvel_id()?)?;
        self.mettre_en_file(commande)
    }

    /// Contrôle une demande d'agrandissement et la met en file.
    ///
    /// # Errors
    /// Voir [`retouche::commande_agrandissement`] ; ou le fil du Plateau est arrêté.
    pub fn commander_agrandissement(&self, demande: &DemandeAgrandissement) -> Result<Commande, String> {
        let commande = retouche::commande_agrandissement(&self.contexte, demande, nouvel_id()?)?;
        self.mettre_en_file(commande)
    }

    /// Contrôle une demande de clip (Montage) et la met en file.
    ///
    /// # Errors
    /// Voir [`crate::montage::commande_clip`] ; ou le fil du Plateau est arrêté.
    pub fn commander_clip(&self, demande: &crate::montage::DemandeClip) -> Result<Commande, String> {
        let commande = crate::montage::commande_clip(&self.contexte, demande, nouvel_id()?)?;
        self.mettre_en_file(commande)
    }

    /// Contrôle une demande d'essai de voix et la met en file.
    ///
    /// # Errors
    /// Voir [`crate::voix::commande_essai`] ; ou le fil du Plateau est arrêté.
    pub fn commander_voix(&self, demande: &crate::voix::DemandeEssai) -> Result<Commande, String> {
        let commande = crate::voix::commande_essai(&self.contexte, demande, nouvel_id()?)?;
        self.mettre_en_file(commande)
    }

    /// Contrôle une demande de tournage de séquence et la met en file ; refusée si la séquence est déjà en tournage.
    ///
    /// # Errors
    /// Voir [`crate::sequences::commande_sequence`] ; séquence déjà en file ; ou le fil du Plateau est arrêté.
    pub fn commander_sequence(&self, id: &str, plans: &[usize]) -> Result<Commande, String> {
        let en_tournage = self.partage.file().commandes.iter().any(|c| {
            c.sequence.as_deref() == Some(id)
                && matches!(c.etat, EtatCommande::Attente | EtatCommande::EnCours)
        });
        if en_tournage {
            return Err("Cette séquence est déjà en tournage".to_owned());
        }
        let commande = crate::sequences::commande_sequence(&self.contexte, id, plans, nouvel_id()?)?;
        self.mettre_en_file(commande)
    }

    /// Lance un casting (comparaison de modèles d'une tâche) ; un seul à la fois.
    ///
    /// # Errors
    /// Un casting déjà en cours, voir [`crate::casting::commande`], ou le fil du Plateau est arrêté.
    pub fn commander_casting(&self, tache: &str, workflows: &[String]) -> Result<Commande, String> {
        if self.casting_en_cours().is_some() {
            return Err("Un casting est déjà en cours".to_owned());
        }
        let commande = crate::casting::commande(&self.contexte, tache, workflows, nouvel_id()?)?;
        self.mettre_en_file(commande)
    }

    /// Le dernier casting lancé (en cours ou fini) de ce lancement.
    pub fn dernier_casting(&self) -> Option<Commande> {
        self.partage
            .file()
            .commandes
            .iter()
            .rev()
            .find(|c| c.genre == "casting")
            .cloned()
    }

    /// Le casting en attente ou en cours, s'il y en a un.
    pub fn casting_en_cours(&self) -> Option<Commande> {
        self.dernier_casting()
            .filter(|c| matches!(c.etat, EtatCommande::Attente | EtatCommande::EnCours))
    }

    /// Les plans en cours de tournage, par séquence (pour l'interface).
    pub fn sequences_en_tournage(&self) -> Vec<String> {
        self.partage
            .file()
            .commandes
            .iter()
            .filter(|c| matches!(c.etat, EtatCommande::Attente | EtatCommande::EnCours))
            .filter_map(|c| c.sequence.clone())
            .collect()
    }

    /// Ajoute une image personnelle aux créations ; rend son identifiant.
    ///
    /// # Errors
    /// Voir [`retouche::importer`].
    pub fn importer(&self, octets: &[u8], nom: &str, dossier: Option<&str>) -> Result<String, String> {
        retouche::importer(&self.contexte, octets, nom, dossier)
    }

    fn mettre_en_file(&self, commande: Commande) -> Result<Commande, String> {
        self.partage.file().commandes.push(commande.clone());
        self.envoi
            .send(commande.id.clone())
            .map_err(|_| "le Plateau est arrêté".to_owned())?;
        Ok(commande)
    }

    /// Les commandes de ce lancement, la plus récente en premier.
    pub fn commandes(&self) -> Vec<Commande> {
        self.partage.file().commandes.iter().rev().cloned().collect()
    }

    /// ✕ Annuler.
    ///
    /// # Errors
    /// Commande inconnue.
    pub fn annuler(&self, id: &str) -> Result<Commande, String> {
        let mut file = self.partage.file();
        let courante = file.courante.as_deref() == Some(id);
        let commande = file
            .commandes
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| "Commande inconnue".to_owned())?;
        if matches!(commande.etat, EtatCommande::Attente | EtatCommande::EnCours) {
            commande.annulee = true;
            if courante {
                commande.etape = "Annulation…".to_owned();
                self.partage.annule.store(true, Ordering::Relaxed);
            } else {
                commande.etat = EtatCommande::Annule;
            }
        }
        Ok(commande.clone())
    }
}

fn boucle(
    contexte: &Contexte,
    mut generateur: impl Generateur,
    partage: &Partage,
    reception: &Receiver<String>,
) {
    for id in reception {
        let commande = {
            let mut file = partage.file();
            let Some(c) = file.commandes.iter_mut().find(|c| c.id == id && !c.annulee) else {
                continue;
            };
            c.etat = EtatCommande::EnCours;
            let commande = c.clone();
            partage.annule.store(false, Ordering::Relaxed);
            file.courante = Some(id.clone());
            commande
        };
        let resultat = executer(contexte, &mut generateur, partage, &commande);
        let mut file = partage.file();
        file.courante = None;
        if let Some(c) = file.commandes.iter_mut().find(|c| c.id == id) {
            c.etape.clear();
            match resultat {
                Ok(()) => c.etat = EtatCommande::Termine,
                Err(_) if c.annulee => c.etat = EtatCommande::Annule,
                Err(erreur) => {
                    c.etat = EtatCommande::Erreur;
                    c.erreur = Some(erreur);
                }
            }
        }
    }
}

fn executer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    partage: &Partage,
    c: &Commande,
) -> Result<(), String> {
    if c.genre == "clip" {
        let suivi = Suivi { partage, id: &c.id };
        return crate::montage::executer(contexte, generateur, &suivi, c);
    }
    if c.genre == "voix" {
        let suivi = Suivi { partage, id: &c.id };
        return crate::voix::executer(contexte, generateur, &suivi, c);
    }
    if c.genre == "sequence" {
        let suivi = Suivi { partage, id: &c.id };
        return crate::sequences::executer(contexte, generateur, &suivi, c);
    }
    if c.genre == "casting" {
        let suivi = Suivi { partage, id: &c.id };
        return crate::casting::executer(contexte, generateur, &suivi, c);
    }
    if c.genre != "creation" {
        let suivi = Suivi { partage, id: &c.id };
        return retouche::executer(contexte, generateur, &suivi, c);
    }
    // Relu au moment de générer : des ajouts écrits entre-temps comptent.
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    filtre
        .verifier(&[c.idee.as_str(), c.prompt.as_str()])
        .map_err(|e| e.to_string())?;
    let wf = workflows::obtenir(&c.workflow)?;
    let reglages = reglages_workflow(&contexte.dossier, &wf.id);
    let origine = creations::textes_origine(&contexte.data, &c.references)?;
    let sources = c
        .references
        .iter()
        .map(|id| -> Result<Vec<u8>, String> {
            let chemin = creations::chemin_image(&contexte.data, id)?;
            std::fs::read(&chemin).map_err(|e| format!("Image de {id} illisible : {e}"))
        })
        .collect::<Result<Vec<Vec<u8>>, String>>()?;
    let inspirer = c.mode_reference.as_deref() == Some("inspirer") && !sources.is_empty();
    let variation = c.mode_reference.as_deref() == Some("variation") && !sources.is_empty();
    let debut_reecriture = Instant::now();
    let (texte_prompt, llm_utilise) = if c.reecrire {
        partage.modifier(&c.id, |cmd| cmd.etape = "Réécriture du prompt".to_owned());
        let vues = c
            .references
            .iter()
            .zip(&sources)
            .map(|(id, png)| -> Result<prompt::Vue, String> {
                Ok(prompt::Vue {
                    png: crate::images::pour_la_vision(png)?,
                    texte: creations::description(&contexte.data, &creations::fiche(&contexte.data, id)?, 3),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let images = prompt::Images {
            references: if inspirer { sources.len() } else { 0 },
            vues: &vues,
        };
        let ecriture = prompt::preparer_reecriture(
            &contexte.lieux(),
            &c.idee,
            c.llm.as_deref(),
            Some(&wf),
            reglages.as_ref(),
            &images,
        )?;
        let reponse = generateur.ecrire(ecriture.lancement.clone(), &ecriture.demande)?;
        let texte = prompt::lire_reecriture(&ecriture, &c.idee, &reponse.texte, &filtre)?;
        partage.modifier(&c.id, |cmd| cmd.prompt.clone_from(&texte));
        (texte, Some(ecriture.modele()))
    } else {
        (c.prompt.clone(), None)
    };
    let mut textes: Vec<&str> = vec![c.idee.as_str(), texte_prompt.as_str()];
    textes.extend(origine.iter().map(String::as_str));
    filtre.verifier(&textes).map_err(|e| e.to_string())?;
    let duree_reecriture = debut_reecriture.elapsed().as_secs_f64();
    let modele = c.modele.clone().or_else(|| {
        wf.regle(&contexte.modeles, reglages.as_ref())
            .get("modele")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let base = match c.seed {
        Some(seed) => seed,
        None => seed_au_hasard()?,
    };
    // Les rushes de départ sont téléversés une seule fois pour toutes les images.
    let mut noms = Vec::new();
    for (i, png) in sources.iter().enumerate() {
        noms.push(generateur.televerser(&format!("aria_{}_{}.png", c.id, i + 1), png)?);
    }
    for i in 0..c.nombre {
        if partage.annule.load(Ordering::Relaxed) {
            return Err(ANNULEE.to_owned());
        }
        let seed = base.wrapping_add(u64::from(i));
        partage.modifier(&c.id, |cmd| {
            cmd.etape = if c.nombre > 1 {
                format!("Génération {}/{}", i + 1, c.nombre)
            } else {
                "Génération".to_owned()
            };
            cmd.progression = 0.0;
        });
        let debut = Instant::now();
        let mut valeurs = Map::new();
        valeurs.insert(
            "prompt".to_owned(),
            json!(avec_declencheurs(&texte_prompt, &c.loras)),
        );
        valeurs.insert("seed".to_owned(), json!(seed));
        valeurs.insert("modele".to_owned(), json!(c.modele));
        valeurs.insert("prefixe".to_owned(), json!(PREFIXE));
        if !c.loras.is_empty() {
            valeurs.insert("loras".to_owned(), json!(c.loras));
        }
        if inspirer {
            valeurs.insert("image".to_owned(), json!(noms.first()));
            valeurs.insert("references".to_owned(), json!(noms.get(1..).unwrap_or_default()));
            valeurs.insert("format_sortie".to_owned(), json!(c.format));
        } else if variation {
            let format = (c.format != crate::retouche::FORMAT_IMAGE).then_some(c.format.as_str());
            valeurs.insert(
                "variation".to_owned(),
                json!({"image": noms.first(), "denoise": c.liberte, "format": format}),
            );
        } else {
            valeurs.insert("ratio".to_owned(), json!(c.format));
        }
        let api = wf.preparer(&valeurs, &contexte.modeles, reglages.as_ref(), &filtre)?;
        let fichiers = generateur
            .generer(
                &api,
                &mut |valeur, maximum| {
                    if maximum > 0 {
                        let part = valeur as f64 / maximum as f64;
                        partage.modifier(&c.id, |cmd| cmd.progression = part.clamp(0.0, 1.0));
                    }
                },
                &partage.annule,
            )
            .map_err(|e| {
                let variantes = wf.variantes(&contexte.modeles, reglages.as_ref());
                expliquer_erreur(&wf, modele.as_deref(), &variantes, &e)
            })?;
        let png = fichiers
            .into_iter()
            .find(|(nom, _)| nom.to_ascii_lowercase().ends_with(".png"))
            .map(|(_, octets)| octets)
            .ok_or_else(|| "ComfyUI n'a renvoyé aucune image".to_owned())?;
        let ecoule = debut.elapsed().as_secs_f64() + if i == 0 { duree_reecriture } else { 0.0 };
        let duree = (ecoule * 100.0).round() / 100.0;
        let infos = json!({
            "type": "creation",
            "idee": c.idee,
            "prompt": texte_prompt,
            "llm": llm_utilise,
            "workflow": c.workflow,
            "modele": modele,
            "loras": c.loras,
            "format": c.format,
            "seed": seed,
            "parent": c.references.first(),
            "references": c.references,
            "mode_reference": c.mode_reference,
            "liberte": if variation { json!(c.liberte) } else { Value::Null },
            "duree_s": duree,
        });
        let mut infos = infos.as_object().cloned().unwrap_or_default();
        if let Some(d) = &c.dossier {
            infos.insert("dossier".to_owned(), json!(d));
        }
        let id_creation = creations::enregistrer(&contexte.data, &png, &infos)?;
        partage.modifier(&c.id, |cmd| {
            cmd.images.push(id_creation);
            cmd.progression = 1.0;
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    const FORMAT: &str = "1:1 (Square)";
    const KREA: &str = "krea2_turbo";

    fn png() -> Vec<u8> {
        let mut octets = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
        octets.extend_from_slice(&13u32.to_be_bytes());
        octets.extend_from_slice(b"IHDR");
        octets.extend_from_slice(&64u32.to_be_bytes());
        octets.extend_from_slice(&48u32.to_be_bytes());
        octets
    }

    /// Un dossier de données et un dossier des modèles où les fichiers de ce workflow sont présents.
    fn contexte(temp: &tempfile::TempDir, workflow: &str) -> Contexte {
        let modeles_dir = temp.path().join("modeles");
        let wf = workflows::obtenir(workflow).unwrap();
        for f in wf.manquants(&modeles_dir) {
            let chemin = modeles::destination(
                &modeles_dir,
                f["dossier"].as_str().unwrap(),
                f["nom"].as_str().unwrap(),
            )
            .unwrap();
            fs::create_dir_all(chemin.parent().unwrap()).unwrap();
            fs::write(chemin, b"poids").unwrap();
        }
        assert!(wf.manquants(&modeles_dir).is_empty());
        Contexte {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles: modeles_dir,
            moteurs: temp.path().join("moteurs"),
            port_llama: 8095,
        }
    }

    fn demande(prompt: &str) -> Demande {
        Demande {
            prompt: prompt.to_owned(),
            workflow: Some(KREA.to_owned()),
            format: FORMAT.to_owned(),
            seed: Some(7),
            nombre: 1,
            modele: None,
            reecrire: false,
            llm: None,
            references: Vec::new(),
            mode_reference: None,
            liberte: LIBERTE_DEFAUT,
            loras: Vec::new(),
            dossier: None,
        }
    }

    #[test]
    fn loras_d_une_demande() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        let wf = workflows::obtenir(KREA).unwrap();
        let dossier = ctx.modeles.join(wf.dossier_loras());
        fs::create_dir_all(&dossier).unwrap();
        fs::write(dossier.join("neon.safetensors"), b"x").unwrap();
        let choix = |fichier: &str, force: f64, declencheurs: &str| ChoixLora {
            fichier: fichier.to_owned(),
            force,
            declencheurs: declencheurs.to_owned(),
        };
        let propres =
            valider_loras(&ctx, &wf, &[choix("neon.safetensors", 0.756, "  neon   glow ")]).unwrap();
        assert_eq!(propres, vec![choix("neon.safetensors", 0.76, "neon glow")]);
        assert!(valider_loras(&ctx, &wf, &[choix("absente.safetensors", 1.0, "")]).is_err());
        assert!(valider_loras(&ctx, &wf, &[choix("neon.safetensors", 2.5, "")]).is_err());
        assert_eq!(avec_declencheurs("a fox", &propres), "neon glow, a fox");
        assert_eq!(
            avec_declencheurs("a fox", &[choix("neon.safetensors", 1.0, "")]),
            "a fox"
        );
        memoriser_loras(&ctx, &wf, &propres).unwrap();
        let proposees = loras_proposees(&ctx, &wf);
        assert_eq!(proposees.len(), 1);
        assert_eq!(
            (
                proposees[0].nom.as_str(),
                proposees[0].declencheurs.as_str(),
                proposees[0].force
            ),
            ("neon", "neon glow", 0.76)
        );
        // Les mots déclencheurs passent au filtre AS-18 avec la demande.
        let mut d = demande("a portrait");
        d.loras = vec![choix("neon.safetensors", 1.0, "twelve-year-old girl, nude")];
        assert!(valider(&ctx, &d).is_err());
        d.loras = vec![choix("neon.safetensors", 1.0, "neon")];
        assert_eq!(valider(&ctx, &d).unwrap().loras.len(), 1);
    }

    /// Ce que le faux llama-server a reçu.
    type Ecrits = Arc<Mutex<Vec<(Lancement, llama::Demande)>>>;
    /// Ce que le faux ComfyUI a reçu en téléversement.
    type Televerses = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

    /// Un llama-server et un ComfyUI de test : notent les demandes et les graphes reçus, rendent un prompt et un PNG,
    /// ou une erreur.
    struct Faux {
        recus: Arc<Mutex<Vec<Value>>>,
        ecrits: Ecrits,
        televerses: Televerses,
        erreur: Option<String>,
        attente: Duration,
    }

    impl Generateur for Faux {
        fn televerser(&mut self, nom: &str, octets: &[u8]) -> Result<String, String> {
            self.televerses
                .lock()
                .unwrap()
                .push((nom.to_owned(), octets.to_vec()));
            Ok(format!("televerse/{nom}"))
        }

        fn ecrire(
            &mut self,
            lancement: Lancement,
            demande: &llama::Demande,
        ) -> Result<llama::Reponse, String> {
            self.ecrits.lock().unwrap().push((lancement, demande.clone()));
            Ok(llama::Reponse {
                texte: "**A red fox** sitting in fresh snow".to_owned(),
                tokens_entree: 900,
                tokens_sortie: 12,
                lecture_tps: 0.0,
                ecriture_tps: 0.0,
            })
        }

        fn generer(
            &mut self,
            api: &Value,
            progression: &mut dyn FnMut(u64, u64),
            annule: &AtomicBool,
        ) -> Result<Vec<Fichier>, String> {
            self.recus.lock().unwrap().push(api.clone());
            progression(1, 2);
            let fin = Instant::now() + self.attente;
            while Instant::now() < fin {
                if annule.load(Ordering::Relaxed) {
                    return Err(ANNULEE.to_owned());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            match &self.erreur {
                Some(e) => Err(e.clone()),
                None => Ok(vec![
                    ("aria_plateau_00001_.txt".to_owned(), b"notes".to_vec()),
                    ("aria_plateau_00001_.png".to_owned(), png()),
                ]),
            }
        }
    }

    fn faux(erreur: Option<&str>, attente: Duration) -> (Faux, Arc<Mutex<Vec<Value>>>) {
        let recus = Arc::new(Mutex::new(Vec::new()));
        (
            Faux {
                recus: Arc::clone(&recus),
                ecrits: Arc::new(Mutex::new(Vec::new())),
                televerses: Arc::new(Mutex::new(Vec::new())),
                erreur: erreur.map(str::to_owned),
                attente,
            },
            recus,
        )
    }

    /// Attend que la commande quitte la file et l'exécution.
    fn attendre(plateau: &Plateau, id: &str) -> Commande {
        let fin = Instant::now() + Duration::from_secs(20);
        loop {
            let c = plateau.commandes().into_iter().find(|c| c.id == id).unwrap();
            if !matches!(c.etat, EtatCommande::Attente | EtatCommande::EnCours) || Instant::now() > fin {
                return c;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn idee_reecrite_par_le_llm() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        let data = ctx.data.clone();
        let mut d = demande("  un renard dans la neige ");
        d.reecrire = true;
        // Aucun LLM installé : le défaut manque.
        assert_eq!(
            valider(&ctx, &d).unwrap_err(),
            format!("GGUF absent de modeles/llm : {}", llm::LLM_ORIGINE)
        );
        let llm_dir = ctx.modeles.join(crate::modeles::DOSSIER_LLM);
        fs::create_dir_all(&llm_dir).unwrap();
        fs::write(llm_dir.join("mistral.gguf"), crate::gguf::essai::gguf(&[12])).unwrap();
        assert_eq!(valider(&ctx, &d).unwrap().llm.as_deref(), Some("mistral.gguf"));
        d.llm = Some("absent.gguf".to_owned());
        assert_eq!(
            valider(&ctx, &d).unwrap_err(),
            "GGUF absent de modeles/llm : absent.gguf"
        );
        d.llm = None;
        let o = options(&ctx);
        assert_eq!(o.llm.len(), 1);
        assert_eq!((o.llm[0].nom.as_str(), o.llm[0].favori), ("mistral", false));
        assert_eq!(o.llm_par_workflow[KREA], json!("mistral.gguf"));

        let (generateur, recus) = faux(None, Duration::ZERO);
        let ecrits = Arc::clone(&generateur.ecrits);
        let plateau = Plateau::demarrer(ctx, generateur);
        let c = plateau.commander(&d).unwrap();
        assert_eq!(
            (c.idee.as_str(), c.prompt.as_str()),
            ("un renard dans la neige", "")
        );
        let fin = attendre(&plateau, &c.id);
        assert_eq!(fin.etat, EtatCommande::Termine, "{:?}", fin.erreur);
        assert_eq!(fin.prompt, "A red fox sitting in fresh snow");

        let ecrits = ecrits.lock().unwrap();
        let (lancement, demande_llm) = &ecrits[0];
        assert!(lancement.modele.ends_with("mistral.gguf"));
        assert_eq!(
            demande_llm.messages[1]["content"],
            json!("un renard dans la neige")
        );
        let consigne = demande_llm.messages[0]["content"].as_str().unwrap();
        assert!(consigne.contains("40 to 150 words"), "{consigne}");
        let wf = workflows::obtenir(KREA).unwrap();
        let (noeud, entree) = wf.entree("prompt").unwrap();
        assert_eq!(
            recus.lock().unwrap()[0][noeud.as_str()]["inputs"][entree.as_str()],
            json!("A red fox sitting in fresh snow")
        );
        let fiche = creations::fiche(&data, &fin.images[0]).unwrap();
        assert_eq!(fiche["idee"], json!("un renard dans la neige"));
        assert_eq!(fiche["prompt"], json!("A red fox sitting in fresh snow"));
        assert_eq!(fiche["llm"], json!("mistral.gguf"));
    }

    #[test]
    fn controle_comme_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        assert_eq!(valider(&ctx, &demande("  ")).unwrap_err(), "Écris ton idée");
        let v = valider(&ctx, &demande("  a red fox in the snow ")).unwrap();
        assert_eq!(
            (v.workflow.as_str(), v.texte.as_str()),
            (KREA, "a red fox in the snow")
        );
        assert_eq!(v.llm, None);

        let mut d = demande("a fox");
        d.format = "5:4".to_owned();
        assert_eq!(valider(&ctx, &d).unwrap_err(), "Format inconnu : 5:4");
        let mut d = demande("a fox");
        d.nombre = 0;
        assert_eq!(valider(&ctx, &d).unwrap_err(), "Nombre d'images entre 1 et 50");
        d.nombre = 51;
        assert!(valider(&ctx, &d).is_err());
        let mut d = demande("a fox");
        d.workflow = Some("minimax_h3_t2v".to_owned());
        assert_eq!(
            valider(&ctx, &d).unwrap_err(),
            "Workflow indisponible pour créer : minimax_h3_t2v"
        );
        // Qwen-Image n'a pas ses fichiers dans ce dossier.
        d.workflow = Some(WORKFLOW_DEFAUT.to_owned());
        assert!(valider(&ctx, &d).is_err());
        d.workflow = Some("inconnu".to_owned());
        assert!(valider(&ctx, &d).is_err());
        let mut d = demande("a fox");
        d.modele = Some("autre.safetensors".to_owned());
        assert!(
            valider(&ctx, &d)
                .unwrap_err()
                .starts_with("Modèle absent de modeles/")
        );
        let refus = valider(&ctx, &demande("a twelve-year-old girl, nude")).unwrap_err();
        assert!(refus.starts_with("ERREUR AS-18"), "{refus}");
    }

    #[test]
    fn favori_de_l_utilisateur() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        assert_eq!(workflow_defaut(&dossier), WORKFLOW_DEFAUT);
        fs::write(
            dossier.reglages(),
            r#"{"favoris": {"generation": {"texte_vers_image": "krea2_turbo"}}}"#,
        )
        .unwrap();
        assert_eq!(workflow_defaut(&dossier), KREA);
        // Un workflow retiré d'Aria ne reste pas favori.
        fs::write(
            dossier.reglages(),
            r#"{"favoris": {"generation": {"texte_vers_image": "retire"}}}"#,
        )
        .unwrap();
        assert_eq!(workflow_defaut(&dossier), WORKFLOW_DEFAUT);
    }

    #[test]
    fn options_proposees() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        let o = options(&ctx);
        assert_eq!(o.formats.len(), FORMATS.len());
        assert_eq!(o.nombre_max, NOMBRE_MAX);
        assert_eq!(o.defaut, WORKFLOW_DEFAUT);
        let krea = o.workflows.iter().find(|w| w.id == KREA).unwrap();
        assert_eq!(krea.modele_defaut, krea.origine);
        assert_eq!(krea.modeles, krea.origine.clone().into_iter().collect::<Vec<_>>());
        assert!(krea.a_valider.is_empty());
        // Les workflows vidéo ne sont pas proposés ; Qwen-Image est incomplet ici.
        assert!(o.workflows.iter().all(|w| w.id != "minimax_h3_t2v"));
        assert_eq!(o.incomplets, vec!["Qwen-Image 2.1".to_owned()]);
    }

    #[test]
    fn un_checkpoint_a_valider_puis_valide() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, WORKFLOW_DEFAUT);
        let qwen = options(&ctx)
            .workflows
            .into_iter()
            .find(|w| w.id == WORKFLOW_DEFAUT)
            .unwrap();
        let origine = qwen.origine.clone().unwrap();
        assert!(qwen.modeles.is_empty());
        assert_eq!(qwen.a_valider, vec![origine.clone()]);
        assert_eq!(qwen.steps_suggeres, vec![25, 40]);
        let mut d = demande("a fox");
        d.workflow = Some(WORKFLOW_DEFAUT.to_owned());
        assert!(
            valider(&ctx, &d)
                .unwrap_err()
                .starts_with("Checkpoint à valider : ")
        );

        let (generateur, recus) = faux(None, Duration::ZERO);
        let plateau = Plateau::demarrer(ctx, generateur);
        plateau.valider_checkpoint(WORKFLOW_DEFAUT, &origine, 25).unwrap();
        let qwen = plateau
            .options()
            .workflows
            .into_iter()
            .find(|w| w.id == WORKFLOW_DEFAUT)
            .unwrap();
        assert_eq!(qwen.modeles, vec![origine]);
        assert!(qwen.a_valider.is_empty());
        let c = plateau.commander(&d).unwrap();
        assert_eq!(attendre(&plateau, &c.id).etat, EtatCommande::Termine);
        let wf = workflows::obtenir(WORKFLOW_DEFAUT).unwrap();
        let (noeud, entree) = wf.entree("steps").unwrap();
        assert_eq!(
            recus.lock().unwrap()[0][noeud.as_str()]["inputs"][entree.as_str()],
            json!(25)
        );
    }

    #[test]
    fn cree_et_enregistre_les_images() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        let data = ctx.data.clone();
        let (generateur, recus) = faux(None, Duration::ZERO);
        let plateau = Plateau::demarrer(ctx, generateur);
        let mut d = demande("a red fox in the snow");
        d.nombre = 2;
        let c = plateau.commander(&d).unwrap();
        assert_eq!(c.etat, EtatCommande::Attente);
        let fin = attendre(&plateau, &c.id);
        assert_eq!(fin.etat, EtatCommande::Termine, "{:?}", fin.erreur);
        assert_eq!(fin.images.len(), 2);
        assert!(fin.etape.is_empty());

        // Graphes : prompt, seeds qui se suivent, steps du checkpoint, préfixe et format posés.
        let recus = recus.lock().unwrap();
        let wf = workflows::obtenir(KREA).unwrap();
        let lire = |api: &Value, nom: &str| {
            let (noeud, entree) = wf.entree(nom).unwrap();
            api[noeud.as_str()]["inputs"][entree.as_str()].clone()
        };
        assert_eq!(recus.len(), 2);
        assert_eq!(lire(&recus[0], "prompt"), json!("a red fox in the snow"));
        assert_eq!(lire(&recus[0], "seed"), json!(7));
        assert_eq!(lire(&recus[1], "seed"), json!(8));
        assert_eq!(lire(&recus[0], "steps"), json!(8));
        assert_eq!(lire(&recus[0], "prefixe"), json!(PREFIXE));
        assert_eq!(lire(&recus[0], "ratio"), json!(FORMAT));

        // Fiches de la v2.
        let fiche = creations::fiche(&data, &fin.images[0]).unwrap();
        assert_eq!(fiche["type"], json!("creation"));
        assert_eq!(fiche["prompt"], json!("a red fox in the snow"));
        assert_eq!(fiche["workflow"], json!(KREA));
        assert_eq!(fiche["modele"], json!(wf.modele_origine()));
        assert_eq!(fiche["seed"], json!(7));
        assert_eq!(fiche["largeur"], json!(64));
        assert_eq!(fiche["hauteur"], json!(48));
        assert_eq!(creations::fiche(&data, &fin.images[1]).unwrap()["seed"], json!(8));
    }

    #[test]
    fn une_erreur_est_rapportee() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        let (generateur, _) = faux(Some("ComfyUI ne répond pas"), Duration::ZERO);
        let plateau = Plateau::demarrer(ctx, generateur);
        let c = plateau.commander(&demande("a fox")).unwrap();
        let fin = attendre(&plateau, &c.id);
        assert_eq!(fin.etat, EtatCommande::Erreur);
        assert_eq!(fin.erreur.as_deref(), Some("ComfyUI ne répond pas"));
        // La file continue après une erreur.
        let (generateur, _) = faux(None, Duration::ZERO);
        let ctx = contexte(&temp, KREA);
        let plateau = Plateau::demarrer(ctx, generateur);
        let c = plateau.commander(&demande("a fox")).unwrap();
        assert_eq!(attendre(&plateau, &c.id).etat, EtatCommande::Termine);
    }

    #[test]
    fn annuler_en_cours_et_en_attente() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        let (generateur, recus) = faux(None, Duration::from_secs(10));
        let plateau = Plateau::demarrer(ctx, generateur);
        let premiere = plateau.commander(&demande("a fox")).unwrap();
        let seconde = plateau.commander(&demande("a cat")).unwrap();
        // La première attend dans ComfyUI.
        let fin = Instant::now() + Duration::from_secs(10);
        while recus.lock().unwrap().is_empty() && Instant::now() < fin {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(plateau.annuler(&seconde.id).unwrap().etat, EtatCommande::Annule);
        let annulee = plateau.annuler(&premiere.id).unwrap();
        assert_eq!(annulee.etape, "Annulation…");
        assert_eq!(attendre(&plateau, &premiere.id).etat, EtatCommande::Annule);
        assert_eq!(attendre(&plateau, &seconde.id).etat, EtatCommande::Annule);
        assert_eq!(recus.lock().unwrap().len(), 1);
        assert!(plateau.annuler("inconnue").is_err());
        // Les commandes, la plus récente en premier.
        let ids: Vec<String> = plateau.commandes().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![seconde.id, premiere.id]);
    }

    #[test]
    fn checkpoint_d_une_autre_architecture() {
        let wf = workflows::obtenir(WORKFLOW_DEFAUT).unwrap();
        let origine = wf.modele_origine().unwrap();
        let brut = "Error while deserializing header: size mismatch for diffusion_model";
        assert_eq!(expliquer_erreur(&wf, Some(&origine), &[], brut), brut);
        assert_eq!(expliquer_erreur(&wf, None, &[], brut), brut);
        assert_eq!(
            expliquer_erreur(&wf, Some("x.safetensors"), &[], "ComfyUI ne répond pas"),
            "ComfyUI ne répond pas"
        );
        let avec_origine = expliquer_erreur(&wf, Some("x.safetensors"), std::slice::from_ref(&origine), brut);
        assert!(
            avec_origine.starts_with("Le checkpoint « x.safetensors » ne se charge pas dans Qwen-Image 2.1")
        );
        assert!(avec_origine.contains(&format!("Reviens au modèle d'origine ({origine})")));
        let sans_origine = expliquer_erreur(&wf, Some("x.safetensors"), &[], brut);
        assert!(sans_origine.contains("Choisis un autre checkpoint de la famille Qwen-Image2.1"));
    }

    /// Krea 2 et Qwen-Image 2.1 Edit installés, steps validés ; deux rushes de départ.
    fn avec_references(temp: &tempfile::TempDir) -> (Contexte, String, String) {
        let ctx = contexte(temp, KREA);
        let edit = workflows::obtenir(WORKFLOW_REFERENCES).unwrap();
        for f in edit.manquants(&ctx.modeles) {
            let chemin = modeles::destination(
                &ctx.modeles,
                f["dossier"].as_str().unwrap(),
                f["nom"].as_str().unwrap(),
            )
            .unwrap();
            fs::create_dir_all(chemin.parent().unwrap()).unwrap();
            fs::write(chemin, b"poids").unwrap();
        }
        if let Some(origine) = edit.modele_origine() {
            workflows::valider_checkpoint(&ctx.dossier, &ctx.modeles, WORKFLOW_REFERENCES, &origine, 25)
                .unwrap();
        }
        let infos = |prompt: &str| {
            json!({"type": "creation", "prompt": prompt})
                .as_object()
                .cloned()
                .unwrap()
        };
        let un = creations::enregistrer(&ctx.data, &png(), &infos("a cat")).unwrap();
        let deux = creations::enregistrer(&ctx.data, &png(), &infos("a dog")).unwrap();
        (ctx, un, deux)
    }

    #[test]
    fn controle_des_rushes_de_depart() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, un, deux) = avec_references(&temp);
        let mut d = demande("le chat et le chien");
        d.workflow = None;
        d.references = vec![un.clone(), deux.clone()];
        d.mode_reference = Some("melanger".to_owned());
        assert_eq!(
            valider(&ctx, &d).unwrap_err(),
            "Mode de référence inconnu : melanger"
        );
        d.mode_reference = Some("inspirer".to_owned());
        assert_eq!(valider(&ctx, &d).unwrap().workflow, WORKFLOW_REFERENCES);
        d.workflow = Some(KREA.to_owned());
        assert_eq!(
            valider(&ctx, &d).unwrap_err(),
            "Krea 2 accepte de 1 à 0 images de référence"
        );
        d.mode_reference = Some("variation".to_owned());
        assert_eq!(
            valider(&ctx, &d).unwrap_err(),
            "Redessiner une image part d'une seule image de référence"
        );
        d.references = vec![un.clone()];
        d.liberte = 1.5;
        assert!(
            valider(&ctx, &d)
                .unwrap_err()
                .starts_with("Liberté entre 0,05 et 1")
        );
        d.liberte = 0.4;
        d.format = crate::retouche::FORMAT_IMAGE.to_owned();
        assert_eq!(valider(&ctx, &d).unwrap().workflow, KREA);
        // « image source » ne vaut que pour redessiner.
        let mut d = demande("x");
        d.format = crate::retouche::FORMAT_IMAGE.to_owned();
        assert_eq!(valider(&ctx, &d).unwrap_err(), "Format inconnu : image source");
        let mut d = demande("x");
        d.references = vec!["20200101-000000-000000".to_owned()];
        d.mode_reference = Some("variation".to_owned());
        assert!(valider(&ctx, &d).unwrap_err().starts_with("Création inconnue"));
    }

    #[test]
    fn s_inspirer_puis_redessiner() {
        let temp = tempfile::tempdir().unwrap();
        let (ctx, un, deux) = avec_references(&temp);
        let data = ctx.data.clone();
        let (generateur, recus) = faux(None, Duration::ZERO);
        let televerses = Arc::clone(&generateur.televerses);
        let plateau = Plateau::demarrer(ctx, generateur);

        let mut d = demande("le chat et le chien sur un canapé");
        d.workflow = None;
        d.format = "16:9 (Widescreen)".to_owned();
        d.references = vec![un.clone(), deux.clone()];
        d.mode_reference = Some("inspirer".to_owned());
        let c = plateau.commander(&d).unwrap();
        let fin = attendre(&plateau, &c.id);
        assert_eq!(fin.etat, EtatCommande::Termine, "{:?}", fin.erreur);
        assert_eq!(televerses.lock().unwrap().len(), 2);
        let api = recus.lock().unwrap()[0].clone();
        let edit = workflows::obtenir(WORKFLOW_REFERENCES).unwrap();
        let (noeud, entree) = edit.entree("image").unwrap();
        assert_eq!(
            api[noeud.as_str()]["inputs"][entree.as_str()],
            json!(format!("televerse/aria_{}_1.png", c.id))
        );
        assert_eq!(
            api["aria_ref_2"]["inputs"]["image"],
            json!(format!("televerse/aria_{}_2.png", c.id))
        );
        assert_eq!(
            api["aria_format"]["inputs"]["aspect_ratio"],
            json!("16:9 (Widescreen)")
        );
        let f = creations::fiche(&data, &fin.images[0]).unwrap();
        assert_eq!(f["references"], json!([un, deux]));
        assert_eq!(f["mode_reference"], json!("inspirer"));
        assert_eq!(f["parent"], json!(un));
        assert_eq!(f["liberte"], Value::Null);

        let mut d = demande("en hiver");
        d.format = crate::retouche::FORMAT_IMAGE.to_owned();
        d.references = vec![un.clone()];
        d.mode_reference = Some("variation".to_owned());
        d.liberte = 0.4;
        let c = plateau.commander(&d).unwrap();
        let fin = attendre(&plateau, &c.id);
        assert_eq!(fin.etat, EtatCommande::Termine, "{:?}", fin.erreur);
        let api = recus.lock().unwrap()[1].clone();
        assert_eq!(
            api["aria_var_image"]["inputs"]["image"],
            json!(format!("televerse/aria_{}_1.png", c.id))
        );
        assert_eq!(
            api["aria_var_echelle"]["class_type"],
            json!("ImageScaleToTotalPixels")
        );
        let f = creations::fiche(&data, &fin.images[0]).unwrap();
        assert_eq!(f["mode_reference"], json!("variation"));
        assert_eq!(f["liberte"], json!(0.4));
        assert_eq!(f["format"], json!(crate::retouche::FORMAT_IMAGE));
    }
}
