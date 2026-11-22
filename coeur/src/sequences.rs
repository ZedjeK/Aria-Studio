//! Les séquences du Montage, comme en v2 (`app/sequences.py`) : plusieurs plans MiniMax H3 enchaînés, tournés dans
//! l'ordre et assemblés en un film.
//!
//! Une séquence (`data\sequences\<id>.json`, au format de la v2) : un titre, un casting (personnage de référence, voix,
//! ambiance, contrainte), une résolution, et de 1 à 12 plans. Le premier plan part d'un rush ou du texte ; chaque plan
//! suivant s'enchaîne au précédent :
//! - prolonger : même plan, sans coupure ; les 39 dernières images du plan précédent et leur son sont ancrés au début
//!   du nouveau (`MiniMaxH3AddGuide`) ;
//! - couper : nouveau plan ; le personnage (image) et la voix (audio) servent de références
//!   (`MiniMaxH3ReferenceToVideo`) ;
//! - transition : le plan part de la dernière image du précédent et se termine sur un rush.
//!
//! Chaque plan tourné est un clip des Bobines (rangé dans le dossier de la séquence) ; le film assemblé aussi. Le
//! prompt d'un plan s'écrit au Scénario ou dans le plan : le LLM n'écrit pas au tournage. AS-18 : les textes des plans,
//! l'ambiance, la contrainte et les textes d'origine de tous les rushes, clips et voix utilisés passent par le filtre.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Local;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::plateau::{self, ChoixLora, Commande, Contexte, Generateur, Suivi};
use crate::workflows::{self, FORMATS, Workflow};
use crate::{comfy, creations, h3, journal, montage, reglages, rushes, securite, voix};

/// Les plans d'une séquence, au plus (environ 2 min de film).
pub const PLANS_MAX: usize = 12;
/// Les images du plan précédent ancrées en « prolonger » (grille 17k+5 du modèle).
pub const GUIDE: u32 = 39;
/// Le premier plan part d'un rush ou du texte ; les suivants s'enchaînent.
pub const DEPARTS: [&str; 2] = ["rush", "texte"];
pub const ENCHAINEMENTS: [&str; 3] = ["prolonger", "couper", "transition"];
pub const LONGUEUR_IDEE: usize = 1000;
pub const LONGUEUR_AMBIANCE: usize = 300;
pub const LONGUEUR_CONTRAINTE: usize = 300;
pub const LONGUEUR_TITRE: usize = 40;
/// Guides, références et premier chargement coûtent environ 40 % de plus qu'un clip seul (mesure de la v2).
pub const SURCOUT_SEQUENCE: f64 = 1.4;
/// Les plans qui parlent montent à 12 steps (la voix en a besoin) ; mesuré +17 % de temps.
pub const STEPS_PAROLE: u64 = 12;
pub const SURCOUT_PAROLE: f64 = 1.17;
/// Le temps d'un plan croît avec sa durée en (D / 5)^EXPOSANT_DUREE (mesure de la v2 pour les séquences).
pub const EXPOSANT_DUREE: f64 = 1.25;
/// Ce qui, s'il change, oblige à retourner le plan (et ceux qui en dépendent).
const CONTENU_PLAN: [&str; 5] = ["prompt", "duree", "enchainement", "rush", "seed"];
/// Un autre checkpoint ou d'autres LoRA : des plans qui ne se ressemblent plus.
const CONTENU_SEQUENCE: [&str; 5] = ["qualite", "format", "casting", "modele", "loras"];

/// Une séquence, au format de la v2.
pub type Sequence = Map<String, Value>;

fn dossier(data: &Path) -> PathBuf {
    data.join("sequences")
}

fn corbeille_dir(data: &Path) -> PathBuf {
    dossier(data).join("corbeille")
}

fn id_valide(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 22
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 15 => *c == b'-',
            16..=21 => c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
            _ => c.is_ascii_digit(),
        })
}

fn texte(v: &Value, cle: &str) -> String {
    v.get(cle).and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// Une valeur texte de la séquence (ou de son casting).
fn champ(s: &Map<String, Value>, cle: &str) -> String {
    s.get(cle).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn plans(s: &Sequence) -> Vec<Value> {
    s.get("plans")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn casting(s: &Sequence) -> Map<String, Value> {
    s.get("casting")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn duree_plan(p: &Value) -> u32 {
    p.get("duree")
        .and_then(Value::as_u64)
        .and_then(|d| u32::try_from(d).ok())
        .unwrap_or(5)
}

// --- calculs ------------------------------------------------------------------------------------------------------

/// Le plan contient une réplique (dans l'idée ou le prompt).
pub fn parle(plan: &Value) -> bool {
    h3::parle(&format!("{} {}", texte(plan, "idee"), texte(plan, "prompt")))
}

/// Le workflow de tous les plans (image vers vidéo).
fn workflow(contexte: &Contexte) -> Result<Workflow, String> {
    workflows::obtenir(&montage::workflow_de(contexte, "image_vers_video"))
}

fn reglage(contexte: &Contexte, wf: &Workflow) -> Option<Map<String, Value>> {
    reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&wf.id).and_then(Value::as_object).cloned())
}

/// Le checkpoint d'une séquence : le sien, sinon celui des réglages.
fn checkpoint(contexte: &Contexte, wf: &Workflow, s: &Sequence) -> Option<String> {
    s.get("modele")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            wf.regle(&contexte.modeles, reglage(contexte, wf).as_ref())
                .get("modele")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

/// Les steps du checkpoint de la séquence ; 0 s'il est à valider.
pub fn steps_base(contexte: &Contexte, s: &Sequence) -> u64 {
    let Ok(wf) = workflow(contexte) else { return 0 };
    checkpoint(contexte, &wf, s).map_or(0, |m| wf.steps_de(reglage(contexte, &wf).as_ref(), &m))
}

/// Les steps d'un plan : ceux du checkpoint, au moins 12 pour un plan qui parle.
pub fn steps_plan(contexte: &Contexte, s: &Sequence, plan: &Value) -> u64 {
    let steps = steps_base(contexte, s);
    if parle(plan) {
        steps.max(STEPS_PAROLE)
    } else {
        steps
    }
}

/// Secondes de calcul sur la carte de la mesure, assemblage compris.
pub fn estimation(contexte: &Contexte, s: &Sequence, indices: &[usize]) -> f64 {
    let cle = champ(s, "qualite");
    let Some(q) = montage::qualites(contexte).into_iter().find(|q| q.cle == cle) else {
        return 0.0;
    };
    let base = match steps_base(contexte, s) {
        0 => f64::from(q.steps),
        n => u32::try_from(n).map_or(f64::from(q.steps), f64::from),
    };
    let tous = plans(s);
    let cout = |plan: &Value| {
        let surcout = if parle(plan) && base < 12.0 {
            SURCOUT_PAROLE
        } else {
            1.0
        };
        q.estimation_s
            * (base / f64::from(q.steps))
            * SURCOUT_SEQUENCE
            * surcout
            * (f64::from(duree_plan(plan)) / 5.0).powf(EXPOSANT_DUREE)
    };
    let total: f64 = indices.iter().filter_map(|k| tous.get(*k)).map(cout).sum();
    total + if indices.is_empty() { 0.0 } else { 5.0 }
}

/// Les plans à tourner : ceux demandés (ou pas encore à jour), plus ceux qui partent du plan d'avant.
pub fn a_tourner(s: &Sequence, demandes: &[usize]) -> Vec<usize> {
    let tous = plans(s);
    let mut choisis: Vec<usize> = demandes.to_vec();
    choisis.extend(tous.iter().enumerate().filter_map(|(k, p)| {
        let a_jour = p.get("clip").is_some_and(|c| !c.is_null())
            && p.get("a_jour").and_then(Value::as_bool) == Some(true);
        (!a_jour).then_some(k)
    }));
    for (k, plan) in tous.iter().enumerate().skip(1) {
        let enchainement = texte(plan, "enchainement");
        if choisis.contains(&(k - 1)) && (enchainement == "prolonger" || enchainement == "transition") {
            choisis.push(k);
        }
    }
    choisis.sort_unstable();
    choisis.dedup();
    choisis
}

/// Le rush de référence des plans coupés : celui du casting, sinon le rush de départ.
pub fn personnage(s: &Sequence) -> Option<String> {
    let premier = plans(s).into_iter().next()?;
    casting(s)
        .get("personnage")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            (texte(&premier, "enchainement") == "rush")
                .then(|| texte(&premier, "rush"))
                .filter(|r| !r.is_empty())
        })
}

fn voix_du_casting(s: &Sequence) -> Option<String> {
    casting(s)
        .get("voix")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// Le clip (ou `voix:<id>`) dont le son sert de voix de référence au plan k.
pub fn source_voix(s: &Sequence, k: usize) -> Option<String> {
    match voix_du_casting(s).as_deref() {
        Some("auto") => plans(s)
            .iter()
            .take(k)
            .find(|p| p.get("clip").and_then(Value::as_str).is_some() && parle(p))
            .and_then(|p| p.get("clip").and_then(Value::as_str).map(str::to_owned)),
        autre => autre.map(str::to_owned),
    }
}

/// La voix de référence du plan k : celle des plans coupés et des transitions qui parlent, et celle du casting pour un
/// plan de départ qui parle. Un plan prolongé garde la voix par le son des images ancrées.
pub fn voix_du_plan(s: &Sequence, k: usize) -> Option<String> {
    let tous = plans(s);
    let plan = tous.get(k)?;
    let enchainement = texte(plan, "enchainement");
    if enchainement == "couper" || (enchainement == "transition" && parle(plan)) {
        return source_voix(s, k);
    }
    let du_casting = voix_du_casting(s);
    if DEPARTS.contains(&enchainement.as_str())
        && parle(plan)
        && du_casting.as_deref().is_some_and(|v| v != "auto")
    {
        return du_casting;
    }
    None
}

/// (personnage, voix) de référence du plan k ; s'il y en a une, le plan passe en `ReferenceToVideo`.
pub fn references(s: &Sequence, k: usize) -> (Option<String>, Option<String>) {
    let Some(plan) = plans(s).into_iter().nth(k) else {
        return (None, None);
    };
    let enchainement = texte(&plan, "enchainement");
    if enchainement == "prolonger" {
        return (None, None);
    }
    let partout = casting(s)
        .get("reference_partout")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let perso = if enchainement == "couper" || partout {
        personnage(s)
    } else {
        None
    };
    (perso, voix_du_plan(s, k))
}

/// L'identifiant d'une voix de la bibliothèque (`voix:<id>`).
fn id_voix(valeur: Option<&str>) -> Option<&str> {
    valeur?.strip_prefix(voix::PREFIXE)
}

/// Les rushes et clips qu'une séquence utilise.
fn rushes_utilises(s: &Sequence) -> Vec<String> {
    let mut ids: Vec<String> = plans(s)
        .iter()
        .map(|p| texte(p, "rush"))
        .filter(|r| !r.is_empty())
        .collect();
    ids.extend(
        casting(s)
            .get("personnage")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .map(str::to_owned),
    );
    if let Some(v) = voix_du_casting(s).filter(|v| v != "auto" && id_voix(Some(v)).is_none()) {
        ids.push(v);
    }
    ids
}

/// AS-18 : les textes d'origine des rushes et clips utilisés, et ce qui a fait la voix de la bibliothèque.
fn textes(data: &Path, s: &Sequence) -> Result<Vec<String>, String> {
    let mut sortie = creations::textes_origine(data, &rushes_utilises(s))?;
    if let Some(id) = id_voix(voix_du_casting(s).as_deref()) {
        sortie.extend(voix::textes(data, id)?);
    }
    Ok(sortie)
}

/// Ce que l'interface doit savoir pour éditer une séquence et estimer son tournage comme le cœur.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsSequence {
    pub plans_max: usize,
    pub longueur_titre: usize,
    pub longueur_idee: usize,
    pub longueur_ambiance: usize,
    pub longueur_contrainte: usize,
    pub surcout: f64,
    pub steps_parole: u64,
    pub surcout_parole: f64,
    pub exposant_duree: f64,
    /// Les steps de chaque checkpoint prêt.
    pub steps: std::collections::BTreeMap<String, u64>,
    /// Le checkpoint des nouvelles séquences.
    pub modele_defaut: Option<String>,
}

/// Les séquences, la corbeille, celles en tournage et les options : tout ce que lit l'écran des séquences.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatSequences {
    pub sequences: Vec<Value>,
    pub corbeille: Vec<Value>,
    pub en_tournage: Vec<String>,
    pub options: OptionsSequence,
}

