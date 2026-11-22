//! Le Plateau : la file des créations, comme en v2 (`app/plateau.py`), réduite pour l'instant à la création d'une
//! image depuis un prompt écrit tel quel (« Sans réécriture » en v2). La réécriture par le LLM, les références, la
//! retouche, l'agrandissement, les LoRA et la vidéo viendront ensuite.
//!
//! Une commande est contrôlée avant d'entrer dans la file (workflow prêt, modèle, format, nombre, filtre AS-18), puis
//! un fil unique les exécute une par une : graphe préparé, ComfyUI, image enregistrée dans `data\creations\` avec la
//! fiche de la v2. Le filtre AS-18 relit le prompt au moment de générer, et `Workflow::preparer` le relit encore.
//!
//! ✕ Annuler : une commande en attente ne part pas ; celle en cours s'arrête (ComfyUI interrompt la sienne). Les
//! images déjà enregistrées restent.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::comfy_serveur::{ANNULEE, Fichier};
use crate::orchestrateur::Orchestrateur;
use crate::workflows::{self, FORMATS, ReglagesWorkflow, Workflow};
use crate::{Dossier, creations, reglages, securite};

/// Images d'une même commande, au plus (contre une faute de frappe, comme en v2).
pub const NOMBRE_MAX: u32 = 50;
/// Le workflow texte → image de la v2 (`config.yaml`, `generation.texte_vers_image`).
pub const WORKFLOW_DEFAUT: &str = "qwen_image_2_1";
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Demande {
    pub prompt: String,
    /// Absent : le favori de l'utilisateur, sinon celui de la v2.
    pub workflow: Option<String>,
    pub format: String,
    /// Absente : tirée au hasard ; les images suivantes prennent la seed suivante.
    pub seed: Option<u64>,
    pub nombre: u32,
    /// Un checkpoint de la famille ; absent : celui des réglages.
    pub modele: Option<String>,
}

/// Une commande du Plateau et son avancement.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commande {
    pub id: String,
    pub prompt: String,
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
}

/// Un workflow proposé par le Plateau.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}

/// Ce que le Plateau propose : relu à chaque appel, un modèle déposé y apparaît aussitôt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub formats: Vec<String>,
    pub nombre_max: u32,
    pub defaut: String,
    /// Les workflows prêts (rien ne manque sur le disque).
    pub workflows: Vec<ChoixWorkflow>,
    /// Ceux auxquels il manque des fichiers, par leur nom.
    pub incomplets: Vec<String>,
}

/// Les dossiers dont le Plateau a besoin.
#[derive(Debug, Clone)]
pub struct Contexte {
    pub dossier: Dossier,
    pub data: PathBuf,
    pub modeles: PathBuf,
}

/// Ce qui exécute un graphe : ComfyUI par l'orchestrateur, un faux dans les tests.
pub trait Generateur: Send + 'static {
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
}

/// ComfyUI, démarré au besoin par l'orchestrateur. L'orchestrateur reste pris pendant la génération : un autre
/// usage de la carte attend la fin de l'image.
impl Generateur for Arc<Mutex<Orchestrateur>> {
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
    let mut prets = Vec::new();
    let mut incomplets = Vec::new();
    for wf in workflows::catalogue()
        .0
        .into_iter()
        .filter(|w| w.tache() == TACHE)
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
            modeles,
            id: wf.id,
        });
    }
    Options {
        formats: FORMATS.iter().map(|f| (*f).to_owned()).collect(),
        nombre_max: NOMBRE_MAX,
        defaut: workflow_defaut(&contexte.dossier),
        workflows: prets,
        incomplets,
    }
}

fn nom_de(wf: &Workflow) -> String {
    wf.fiche
        .get("nom")
        .and_then(Value::as_str)
        .unwrap_or(&wf.id)
        .to_owned()
}

