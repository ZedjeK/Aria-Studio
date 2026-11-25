//! Le Casting, comme en v2 (`app/casting.py`) : mêmes prompts et mêmes seeds sur plusieurs modèles d'une tâche, puis
//! comparaison à l'aveugle (notes de 1 à 5, révélation, modèle par défaut).
//!
//! Ce module tient le jeu d'essai livré (`casting/`, les prompts et les idées ; les images de départ sont celles de
//! l'utilisateur, dans `data\bancs\sources\`), ce qu'un casting comparerait avant de partir (la préparation),
//! son lancement (une commande de la file du Plateau : générations et mesures ; pour le Casting LLM, réécriture des
//! idées par chaque candidat, contrôles, puis rendu) et ses résultats, au format de la v2
//! (`data\bancs\<id>\resultats.json`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Local;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::plateau::{self, Commande, Contexte, Generateur, Suivi};
use crate::securite::{self, Filtre};
use crate::workflows::{self, Workflow};
use crate::{carte, comfy_serveur, fiches, llm, montage, prompt, reglages, retouche};

/// Le jeu de prompts des castings de génération, et celui des idées du Casting LLM (livrés avec l'appli).
pub const PROMPTS: &str = include_str!("../../casting/prompts.yaml");
pub const IDEES: &str = include_str!("../../casting/idees.yaml");

/// Les tâches, dans l'ordre de la page.
pub const TACHES: [&str; 5] = [
    "texte_vers_image",
    "retouche",
    "reecriture",
    "image_vers_video",
    "texte_vers_video",
];
/// Les tâches dont chaque essai part d'une image de départ.
pub const AVEC_SOURCE: [&str; 2] = ["retouche", "image_vers_video"];
/// Les seeds de chaque prompt, les mêmes pour tous les modèles (config.yaml de la v2).
pub const SEEDS: [u64; 2] = [42, 1234];
/// La seed de rendu du Casting LLM.
pub const SEED_LLM: u64 = 42;
/// Un casting refuse de partir si d'autres applis occupent déjà plus de VRAM que ça.
pub const VRAM_MAX_OCCUPEE_GO: f64 = 2.5;
/// Les candidats du Casting LLM (s'ils sont installés), en plus du spécialiste du modèle de rendu.
pub const CANDIDATS_LLM: [&str; 2] = [
    "Ternary-Bonsai-2-27B-PTQ1_0.gguf",
    "Ternary-Bonsai-2-27B-Uncensored-Heretic-PQ2_0.gguf",
];
/// Le candidat du Casting LLM qui envoie l'idée brute, sans LLM.
pub const TEMOIN: &str = "temoin";
/// Les modèles d'un casting à l'aveugle s'appellent A, B, C…
pub const LETTRES: &str = "ABCDEFGH";

/// Un essai du jeu : son identifiant (qui nomme les images), ce qu'il teste, son prompt, et son image de départ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Essai {
    pub id: String,
    pub teste: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Le jeu d'essai d'une tâche.
///
/// # Errors
/// Tâche absente du jeu livré, ou YAML illisible.
pub fn jeu_de_prompts(tache: &str) -> Result<Vec<Essai>, String> {
    let texte = if tache == "reecriture" { IDEES } else { PROMPTS };
    let jeux: BTreeMap<String, Vec<Essai>> =
        serde_saphyr::from_str(texte).map_err(|e| format!("jeu d'essai du Casting illisible : {e}"))?;
    jeux.get(tache)
        .cloned()
        .ok_or_else(|| format!("Tâche inconnue : {tache}"))
}

// --- images de départ ---------------------------------------------------------------------------------------------

fn dossier_bancs(data: &Path) -> PathBuf {
    data.join("bancs")
}

/// Les images de départ des castings de retouche et d'image vers vidéo : celles de l'utilisateur, figées depuis un
/// casting texte vers image (« Utiliser comme source »), importées d'un fichier ou prises dans les Rushes. La v2 en
/// livrait ; la v3 n'en livre plus : elles avaient été produites par des modèles dont la licence ne permet pas de les
/// redistribuer dans un exe public.
fn dossier_sources(data: &Path) -> PathBuf {
    dossier_bancs(data).join("sources")
}

fn nom_source_valide(nom: &str) -> bool {
    !nom.is_empty()
        && nom.ends_with(".png")
        && nom
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && !nom.starts_with('.')
}

/// Une image de départ de l'utilisateur.
pub fn source(data: &Path, nom: &str) -> Option<Vec<u8>> {
    if !nom_source_valide(nom) {
        return None;
    }
    fs::read(dossier_sources(data).join(nom)).ok()
}

fn source_presente(data: &Path, nom: &str) -> bool {
    nom_source_valide(nom) && dossier_sources(data).join(nom).is_file()
}

/// D'où vient chaque image de départ : `{"portrait.png": {"banc": …, "image": …}}` pour une image figée depuis un
/// casting, `{"fichier": …}` pour une image importée, `{"rush": …}` pour une image prise dans les Rushes.
pub fn origine_sources(data: &Path) -> Map<String, Value> {
    let lu = fs::read_to_string(dossier_sources(data).join("origine.json")).ok();
    match lu.as_deref().map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Object(m))) => m,
        _ => Map::new(),
    }
}

/// Les images de départ que les castings de retouche et d'image vers vidéo attendent, par leur nom.
///
/// # Errors
/// Jeu d'essai illisible.
pub fn noms_de_toutes_les_sources() -> Result<Vec<String>, String> {
    let mut noms = Vec::new();
    for tache in AVEC_SOURCE {
        noms.extend(noms_sources(tache, &jeu_de_prompts(tache)?));
    }
    noms.sort();
    noms.dedup();
    Ok(noms)
}

/// Range une image de départ et note d'où elle vient.
fn ranger_source(data: &Path, nom: &str, png: &[u8], origine_image: Value) -> Result<(), String> {
    let d = dossier_sources(data);
    fs::create_dir_all(&d).map_err(|e| format!("{} : {e}", d.display()))?;
    fs::write(d.join(nom), png).map_err(|e| format!("{nom} : {e}"))?;
    let mut origine = origine_sources(data);
    origine.insert(nom.to_owned(), origine_image);
    let texte_origine = serde_json::to_string_pretty(&origine).map_err(|e| e.to_string())?;
    fs::write(d.join("origine.json"), texte_origine).map_err(|e| format!("origine.json : {e}"))
}

/// Pose une image de l'utilisateur comme image de départ `nom` (un fichier importé, ou une image des Rushes) : elle
/// est relue et réécrite en PNG, et sa fiche d'origine est notée (`{"fichier": …}` ou `{"rush": …}`).
///
/// # Errors
/// Nom qu'aucun casting n'attend, image illisible, ou fichier qui ne s'écrit pas.
pub fn importer_source(data: &Path, nom: &str, octets: &[u8], origine_image: Value) -> Result<(), String> {
    if !nom_source_valide(nom) || !noms_de_toutes_les_sources()?.iter().any(|n| n == nom) {
        return Err(format!("Aucun casting n'attend d'image de départ « {nom} »"));
    }
    let png = crate::images::png(&crate::images::ouvrir(octets)?)?;
    ranger_source(data, nom, &png, origine_image)
}

// --- préparation ----------------------------------------------------------------------------------------------------

/// Un candidat d'un casting : son identifiant, son nom, et le nombre de fichiers qui lui manquent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidat {
    pub id: String,
    pub nom: String,
    pub manquants: usize,
}

/// Une image de départ, et si elle est là.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceBanc {
    pub nom: String,
    pub presente: bool,
}

/// Ce que ferait un casting de cette tâche, sans rien lancer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preparation {
    pub tache: String,
    pub candidats: Vec<Candidat>,
    /// Casting LLM : le modèle qui rend les prompts.
    pub rendu: Option<Candidat>,
    pub sources: Vec<SourceBanc>,
    pub essais: Vec<Essai>,
    pub seeds: Vec<u64>,
    /// Les générations à produire.
    pub total: usize,
    /// Ce qui empêche le casting de partir ; rien s'il peut partir.
    pub blocage: Option<String>,
}

/// La préparation de toutes les tâches, dans l'ordre de la page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preparations {
    pub vram_max_occupee_go: f64,
    pub taches: Vec<Preparation>,
}

fn nom_workflow(wf: &Workflow) -> String {
    wf.fiche
        .get("nom")
        .and_then(Value::as_str)
        .unwrap_or(&wf.id)
        .to_owned()
}

fn candidats(tache: &str, ids: &[String]) -> Vec<Workflow> {
    workflows::catalogue()
        .0
        .into_iter()
        .filter(|wf| wf.tache() == tache)
        .filter(|wf| ids.is_empty() || ids.contains(&wf.id))
        .collect()
}

fn pas_complets(noms: &[String]) -> String {
    let sujet = match noms {
        [seul] => format!("{seul} n'est pas complet"),
        _ => format!("{} ne sont pas complets", noms.join(", ")),
    };
    format!("{sujet} : télécharge ce qui manque dans Modèles")
}