/// Ce que lit l'écran des séquences.
pub fn etat(contexte: &Contexte, en_tournage: Vec<String>) -> EtatSequences {
    let (steps, modele_defaut) = workflow(contexte).map_or_else(
        |_| (std::collections::BTreeMap::new(), None),
        |wf| {
            let r = reglage(contexte, &wf);
            let steps = wf
                .variantes(&contexte.modeles, r.as_ref())
                .into_iter()
                .map(|m| {
                    let n = wf.steps_de(r.as_ref(), &m);
                    (m, n)
                })
                .collect();
            (steps, checkpoint(contexte, &wf, &Map::new()))
        },
    );
    EtatSequences {
        sequences: liste(contexte),
        corbeille: corbeille(&contexte.data),
        en_tournage,
        options: OptionsSequence {
            plans_max: PLANS_MAX,
            longueur_titre: LONGUEUR_TITRE,
            longueur_idee: LONGUEUR_IDEE,
            longueur_ambiance: LONGUEUR_AMBIANCE,
            longueur_contrainte: LONGUEUR_CONTRAINTE,
            surcout: SURCOUT_SEQUENCE,
            steps_parole: STEPS_PAROLE,
            surcout_parole: SURCOUT_PAROLE,
            exposant_duree: EXPOSANT_DUREE,
            steps,
            modele_defaut,
        },
    }
}

// --- stockage -----------------------------------------------------------------------------------------------------

fn chemin(data: &Path, id: &str) -> Result<PathBuf, String> {
    let c = dossier(data).join(format!("{id}.json"));
    if id_valide(id) && c.is_file() {
        Ok(c)
    } else {
        Err(format!("Séquence inconnue : {id}"))
    }
}

/// Une séquence.
///
/// # Errors
/// Séquence inconnue ou illisible.
pub fn lire(data: &Path, id: &str) -> Result<Sequence, String> {
    let t = fs::read_to_string(chemin(data, id)?).map_err(|e| e.to_string())?;
    let mut s = match serde_json::from_str::<Value>(&t) {
        Ok(Value::Object(s)) => s,
        _ => return Err(format!("Séquence illisible : {id}")),
    };
    s.entry("loras").or_insert_with(|| json!([])); // séquence d'avant les LoRA vidéo
    Ok(s)
}

fn ecrire_dans(d: &Path, s: &Sequence) -> Result<(), String> {
    let id = champ(s, "id");
    fs::create_dir_all(d).map_err(|e| format!("{} : {e}", d.display()))?;
    let temporaire = d.join(format!("{id}.tmp"));
    let t = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(&temporaire, t)
        .and_then(|()| fs::rename(&temporaire, d.join(format!("{id}.json"))))
        .map_err(|e| format!("séquence {id} : {e}"))
}

fn ecrire(data: &Path, s: &Sequence) -> Result<(), String> {
    ecrire_dans(&dossier(data), s)
}

/// Ce que l'interface reçoit d'une séquence : elle-même, ses plans à tourner, l'estimation et la durée du film.
pub fn public(contexte: &Contexte, s: &Sequence) -> Value {
    let indices = a_tourner(s, &[]);
    let mut p = s.clone();
    p.insert("a_tourner".to_owned(), json!(indices));
    p.insert(
        "estimation_s".to_owned(),
        json!(estimation(contexte, s, &indices).round()),
    );
    p.insert(
        "duree_totale".to_owned(),
        json!(plans(s).iter().map(duree_plan).sum::<u32>()),
    );
    Value::Object(p)
}

/// Les séquences, les plus récentes d'abord.
pub fn liste(contexte: &Contexte) -> Vec<Value> {
    let Ok(lecture) = fs::read_dir(dossier(&contexte.data)) else {
        return Vec::new();
    };
    let mut sortie: Vec<Sequence> = lecture
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|p| fs::read_to_string(p).ok())
        .filter_map(|t| serde_json::from_str::<Value>(&t).ok())
        .filter_map(|v| v.as_object().cloned())
        .collect();
    sortie.sort_by(|a, b| {
        let d = |s: &Sequence| champ(s, "date");
        d(b).cmp(&d(a))
    });
    sortie.iter().map(|s| public(contexte, s)).collect()
}

