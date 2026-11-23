//! Le Casting, comme en v2 (`app/casting.py`) : mêmes prompts et mêmes seeds sur plusieurs modèles d'une tâche, puis
//! comparaison à l'aveugle (notes de 1 à 5, révélation, modèle par défaut).
//!
//! Ce module tient le jeu d'essai livré (`casting/`), ce qu'un casting comparerait avant de partir (la préparation),
//! et les résultats d'un casting fini, au format de la v2 (`data\bancs\<id>\resultats.json`). Le lancement des bancs
//! (générations et mesures) viendra avec la suite du portage.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::plateau::{self, Contexte};
use crate::workflows::{self, Workflow};
use crate::{fiches, llm, montage, reglages, retouche};

/// Le jeu de prompts des castings de génération, et celui des idées du Casting LLM (livrés avec l'appli).
pub const PROMPTS: &str = include_str!("../../casting/prompts.yaml");
pub const IDEES: &str = include_str!("../../casting/idees.yaml");
/// D'où viennent les images de départ livrées (un banc texte vers image de l'auteur).
const ORIGINE_LIVREE: &str = include_str!("../../casting/sources/origine.json");
/// Les images de départ livrées des castings de retouche et d'image vers vidéo.
const SOURCES_LIVREES: [(&str, &[u8]); 6] = [
    ("dragon.png", include_bytes!("../../casting/sources/dragon.png")),
    ("jeu.png", include_bytes!("../../casting/sources/jeu.png")),
    (
        "miniature.png",
        include_bytes!("../../casting/sources/miniature.png"),
    ),
    ("neon.png", include_bytes!("../../casting/sources/neon.png")),
    ("pilote.png", include_bytes!("../../casting/sources/pilote.png")),
    (
        "portrait.png",
        include_bytes!("../../casting/sources/portrait.png"),
    ),
];

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

/// Les images de départ choisies dans un casting (« Utiliser comme source ») : elles remplacent celles livrées.
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

/// Une image de départ : celle choisie dans un casting, sinon celle livrée.
pub fn source(data: &Path, nom: &str) -> Option<Vec<u8>> {
    if !nom_source_valide(nom) {
        return None;
    }
    fs::read(dossier_sources(data).join(nom)).ok().or_else(|| {
        SOURCES_LIVREES
            .iter()
            .find(|(n, _)| *n == nom)
            .map(|(_, o)| o.to_vec())
    })
}

fn source_presente(data: &Path, nom: &str) -> bool {
    nom_source_valide(nom)
        && (dossier_sources(data).join(nom).is_file() || SOURCES_LIVREES.iter().any(|(n, _)| *n == nom))
}

/// D'où vient chaque image de départ : `{"portrait.png": {"banc": …, "image": …}}`.
pub fn origine_sources(data: &Path) -> Map<String, Value> {
    let lu = fs::read_to_string(dossier_sources(data).join("origine.json")).ok();
    let texte = lu.as_deref().unwrap_or(ORIGINE_LIVREE);
    match serde_json::from_str::<Value>(texte) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
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
        format!(
            "Images de départ absentes de casting/sources : {}",
            absentes.join(", ")
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

/// Ce que lit la page Casting : le casting en cours (le lancement vient avec la suite du portage), les castings
/// faits, et la préparation de chaque tâche.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EtatCasting {
    pub actif: Option<Value>,
    pub castings: Vec<Value>,
    pub preparation: Preparations,
}

/// Ce que lit la page Casting.
///
/// # Errors
/// Jeu d'essai illisible.
pub fn etat(contexte: &Contexte) -> Result<EtatCasting, String> {
    Ok(EtatCasting {
        actif: None,
        castings: liste(&contexte.data),
        preparation: preparation(contexte)?,
    })
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

/// Le LLM par défaut retenu par un Casting LLM (`favoris.llama.modele`, comme en v2), sinon celui de l'appli.
fn llm_par_defaut(contexte: &Contexte) -> String {
    reglages::section(&contexte.dossier, "favoris")
        .and_then(|f| f.get("llama")?.get("modele")?.as_str().map(str::to_owned))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| llm::defaut(&contexte.modeles))
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
        noms_sources("retouche", &jeu_de_prompts("retouche").unwrap_or_default())
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

/// Fait du modèle choisi le modèle par défaut de sa tâche (`favoris.generation.<tâche>`, ou `favoris.llama.modele`
/// pour un Casting LLM : le témoin gagnant veut dire « pas de LLM », un modèle vide).
///
/// # Errors
/// Casting pas encore révélé, modèle inconnu, ou fichiers qui ne s'écrivent pas.
pub fn choisir_favori(contexte: &Contexte, id: &str, workflow: &str) -> Result<(), String> {
    let mut r = charger(&contexte.data, id)?;
    if !revele(&r) {
        return Err("Révèle d'abord les modèles".to_owned());
    }
    workflow_de(&r, workflow)?;
    r.insert("favori".to_owned(), json!(workflow));
    ecrire(&contexte.data, &r)?;
    let tache = r
        .get("tache")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
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
    if !noms_sources("retouche", &jeu_de_prompts("retouche")?).contains(&nom) {
        return Err(format!("Aucune retouche n'utilise de source « {prompt} »"));
    }
    let d = dossier_sources(data);
    fs::create_dir_all(&d).map_err(|e| format!("{} : {e}", d.display()))?;
    fs::copy(&chemin, d.join(&nom)).map_err(|e| format!("{nom} : {e}"))?;
    let mut origine = origine_sources(data);
    origine.insert(nom.clone(), json!({"banc": id, "image": image}));
    let texte_origine = serde_json::to_string_pretty(&origine).map_err(|e| e.to_string())?;
    fs::write(d.join("origine.json"), texte_origine).map_err(|e| format!("origine.json : {e}"))?;
    Ok(nom)
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
        // Chaque image de départ des castings de retouche et d'image vers vidéo est livrée.
        let temp = tempfile::tempdir().unwrap();
        for tache in AVEC_SOURCE {
            for nom in noms_sources(tache, &jeu_de_prompts(tache).unwrap()) {
                assert!(source(temp.path(), &nom).is_some(), "{nom}");
            }
        }
        assert!(source(temp.path(), "../reglages.json").is_none());
        assert!(origine_sources(temp.path()).contains_key("portrait.png"));
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
        assert!(!retouche.sources.is_empty() && retouche.sources.iter().all(|s| s.presente));
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
}