fn blocage_candidats(candidats: &[Workflow], modeles: &Path, choisis: bool) -> Option<String> {
    if candidats.len() < 2 {
        let message = match (choisis, candidats.len()) {
            (true, _) => "Il faut au moins deux modèles à comparer",
            (false, 1) => "Il faut au moins deux modèles à comparer : un seul fait cette tâche",
            _ => "Aucun modèle ne fait cette tâche",
        };
        return Some(message.to_owned());
    }
    let incomplets: Vec<String> = candidats
        .iter()
        .filter(|wf| !wf.manquants(modeles).is_empty())
        .map(nom_workflow)
        .collect();
    (!incomplets.is_empty()).then(|| pas_complets(&incomplets))
}

fn noms_sources(tache: &str, essais: &[Essai]) -> Vec<String> {
    if !AVEC_SOURCE.contains(&tache) {
        return Vec::new();
    }
    let mut noms: Vec<String> = essais.iter().filter_map(|e| e.source.clone()).collect();
    noms.sort();
    noms.dedup();
    noms
}

fn blocage_sources(data: &Path, tache: &str, essais: &[Essai]) -> Option<String> {
    let absentes: Vec<String> = noms_sources(tache, essais)
        .into_iter()
        .filter(|s| !source_presente(data, s))
        .collect();
    (!absentes.is_empty()).then(|| {
        let noms: Vec<&str> = absentes
            .iter()
            .map(|n| n.strip_suffix(".png").unwrap_or(n))
            .collect();
        format!(
            "Images de départ à ajouter : {}. Pour chacune, importe une image ou prends-la dans les Rushes, ou fige-en \
             une depuis un casting Texte vers image (« Utiliser comme source »)",
            noms.join(", ")
        )
    })
}

/// Le workflow qui rend les prompts du Casting LLM : le modèle par défaut de Texte vers image.
fn rendu_llm(contexte: &Contexte) -> Result<Workflow, String> {
    let id = plateau::workflow_defaut(&contexte.dossier);
    workflows::obtenir(&id)
        .map_err(|_| format!("Le modèle par défaut de Texte vers image est inconnu : {id}"))
}

/// Les candidats du Casting LLM : ceux livrés qui sont installés, puis le spécialiste du modèle de rendu.
fn candidats_llm(modeles: &Path, id_rendu: &str) -> Vec<String> {
    let installes = llm::disponibles(modeles);
    let mut candidats: Vec<String> = CANDIDATS_LLM
        .iter()
        .filter(|c| **c == TEMOIN || installes.iter().any(|i| i.as_str() == **c))
        .map(|c| (*c).to_owned())
        .collect();
    for fiche in fiches::catalogue_llm().unwrap_or_default() {
        let Some(id) = fiche.get("id").and_then(Value::as_str) else {
            continue;
        };
        let specialiste = fiche.get("specialiste");
        let pour_le_rendu = specialiste
            .and_then(|s| s.get("workflows"))
            .and_then(Value::as_array)
            .is_some_and(|w| w.iter().any(|x| x.as_str() == Some(id_rendu)));
        let creation = specialiste.and_then(|s| s.get("usage")).and_then(Value::as_str) == Some("creation");
        if pour_le_rendu
            && creation
            && installes.iter().any(|i| i == id)
            && !candidats.iter().any(|c| c == id)
        {
            candidats.push(id.to_owned());
        }
    }
    candidats
}

fn cle_llm(candidat: &str) -> String {
    candidat.strip_suffix(".gguf").unwrap_or(candidat).to_owned()
}

fn nom_llm(candidat: &str) -> String {
    if candidat == TEMOIN {
        "Sans LLM (témoin)".to_owned()
    } else {
        cle_llm(candidat)
    }
}

/// Ce que ferait un casting de cette tâche, sans rien lancer.
///
/// # Errors
/// Tâche inconnue, ou jeu d'essai illisible.
pub fn preparation_de(contexte: &Contexte, tache: &str) -> Result<Preparation, String> {
    let essais = jeu_de_prompts(tache)?;
    let mut p = Preparation {
        tache: tache.to_owned(),
        candidats: Vec::new(),
        rendu: None,
        sources: Vec::new(),
        essais: essais.clone(),
        seeds: Vec::new(),
        total: 0,
        blocage: None,
    };
    if tache == "reecriture" {
        p.seeds = vec![SEED_LLM];
        let rendu = match rendu_llm(contexte) {
            Ok(r) => r,
            Err(e) => {
                p.blocage = Some(e);
                return Ok(p);
            }
        };
        let candidats = candidats_llm(&contexte.modeles, &rendu.id);
        let manquants = rendu.manquants(&contexte.modeles).len();
        p.total = 2 * candidats.len() * essais.len(); // réécriture puis rendu
        p.blocage = if candidats.len() < 2 {
            Some("Il faut au moins deux candidats à comparer".to_owned())
        } else {
            (manquants > 0).then(|| pas_complets(&[nom_workflow(&rendu)]))
        };
        p.candidats = candidats
            .iter()
            .map(|c| Candidat {
                id: cle_llm(c),
                nom: nom_llm(c),
                manquants: 0,
            })
            .collect();
        p.rendu = Some(Candidat {
            id: rendu.id.clone(),
            nom: nom_workflow(&rendu),
            manquants,
        });
        return Ok(p);
    }
    let candidats = candidats(tache, &[]);
    p.seeds = SEEDS.to_vec();
    p.total = candidats.len() * essais.len() * SEEDS.len();
    p.sources = noms_sources(tache, &essais)
        .into_iter()
        .map(|nom| SourceBanc {
            presente: source_presente(&contexte.data, &nom),
            nom,
        })
        .collect();
    p.blocage = blocage_candidats(&candidats, &contexte.modeles, false)
        .or_else(|| blocage_sources(&contexte.data, tache, &essais));
    p.candidats = candidats
        .iter()
        .map(|wf| Candidat {
            id: wf.id.clone(),
            nom: nom_workflow(wf),
            manquants: wf.manquants(&contexte.modeles).len(),
        })
        .collect();
    Ok(p)
}

/// Pour la page Casting : chaque tâche, et la VRAM occupée au-delà de laquelle un casting refuse de partir.
///
/// # Errors
/// Jeu d'essai illisible.
pub fn preparation(contexte: &Contexte) -> Result<Preparations, String> {
    Ok(Preparations {
        vram_max_occupee_go: VRAM_MAX_OCCUPEE_GO,
        taches: TACHES
            .iter()
            .map(|t| preparation_de(contexte, t))
            .collect::<Result<_, _>>()?,
    })
}

/// Le dernier casting lancé, tel que la page le suit (comme `etat_public` de la v2).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Actif {
    /// L'identifiant du casting, et celui de sa commande (pour ✕ Annuler).
    pub id: String,
    pub commande: String,
    pub tache: String,
    /// `attente`, `en_cours`, `termine`, `erreur` ou `annule`.
    pub etat: Value,
    pub erreur: Option<String>,
    pub fait: u32,
    pub total: u32,
    pub courant: String,
}

/// Ce que lit la page Casting : le dernier casting lancé, les castings faits, et la préparation de chaque tâche.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EtatCasting {
    pub actif: Option<Actif>,
    pub castings: Vec<Value>,
    pub preparation: Preparations,
}

/// Ce que lit la page Casting ; `dernier` : le dernier casting lancé dans la file du Plateau.
///
/// # Errors
/// Jeu d'essai illisible.
pub fn etat(contexte: &Contexte, dernier: Option<&Commande>) -> Result<EtatCasting, String> {
    Ok(EtatCasting {
        actif: dernier.and_then(|c| {
            Some(Actif {
                id: c.banc.clone()?,
                commande: c.id.clone(),
                tache: c.mode_video.clone().unwrap_or_default(),
                etat: serde_json::to_value(c.etat).unwrap_or(Value::Null),
                erreur: c.erreur.clone(),
                fait: c.fait,
                total: c.nombre,
                courant: c.etape.clone(),
            })
        }),
        castings: liste(&contexte.data),
        preparation: preparation(contexte)?,
    })
}

// --- lancement ------------------------------------------------------------------------------------------------------

const GO: f64 = 1_073_741_824.0;

fn go(octets: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // des gigaoctets arrondis au centième
    let g = octets as f64 / GO;
    (g * 100.0).round() / 100.0
}

fn arrondi(secondes: f64) -> f64 {
    (secondes * 100.0).round() / 100.0
}