fn espaces(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Une séquence propre à partir de ce qu'envoie l'interface. Brouillon accepté (transition sans rush d'arrivée) ;
/// `pour_tourner` refuse en plus tout ce qui ne se tournerait pas.
///
/// # Errors
/// Le message de la v2 : titre, résolution, format, ambiance, contrainte, personnage, voix, plans, checkpoint, LoRA,
/// ou filtre AS-18.
#[allow(clippy::too_many_lines)] // la validation de la v2, gardée d'un tenant pour se relire à côté d'elle
pub fn valider(contexte: &Contexte, donnees: &Value, pour_tourner: bool) -> Result<Sequence, String> {
    let titre = texte(donnees, "titre").trim().to_owned();
    if titre.is_empty() || titre.chars().count() > LONGUEUR_TITRE {
        return Err(format!("Titre de 1 à {LONGUEUR_TITRE} caractères"));
    }
    let qualite = donnees
        .get("qualite")
        .and_then(Value::as_str)
        .unwrap_or(montage::QUALITE_DEFAUT)
        .to_owned();
    if !montage::qualites(contexte).iter().any(|q| q.cle == qualite) {
        return Err(format!("Résolution inconnue : {qualite}"));
    }
    let format = donnees
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("16:9 (Widescreen)")
        .to_owned();
    if !FORMATS.contains(&format.as_str()) {
        return Err(format!("Format inconnu : {format}"));
    }
    let c = donnees.get("casting").cloned().unwrap_or_else(|| json!({}));
    let ambiance = texte(&c, "ambiance").trim().to_owned();
    if ambiance.chars().count() > LONGUEUR_AMBIANCE {
        return Err(format!("Ambiance de {LONGUEUR_AMBIANCE} caractères au plus"));
    }
    let contrainte = espaces(&texte(&c, "contrainte"));
    if contrainte.chars().count() > LONGUEUR_CONTRAINTE {
        return Err(format!("Contrainte de {LONGUEUR_CONTRAINTE} caractères au plus"));
    }
    let perso = Some(texte(&c, "personnage")).filter(|p| !p.is_empty());
    let voix_casting = match c.get("voix") {
        None => Some("auto".to_owned()),
        Some(v) => v.as_str().filter(|v| !v.is_empty()).map(str::to_owned),
    };
    if let Some(p) = &perso {
        creations::chemin_image(&contexte.data, p)?;
    }
    match voix_casting.as_deref() {
        Some(v) if id_voix(Some(v)).is_some() => {
            voix::gardee(&contexte.data, id_voix(Some(v)).unwrap_or_default())?;
        }
        Some("auto") | None => {}
        Some(clip) => {
            creations::chemin_video(&contexte.data, clip)?;
        }
    }
    let casting = json!({
        "personnage": perso, "voix": voix_casting, "ambiance": ambiance, "contrainte": contrainte,
        "reference_partout": c.get("reference_partout").and_then(Value::as_bool).unwrap_or(true),
    });

    let bruts = donnees
        .get("plans")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !(1..=PLANS_MAX).contains(&bruts.len()) {
        return Err(format!("De 1 à {PLANS_MAX} plans"));
    }
    let mut propres = Vec::new();
    for (k, p) in bruts.iter().enumerate() {
        let nom = format!("Plan {}", k + 1);
        let enchainement = texte(p, "enchainement");
        let permis: &[&str] = if k == 0 { &DEPARTS } else { &ENCHAINEMENTS };
        if !permis.contains(&enchainement.as_str()) {
            return Err(format!("{nom} : enchaînement possible : {}", permis.join(", ")));
        }
        let idee = texte(p, "idee").trim().to_owned();
        let prompt = Some(texte(p, "prompt").trim().to_owned()).filter(|t| !t.is_empty());
        if idee.is_empty() && prompt.is_none() {
            return Err(format!("{nom} : écris l'idée du plan"));
        }
        if idee.chars().count() > LONGUEUR_IDEE {
            return Err(format!("{nom} : idée de {LONGUEUR_IDEE} caractères au plus"));
        }
        let duree = p.get("duree").and_then(Value::as_u64).unwrap_or(5);
        if !montage::DUREES.iter().any(|d| u64::from(*d) == duree) {
            let durees: Vec<String> = montage::DUREES.iter().map(u32::to_string).collect();
            return Err(format!("{nom} : durée proposée : {} s", durees.join(", ")));
        }
        let mut rush = Some(texte(p, "rush")).filter(|r| !r.is_empty());
        if enchainement == "rush" || enchainement == "transition" {
            if rush.is_none() && (enchainement == "rush" || pour_tourner) {
                let lequel = if k == 0 { "de départ" } else { "d'arrivée" };
                return Err(format!("{nom} : choisis le rush {lequel}"));
            }
            if let Some(r) = &rush {
                if creations::fiche(&contexte.data, r)?
                    .get("type")
                    .and_then(Value::as_str)
                    == Some("clip")
                {
                    return Err(format!("{nom} : il faut une image, pas un clip"));
                }
                creations::chemin_image(&contexte.data, r)?;
            }
        } else {
            rush = None;
        }
        let id = Some(texte(p, "id"))
            .filter(|i| !i.is_empty())
            .map_or_else(id_plan, Ok)?;
        propres.push(json!({
            "id": id, "idee": idee, "prompt": prompt, "duree": duree, "enchainement": enchainement, "rush": rush,
            "seed": p.get("seed").and_then(Value::as_u64),
            "ambiance": p.get("ambiance").and_then(Value::as_bool).unwrap_or(true),
            "clip": p.get("clip").cloned().unwrap_or(Value::Null),
            "prompt_final": p.get("prompt_final").cloned().unwrap_or(Value::Null),
            "a_jour": p.get("a_jour").and_then(Value::as_bool).unwrap_or(false),
        }));
    }

    let wf = workflow(contexte)?;
    let modele = Some(texte(donnees, "modele")).filter(|m| !m.is_empty());
    if let Some(m) = &modele
        && !wf
            .variantes(&contexte.modeles, reglage(contexte, &wf).as_ref())
            .contains(m)
    {
        return Err(format!(
            "Checkpoint absent de modeles/{} : {m}",
            wf.dossier_modeles()
        ));
    }
    let loras: Vec<ChoixLora> = donnees
        .get("loras")
        .filter(|l| !l.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| format!("LoRA illisibles : {e}"))?
        .unwrap_or_default();
    let loras = plateau::valider_loras(contexte, &wf, &loras)?;
    let mut s = Map::new();
    s.insert("titre".to_owned(), json!(titre));
    s.insert("qualite".to_owned(), json!(qualite));
    s.insert("format".to_owned(), json!(format));
    s.insert("casting".to_owned(), casting);
    s.insert("plans".to_owned(), Value::Array(propres));
    s.insert(
        "llm".to_owned(),
        json!(Some(texte(donnees, "llm")).filter(|l| !l.is_empty())),
    );
    s.insert("modele".to_owned(), json!(modele));
    s.insert("loras".to_owned(), json!(loras));
    // AS-18 : tout le texte de la séquence, avec les textes d'origine des rushes, clips et voix qu'elle utilise.
    let origine = textes(&contexte.data, &s)?;
    let mut a_lire: Vec<String> = vec![ambiance, contrainte];
    for p in plans(&s) {
        a_lire.push(texte(&p, "idee"));
        a_lire.push(texte(&p, "prompt"));
    }
    a_lire.extend(loras.iter().map(|l| l.declencheurs.clone()));
    a_lire.extend(origine);
    let refs: Vec<&str> = a_lire.iter().map(String::as_str).collect();
    securite::verifier(&contexte.dossier, &refs).map_err(|e| e.to_string())?;
    Ok(s)
}

fn id_plan() -> Result<String, String> {
    let mut o = [0u8; 4];
    getrandom::fill(&mut o).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    Ok(o.iter().map(|b| format!("{b:02x}")).collect())
}

fn nouvel_id() -> Result<String, String> {
    let mut o = [0u8; 3];
    getrandom::fill(&mut o).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    let suffixe: String = o.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("{}-{suffixe}", Local::now().format("%Y%m%d-%H%M%S")))
}

/// Le dossier des Bobines d'une séquence (créé au besoin).
fn dossier_bobines(data: &Path, titre: &str) -> Result<String, String> {
    let nom: String = format!("Séquence {titre}")
        .chars()
        .filter(|c| c.is_alphanumeric() || " .'’-_".contains(*c))
        .take(40)
        .collect();
    let nom = nom.trim().to_owned();
    if !rushes::dossiers(data, "clips")?.contains(&nom) {
        rushes::creer_dossier(data, &nom, "clips")?;
    }
    Ok(nom)
}

fn contrainte_traduite(s: &mut Sequence, traduction: Option<&Value>) {
    let contrainte = champ(&casting(s), "contrainte");
    if let Some(t) = traduction
        && !contrainte.is_empty()
        && texte(t, "fr") == contrainte
        && !texte(t, "en").is_empty()
    {
        let en: String = espaces(&texte(t, "en"))
            .chars()
            .take(LONGUEUR_CONTRAINTE * 2)
            .collect();
        s.insert("contrainte_en".to_owned(), json!({"fr": contrainte, "en": en}));
    }
}

/// Crée une séquence.
///
/// # Errors
/// Voir [`valider`] ; ou fichier qui ne s'écrit pas.
pub fn creer(contexte: &Contexte, donnees: &Value) -> Result<Value, String> {
    let mut s = valider(contexte, donnees, false)?;
    let wf = workflow(contexte)?;
    let loras: Vec<ChoixLora> =
        serde_json::from_value(s.get("loras").cloned().unwrap_or_default()).unwrap_or_default();
    plateau::memoriser_loras(contexte, &wf, &loras)?;
    // La séquence garde son checkpoint, même si le défaut change.
    let modele = checkpoint(contexte, &wf, &s);
    s.insert("modele".to_owned(), json!(modele));
    contrainte_traduite(&mut s, donnees.get("contrainte_en"));
    if let Some(Value::Array(ps)) = s.get_mut("plans") {
        for p in ps.iter_mut().filter_map(Value::as_object_mut) {
            p.insert("clip".to_owned(), Value::Null);
            p.insert("prompt_final".to_owned(), Value::Null);
            p.insert("a_jour".to_owned(), json!(false));
        }
    }
    let titre = champ(&s, "titre");
    s.insert("id".to_owned(), json!(nouvel_id()?));
    s.insert(
        "date".to_owned(),
        json!(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
    );
    s.insert("film".to_owned(), Value::Null);
    s.insert(
        "dossier".to_owned(),
        json!(dossier_bobines(&contexte.data, &titre)?),
    );
    ecrire(&contexte.data, &s)?;
    Ok(public(contexte, &s))
}

/// Enregistre les changements ; un plan modifié (ou toute la séquence si la résolution, le format, le casting, le
/// checkpoint ou les LoRA changent) n'est plus « à jour » : il sera retourné, ses anciens clips restent dans les
/// Bobines.
///
/// # Errors
/// Séquence inconnue, voir [`valider`], ou fichier qui ne s'écrit pas.
pub fn modifier(contexte: &Contexte, id: &str, donnees: &Value) -> Result<Value, String> {
    let ancienne = lire(&contexte.data, id)?;
    let wf = workflow(contexte)?;
    let mut s = valider(contexte, donnees, false)?;
    let loras: Vec<ChoixLora> =
        serde_json::from_value(s.get("loras").cloned().unwrap_or_default()).unwrap_or_default();
    plateau::memoriser_loras(contexte, &wf, &loras)?;
    if s.get("modele").is_none_or(Value::is_null) {
        let garde = ancienne
            .get("modele")
            .cloned()
            .filter(|m| !m.is_null())
            .unwrap_or_else(|| json!(wf.modele_origine()));
        s.insert("modele".to_owned(), garde);
    }
    let tout_change = CONTENU_SEQUENCE.iter().any(|c| s.get(*c) != ancienne.get(*c));
    match ancienne.get("contrainte_en") {
        Some(t) if texte(t, "fr") == champ(&casting(&s), "contrainte") => {
            s.insert("contrainte_en".to_owned(), t.clone());
        }
        _ => contrainte_traduite(&mut s, donnees.get("contrainte_en")),
    }
    let avant = plans(&ancienne);
    if let Some(Value::Array(ps)) = s.get_mut("plans") {
        for p in ps.iter_mut().filter_map(Value::as_object_mut) {
            let a = avant.iter().find(|a| a.get("id") == p.get("id"));
            let change = a.is_none_or(|a| tout_change || CONTENU_PLAN.iter().any(|c| p.get(*c) != a.get(*c)));
            let clip = a.and_then(|a| a.get("clip")).cloned().unwrap_or(Value::Null);
            p.insert("clip".to_owned(), clip);
            if change {
                p.insert("prompt_final".to_owned(), Value::Null);
                p.insert("a_jour".to_owned(), json!(false));
            } else if let Some(a) = a {
                p.insert(
                    "prompt_final".to_owned(),
                    a.get("prompt_final").cloned().unwrap_or(Value::Null),
                );
                p.insert(
                    "a_jour".to_owned(),
                    a.get("a_jour").cloned().unwrap_or(json!(false)),
                );
            }
        }
    }
    for cle in ["id", "date", "film", "dossier"] {
        s.insert(cle.to_owned(), ancienne.get(cle).cloned().unwrap_or(Value::Null));
    }
    ecrire(&contexte.data, &s)?;
    Ok(public(contexte, &s))
}

/// À la corbeille : la séquence seulement ; ses clips et son film restent dans les Bobines.
///
/// # Errors
/// Séquence inconnue, ou une du même identifiant déjà à la corbeille.
pub fn supprimer(contexte: &Contexte, id: &str) -> Result<(), String> {
    let c = chemin(&contexte.data, id)?;
    let cible = corbeille_dir(&contexte.data).join(format!("{id}.json"));
    if cible.exists() {
        return Err(
            "Une séquence du même identifiant est déjà dans la corbeille : vide-la d'abord".to_owned(),
        );
    }
    let mut s = lire(&contexte.data, id)?;
    s.insert(
        "corbeille".to_owned(),
        json!(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
    );
    ecrire_dans(&corbeille_dir(&contexte.data), &s)?;
    fs::remove_file(&c).map_err(|e| e.to_string())?;
    let titre = champ(&s, "titre");
    journal::noter(
        &contexte.dossier,
        "Séquence mise à la corbeille",
        &format!("{titre} ({id})"),
        journal::Resultat::Ok,
        None,
    );
    Ok(())
}

/// Les séquences de la corbeille, les plus récentes d'abord.
pub fn corbeille(data: &Path) -> Vec<Value> {
    let Ok(lecture) = fs::read_dir(corbeille_dir(data)) else {
        return Vec::new();
    };
    let mut sortie: Vec<Value> = lecture
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|p| {
            let id = p.file_stem()?.to_string_lossy().into_owned();
            let s: Value = serde_json::from_str(&fs::read_to_string(&p).ok()?).ok()?;
            let titre = Some(texte(&s, "titre"))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "sans titre".to_owned());
            Some(json!({
                "id": id, "titre": titre,
                "plans": s.get("plans").and_then(Value::as_array).map_or(0, Vec::len),
                "date": s.get("corbeille"),
            }))
        })
        .collect();
    sortie.sort_by_key(|s| std::cmp::Reverse(texte(s, "date")));
    sortie
}

/// Revient dans la liste des séquences, telle qu'elle était.
///
/// # Errors
/// Pas dans la corbeille, ou une séquence du même identifiant existe.
pub fn restaurer(contexte: &Contexte, id: &str) -> Result<Value, String> {
    let source = corbeille_dir(&contexte.data).join(format!("{id}.json"));
    if !id_valide(id) || !source.is_file() {
        return Err(format!("Séquence inconnue dans la corbeille : {id}"));
    }
    if dossier(&contexte.data).join(format!("{id}.json")).exists() {
        return Err("Une séquence du même identifiant existe déjà : rien n'est restauré".to_owned());
    }
    let mut s = match serde_json::from_str::<Value>(&fs::read_to_string(&source).map_err(|e| e.to_string())?)
    {
        Ok(Value::Object(s)) => s,
        _ => return Err(format!("Séquence illisible : {id}")),
    };
    s.remove("corbeille");
    ecrire(&contexte.data, &s)?;
    fs::remove_file(&source).map_err(|e| e.to_string())?;
    let titre = champ(&s, "titre");
    journal::noter(
        &contexte.dossier,
        "Séquence restaurée",
        &format!("{titre} ({id})"),
        journal::Resultat::Ok,
        None,
    );
    Ok(public(contexte, &lire(&contexte.data, id)?))
}

/// La seule suppression définitive d'une séquence ; rend le nombre de séquences effacées.
///
/// # Errors
/// Un fichier qui ne s'efface pas.
pub fn vider_corbeille(contexte: &Contexte) -> Result<usize, String> {
    let Ok(lecture) = fs::read_dir(corbeille_dir(&contexte.data)) else {
        return Ok(0);
    };
    let mut effacees = 0;
    for e in lecture.filter_map(Result::ok) {
        if e.path().extension().and_then(|x| x.to_str()) == Some("json") {
            fs::remove_file(e.path()).map_err(|err| err.to_string())?;
            effacees += 1;
        }
    }
    journal::noter(
        &contexte.dossier,
        "Corbeille des séquences vidée",
        &format!("{effacees} séquence(s)"),
        journal::Resultat::Ok,
        None,
    );
    Ok(effacees)
}

/// Une voix de la bibliothèque est supprimée : les séquences qui l'utilisaient repassent en Auto (tous leurs plans à
/// retourner) et le disent. Rend leurs identifiants.
///
/// # Errors
/// Une séquence qui ne s'écrit pas.
pub fn oublier_voix(data: &Path, id_voix: &str, nom: &str) -> Result<Vec<String>, String> {
    let valeur = format!("{}{id_voix}", voix::PREFIXE);
    let Ok(lecture) = fs::read_dir(dossier(data)) else {
        return Ok(Vec::new());
    };
    let mut touchees = Vec::new();
    for e in lecture.filter_map(Result::ok) {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Some(mut s) = fs::read_to_string(&p)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
        else {
            continue;
        };
        if voix_du_casting(&s).as_deref() != Some(valeur.as_str()) {
            continue;
        }
        if let Some(c) = s.get_mut("casting").and_then(Value::as_object_mut) {
            c.insert("voix".to_owned(), json!("auto"));
        }
        if let Some(Value::Array(ps)) = s.get_mut("plans") {
            for plan in ps.iter_mut().filter_map(Value::as_object_mut) {
                plan.insert("prompt_final".to_owned(), Value::Null);
                plan.insert("a_jour".to_owned(), json!(false));
            }
        }
        s.insert(
            "alerte".to_owned(),
            json!(format!("La voix « {nom} » a été supprimée : la séquence repasse en Auto, tous ses plans sont à retourner")),
        );
        ecrire(data, &s)?;
        touchees.push(champ(&s, "id"));
    }
    Ok(touchees)
}

// --- graphes ComfyUI ----------------------------------------------------------------------------------------------

/// La taille de tous les plans (sinon pas d'assemblage) : celle du rush de départ, sinon du format.
fn dimensions_sequence(contexte: &Contexte, s: &Sequence, megapixels: f64) -> Result<(u32, u32), String> {
    let premier = plans(s)
        .into_iter()
        .next()
        .ok_or_else(|| "Séquence sans plan".to_owned())?;
    if texte(&premier, "enchainement") == "rush" {
        let fiche = creations::fiche(&contexte.data, &texte(&premier, "rush"))?;
        let l = fiche.get("largeur").and_then(Value::as_f64).unwrap_or(1.0);
        let h = fiche.get("hauteur").and_then(Value::as_f64).unwrap_or(1.0);
        return Ok(h3::dimensions(l / h.max(1.0), megapixels));
    }
    let ratio = h3::ratio_format(&champ(s, "format")).unwrap_or(16.0 / 9.0);
    Ok(h3::dimensions(ratio, megapixels))
}

/// Les fichiers déjà téléversés dans ComfyUI pour un plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Televerses {
    pub rush: Option<String>,
    pub precedent: Option<String>,
    pub personnage: Option<String>,
    /// La voix d'un clip (son tiré de la vidéo)…
    pub voix_clip: Option<String>,
    /// … ou un son de la bibliothèque.
    pub voix_son: Option<String>,
}