/// Contrôle une demande, comme la v2 avant de la mettre en file. Rend le workflow retenu et le prompt nettoyé.
///
/// # Errors
/// Le message de la v2 : prompt vide, workflow indisponible, modèle absent, format inconnu, nombre hors bornes, ou
/// refus du filtre AS-18.
pub fn valider(contexte: &Contexte, demande: &Demande) -> Result<(String, String), String> {
    let prompt = demande.prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err("Écris ton idée".to_owned());
    }
    let id = demande
        .workflow
        .clone()
        .unwrap_or_else(|| workflow_defaut(&contexte.dossier));
    let indisponible = || format!("Workflow indisponible pour créer : {id}");
    let wf = workflows::obtenir(&id).map_err(|_| indisponible())?;
    if wf.tache() != TACHE || !wf.manquants(&contexte.modeles).is_empty() {
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
    if !FORMATS.contains(&demande.format.as_str()) {
        return Err(format!("Format inconnu : {}", demande.format));
    }
    if !(1..=NOMBRE_MAX).contains(&demande.nombre) {
        return Err(format!("Nombre d'images entre 1 et {NOMBRE_MAX}"));
    }
    securite::verifier(&contexte.dossier, &[prompt.as_str()]).map_err(|e| e.to_string())?;
    Ok((id, prompt))
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

/// L'état partagé entre les commandes de l'interface et le fil du Plateau.
#[derive(Debug, Default)]
struct File {
    commandes: Vec<Commande>,
    /// La commande en cours.
    courante: Option<String>,
}

#[derive(Debug, Default)]
struct Partage {
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
        let (workflow, prompt) = valider(&self.contexte, demande)?;
        let commande = Commande {
            id: nouvel_id()?,
            prompt,
            workflow,
            format: demande.format.clone(),
            seed: demande.seed,
            nombre: demande.nombre,
            modele: demande.modele.clone(),
            etat: EtatCommande::Attente,
            annulee: false,
            etape: String::new(),
            progression: 0.0,
            images: Vec::new(),
            erreur: None,
        };
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
    // Relu au moment de générer : des ajouts écrits entre-temps comptent.
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    filtre.verifier(&[c.prompt.as_str()]).map_err(|e| e.to_string())?;
    let wf = workflows::obtenir(&c.workflow)?;
    let reglages = reglages_workflow(&contexte.dossier, &wf.id);
    let modele = c.modele.clone().or_else(|| {
        wf.regle(&contexte.modeles, reglages.as_ref())
            .get("modele")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let base = match c.seed {
        Some(seed) => seed,
        None => aleatoire(6)? % SEED_MAX,
    };
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
        valeurs.insert("prompt".to_owned(), json!(c.prompt));
        valeurs.insert("seed".to_owned(), json!(seed));
        valeurs.insert("modele".to_owned(), json!(c.modele));
        valeurs.insert("prefixe".to_owned(), json!(PREFIXE));
        valeurs.insert("ratio".to_owned(), json!(c.format));
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
        let duree = (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0;
        let infos = json!({
            "type": "creation",
            "idee": c.prompt,
            "prompt": c.prompt,
            "llm": null,
            "workflow": c.workflow,
            "modele": modele,
            "loras": [],
            "format": c.format,
            "seed": seed,
            "parent": null,
            "references": [],
            "mode_reference": null,
            "liberte": null,
            "duree_s": duree,
        });
        let infos = infos.as_object().cloned().unwrap_or_default();
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
    use crate::modeles;
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
        }
    }

    /// Un ComfyUI de test : note les graphes reçus et rend un PNG, ou une erreur.
    struct Faux {
        recus: Arc<Mutex<Vec<Value>>>,
        erreur: Option<String>,
        attente: Duration,
    }

    impl Generateur for Faux {
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
    fn controle_comme_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, KREA);
        assert_eq!(valider(&ctx, &demande("  ")).unwrap_err(), "Écris ton idée");
        let (id, prompt) = valider(&ctx, &demande("  a red fox in the snow ")).unwrap();
        assert_eq!((id.as_str(), prompt.as_str()), (KREA, "a red fox in the snow"));

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
}