/// Contrôle une demande de casting comme la v2, écrit ses résultats vides (ordre à l'aveugle tiré) et rend la
/// commande à mettre en file. `workflows` : les modèles choisis, sinon tous ceux de la tâche.
///
/// # Errors
/// Casting LLM (pas encore porté), tâche inconnue, moins de deux modèles, modèle incomplet, image de départ absente,
/// ou fichier qui ne s'écrit pas.
pub fn commande(
    contexte: &Contexte,
    tache: &str,
    ids: &[String],
    id_commande: String,
) -> Result<Commande, String> {
    if !TACHES.contains(&tache) {
        return Err(format!("Tâche inconnue : {tache}"));
    }
    let essais = jeu_de_prompts(tache)?;
    let mut ws = Map::new();
    let (seeds, total, rendu) = if tache == "reecriture" {
        // Casting LLM : chaque candidat réécrit les idées, puis le modèle par défaut de Texte vers image les rend.
        let rendu = rendu_llm(contexte)?;
        let candidats = candidats_llm(&contexte.modeles, &rendu.id);
        if candidats.len() < 2 {
            return Err("Il faut au moins deux candidats à comparer".to_owned());
        }
        if !rendu.manquants(&contexte.modeles).is_empty() {
            return Err(pas_complets(&[nom_workflow(&rendu)]));
        }
        for c in &candidats {
            let fichier = if c == TEMOIN { Value::Null } else { json!(c) };
            ws.insert(
                cle_llm(c),
                json!({"nom": nom_llm(c), "fichier": fichier, "generations": [], "liberation": null, "resume": null}),
            );
        }
        (vec![SEED_LLM], 2 * candidats.len() * essais.len(), Some(rendu.id)) // réécriture puis rendu
    } else {
        let candidats = candidats(tache, ids);
        if let Some(blocage) = blocage_candidats(&candidats, &contexte.modeles, !ids.is_empty()) {
            return Err(blocage);
        }
        if let Some(blocage) = blocage_sources(&contexte.data, tache, &essais) {
            return Err(blocage);
        }
        for wf in &candidats {
            ws.insert(
                wf.id.clone(),
                json!({"nom": nom_workflow(wf), "generations": [], "liberation": null, "resume": null}),
            );
        }
        (SEEDS.to_vec(), candidats.len() * essais.len() * SEEDS.len(), None)
    };
    let maintenant = Local::now();
    let id = format!("{}_{tache}", maintenant.format("%Y-%m-%d_%H%M%S"));
    if dossier_bancs(&contexte.data).join(&id).exists() {
        return Err("Un casting de cette tâche vient de partir : attends une seconde".to_owned());
    }
    let cles: Vec<String> = ws.keys().cloned().collect();
    let mut r = Map::new();
    r.insert("id".to_owned(), json!(id));
    r.insert("tache".to_owned(), json!(tache));
    r.insert(
        "date".to_owned(),
        json!(maintenant.format("%Y-%m-%dT%H:%M:%S").to_string()),
    );
    r.insert("seeds".to_owned(), json!(seeds));
    r.insert("prompts".to_owned(), json!(essais));
    if let Some(rendu) = &rendu {
        r.insert("rendu".to_owned(), json!(rendu));
    }
    r.insert("vram_base_go".to_owned(), Value::Null);
    r.insert("workflows".to_owned(), Value::Object(ws));
    r.insert("aveugle".to_owned(), json!(melanger(cles)?));
    r.insert("revele".to_owned(), json!(false));
    r.insert("notes".to_owned(), json!({}));
    r.insert("favori".to_owned(), Value::Null);
    ecrire(&contexte.data, &r)?;
    let nom = TACHES_NOMMEES
        .iter()
        .find(|(t, _)| *t == tache)
        .map_or(tache, |(_, n)| *n);
    let mut c = Commande::nouvelle(
        id_commande,
        "casting",
        format!("Casting · {nom}"),
        String::new(),
        "",
    );
    c.nombre = u32::try_from(total).unwrap_or(u32::MAX);
    c.banc = Some(id);
    c.mode_video = Some(tache.to_owned());
    Ok(c)
}

/// Les noms des tâches, pour la file du Plateau.
const TACHES_NOMMEES: [(&str, &str); 5] = [
    ("texte_vers_image", "Texte vers image"),
    ("retouche", "Retouche"),
    ("reecriture", "Écriture (LLM)"),
    ("image_vers_video", "Image vers vidéo"),
    ("texte_vers_video", "Texte vers vidéo"),
];

/// Le pic de VRAM pendant une génération, relevé toutes les 100 ms (NVML, toute la carte).
struct Sondeur {
    pic: Arc<AtomicU64>,
    arret: Arc<AtomicBool>,
    fil: Option<JoinHandle<()>>,
}