fn poser(api: &mut Value, id: &str, noeud: Value) {
    if let Some(g) = api.as_object_mut() {
        g.insert(id.to_owned(), noeud);
    }
}

fn video(api: &mut Value, cle: &str, fichier: &str) {
    poser(
        api,
        &format!("aria_{cle}"),
        json!({"class_type": "LoadVideo", "inputs": {"file": fichier}}),
    );
    poser(
        api,
        &format!("aria_{cle}_parts"),
        json!({"class_type": "GetVideoComponents", "inputs": {"video": [format!("aria_{cle}"), 0]}}),
    );
}

/// Le graphe d'un plan, comme en v2.
///
/// # Errors
/// Workflow absent, graphe H3 inattendu, checkpoint à valider, ou filtre AS-18.
#[allow(clippy::too_many_lines)] // le graphe de la v2, gardé d'un tenant pour se relire à côté d'elle
pub fn graphe_plan(
    contexte: &Contexte,
    s: &Sequence,
    k: usize,
    prompt: &str,
    seed: u64,
    fichiers: &Televerses,
) -> Result<Value, String> {
    let tous = plans(s);
    let p = tous.get(k).ok_or_else(|| "Plan inconnu".to_owned())?;
    let enchainement = texte(p, "enchainement");
    let cle = champ(s, "qualite");
    let megapixels = montage::qualites(contexte)
        .into_iter()
        .find(|q| q.cle == cle)
        .map_or(0.6, |q| q.megapixels);
    let wf = workflow(contexte)?;
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let mut valeurs = Map::new();
    valeurs.insert("prompt".to_owned(), json!(prompt));
    valeurs.insert("seed".to_owned(), json!(seed));
    valeurs.insert("duree".to_owned(), json!(duree_plan(p)));
    valeurs.insert("megapixels".to_owned(), json!(megapixels));
    valeurs.insert(
        "modele".to_owned(),
        s.get("modele").cloned().unwrap_or(Value::Null),
    );
    valeurs.insert(
        "loras".to_owned(),
        s.get("loras").cloned().unwrap_or_else(|| json!([])),
    );
    if wf.entree("steps").is_some() {
        valeurs.insert("steps".to_owned(), json!(steps_plan(contexte, s, p)));
    }
    valeurs.insert("prefixe".to_owned(), json!("aria_sequence"));
    if enchainement == "rush" {
        valeurs.insert("image".to_owned(), json!(fichiers.rush));
    }
    let mut api = wf.preparer(
        &valeurs,
        &contexte.modeles,
        reglage(contexte, &wf).as_ref(),
        &filtre,
    )?;
    let i2v = workflows::noeud_unique(&api, "MiniMaxH3ImageToVideo")
        .ok_or_else(|| format!("{} n'a pas de MiniMaxH3ImageToVideo unique", wf.id))?;
    let guider = workflows::noeud_unique(&api, "BasicGuider")
        .ok_or_else(|| format!("{} n'a pas de BasicGuider unique", wf.id))?;
    let (vae, vae_audio) = h3::vaes(&api)?;
    let (largeur, hauteur) = dimensions_sequence(contexte, s, megapixels)?;
    let mut entrees = api
        .pointer(&format!("/{i2v}/inputs"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "graphe H3 illisible".to_owned())?;
    entrees.insert("width".to_owned(), json!(largeur));
    entrees.insert("height".to_owned(), json!(hauteur));
    let premiere_image = entrees.remove("first_frame");
    let precedent = k.checked_sub(1).and_then(|j| tous.get(j));

    if enchainement == "transition" {
        let n = h3::images(precedent.map_or(5, duree_plan));
        video(
            &mut api,
            "precedent",
            fichiers.precedent.as_deref().unwrap_or_default(),
        );
        poser(
            &mut api,
            "aria_derniere",
            json!({"class_type": "ImageFromBatch",
                   "inputs": {"image": ["aria_precedent_parts", 0], "batch_index": n - 1, "length": 1}}),
        );
        poser(
            &mut api,
            "aria_arrivee",
            json!({"class_type": "LoadImage", "inputs": {"image": fichiers.rush}}),
        );
    }

    let refs_ici =
        fichiers.personnage.is_some() || fichiers.voix_clip.is_some() || fichiers.voix_son.is_some();
    if enchainement == "couper" || (enchainement != "prolonger" && refs_ici) {
        let mut reference = json!({
            "clip": entrees.get("clip"), "prompt": entrees.get("prompt"), "width": largeur, "height": hauteur,
            "length": entrees.get("length"), "ref_image_size": "match", "vae": [vae, 0], "audio_vae": [vae_audio, 0],
        });
        let mut ajouter = |cle: &str, valeur: Value| {
            if let Some(r) = reference.as_object_mut() {
                r.insert(cle.to_owned(), valeur);
            }
        };
        if let Some(perso) = &fichiers.personnage {
            poser(
                &mut api,
                "aria_personnage",
                json!({"class_type": "LoadImage", "inputs": {"image": perso}}),
            );
            ajouter("ref_images.ref_image_0", json!(["aria_personnage", 0]));
        }
        if let Some(son) = &fichiers.voix_son {
            poser(
                &mut api,
                "aria_voix_son",
                json!({"class_type": "LoadAudio", "inputs": {"audio": son}}),
            );
            poser(
                &mut api,
                "aria_voix_coupee",
                json!({"class_type": "TrimAudioDuration",
                       "inputs": {"audio": ["aria_voix_son", 0], "start_index": 0.0, "duration": voix::DUREE_MAX}}),
            );
            ajouter("ref_audios.ref_audio_0", json!(["aria_voix_coupee", 0]));
        } else if let Some(clip) = &fichiers.voix_clip {
            video(&mut api, "voix", clip);
            ajouter("ref_audios.ref_audio_0", json!(["aria_voix_parts", 1]));
        }
        poser(
            &mut api,
            &i2v,
            json!({"class_type": "MiniMaxH3ReferenceToVideo", "inputs": reference}),
        );
        // Ce qui relie le plan devient un ancrage : le rush de départ à l'image 0 ; la transition à ses deux bouts.
        let ancres: Vec<(i64, Value)> = match enchainement.as_str() {
            "rush" => premiere_image.clone().map(|i| vec![(0, i)]).unwrap_or_default(),
            "transition" => vec![(0, json!(["aria_derniere", 0])), (-1, json!(["aria_arrivee", 0]))],
            _ => Vec::new(),
        };
        let mut positif = json!([i2v, 0]);
        for (j, (indice, image)) in ancres.into_iter().enumerate() {
            let id = format!("aria_ancre_{j}");
            poser(
                &mut api,
                &id,
                json!({"class_type": "MiniMaxH3AddGuide",
                       "inputs": {"positive": positif, "latent": [i2v, 1], "frame_idx": indice,
                                  "vae": [vae, 0], "image": image}}),
            );
            positif = json!([id, 0]);
        }
        if let Some(c) = api.pointer_mut(&format!("/{guider}/inputs/conditioning")) {
            *c = positif;
        }
    } else {
        match enchainement.as_str() {
            "rush" => {
                if let Some(i) = premiere_image {
                    entrees.insert("first_frame".to_owned(), i);
                }
            }
            "prolonger" => {
                // Les 39 dernières images du plan précédent (une de plus : AddGuide recadre à 17k+5) et leur son.
                let n = h3::images(precedent.map_or(5, duree_plan));
                video(
                    &mut api,
                    "precedent",
                    fichiers.precedent.as_deref().unwrap_or_default(),
                );
                let ips = f64::from(h3::IPS);
                poser(
                    &mut api,
                    "aria_fin",
                    json!({"class_type": "Video Slice",
                           "inputs": {"video": ["aria_precedent", 0],
                                      "start_time": f64::from(n.saturating_sub(GUIDE + 1)) / ips,
                                      "duration": f64::from(GUIDE + 1) / ips, "strict_duration": false}}),
                );
                poser(
                    &mut api,
                    "aria_fin_parts",
                    json!({"class_type": "GetVideoComponents", "inputs": {"video": ["aria_fin", 0]}}),
                );
                poser(
                    &mut api,
                    "aria_guide",
                    json!({"class_type": "MiniMaxH3AddGuide",
                           "inputs": {"positive": [i2v, 0], "latent": [i2v, 1], "frame_idx": 0, "vae": [vae, 0],
                                      "audio_vae": [vae_audio, 0], "image": ["aria_fin_parts", 0],
                                      "audio": ["aria_fin_parts", 1]}}),
                );
                if let Some(c) = api.pointer_mut(&format!("/{guider}/inputs/conditioning")) {
                    *c = json!(["aria_guide", 0]);
                }
            }
            "transition" => {
                entrees.insert("first_frame".to_owned(), json!(["aria_derniere", 0]));
                entrees.insert("last_frame".to_owned(), json!(["aria_arrivee", 0]));
            }
            _ => {}
        }
        if let Some(e) = api.pointer_mut(&format!("/{i2v}/inputs")) {
            *e = Value::Object(entrees);
        }
    }
    wf.ajouter_affiche(&mut api, "aria_sequence")?;
    Ok(h3::elaguer(&api))
}

/// L'assemblage : chaque plan, sans les images qu'il partage avec le précédent, recollés avec leur son.
pub fn graphe_film(s: &Sequence, fichiers: &[String]) -> Value {
    let tous = plans(s);
    let ips = f64::from(h3::IPS);
    let mut api = Map::new();
    let mut entrees = Map::new();
    for (k, (p, fichier)) in tous.iter().zip(fichiers).enumerate() {
        let enchainement = texte(p, "enchainement");
        let debut = match enchainement.as_str() {
            "prolonger" => f64::from(GUIDE) / ips,
            "transition" => 1.0 / ips,
            _ => 0.0,
        };
        let suivant = tous.get(k + 1).map(|q| texte(q, "enchainement"));
        // La dernière image est refaite par le plan prolongé qui suit.
        let fin =
            (suivant.as_deref() == Some("prolonger")).then(|| f64::from(h3::images(duree_plan(p)) - 1) / ips);
        api.insert(
            format!("v{k}"),
            json!({"class_type": "LoadVideo", "inputs": {"file": fichier}}),
        );
        let mut source = json!([format!("v{k}"), 0]);
        if debut > 0.0 || fin.is_some() {
            api.insert(
                format!("t{k}"),
                json!({"class_type": "Video Slice",
                       "inputs": {"video": source, "start_time": debut, "duration": fin.map_or(0.0, |f| f - debut),
                                  "strict_duration": false}}),
            );
            source = json!([format!("t{k}"), 0]);
        }
        entrees.insert(format!("videos.video{k}"), source);
    }
    entrees.insert("codec".to_owned(), json!("auto"));
    api.insert(
        "film".to_owned(),
        json!({"class_type": "ConcatenateVideo", "inputs": entrees}),
    );
    api.insert(
        "film_sortie".to_owned(),
        json!({"class_type": "SaveVideo",
               "inputs": {"video": ["film", 0], "filename_prefix": "aria_film", "format": "auto", "codec": "auto"}}),
    );
    api.insert(
        "film_parts".to_owned(),
        json!({"class_type": "GetVideoComponents", "inputs": {"video": ["film", 0]}}),
    );
    api.insert(
        "film_affiche_lot".to_owned(),
        json!({"class_type": "ImageFromBatch", "inputs": {"image": ["film_parts", 0], "batch_index": 0, "length": 1}}),
    );
    api.insert(
        "film_affiche".to_owned(),
        json!({"class_type": "SaveImage",
               "inputs": {"images": ["film_affiche_lot", 0], "filename_prefix": "aria_film_affiche"}}),
    );
    Value::Object(api)
}

// --- tournage -----------------------------------------------------------------------------------------------------

/// La contrainte à ajouter à chaque plan : sa traduction faite au découpage si elle vaut toujours, sinon le texte de
/// l'utilisateur tel quel (H3 lit aussi le français).
pub fn contrainte_tournage(s: &Sequence) -> Option<String> {
    let contrainte = champ(&casting(s), "contrainte");
    if contrainte.is_empty() {
        return None;
    }
    let traduction = s.get("contrainte_en");
    Some(match traduction {
        Some(t) if texte(t, "fr") == contrainte && !texte(t, "en").is_empty() => texte(t, "en"),
        _ => contrainte,
    })
}

/// La contrainte à la fin du prompt de chaque plan : le LLM peut l'oublier, le tournage la garantit.
pub fn avec_contrainte(prompt: &str, contrainte: Option<&str>) -> String {
    let Some(c) = contrainte.filter(|c| !c.trim().is_empty()) else {
        return prompt.to_owned();
    };
    let ajout = format!("Constraint for every shot: {}.", c.trim().trim_end_matches('.'));
    h3::dans_la_description(prompt, &ajout, false).unwrap_or_else(|| format!("{} {ajout}", prompt.trim_end()))
}

/// Contrôle une demande de tournage et rend la commande à mettre en file.
///
/// # Errors
/// Séquence inconnue, plan inconnu, tout à jour, plans sans prompt, workflow incomplet, ou ce que [`valider`] refuse.
pub fn commande_sequence(
    contexte: &Contexte,
    id: &str,
    demandes: &[usize],
    id_commande: String,
) -> Result<Commande, String> {
    let s = lire(&contexte.data, id)?;
    let tous = plans(&s);
    if demandes.iter().any(|k| *k >= tous.len()) {
        return Err("Plan inconnu".to_owned());
    }
    let indices = a_tourner(&s, demandes);
    if indices.is_empty() {
        return Err("Tous les plans sont à jour : choisis ceux à retourner".to_owned());
    }
    valider(contexte, &Value::Object(s.clone()), true)?;
    let sans_prompt: Vec<String> = indices
        .iter()
        .filter(|k| tous.get(**k).is_none_or(|p| texte(p, "prompt").trim().is_empty()))
        .map(|k| (k + 1).to_string())
        .collect();
    if !sans_prompt.is_empty() {
        let s_pluriel = if sans_prompt.len() > 1 { "s" } else { "" };
        return Err(format!(
            "Plan{s_pluriel} {} sans prompt : écris-le dans le plan, ou découpe la séquence au Scénario",
            sans_prompt.join(", ")
        ));
    }
    let wf = workflow(contexte)?;
    if !wf.manquants(&contexte.modeles).is_empty() {
        return Err(format!("Workflow vidéo incomplet : {}", wf.id));
    }
    if wf.entree("steps").is_some() && steps_base(contexte, &s) == 0 {
        return Err(
            "Checkpoint à valider : ses steps ne sont pas connus ; valide-les dans Modèles → Ma collection"
                .to_owned(),
        );
    }
    let titre = champ(&s, "titre");
    let format = champ(&s, "format");
    let mut c = Commande::nouvelle(id_commande, "sequence", titre, wf.id.clone(), &format);
    c.modele = s.get("modele").and_then(Value::as_str).map(str::to_owned);
    c.qualite = s.get("qualite").and_then(Value::as_str).map(str::to_owned);
    c.sequence = Some(id.to_owned());
    c.plans = indices;
    Ok(c)
}

fn televerser_creation(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    id: &str,
    video: bool,
) -> Result<String, String> {
    let chemin = if video {
        creations::chemin_video(&contexte.data, id)?
    } else {
        creations::chemin_image(&contexte.data, id)?
    };
    let octets = fs::read(&chemin).map_err(|e| format!("{id} illisible : {e}"))?;
    let nom = chemin
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    generateur.televerser(&nom, &octets)
}

fn enregistrer_plan(s: &mut Sequence, k: usize, cle: &str, valeur: Value) {
    if let Some(p) = s
        .get_mut("plans")
        .and_then(Value::as_array_mut)
        .and_then(|ps| ps.get_mut(k))
        .and_then(Value::as_object_mut)
    {
        p.insert(cle.to_owned(), valeur);
    }
}

/// Tourne les plans demandés de la séquence, dans l'ordre, puis assemble le film.
///
/// # Errors
/// Séquence inconnue, plan sans prompt, filtre AS-18, ComfyUI qui échoue ou ne rend pas la vidéo, ou annulation.
#[allow(clippy::too_many_lines)] // le tournage de la v2, gardé d'un tenant pour se relire à côté d'elle
pub fn executer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    suivi: &Suivi<'_>,
    c: &Commande,
) -> Result<(), String> {
    let id = c
        .sequence
        .as_deref()
        .ok_or_else(|| "Séquence manquante".to_owned())?;
    let mut s = lire(&contexte.data, id)?;
    let origine = textes(&contexte.data, &s)?;
    let contrainte = contrainte_tournage(&s);
    let contrainte_texte = champ(&casting(&s), "contrainte");
    let loras: Vec<ChoixLora> =
        serde_json::from_value(s.get("loras").cloned().unwrap_or_default()).unwrap_or_default();
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;

    // 1. Les prompts : écrits au Scénario ou dans le plan, la contrainte garantie, les mots déclencheurs en tête.
    for k in &c.plans {
        let plan = plans(&s)
            .into_iter()
            .nth(*k)
            .ok_or_else(|| "Plan inconnu".to_owned())?;
        let prompt = texte(&plan, "prompt");
        if prompt.trim().is_empty() {
            return Err(format!(
                "Plan {} sans prompt : écris-le dans le plan, ou découpe la séquence au Scénario",
                k + 1
            ));
        }
        let final_ =
            plateau::avec_declencheurs(&avec_contrainte(prompt.trim(), contrainte.as_deref()), &loras);
        let idee = texte(&plan, "idee");
        let mut a_lire: Vec<&str> = vec![contrainte_texte.as_str(), idee.as_str(), final_.as_str()];
        a_lire.extend(origine.iter().map(String::as_str));
        filtre.verifier(&a_lire).map_err(|e| e.to_string())?;
        enregistrer_plan(&mut s, *k, "prompt_final", json!(final_));
    }
    ecrire(&contexte.data, &s)?;

    // 2. Le tournage, plan après plan : chacun peut partir du précédent.
    let total = c.plans.len();
    let cle = champ(&s, "qualite");
    let megapixels = montage::qualites(contexte)
        .into_iter()
        .find(|q| q.cle == cle)
        .map_or(0.6, |q| q.megapixels);
    let wf = workflow(contexte)?;
    let dossier_bobines = champ(&s, "dossier");
    for (rang, k) in c.plans.iter().enumerate() {
        if suivi.annulee() {
            return Err(crate::comfy_serveur::ANNULEE.to_owned());
        }
        let plan = plans(&s)
            .into_iter()
            .nth(*k)
            .ok_or_else(|| "Plan inconnu".to_owned())?;
        let debut = Instant::now();
        suivi.etape(&format!("Plan {} · tournage ({}/{total})", k + 1, rang + 1));
        suivi.progression(0.0);
        let seed = match plan.get("seed").and_then(Value::as_u64) {
            Some(graine) => graine,
            None => plateau::seed_au_hasard()?,
        };
        let (ref_perso, ref_voix) = references(&s, *k);
        let prompt = h3::en_reference(
            &texte(&plan, "prompt_final"),
            ref_perso.is_some(),
            ref_voix.is_some(),
        );
        let enchainement = texte(&plan, "enchainement");
        let mut fichiers = Televerses::default();
        let rush = texte(&plan, "rush");
        if !rush.is_empty() {
            fichiers.rush = Some(televerser_creation(contexte, generateur, &rush, false)?);
        }
        if enchainement == "prolonger" || enchainement == "transition" {
            let precedent = plans(&s)
                .into_iter()
                .nth(k.saturating_sub(1))
                .map(|p| texte(&p, "clip"))
                .filter(|c| !c.is_empty())
                .ok_or_else(|| format!("Plan {} : le plan précédent n'est pas tourné", k + 1))?;
            fichiers.precedent = Some(televerser_creation(contexte, generateur, &precedent, true)?);
        }
        if let Some(p) = &ref_perso {
            fichiers.personnage = Some(televerser_creation(contexte, generateur, p, false)?);
        }
        match (id_voix(ref_voix.as_deref()), &ref_voix) {
            (Some(id_v), _) => {
                let (chemin, _) = voix::son(&contexte.data, id_v)?;
                let octets = fs::read(&chemin).map_err(|e| format!("voix {id_v} illisible : {e}"))?;
                let nom = chemin
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                fichiers.voix_son = Some(generateur.televerser(&nom, &octets)?);
            }
            (None, Some(clip)) => {
                fichiers.voix_clip = Some(televerser_creation(contexte, generateur, clip, true)?)
            }
            (None, None) => {}
        }
        let api = graphe_plan(contexte, &s, *k, &prompt, seed, &fichiers)?;
        let sorties = generateur.generer(&api, &mut |v, m| suivi.progression_de(v, m), suivi.drapeau())?;
        let (mp4, affiche) = video_et_affiche(&sorties)
            .ok_or_else(|| format!("Plan {} : ComfyUI n'a pas renvoyé la vidéo attendue", k + 1))?;
        let modele = checkpoint(contexte, &wf, &s);
        let mut infos = Map::new();
        for (cle_info, valeur) in [
            ("type", json!("clip")),
            ("mode_video", json!("sequence")),
            ("sequence", json!(id)),
            ("plan", json!(k + 1)),
            ("enchainement", json!(enchainement)),
            ("idee", json!(texte(&plan, "idee"))),
            ("prompt", json!(prompt)),
            ("llm", s.get("llm").cloned().unwrap_or(Value::Null)),
            ("workflow", json!(wf.id)),
            ("qualite", json!(cle)),
            ("duree", json!(duree_plan(&plan))),
            ("megapixels", json!(megapixels)),
            ("steps", json!(steps_plan(contexte, &s, &plan))),
            ("voix", json!(ref_voix)),
            ("modele", json!(modele)),
            ("seed", json!(seed)),
            ("parent", json!(Some(rush.clone()).filter(|r| !r.is_empty()))),
            ("dossier", json!(dossier_bobines)),
            (
                "duree_s",
                json!((debut.elapsed().as_secs_f64() * 100.0).round() / 100.0),
            ),
        ] {
            infos.insert(cle_info.to_owned(), valeur);
        }
        let ancien = texte(&plan, "clip");
        let id_clip = creations::enregistrer_clip(&contexte.data, &mp4, &affiche, &infos)?;
        enregistrer_plan(&mut s, *k, "clip", json!(&id_clip));
        enregistrer_plan(&mut s, *k, "a_jour", json!(true));
        suivi.image(id_clip);
        ecrire(&contexte.data, &s)?; // un plan tourné est acquis, même si la suite échoue
        if !ancien.is_empty() {
            let _ = rushes::agir(&contexte.data, &[ancien], "corbeille", None);
        }
    }

    // 3. L'assemblage du film (quelques secondes).
    suivi.etape("Assemblage du film");
    suivi.progression(0.0);
    let debut = Instant::now();
    let mut noms = Vec::new();
    for p in plans(&s) {
        noms.push(televerser_creation(
            contexte,
            generateur,
            &texte(&p, "clip"),
            true,
        )?);
    }
    let sorties = generateur.generer(
        &graphe_film(&s, &noms),
        &mut |v, m| suivi.progression_de(v, m),
        suivi.drapeau(),
    )?;
    let (mp4, affiche) = video_et_affiche(&sorties)
        .ok_or_else(|| "Assemblage : ComfyUI n'a pas renvoyé le film".to_owned())?;
    let finals: Vec<String> = plans(&s).iter().map(|p| texte(p, "prompt_final")).collect();
    let clips: Vec<String> = plans(&s).iter().map(|p| texte(p, "clip")).collect();
    let mut infos = Map::new();
    for (cle_info, valeur) in [
        ("type", json!("clip")),
        ("mode_video", json!("film")),
        ("sequence", json!(id)),
        ("idee", s.get("titre").cloned().unwrap_or(Value::Null)),
        ("prompt", json!(finals.join(" | "))),
        ("llm", s.get("llm").cloned().unwrap_or(Value::Null)),
        ("workflow", Value::Null),
        ("qualite", json!(cle)),
        ("duree", json!(plans(&s).iter().map(duree_plan).sum::<u32>())),
        ("plans", json!(clips)),
        ("seed", Value::Null),
        ("parent", Value::Null),
        ("dossier", json!(dossier_bobines)),
        (
            "duree_s",
            json!((debut.elapsed().as_secs_f64() * 100.0).round() / 100.0),
        ),
    ] {
        infos.insert(cle_info.to_owned(), valeur);
    }
    let ancien_film = champ(&s, "film");
    let film = creations::enregistrer_clip(&contexte.data, &mp4, &affiche, &infos)?;
    s.insert("film".to_owned(), json!(&film));
    suivi.image(film);
    ecrire(&contexte.data, &s)?;
    if !ancien_film.is_empty() {
        let _ = rushes::agir(&contexte.data, &[ancien_film], "corbeille", None);
    }
    Ok(())
}

/// La vidéo et son affiche parmi les sorties de ComfyUI.
fn video_et_affiche(sorties: &[(String, Vec<u8>)]) -> Option<(Vec<u8>, Vec<u8>)> {
    let noms: Vec<String> = sorties.iter().map(|(n, _)| n.clone()).collect();
    let (v, a) = comfy::video_et_affiche(&noms);
    let octets =
        |nom: Option<&String>| nom.and_then(|n| sorties.iter().find(|(f, _)| f == n).map(|(_, o)| o.clone()));
    Some((octets(v)?, octets(a)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(v: Value) -> Sequence {
        v.as_object().cloned().unwrap()
    }

    fn exemple() -> Sequence {
        sequence(json!({
            "id": "20261122-101500-abcdef", "titre": "Essai", "qualite": "compromis", "format": "16:9 (Widescreen)",
            "casting": {"personnage": null, "voix": "auto", "ambiance": "", "contrainte": "", "reference_partout": true},
            "plans": [
                {"id": "p0", "enchainement": "rush", "rush": "r0", "duree": 5, "idee": "Elle dit « Bonjour »",
                 "prompt": "x", "clip": "c0", "a_jour": true},
                {"id": "p1", "enchainement": "couper", "rush": null, "duree": 8, "idee": "Il répond « Salut »",
                 "prompt": "y", "clip": "c1", "a_jour": true},
                {"id": "p2", "enchainement": "prolonger", "rush": null, "duree": 5, "idee": "La pluie tombe",
                 "prompt": "z", "clip": null, "a_jour": false},
            ],
        }))
    }

    #[test]
    fn identifiants() {
        assert!(id_valide("20261122-101500-abcdef"));
        assert!(!id_valide("20261122-101500-ABCDEF") && !id_valide("../x") && !id_valide(""));
    }

    #[test]
    fn plans_a_tourner() {
        let s = exemple();
        assert_eq!(a_tourner(&s, &[]), [2]);
        // Retourner le plan 2 retourne aussi le 3, qui le prolonge.
        assert_eq!(a_tourner(&s, &[1]), [1, 2]);
        assert_eq!(a_tourner(&s, &[0]), [0, 2]);
    }

    #[test]
    fn references_des_plans() {
        let mut s = exemple();
        // Le personnage : le rush de départ ; la voix (Auto) : le premier clip tourné qui parle.
        assert_eq!(personnage(&s).as_deref(), Some("r0"));
        assert_eq!(references(&s, 1), (Some("r0".to_owned()), Some("c0".to_owned())));
        assert_eq!(references(&s, 2), (None, None));
        assert_eq!(references(&s, 0), (Some("r0".to_owned()), None));
        s.insert(
            "casting".to_owned(),
            json!({"personnage": "perso", "voix": "voix:20261122-101500-abcdef", "reference_partout": false}),
        );
        assert_eq!(
            references(&s, 0),
            (None, Some("voix:20261122-101500-abcdef".to_owned()))
        );
        assert_eq!(references(&s, 1).0.as_deref(), Some("perso"));
        assert_eq!(rushes_utilises(&s), ["r0", "perso"]);
    }

    #[test]
    fn contrainte_garantie() {
        assert_eq!(avec_contrainte("A cat sleeps.", None), "A cat sleeps.");
        assert_eq!(
            avec_contrainte("A cat sleeps.", Some("no text.")),
            "A cat sleeps. Constraint for every shot: no text."
        );
        let champs = "integrated_multimodal_description: [Shot 1] A cat sleeps.\n\noverall_soundscape: rain";
        let r = avec_contrainte(champs, Some("no text"));
        assert!(r.contains("A cat sleeps. Constraint for every shot: no text.\n\noverall_soundscape: rain"));
        let mut s = exemple();
        assert_eq!(contrainte_tournage(&s), None);
        s.insert("casting".to_owned(), json!({"contrainte": "pas de texte"}));
        assert_eq!(contrainte_tournage(&s).as_deref(), Some("pas de texte"));
        s.insert(
            "contrainte_en".to_owned(),
            json!({"fr": "pas de texte", "en": "no text"}),
        );
        assert_eq!(contrainte_tournage(&s).as_deref(), Some("no text"));
    }

    #[test]
    fn film_assemble() {
        let s = exemple();
        let noms = ["a.mp4".to_owned(), "b.mp4".to_owned(), "c.mp4".to_owned()];
        let api = graphe_film(&s, &noms);
        // Le plan 2 perd sa dernière image (refaite par le plan 3), le plan 3 ses 39 premières.
        assert!(api.get("t0").is_none());
        let fin = f64::from(h3::images(8) - 1) / f64::from(h3::IPS);
        assert_eq!(api["t1"]["inputs"]["duration"], json!(fin));
        assert_eq!(
            api["t2"]["inputs"]["start_time"],
            json!(f64::from(GUIDE) / f64::from(h3::IPS))
        );
        assert_eq!(api["film"]["inputs"]["videos.video2"], json!(["t2", 0]));
        assert_eq!(api["film"]["class_type"], "ConcatenateVideo");
    }

    #[test]
    fn voix_supprimee_oubliee() {
        let temp = tempfile::tempdir().unwrap();
        let mut s = exemple();
        s.insert(
            "casting".to_owned(),
            json!({"voix": "voix:20261122-101500-abcdef"}),
        );
        ecrire(temp.path(), &s).unwrap();
        let touchees = oublier_voix(temp.path(), "20261122-101500-abcdef", "Léa").unwrap();
        assert_eq!(touchees, ["20261122-101500-abcdef"]);
        let relue = lire(temp.path(), "20261122-101500-abcdef").unwrap();
        assert_eq!(relue["casting"]["voix"], "auto");
        assert!(plans(&relue).iter().all(|p| p["a_jour"] == json!(false)));
        assert!(champ(&relue, "alerte").contains("Léa"));
        assert!(
            oublier_voix(temp.path(), "20261122-101500-abcdef", "Léa")
                .unwrap()
                .is_empty()
        );
    }
}