impl Sondeur {
    fn lancer() -> Self {
        let pic = Arc::new(AtomicU64::new(carte::vram_utilisee().unwrap_or(0)));
        let arret = Arc::new(AtomicBool::new(false));
        let (p, a) = (Arc::clone(&pic), Arc::clone(&arret));
        let fil = thread::spawn(move || {
            while !a.load(Ordering::Relaxed) {
                if let Some(v) = carte::vram_utilisee() {
                    p.fetch_max(v, Ordering::Relaxed);
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        Self {
            pic,
            arret,
            fil: Some(fil),
        }
    }

    fn arreter(mut self) -> u64 {
        self.arret.store(true, Ordering::Relaxed);
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
        self.pic.load(Ordering::Relaxed)
    }
}

/// Le fichier rendu d'une génération : la vidéo s'il y en a une, sinon la première image.
fn sortie(fichiers: Vec<(String, Vec<u8>)>) -> Option<(String, Vec<u8>)> {
    let video = |n: &str| {
        let n = n.to_ascii_lowercase();
        n.ends_with(".mp4") || n.ends_with(".webm")
    };
    let mut premiere = None;
    for (nom, octets) in fichiers {
        if video(&nom) {
            return Some((nom, octets));
        }
        if premiere.is_none() {
            premiere = Some((nom, octets));
        }
    }
    premiere
}

/// Exécute un casting de génération : chaque modèle part d'une VRAM vide (sa première génération est « à froid »),
/// tous les prompts avec toutes les seeds, mesures de durée et de pic de VRAM, puis la libération mesurée.
///
/// # Errors
/// VRAM déjà occupée, filtre AS-18, workflow inconnu, ComfyUI qui échoue ou ne rend rien, annulation, ou fichiers
/// qui ne s'écrivent pas. Les générations faites restent dans les résultats.
#[allow(clippy::too_many_lines)] // le banc de la v2, gardé d'un tenant pour se relire à côté de lui
pub fn executer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    suivi: &Suivi<'_>,
    c: &Commande,
) -> Result<(), String> {
    let id = c.banc.as_deref().ok_or_else(|| "Casting manquant".to_owned())?;
    let mut r = charger(&contexte.data, id)?;
    let tache = r
        .get("tache")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    // Refuse de partir si d'autres applis occupent déjà la VRAM (sans NVML, rien ne se mesure).
    let base = carte::vram_utilisee();
    if let Some(b) = base
        && go(b) > VRAM_MAX_OCCUPEE_GO
    {
        return Err(format!("VRAM déjà occupée : {} Go", go(b)).replace('.', ","));
    }
    r.insert("vram_base_go".to_owned(), json!(base.map(go)));
    ecrire(&contexte.data, &r)?;
    let essais: Vec<Essai> = serde_json::from_value(r.get("prompts").cloned().unwrap_or_default())
        .map_err(|e| format!("Casting illisible : {e}"))?;
    let seeds: Vec<u64> = serde_json::from_value(r.get("seeds").cloned().unwrap_or_default())
        .map_err(|e| format!("Casting illisible : {e}"))?;
    let ordre = aveugle(&r);
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    if tache == "reecriture" {
        let banc = BancLlm {
            essais: &essais,
            seed: seeds.first().copied().unwrap_or(SEED_LLM),
            ordre: &ordre,
            filtre: &filtre,
        };
        return executer_llm(contexte, generateur, suivi, &mut r, &banc);
    }
    let mut fait: u32 = 0;
    let ids: Vec<String> = workflows_de(&r).keys().cloned().collect();
    for wid in &ids {
        let wf = workflows::obtenir(wid)?;
        let reglage = reglages::section(&contexte.dossier, "workflows")
            .and_then(|s| s.get(wid).and_then(Value::as_object).cloned());
        let lettre = ordre
            .iter()
            .position(|o| o == wid)
            .and_then(|i| LETTRES.chars().nth(i))
            .unwrap_or('?');
        let d = dossier_bancs(&contexte.data).join(id).join(wid);
        fs::create_dir_all(&d).map_err(|e| format!("{} : {e}", d.display()))?;
        generateur.liberer()?; // chaque modèle part d'une VRAM vide : sa 1re génération est « à froid »
        let mut gens: Vec<Value> = Vec::new();
        for essai in &essais {
            for seed in &seeds {
                if suivi.annulee() {
                    return Err(comfy_serveur::ANNULEE.to_owned());
                }
                // Jamais le nom du modèle : le casting est à l'aveugle.
                suivi.etape(&format!("Modèle {lettre} · {} · seed {seed}", essai.id));
                suivi.progression(0.0);
                let mut valeurs = Map::new();
                valeurs.insert("prompt".to_owned(), json!(essai.prompt));
                valeurs.insert("seed".to_owned(), json!(seed));
                valeurs.insert("prefixe".to_owned(), json!(format!("aria_banc/{id}/{wid}")));
                if AVEC_SOURCE.contains(&tache.as_str()) {
                    let nom = essai.source.clone().unwrap_or_default();
                    let octets = source(&contexte.data, &nom)
                        .ok_or_else(|| format!("Image de départ absente : {nom}"))?;
                    valeurs.insert(
                        "image".to_owned(),
                        json!(generateur.televerser(&format!("aria_banc_{nom}"), &octets)?),
                    );
                }
                let api = wf.preparer(&valeurs, &contexte.modeles, reglage.as_ref(), &filtre)?;
                let sondeur = Sondeur::lancer();
                let debut = Instant::now();
                let resultat =
                    generateur.generer(&api, &mut |v, m| suivi.progression_de(v, m), suivi.drapeau());
                let duree = debut.elapsed().as_secs_f64();
                let pic = sondeur.arreter();
                let (nom_sortie, octets) =
                    sortie(resultat?).ok_or_else(|| format!("{wid} n'a rien produit pour {}", essai.id))?;
                let extension = Path::new(&nom_sortie)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map_or_else(|| "png".to_owned(), str::to_ascii_lowercase);
                let nom = format!("{}_{seed}.{extension}", essai.id);
                fs::write(d.join(&nom), octets).map_err(|e| format!("{nom} : {e}"))?;
                gens.push(json!({
                    "prompt": essai.id, "seed": seed, "image": format!("{wid}/{nom}"), "froid": gens.is_empty(),
                    "duree_s": arrondi(duree), "pic_vram_go": go(pic), "pic_ram_go": null,
                }));
                if let Some(w) = r
                    .get_mut("workflows")
                    .and_then(|ws| ws.get_mut(wid))
                    .and_then(Value::as_object_mut)
                {
                    w.insert("generations".to_owned(), Value::Array(gens.clone()));
                }
                ecrire(&contexte.data, &r)?;
                fait += 1;
                suivi.fait(fait);
            }
        }
        // La libération, mesurée : ce que la VRAM retrouve quand le modèle s'en va.
        suivi.etape(&format!("Modèle {lettre} · libération"));
        let avant = carte::vram_utilisee();
        let debut = Instant::now();
        generateur.liberer()?;
        let liberation = json!({
            "vram_avant_go": avant.map(go), "vram_apres_go": carte::vram_utilisee().map(go),
            "duree_s": arrondi(debut.elapsed().as_secs_f64()),
        });
        let durees = |froid: bool| -> Vec<f64> {
            gens.iter()
                .filter(|g| g.get("froid").and_then(Value::as_bool) == Some(froid))
                .filter_map(|g| g.get("duree_s").and_then(Value::as_f64))
                .collect()
        };
        let chaud = durees(false);
        #[allow(clippy::cast_precision_loss)] // quelques générations
        let moyenne = (!chaud.is_empty()).then(|| arrondi(chaud.iter().sum::<f64>() / chaud.len() as f64));
        let pic = gens
            .iter()
            .filter_map(|g| g.get("pic_vram_go").and_then(Value::as_f64))
            .fold(0.0_f64, f64::max);
        let resume = json!({
            "temps_froid_s": durees(true).first(), "temps_chaud_moyen_s": moyenne,
            "pic_vram_go": pic, "pic_ram_go": null,
        });
        if let Some(w) = r
            .get_mut("workflows")
            .and_then(|ws| ws.get_mut(wid))
            .and_then(Value::as_object_mut)
        {
            w.insert("liberation".to_owned(), liberation);
            w.insert("resume".to_owned(), resume);
        }
        ecrire(&contexte.data, &r)?;
    }
    suivi.etape("");
    Ok(())
}

// --- résultats ------------------------------------------------------------------------------------------------------

fn id_valide(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn fichier(data: &Path, id: &str) -> Result<PathBuf, String> {
    let f = dossier_bancs(data).join(id).join("resultats.json");
    if id_valide(id) && f.is_file() {
        Ok(f)
    } else {
        Err(format!("Casting inconnu : {id}"))
    }
}

/// Un ordre tiré au hasard (Fisher-Yates sur l'aléa du système).
fn melanger(mut cles: Vec<String>) -> Result<Vec<String>, String> {
    for i in (1..cles.len()).rev() {
        let mut o = [0u8; 4];
        getrandom::fill(&mut o).map_err(|e| format!("aléa du système indisponible : {e}"))?;
        let j = usize::try_from(u32::from_le_bytes(o)).unwrap_or(0) % (i + 1);
        cles.swap(i, j);
    }
    Ok(cles)
}

fn ecrire(data: &Path, r: &Map<String, Value>) -> Result<(), String> {
    let id = r.get("id").and_then(Value::as_str).unwrap_or_default();
    if !id_valide(id) {
        return Err(format!("Casting inconnu : {id}"));
    }
    let d = dossier_bancs(data).join(id);
    fs::create_dir_all(&d).map_err(|e| format!("{} : {e}", d.display()))?;
    let texte = serde_json::to_string_pretty(r).map_err(|e| e.to_string())?;
    let temporaire = d.join("resultats.tmp");
    fs::write(&temporaire, texte)
        .and_then(|()| fs::rename(&temporaire, d.join("resultats.json")))
        .map_err(|e| format!("casting {id} : {e}"))
}

fn workflows_de(r: &Map<String, Value>) -> Map<String, Value> {
    r.get("workflows")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn aveugle(r: &Map<String, Value>) -> Vec<String> {
    r.get("aveugle")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn revele(r: &Map<String, Value>) -> bool {
    r.get("revele").and_then(Value::as_bool).unwrap_or(false)
}

/// Les résultats d'un casting ; un casting d'avant l'aveugle reçoit son ordre tiré au hasard (comme en v2).
///
/// # Errors
/// Casting inconnu ou illisible.
pub fn charger(data: &Path, id: &str) -> Result<Map<String, Value>, String> {
    let f = fichier(data, id)?;
    let mut r = match serde_json::from_str::<Value>(&fs::read_to_string(&f).map_err(|e| e.to_string())?) {
        Ok(Value::Object(r)) => r,
        _ => return Err(format!("Casting illisible : {id}")),
    };
    if !r.contains_key("aveugle") {
        let cles: Vec<String> = workflows_de(&r).keys().cloned().collect();
        r.insert("aveugle".to_owned(), json!(melanger(cles)?));
        r.insert("revele".to_owned(), json!(false));
        ecrire(data, &r)?;
    }
    r.entry("notes").or_insert_with(|| json!({}));
    Ok(r)
}

fn generations(w: &Value) -> Vec<Value> {
    w.get("generations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn total(r: &Map<String, Value>) -> usize {
    workflows_de(r).values().map(|w| generations(w).len()).sum()
}

fn notes(r: &Map<String, Value>) -> Map<String, Value> {
    r.get("notes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn texte(v: &Value, cle: &str) -> String {
    v.get(cle).and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// Les castings, les plus récents d'abord.
pub fn liste(data: &Path) -> Vec<Value> {
    let Ok(lecture) = fs::read_dir(dossier_bancs(data)) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = lecture
        .filter_map(Result::ok)
        .filter(|e| e.path().join("resultats.json").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|id| id_valide(id))
        .collect();
    ids.sort_unstable();
    ids.reverse();
    ids.iter()
        .filter_map(|id| charger(data, id).ok())
        .map(|r| {
            let est_revele = revele(&r);
            json!({
                "id": r.get("id"), "tache": r.get("tache"), "date": r.get("date"), "revele": est_revele,
                "nb_workflows": workflows_de(&r).len(),
                "favori": if est_revele { r.get("favori").cloned().unwrap_or(Value::Null) } else { Value::Null },
                "notees": notes(&r).len(), "total": total(&r),
            })
        })
        .collect()
}

/// L'identifiant du modèle derrière `cle` : une lettre tant que le casting est à l'aveugle, l'identifiant ensuite.
fn workflow_de(r: &Map<String, Value>, cle: &str) -> Result<String, String> {
    if !revele(r) {
        let ordre = aveugle(r);
        let rang = LETTRES.find(cle).filter(|_| cle.len() == 1);
        return rang
            .and_then(|i| ordre.get(i).cloned())
            .ok_or_else(|| format!("Modèle inconnu : {cle}"));
    }
    if workflows_de(r).contains_key(cle) {
        Ok(cle.to_owned())
    } else {
        Err(format!("Workflow inconnu : {cle}"))
    }
}

/// Le modèle de ce casting qui sert par défaut aujourd'hui (pas forcément le favori choisi dans ce casting).
fn par_defaut(contexte: &Contexte, r: &Map<String, Value>) -> Option<String> {
    let tache = r.get("tache").and_then(Value::as_str).unwrap_or_default();
    let ws = workflows_de(r);
    if tache == "reecriture" {
        let fichier = llm_par_defaut(contexte);
        return ws
            .iter()
            .find(|(_, w)| w.get("fichier").and_then(Value::as_str) == Some(fichier.as_str()))
            .map(|(cle, _)| cle.clone());
    }
    let id = match tache {
        "texte_vers_image" => plateau::workflow_defaut(&contexte.dossier),
        "retouche" => retouche::workflow_defaut(contexte),
        _ => montage::workflow_de(contexte, tache),
    };
    ws.contains_key(&id).then_some(id)
}

/// Le LLM par défaut aujourd'hui : celui retenu par un Casting LLM (`favoris.llama.modele`) s'il est installé, sinon
/// celui de l'appli (comme `modeles.llm_defaut` de la v2).
fn llm_par_defaut(contexte: &Contexte) -> String {
    llm::defaut(&contexte.dossier, &contexte.modeles)
}

/// Les résultats présentables : noms, textes et mesures masqués tant que le casting est à l'aveugle.
///
/// # Errors
/// Casting inconnu ou illisible.
pub fn vue(contexte: &Contexte, id: &str) -> Result<Value, String> {
    let r = charger(&contexte.data, id)?;
    let est_revele = revele(&r);
    let tache = r
        .get("tache")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let origine = origine_sources(&contexte.data);
    let sources_retouche: Vec<String> = if tache == "texte_vers_image" {
        noms_de_toutes_les_sources().unwrap_or_default()
    } else {
        Vec::new()
    };
    let ns = notes(&r);
    let ws = workflows_de(&r);
    let mut modeles = Vec::new();
    for (i, wid) in aveugle(&r).iter().enumerate() {
        let Some(w) = ws.get(wid) else { continue };
        let lettre = LETTRES.chars().nth(i).map(String::from).unwrap_or_default();
        let cle = if est_revele { wid.clone() } else { lettre.clone() };
        let gens: Vec<Value> = generations(w)
            .iter()
            .map(|g| {
                let image = texte(g, "image");
                let fichier = image.split_once('/').map_or(image.as_str(), |(_, f)| f).to_owned();
                let source = format!("{}.png", texte(g, "prompt"));
                json!({
                    "prompt": g.get("prompt"), "seed": g.get("seed"), "fichier": fichier,
                    "note": ns.get(&image),
                    "peut_servir_de_source": sources_retouche.contains(&source),
                    "est_la_source": origine.get(&source) == Some(&json!({"banc": id, "image": image})),
                    // Le texte du prompt trahirait le témoin (resté en français) : masqué jusqu'à la révélation.
                    "texte": if est_revele { g.get("texte").cloned().unwrap_or(Value::Null) } else { Value::Null },
                    "controles": if est_revele { g.get("controles").cloned().unwrap_or(Value::Null) } else { Value::Null },
                })
            })
            .collect();
        modeles.push(json!({
            "cle": cle,
            "nom": if est_revele { w.get("nom").cloned().unwrap_or(json!(wid)) } else { json!(format!("Modèle {lettre}")) },
            "generations": gens,
            "resume": if est_revele { w.get("resume").cloned().unwrap_or(Value::Null) } else { Value::Null },
            "liberation": if est_revele { w.get("liberation").cloned().unwrap_or(Value::Null) } else { Value::Null },
            // Le témoin n'a pas de bouton « Choisir par défaut » ; à l'aveugle, rien ne le trahit.
            "choisissable": !est_revele || peut_etre_par_defaut(&tache, w),
        }));
    }
    Ok(json!({
        "id": r.get("id"), "tache": tache, "date": r.get("date"), "seeds": r.get("seeds"),
        "prompts": r.get("prompts"), "vram_base_go": r.get("vram_base_go"), "revele": est_revele,
        "favori": if est_revele { r.get("favori").cloned().unwrap_or(Value::Null) } else { Value::Null },
        "par_defaut": if est_revele { json!(par_defaut(contexte, &r)) } else { Value::Null },
        "notees": ns.len(), "total": total(&r), "modeles": modeles,
    }))
}

/// Une image (ou un clip) d'un casting.
///
/// # Errors
/// Casting, modèle ou image inconnus.
pub fn chemin_image(data: &Path, id: &str, cle: &str, fichier_image: &str) -> Result<PathBuf, String> {
    let r = charger(data, id)?;
    let wid = workflow_de(&r, cle)?;
    let propre = !fichier_image.is_empty()
        && !fichier_image.starts_with('.')
        && fichier_image
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
    let chemin = dossier_bancs(data).join(id).join(&wid).join(fichier_image);
    if propre && chemin.is_file() {
        Ok(chemin)
    } else {
        Err("Image introuvable".to_owned())
    }
}

/// Note une image de 1 à 5.
///
/// # Errors
/// Note hors bornes, casting, modèle ou image inconnus, ou fichier qui ne s'écrit pas.
pub fn noter(data: &Path, id: &str, cle: &str, fichier_image: &str, note: u8) -> Result<(), String> {
    if !(1..=5).contains(&note) {
        return Err("La note va de 1 à 5".to_owned());
    }
    let mut r = charger(data, id)?;
    let image = format!("{}/{fichier_image}", workflow_de(&r, cle)?);
    let connue = workflows_de(&r)
        .values()
        .any(|w| generations(w).iter().any(|g| texte(g, "image") == image));
    if !connue {
        return Err("Image inconnue".to_owned());
    }
    let mut ns = notes(&r);
    ns.insert(image, json!(note));
    r.insert("notes".to_owned(), Value::Object(ns));
    ecrire(data, &r)
}

/// Révèle les modèles : ça ne se défait pas.
///
/// # Errors
/// Casting inconnu, ou fichier qui ne s'écrit pas.
pub fn reveler(data: &Path, id: &str) -> Result<(), String> {
    let mut r = charger(data, id)?;
    r.insert("revele".to_owned(), json!(true));
    ecrire(data, &r)
}

/// Ce que dit le choix du témoin « Sans LLM » : il n'est pas un LLM, il ne peut pas être le LLM par défaut.
const TEMOIN_SANS_DEFAUT: &str = "« Sans LLM (témoin) » n'est pas un LLM : il ne peut pas être le LLM par défaut. \
     Choisis un des modèles ; pour écrire sans LLM, laisse la réécriture décochée au Plateau.";

/// Ce modèle peut-il devenir le modèle par défaut de sa tâche ? Tout modèle, sauf le témoin d'un Casting LLM, qui
/// n'a pas de fichier : un LLM par défaut vide ne coupe pas le LLM, il fait retomber l'appli sur un autre GGUF.
fn peut_etre_par_defaut(tache: &str, workflow: &Value) -> bool {
    tache != "reecriture" || !texte(workflow, "fichier").is_empty()
}

/// Fait du modèle choisi le modèle par défaut de sa tâche (`favoris.generation.<tâche>`, ou `favoris.llama.modele`
/// pour un Casting LLM). Le témoin « Sans LLM » d'un Casting LLM est refusé : « pas de LLM » n'est pas un état de
/// l'appli (le LLM par défaut retomberait sur un autre GGUF sans le dire).
///
/// # Errors
/// Casting pas encore révélé, modèle inconnu, témoin d'un Casting LLM, ou fichiers qui ne s'écrivent pas.
pub fn choisir_favori(contexte: &Contexte, id: &str, workflow: &str) -> Result<(), String> {
    let mut r = charger(&contexte.data, id)?;
    if !revele(&r) {
        return Err("Révèle d'abord les modèles".to_owned());
    }
    workflow_de(&r, workflow)?;
    let tache = r
        .get("tache")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let ok = workflows_de(&r)
        .get(workflow)
        .is_some_and(|w| peut_etre_par_defaut(&tache, w));
    if !ok {
        return Err(TEMOIN_SANS_DEFAUT.to_owned());
    }
    r.insert("favori".to_owned(), json!(workflow));
    ecrire(&contexte.data, &r)?;
    let (section, cle, valeur) = if tache == "reecriture" {
        let fichier = workflows_de(&r)
            .get(workflow)
            .and_then(|w| w.get("fichier"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        ("llama".to_owned(), "modele".to_owned(), fichier)
    } else {
        ("generation".to_owned(), tache, workflow.to_owned())
    };
    reglages::modifier_section(&contexte.dossier, "favoris", |favoris| {
        let s = favoris.entry(section).or_insert_with(|| json!({}));
        if !s.is_object() {
            *s = json!({});
        }
        if let Some(s) = s.as_object_mut() {
            s.insert(cle, json!(valeur));
        }
    })
    .map_err(|e| e.to_string())
}

/// Fige une image d'un casting texte vers image comme image de départ des castings de retouche ; rend son nom.
///
/// # Errors
/// Casting d'une autre tâche, image inconnue, aucune retouche qui en part, ou fichiers qui ne s'écrivent pas.
pub fn utiliser_comme_source(
    data: &Path,
    id: &str,
    cle: &str,
    fichier_image: &str,
) -> Result<String, String> {
    let r = charger(data, id)?;
    if r.get("tache").and_then(Value::as_str) != Some("texte_vers_image") {
        return Err("Seules les images d'un casting texte vers image peuvent servir de source".to_owned());
    }
    let chemin = chemin_image(data, id, cle, fichier_image)?;
    let image = format!("{}/{fichier_image}", workflow_de(&r, cle)?);
    let prompt = workflows_de(&r)
        .values()
        .flat_map(generations)
        .find(|g| texte(g, "image") == image)
        .map(|g| texte(&g, "prompt"))
        .unwrap_or_default();
    let nom = format!("{prompt}.png");
    if !noms_de_toutes_les_sources()?.contains(&nom) {
        return Err(format!("Aucun casting n'utilise de source « {prompt} »"));
    }
    let png = fs::read(&chemin).map_err(|e| format!("{nom} : {e}"))?;
    ranger_source(data, &nom, &png, json!({"banc": id, "image": image}))?;
    Ok(nom)
}

// --- Casting LLM ----------------------------------------------------------------------------------------------------

/// Ce que partagent les deux phases du Casting LLM.
struct BancLlm<'a> {
    essais: &'a [Essai],
    seed: u64,
    ordre: &'a [String],
    filtre: &'a Filtre,
}

impl BancLlm<'_> {
    /// La lettre d'un candidat : jamais son nom, le casting est à l'aveugle.
    fn lettre(&self, cle: &str) -> char {
        self.ordre
            .iter()
            .position(|o| o == cle)
            .and_then(|i| LETTRES.chars().nth(i))
            .unwrap_or('?')
    }
}

/// Le temps qu'ont pris ces tokens à ce débit ; rien sans débit connu.
fn temps(tokens: u64, par_seconde: f64) -> f64 {
    if par_seconde > 0.0 {
        #[allow(clippy::cast_precision_loss)] // quelques milliers de tokens
        let t = tokens as f64 / par_seconde;
        t
    } else {
        0.0
    }
}

fn moyenne(valeurs: &[f64]) -> Option<f64> {
    #[allow(clippy::cast_precision_loss)] // quelques idées
    let n = valeurs.len() as f64;
    (!valeurs.is_empty()).then(|| valeurs.iter().sum::<f64>() / n)
}

fn nombres(gens: &[Value], cle: &str) -> Vec<f64> {
    gens.iter()
        .filter_map(|g| g.get(cle).and_then(Value::as_f64))
        .collect()
}

/// « 21/24 » : les contrôles réussis sur ceux faits (le nombre de mots n'en est pas un), comme en v2.
fn controles_reussis(gens: &[Value]) -> String {
    let resultats: Vec<bool> = gens
        .iter()
        .filter_map(|g| g.get("controles").and_then(Value::as_object))
        .flat_map(|c| {
            c.iter()
                .filter(|(cle, _)| cle.as_str() != "mots")
                .filter_map(|(_, v)| v.as_bool())
                .collect::<Vec<_>>()
        })
        .collect();
    format!("{}/{}", resultats.iter().filter(|r| **r).count(), resultats.len())
}

/// Remplace un champ d'un candidat dans les résultats.
fn poser_champ(r: &mut Map<String, Value>, cle: &str, champ: &str, valeur: Value) {
    if let Some(w) = r
        .get_mut("workflows")
        .and_then(|ws| ws.get_mut(cle))
        .and_then(Value::as_object_mut)
    {
        w.insert(champ.to_owned(), valeur);
    }
}

/// Le Casting LLM, comme `BancLLM` de la v2 : chaque candidat réécrit les idées comme ✨ Écrire le prompt pour le
/// modèle de rendu (ses règles et sa longueur, ou la consigne du spécialiste), les prompts sont contrôlés, puis le
/// modèle de rendu les tourne tous avec la même seed. Le témoin envoie l'idée brute, sans LLM.
#[allow(clippy::too_many_lines)] // le banc de la v2, gardé d'un tenant pour se relire à côté de lui
fn executer_llm(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    suivi: &Suivi<'_>,
    r: &mut Map<String, Value>,
    b: &BancLlm<'_>,
) -> Result<(), String> {
    let id = r.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
    let id_rendu = r
        .get("rendu")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let rendu = workflows::obtenir(&id_rendu)?;
    let reglage = reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&id_rendu).and_then(Value::as_object).cloned());
    let mots_max = prompt::mots_max_de(Some(&rendu), reglage.as_ref());
    let lieux = contexte.lieux();
    let d_banc = dossier_bancs(&contexte.data).join(&id);
    let cles: Vec<String> = workflows_de(r).keys().cloned().collect();
    let mut fait: u32 = 0;

    // 1. La réécriture, candidat après candidat (chacun chargé une fois).
    for cle in &cles {
        let fichier = workflows_de(r)
            .get(cle)
            .and_then(|w| w.get("fichier"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let d = d_banc.join(cle);
        fs::create_dir_all(&d).map_err(|e| format!("{} : {e}", d.display()))?;
        let mut gens: Vec<Value> = Vec::new();
        let mut chargement: Option<f64> = None;
        for essai in b.essais {
            if suivi.annulee() {
                return Err(comfy_serveur::ANNULEE.to_owned());
            }
            let image = format!("{cle}/{}_{}.png", essai.id, b.seed);
            let Some(candidat) = &fichier else {
                // Le témoin : l'idée brute, telle quelle.
                gens.push(json!({
                    "prompt": essai.id, "seed": b.seed, "image": image, "texte": essai.prompt,
                    "controles": prompt::controler(&essai.prompt, &essai.prompt, mots_max),
                }));
                fait += 1;
                suivi.fait(fait);
                continue;
            };
            suivi.etape(&format!("Modèle {} · réécriture · {}", b.lettre(cle), essai.id));
            b.filtre
                .verifier(&[essai.prompt.as_str()])
                .map_err(|e| e.to_string())?;
            let mut ecriture = prompt::preparer_reecriture(
                &lieux,
                &essai.prompt,
                Some(candidat),
                Some(&rendu),
                reglage.as_ref(),
                &prompt::Images {
                    references: 0,
                    vues: &[],
                },
            )?;
            ecriture.demande.seed = Some(b.seed);
            let sondeur = Sondeur::lancer();
            let debut = Instant::now();
            let reponse = generateur.ecrire(ecriture.lancement.clone(), &ecriture.demande);
            let mut duree = debut.elapsed().as_secs_f64();
            let pic = sondeur.arreter();
            let reponse = reponse?;
            if chargement.is_none() {
                // Le premier appel charge le GGUF : la génération seule se déduit des débits de llama-server, le
                // reste est le chargement (la v2 le mesurait à part).
                let generation = temps(reponse.tokens_entree, reponse.lecture_tps)
                    + temps(reponse.tokens_sortie, reponse.ecriture_tps);
                let seule = if generation > 0.0 && generation < duree {
                    generation
                } else {
                    duree
                };
                chargement = Some(duree - seule);
                duree = seule;
            }
            let ecrit = prompt::lire_reecriture(&ecriture, &essai.prompt, &reponse.texte, b.filtre)?;
            b.filtre
                .verifier(&[essai.prompt.as_str(), ecrit.as_str()])
                .map_err(|e| e.to_string())?;
            gens.push(json!({
                "prompt": essai.id, "seed": b.seed, "image": image, "texte": ecrit,
                "controles": prompt::controler(&ecrit, &essai.prompt, mots_max),
                "duree_s": arrondi(duree), "ecriture_tps": (reponse.ecriture_tps * 10.0).round() / 10.0,
                "pic_vram_go": go(pic),
            }));
            poser_champ(r, cle, "generations", Value::Array(gens.clone()));
            ecrire(&contexte.data, r)?;
            fait += 1;
            suivi.fait(fait);
        }
        let resume = if fichier.is_none() {
            json!({"controles_reussis": controles_reussis(&gens)})
        } else {
            json!({
                "chargement_s": chargement.map(arrondi),
                "duree_moyenne_s": moyenne(&nombres(&gens, "duree_s")).map(arrondi),
                "ecriture_tps_moyen": moyenne(&nombres(&gens, "ecriture_tps")).map(|v| (v * 10.0).round() / 10.0),
                "pic_vram_go": nombres(&gens, "pic_vram_go").into_iter().fold(0.0_f64, f64::max),
                "controles_reussis": controles_reussis(&gens),
            })
        };
        poser_champ(r, cle, "generations", Value::Array(gens));
        poser_champ(r, cle, "resume", resume);
        ecrire(&contexte.data, r)?;
    }

    // 2. Le rendu : ComfyUI prend la carte (llama-server s'arrête) et tourne chaque prompt avec la même seed.
    for cle in &cles {
        let lus = workflows_de(r).get(cle).map(generations).unwrap_or_default();
        let mut gens = lus.clone();
        for (i, g) in lus.iter().enumerate() {
            if suivi.annulee() {
                return Err(comfy_serveur::ANNULEE.to_owned());
            }
            let (essai, ecrit, image) = (texte(g, "prompt"), texte(g, "texte"), texte(g, "image"));
            suivi.etape(&format!("Modèle {} · rendu · {essai}", b.lettre(cle)));
            suivi.progression(0.0);
            let mut valeurs = Map::new();
            valeurs.insert("prompt".to_owned(), json!(ecrit));
            valeurs.insert("seed".to_owned(), json!(b.seed));
            valeurs.insert("prefixe".to_owned(), json!(format!("aria_banc/{id}/{cle}")));
            let api = rendu.preparer(&valeurs, &contexte.modeles, reglage.as_ref(), b.filtre)?;
            let debut = Instant::now();
            let fichiers =
                generateur.generer(&api, &mut |v, m| suivi.progression_de(v, m), suivi.drapeau())?;
            let (_, octets) =
                sortie(fichiers).ok_or_else(|| format!("Aucune image rendue pour {cle} · {essai}"))?;
            fs::write(d_banc.join(&image), octets).map_err(|e| format!("{image} : {e}"))?;
            if let Some(g) = gens.get_mut(i).and_then(Value::as_object_mut) {
                g.insert(
                    "rendu_s".to_owned(),
                    json!(arrondi(debut.elapsed().as_secs_f64())),
                );
            }
            poser_champ(r, cle, "generations", Value::Array(gens.clone()));
            ecrire(&contexte.data, r)?;
            fait += 1;
            suivi.fait(fait);
        }
    }
    generateur.liberer()?;
    suivi.etape("");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dossier;

    fn contexte(temp: &tempfile::TempDir) -> Contexte {
        Contexte {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles: temp.path().join("modeles"),
            moteurs: temp.path().join("moteurs"),
            port_llama: 0,
        }
    }

    /// Un casting fini, au format de la v2 : deux modèles, deux images chacun.
    fn casting_fini(data: &Path) -> String {
        let id = "2026-10-19_222230_texte_vers_image";
        let d = dossier_bancs(data).join(id);
        for wid in ["krea2_turbo", "qwen_image_2_1"] {
            fs::create_dir_all(d.join(wid)).unwrap();
            for f in ["portrait_42.png", "neon_42.png"] {
                fs::write(d.join(wid).join(f), b"png").unwrap();
            }
        }
        let gens = |wid: &str| {
            json!([
                {"prompt": "portrait", "seed": 42, "image": format!("{wid}/portrait_42.png"), "froid": true, "duree_s": 9.1},
                {"prompt": "neon", "seed": 42, "image": format!("{wid}/neon_42.png"), "froid": false, "duree_s": 4.2},
            ])
        };
        let r = json!({
            "id": id, "tache": "texte_vers_image", "date": "2026-10-19T22:22:30", "seeds": [42],
            "prompts": [], "vram_base_go": 0.4,
            "workflows": {
                "krea2_turbo": {"nom": "Krea 2 Turbo", "generations": gens("krea2_turbo"), "resume": {"temps_froid_s": 9.1}},
                "qwen_image_2_1": {"nom": "Qwen Image 2.1", "generations": gens("qwen_image_2_1"), "resume": null},
            },
            "notes": {}, "favori": null,
        });
        fs::write(
            d.join("resultats.json"),
            serde_json::to_string_pretty(&r).unwrap(),
        )
        .unwrap();
        id.to_owned()
    }

    #[test]
    fn jeux_d_essai_livres() {
        for tache in TACHES {
            assert!(!jeu_de_prompts(tache).unwrap().is_empty(), "{tache}");
        }
        assert!(jeu_de_prompts("autre").is_err());
        // Aucune image de départ n'est livrée : sans celles de l'utilisateur, il n'y en a pas.
        let temp = tempfile::tempdir().unwrap();
        let noms = noms_de_toutes_les_sources().unwrap();
        assert!(noms.iter().any(|n| n == "portrait.png") && noms.iter().any(|n| n == "pilote.png"));
        for nom in &noms {
            assert!(source(temp.path(), nom).is_none(), "{nom}");
        }
        assert!(source(temp.path(), "../reglages.json").is_none());
        assert!(origine_sources(temp.path()).is_empty());
    }

    #[test]
    fn images_de_depart_de_l_utilisateur() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path();
        let essais = jeu_de_prompts("retouche").unwrap();
        // Sans source, la préparation dit lesquelles ajouter et comment.
        let blocage = blocage_sources(data, "retouche", &essais).unwrap();
        assert!(blocage.contains("portrait") && blocage.contains("importe une image"));
        // Un fichier importé : relu, réécrit en PNG, sa fiche d'origine notée.
        let image = crate::images::png(&image::RgbImage::new(8, 6)).unwrap();
        importer_source(data, "portrait.png", &image, json!({"fichier": "moi.jpg"})).unwrap();
        assert_eq!(source(data, "portrait.png").unwrap(), image);
        assert_eq!(
            origine_sources(data)["portrait.png"],
            json!({"fichier": "moi.jpg"})
        );
        // Une image des Rushes, pour une source de l'image vers vidéo seulement.
        importer_source(
            data,
            "pilote.png",
            &image,
            json!({"rush": "20261123-101500-abcdef"}),
        )
        .unwrap();
        assert_eq!(
            origine_sources(data)["pilote.png"]["rush"],
            "20261123-101500-abcdef"
        );
        // Un nom qu'aucun casting n'attend, ou une image illisible, sont refusés.
        assert!(importer_source(data, "autre.png", &image, json!({})).is_err());
        assert!(importer_source(data, "../x.png", &image, json!({})).is_err());
        assert!(importer_source(data, "neon.png", b"rien", json!({})).is_err());
        // Toutes là : plus de blocage.
        for nom in noms_sources("retouche", &essais) {
            importer_source(data, &nom, &image, json!({"fichier": "x.png"})).unwrap();
        }
        assert!(blocage_sources(data, "retouche", &essais).is_none());
    }

    #[test]
    fn preparation_sans_modele_installe() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let p = preparation(&ctx).unwrap();
        assert_eq!(p.taches.len(), TACHES.len());
        let t2i = &p.taches[0];
        assert_eq!(t2i.tache, "texte_vers_image");
        assert!(t2i.candidats.len() >= 2 && t2i.candidats.iter().all(|c| c.manquants > 0));
        assert!(
            t2i.blocage
                .as_deref()
                .is_some_and(|b| b.contains("télécharge ce qui manque"))
        );
        assert_eq!(t2i.total, t2i.candidats.len() * t2i.essais.len() * SEEDS.len());
        let llm = p.taches.iter().find(|t| t.tache == "reecriture").unwrap();
        assert_eq!(
            llm.blocage.as_deref(),
            Some("Il faut au moins deux candidats à comparer")
        );
        assert!(llm.rendu.is_some());
        let retouche = p.taches.iter().find(|t| t.tache == "retouche").unwrap();
        assert!(!retouche.sources.is_empty() && retouche.sources.iter().all(|s| !s.presente));
    }

    #[test]
    fn comparaison_a_l_aveugle() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let id = casting_fini(&ctx.data);
        // Un casting d'avant l'aveugle reçoit son ordre, et reste à l'aveugle.
        let v = vue(&ctx, &id).unwrap();
        assert_eq!(v["revele"], false);
        assert_eq!(v["modeles"][0]["cle"], "A");
        assert_eq!(v["modeles"][1]["nom"], "Modèle B");
        assert!(v["modeles"][0]["resume"].is_null() && v["par_defaut"].is_null());
        assert_eq!(v["modeles"][0]["generations"][0]["peut_servir_de_source"], true);
        assert!(chemin_image(&ctx.data, &id, "A", "portrait_42.png").is_ok());
        assert!(chemin_image(&ctx.data, &id, "C", "portrait_42.png").is_err());
        assert!(chemin_image(&ctx.data, &id, "A", "../resultats.json").is_err());
        noter(&ctx.data, &id, "A", "portrait_42.png", 4).unwrap();
        assert!(noter(&ctx.data, &id, "A", "portrait_42.png", 6).is_err());
        assert!(noter(&ctx.data, &id, "A", "inconnue.png", 3).is_err());
        assert!(choisir_favori(&ctx, &id, "krea2_turbo").is_err());
        let l = liste(&ctx.data);
        assert_eq!(
            (l[0]["notees"].clone(), l[0]["total"].clone()),
            (json!(1), json!(4))
        );
        // Révélé : les noms, les mesures et le modèle par défaut.
        reveler(&ctx.data, &id).unwrap();
        let v = vue(&ctx, &id).unwrap();
        let noms: Vec<&str> = v["modeles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["cle"].as_str().unwrap())
            .collect();
        assert!(noms.contains(&"krea2_turbo") && noms.contains(&"qwen_image_2_1"));
        choisir_favori(&ctx, &id, "qwen_image_2_1").unwrap();
        let favoris = reglages::section(&ctx.dossier, "favoris").unwrap();
        assert_eq!(favoris["generation"]["texte_vers_image"], "qwen_image_2_1");
        assert_eq!(vue(&ctx, &id).unwrap()["par_defaut"], "qwen_image_2_1");
        assert!(charger(&ctx.data, "../x").is_err());
    }

    /// Un Casting LLM révélé : un GGUF et le témoin « Sans LLM ».
    fn casting_llm_revele(data: &Path) -> String {
        let id = "2026-11-25_101500_reecriture";
        let d = dossier_bancs(data).join(id);
        fs::create_dir_all(&d).unwrap();
        let r = json!({
            "id": id, "tache": "reecriture", "date": "2026-11-25T10:15:00", "seeds": [42],
            "prompts": [], "vram_base_go": 0.4, "revele": true, "aveugle": ["temoin", "bonsai"],
            "workflows": {
                "bonsai": {"nom": "bonsai", "fichier": "bonsai.gguf", "generations": [], "resume": null},
                "temoin": {"nom": "Sans LLM (témoin)", "fichier": null, "generations": [], "resume": null},
            },
            "notes": {}, "favori": null,
        });
        fs::write(
            d.join("resultats.json"),
            serde_json::to_string_pretty(&r).unwrap(),
        )
        .unwrap();
        id.to_owned()
    }

    #[test]
    fn le_temoin_n_est_pas_un_llm_par_defaut() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let id = casting_llm_revele(&ctx.data);
        let v = vue(&ctx, &id).unwrap();
        let choisissable = |cle: &str| {
            let modele = v["modeles"].as_array().unwrap().iter().find(|m| m["cle"] == cle);
            modele.unwrap()["choisissable"].clone()
        };
        assert_eq!(choisissable("bonsai"), json!(true));
        assert_eq!(choisissable("temoin"), json!(false));

        let refus = choisir_favori(&ctx, &id, "temoin").unwrap_err();
        assert!(refus.contains("n'est pas un LLM"), "{refus}");
        // Rien n'a été écrit : ni le favori du casting, ni le LLM par défaut.
        assert_eq!(charger(&ctx.data, &id).unwrap()["favori"], Value::Null);
        let favoris = reglages::section(&ctx.dossier, "favoris");
        assert!(favoris.is_none_or(|f| f.get("llama").is_none()));

        choisir_favori(&ctx, &id, "bonsai").unwrap();
        let favoris = reglages::section(&ctx.dossier, "favoris").unwrap();
        assert_eq!(favoris["llama"]["modele"], "bonsai.gguf");
        assert_eq!(vue(&ctx, &id).unwrap()["favori"], "bonsai");
    }

    #[test]
    fn a_l_aveugle_rien_ne_trahit_le_temoin() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let id = casting_llm_revele(&ctx.data);
        let mut r = charger(&ctx.data, &id).unwrap();
        r.insert("revele".to_owned(), json!(false));
        ecrire(&ctx.data, &r).unwrap();
        let v = vue(&ctx, &id).unwrap();
        assert!(
            v["modeles"]
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m["choisissable"] == json!(true))
        );
    }

    #[test]
    fn image_de_depart_figee() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let id = casting_fini(&ctx.data);
        reveler(&ctx.data, &id).unwrap();
        let nom = utiliser_comme_source(&ctx.data, &id, "krea2_turbo", "portrait_42.png").unwrap();
        assert_eq!(nom, "portrait.png");
        assert_eq!(source(&ctx.data, "portrait.png").unwrap(), b"png");
        assert_eq!(
            origine_sources(&ctx.data)["portrait.png"],
            json!({"banc": id, "image": "krea2_turbo/portrait_42.png"})
        );
        let v = vue(&ctx, &id).unwrap();
        let krea = v["modeles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["cle"] == "krea2_turbo")
            .unwrap();
        assert_eq!(krea["generations"][0]["est_la_source"], true);
    }

    /// Un ComfyUI et un llama-server de test : rendent une image par graphe et un prompt par demande, comptent les
    /// libérations.
    struct Faux {
        graphes: usize,
        liberations: usize,
        ecrits: usize,
    }

    impl Generateur for Faux {
        fn ecrire(
            &mut self,
            _lancement: crate::llama::Lancement,
            demande: &crate::llama::Demande,
        ) -> Result<crate::llama::Reponse, String> {
            self.ecrits += 1;
            assert_eq!(demande.seed, Some(SEED_LLM));
            Ok(crate::llama::Reponse {
                texte: "**A red fox** sitting in fresh snow".to_owned(),
                tokens_entree: 900,
                tokens_sortie: 12,
                lecture_tps: 900.0,
                ecriture_tps: 12.0,
            })
        }

        fn televerser(&mut self, nom: &str, _octets: &[u8]) -> Result<String, String> {
            Ok(nom.to_owned())
        }

        fn generer(
            &mut self,
            _api: &Value,
            _progression: &mut dyn FnMut(u64, u64),
            _annule: &AtomicBool,
        ) -> Result<Vec<crate::comfy_serveur::Fichier>, String> {
            self.graphes += 1;
            Ok(vec![("aria_banc_00001_.png".to_owned(), b"png".to_vec())])
        }

        fn liberer(&mut self) -> Result<(), String> {
            self.liberations += 1;
            Ok(())
        }
    }

    /// Les fichiers des workflows texte vers image posés, et leurs checkpoints validés.
    fn texte_vers_image_complet(ctx: &Contexte) -> Vec<String> {
        let ids: Vec<String> = candidats("texte_vers_image", &[])
            .into_iter()
            .map(|w| w.id)
            .collect();
        for id in &ids {
            let wf = workflows::obtenir(id).unwrap();
            for f in wf.manquants(&ctx.modeles) {
                let chemin = crate::modeles::destination(
                    &ctx.modeles,
                    f["dossier"].as_str().unwrap(),
                    f["nom"].as_str().unwrap(),
                )
                .unwrap();
                fs::create_dir_all(chemin.parent().unwrap()).unwrap();
                fs::write(chemin, b"poids").unwrap();
            }
            if let Some(origine) = wf.modele_origine() {
                let steps = wf.steps_suggeres().first().copied().unwrap_or(8);
                let _ = workflows::valider_checkpoint(&ctx.dossier, &ctx.modeles, id, &origine, steps);
            }
        }
        ids
    }

    #[test]
    fn lancement_d_un_casting() {
        // Une carte déjà occupée par d'autres applis refuse le casting : le test n'a de sens que sans elle.
        if carte::vram_utilisee().is_some_and(|v| go(v) > VRAM_MAX_OCCUPEE_GO) {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        assert!(commande(&ctx, "reecriture", &[], "c0".to_owned()).is_err());
        let ids = texte_vers_image_complet(&ctx);
        assert_eq!(
            commande(&ctx, "texte_vers_image", &ids[..1], "c0".to_owned()).unwrap_err(),
            "Il faut au moins deux modèles à comparer"
        );
        let c = commande(&ctx, "texte_vers_image", &[], "c1".to_owned()).unwrap();
        let essais = jeu_de_prompts("texte_vers_image").unwrap().len();
        let total = ids.len() * essais * SEEDS.len();
        assert_eq!(c.nombre as usize, total);
        let banc = c.banc.clone().unwrap();
        // À peine lancé, le casting est là, vide et à l'aveugle.
        assert_eq!(liste(&ctx.data)[0]["total"], json!(0));
        let partage = plateau::Partage::default();
        partage.ajouter(c.clone());
        let suivi = Suivi::de(&partage, "c1");
        let mut faux = Faux {
            graphes: 0,
            liberations: 0,
            ecrits: 0,
        };
        executer(&ctx, &mut faux, &suivi, &c).unwrap();
        assert_eq!(faux.graphes, total);
        assert_eq!(faux.liberations, 2 * ids.len()); // avant chaque modèle, et sa libération mesurée
        assert_eq!(partage.commande("c1").unwrap().fait as usize, total);
        let r = charger(&ctx.data, &banc).unwrap();
        for id in &ids {
            let w = &r["workflows"][id];
            let gens = w["generations"].as_array().unwrap();
            assert_eq!(gens.len(), essais * SEEDS.len());
            assert_eq!(gens[0]["froid"], true);
            assert_eq!(gens[1]["froid"], false);
            let image = gens[0]["image"].as_str().unwrap();
            assert!(
                dossier_bancs(&ctx.data).join(&banc).join(image).is_file(),
                "{image}"
            );
            assert!(w["resume"]["temps_froid_s"].is_number() && !w["liberation"].is_null());
        }
        // La page suit le dernier casting lancé.
        let e = etat(&ctx, partage.commande("c1").as_ref()).unwrap();
        let actif = e.actif.unwrap();
        assert_eq!(
            (actif.id.as_str(), actif.tache.as_str()),
            (banc.as_str(), "texte_vers_image")
        );
        assert_eq!(vue(&ctx, &banc).unwrap()["total"], json!(total));
    }

    #[test]
    fn casting_llm() {
        if carte::vram_utilisee().is_some_and(|v| go(v) > VRAM_MAX_OCCUPEE_GO) {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        assert_eq!(
            commande(&ctx, "reecriture", &[], "c0".to_owned()).unwrap_err(),
            "Il faut au moins deux candidats à comparer"
        );
        texte_vers_image_complet(&ctx);
        let d = ctx.modeles.join("llm");
        fs::create_dir_all(&d).unwrap();
        for candidat in CANDIDATS_LLM {
            fs::write(d.join(candidat), b"gguf").unwrap();
        }
        let c = commande(&ctx, "reecriture", &[], "c1".to_owned()).unwrap();
        let idees = jeu_de_prompts("reecriture").unwrap().len();
        let total = 2 * CANDIDATS_LLM.len() * idees; // réécriture puis rendu
        assert_eq!(c.nombre as usize, total);
        let banc = c.banc.clone().unwrap();
        let partage = plateau::Partage::default();
        partage.ajouter(c.clone());
        let suivi = Suivi::de(&partage, "c1");
        let mut faux = Faux {
            graphes: 0,
            liberations: 0,
            ecrits: 0,
        };
        executer(&ctx, &mut faux, &suivi, &c).unwrap();
        assert_eq!(faux.ecrits, CANDIDATS_LLM.len() * idees);
        assert_eq!(faux.graphes, CANDIDATS_LLM.len() * idees);
        assert_eq!(partage.commande("c1").unwrap().fait as usize, total);
        let r = charger(&ctx.data, &banc).unwrap();
        assert_eq!(r["rendu"], json!(plateau::workflow_defaut(&ctx.dossier)));
        for candidat in CANDIDATS_LLM {
            let w = &r["workflows"][&cle_llm(candidat)];
            assert_eq!(w["fichier"], candidat);
            let gens = w["generations"].as_array().unwrap();
            assert_eq!(gens.len(), idees);
            // Le prompt lu comme ✨ (Markdown retiré), contrôlé, puis rendu.
            assert_eq!(gens[0]["texte"], "A red fox sitting in fresh snow");
            assert_eq!(gens[0]["controles"]["anglais"], true);
            assert!(gens[0]["rendu_s"].is_number() && gens[0]["ecriture_tps"] == json!(12.0));
            let image = gens[0]["image"].as_str().unwrap();
            assert!(
                dossier_bancs(&ctx.data).join(&banc).join(image).is_file(),
                "{image}"
            );
            assert!(w["resume"]["controles_reussis"].as_str().unwrap().contains('/'));
            assert!(w["resume"]["chargement_s"].is_number());
        }
        // Révélé, le texte écrit et ses contrôles se lisent.
        reveler(&ctx.data, &banc).unwrap();
        let v = vue(&ctx, &banc).unwrap();
        assert_eq!(
            v["modeles"][0]["generations"][0]["texte"],
            "A red fox sitting in fresh snow"
        );
    }
}
