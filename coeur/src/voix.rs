//! Les voix de référence des séquences, comme en v2 (`app/voix.py`) : une bibliothèque de voix propres
//! (`data\voix\`), réutilisables d'un film à l'autre. Une voix vient :
//! - d'un essai de voix : H3 tourne un gros plan en cabine d'enregistrement capitonnée, sans musique ni ambiance, où le
//!   personnage dit une phrase ; seul le son est gardé (FLAC) ;
//! - d'un fichier audio de l'utilisateur (wav, mp3, flac ou ogg).
//!
//! Un essai reste un brouillon tant qu'il n'est pas gardé sous un nom ; les brouillons de plus d'un jour sont effacés.
//! Une voix supprimée part dans `data\voix\corbeille\`. AS-18 : la phrase, la description et les textes d'origine du
//! personnage passent par le filtre.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{Duration, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::plateau::{self, Commande, Contexte, Generateur, Suivi};
use crate::{creations, h3, montage, reglages, securite, workflows};

/// La valeur du casting d'une séquence qui désigne une voix de la bibliothèque : `voix:<id>`.
pub const PREFIXE: &str = "voix:";
pub const PHRASE_DEFAUT: &str = "Bonjour. Voici ma voix, calme et posée, pour que tu l'entendes bien.";
pub const LONGUEUR_PHRASE: usize = 200;
pub const MOTS_MAX: usize = 25;
pub const LONGUEUR_DESCRIPTION: usize = 300;
pub const LONGUEUR_NOM: usize = 40;
/// L'image ne sert à rien : la plus petite résolution.
const QUALITE: &str = "rapide";
/// Comme les plans qui parlent : la voix a besoin de 12 steps.
const STEPS: u32 = 12;
/// La durée de l'essai selon la phrase (environ 2,5 mots par seconde, avec de la marge).
const DUREES: [(usize, u32); 3] = [(13, 5), (20, 8), (MOTS_MAX, 10)];
/// Les formats acceptés et leur type.
pub const FORMATS: [(&str, &str); 4] = [
    ("flac", "audio/flac"),
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("wav", "audio/wav"),
];
pub const TAILLE_MAX: usize = 8 * 1024 * 1024;
/// La voix d'une référence : au moins 3 s ; au-delà de 15 s, elle est recadrée sur ses 15 premières secondes.
pub const DUREE_MIN: f64 = 3.0;
pub const DUREE_MAX: f64 = 15.0;
/// Repris de l'essai V2 de la v2 (le plus propre).
const SILENCE: &str = "The room is completely silent: no music, no ambient sound, no machine noise, no echo, no breathing noise. Only the character's clear voice, close to the microphone.";

/// Une voix, au format de la v2 (`data\voix\<id>.json`).
pub type Voix = Map<String, Value>;

fn dossier(data: &Path) -> PathBuf {
    data.join("voix")
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

fn chemin(data: &Path, id: &str) -> Result<PathBuf, String> {
    let c = dossier(data).join(format!("{id}.json"));
    if id_valide(id) && c.is_file() {
        Ok(c)
    } else {
        Err(format!("Voix inconnue : {id}"))
    }
}

/// Une voix de la bibliothèque (gardée ou brouillon).
///
/// # Errors
/// Voix inconnue ou illisible.
pub fn lire(data: &Path, id: &str) -> Result<Voix, String> {
    let texte = fs::read_to_string(chemin(data, id)?).map_err(|e| e.to_string())?;
    match serde_json::from_str::<Value>(&texte) {
        Ok(Value::Object(v)) => Ok(v),
        _ => Err(format!("Voix illisible : {id}")),
    }
}

fn ecrire(data: &Path, v: &Voix) -> Result<(), String> {
    let id = v.get("id").and_then(Value::as_str).unwrap_or_default();
    let d = dossier(data);
    fs::create_dir_all(&d).map_err(|e| format!("{} : {e}", d.display()))?;
    let temporaire = d.join(format!("{id}.tmp"));
    let texte = serde_json::to_string_pretty(&Value::Object(v.clone())).map_err(|e| e.to_string())?;
    fs::write(&temporaire, texte)
        .and_then(|()| fs::rename(&temporaire, d.join(format!("{id}.json"))))
        .map_err(|e| format!("voix {id} : {e}"))
}

/// Le fichier son d'une voix, et son type.
///
/// # Errors
/// Voix inconnue.
pub fn son(data: &Path, id: &str) -> Result<(PathBuf, &'static str), String> {
    let v = lire(data, id)?;
    let fichier = v.get("fichier").and_then(Value::as_str).unwrap_or_default();
    let extension = fichier.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    let (_, mime) = FORMATS
        .iter()
        .find(|(e, _)| *e == extension)
        .ok_or_else(|| format!("Son de la voix illisible : {id}"))?;
    if fichier.contains(['/', '\\']) {
        return Err(format!("Son de la voix illisible : {id}"));
    }
    Ok((dossier(data).join(fichier), *mime))
}

fn date_de(v: &Voix) -> Option<NaiveDateTime> {
    v.get("date")
        .and_then(Value::as_str)
        .and_then(|d| NaiveDateTime::parse_from_str(d, "%Y-%m-%dT%H:%M:%S").ok())
}

fn gardee_ou_non(v: &Voix) -> bool {
    v.get("gardee").and_then(Value::as_bool).unwrap_or(false)
}

/// Efface les essais jamais gardés de plus d'un jour.
fn nettoyer(data: &Path) {
    let Ok(lecture) = fs::read_dir(dossier(data)) else {
        return;
    };
    let limite = Local::now().naive_local() - Duration::days(1);
    for e in lecture.filter_map(Result::ok) {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Some(v) = fs::read_to_string(&p)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
        else {
            continue;
        };
        if !gardee_ou_non(&v) && date_de(&v).is_some_and(|d| d < limite) {
            if let Some(f) = v
                .get("fichier")
                .and_then(Value::as_str)
                .filter(|f| !f.contains(['/', '\\']))
            {
                let _ = fs::remove_file(dossier(data).join(f));
            }
            let _ = fs::remove_file(&p);
        }
    }
}

/// Les voix gardées, les plus récentes d'abord (et les brouillons si demandé).
pub fn liste(data: &Path, brouillons: bool) -> Vec<Voix> {
    nettoyer(data);
    let Ok(lecture) = fs::read_dir(dossier(data)) else {
        return Vec::new();
    };
    let mut voix: Vec<Voix> = lecture
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|p| fs::read_to_string(p).ok())
        .filter_map(|t| serde_json::from_str::<Value>(&t).ok())
        .filter_map(|v| v.as_object().cloned())
        .filter(|v| brouillons || gardee_ou_non(v))
        .collect();
    voix.sort_by(|a, b| {
        let d = |v: &Voix| {
            v.get("date")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        d(b).cmp(&d(a))
    });
    voix
}

/// La voix, si elle est gardée dans la bibliothèque (un brouillon ne sert pas de référence).
///
/// # Errors
/// Voix inconnue, ou brouillon.
pub fn gardee(data: &Path, id: &str) -> Result<Voix, String> {
    let v = lire(data, id)?;
    if gardee_ou_non(&v) {
        Ok(v)
    } else {
        Err("Garde d'abord cet essai de voix sous un nom".to_owned())
    }
}

fn nom_valide(nom: &str) -> Result<String, String> {
    let nom = nom.split_whitespace().collect::<Vec<_>>().join(" ");
    if nom.is_empty() || nom.chars().count() > LONGUEUR_NOM {
        Err(format!("Nom de 1 à {LONGUEUR_NOM} caractères"))
    } else {
        Ok(nom)
    }
}

/// Garde un essai dans la bibliothèque sous ce nom (ou renomme une voix déjà gardée).
///
/// # Errors
/// Voix inconnue, nom invalide, ou fichier qui ne s'écrit pas.
pub fn garder(data: &Path, id: &str, nom: &str) -> Result<Voix, String> {
    let mut v = lire(data, id)?;
    v.insert("nom".to_owned(), json!(nom_valide(nom)?));
    v.insert("gardee".to_owned(), json!(true));
    ecrire(data, &v)?;
    Ok(v)
}

/// Met la voix à la corbeille (`data\voix\corbeille`).
///
/// # Errors
/// Voix inconnue, ou fichier qui ne se déplace pas.
pub fn supprimer(data: &Path, id: &str) -> Result<Voix, String> {
    let v = lire(data, id)?;
    let corbeille = dossier(data).join("corbeille");
    fs::create_dir_all(&corbeille).map_err(|e| e.to_string())?;
    let mut fichiers = vec![chemin(data, id)?];
    if let Some(f) = v
        .get("fichier")
        .and_then(Value::as_str)
        .filter(|f| !f.contains(['/', '\\']))
    {
        fichiers.push(dossier(data).join(f));
    }
    for f in fichiers.iter().rev() {
        if let Some(nom) = f.file_name()
            && f.is_file()
        {
            fs::rename(f, corbeille.join(nom)).map_err(|e| format!("voix {id} : {e}"))?;
        }
    }
    Ok(v)
}

fn nouvel_id() -> Result<String, String> {
    let mut octets = [0u8; 3];
    getrandom::fill(&mut octets).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    let suffixe: String = octets.iter().map(|o| format!("{o:02x}")).collect();
    Ok(format!("{}-{suffixe}", Local::now().format("%Y%m%d-%H%M%S")))
}

fn nouvelle(data: &Path, son: &[u8], extension: &str, infos: Map<String, Value>) -> Result<Voix, String> {
    let d = dossier(data);
    fs::create_dir_all(&d).map_err(|e| format!("{} : {e}", d.display()))?;
    let id = nouvel_id()?;
    let fichier = format!("{id}.{extension}");
    fs::write(d.join(&fichier), son).map_err(|e| format!("son de la voix : {e}"))?;
    let mut v = Map::new();
    v.insert("id".to_owned(), json!(id));
    v.insert(
        "date".to_owned(),
        json!(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
    );
    v.insert("fichier".to_owned(), json!(fichier));
    v.insert("duree_son".to_owned(), json!(duree_audio(son, extension)));
    v.extend(infos);
    ecrire(data, &v)?;
    Ok(v)
}

/// AS-18 : ce qui a fait la voix (phrase, description, textes d'origine du personnage de l'essai).
///
/// # Errors
/// Voix inconnue.
pub fn textes(data: &Path, id: &str) -> Result<Vec<String>, String> {
    let v = lire(data, id)?;
    let mut sortie: Vec<String> = ["phrase", "description"]
        .iter()
        .filter_map(|k| {
            v.get(*k)
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        })
        .collect();
    if let Some(p) = v.get("personnage").and_then(Value::as_str) {
        sortie.extend(creations::textes_origine(data, &[p.to_owned()])?);
    }
    Ok(sortie)
}

/// Le format d'un son d'après ses premiers octets (jamais d'après son nom).
pub fn format_audio(son: &[u8]) -> Option<&'static str> {
    match son {
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'A', b'V', b'E', ..] => Some("wav"),
        [b'f', b'L', b'a', b'C', ..] => Some("flac"),
        [b'O', b'g', b'g', b'S', ..] => Some("ogg"),
        [b'I', b'D', b'3', ..] => Some("mp3"),
        [0xFF, second, ..] if second & 0xE0 == 0xE0 => Some("mp3"),
        _ => None,
    }
}

/// La durée d'un son en secondes quand elle se lit sans décodeur (wav, flac) ; rien sinon.
pub fn duree_audio(son: &[u8], format: &str) -> Option<f64> {
    let u32_le = |i: usize| {
        son.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    match format {
        "wav" => {
            // Le bloc « fmt » donne l'octet-rate, le bloc « data » la taille : on parcourt les blocs.
            let mut i = 12;
            let mut octets_par_seconde = None;
            while i + 8 <= son.len() {
                let id = son.get(i..i + 4)?;
                let taille = u32_le(i + 4)? as usize;
                if id == b"fmt " {
                    octets_par_seconde = u32_le(i + 16);
                } else if id == b"data" {
                    let debit = f64::from(octets_par_seconde.filter(|d| *d > 0)?);
                    #[allow(clippy::cast_precision_loss)] // quelques mégaoctets
                    let secondes = taille as f64 / debit;
                    return Some((secondes * 100.0).round() / 100.0);
                }
                i += 8 + taille + taille % 2;
            }
            None
        }
        "flac" => {
            // STREAMINFO, premier bloc après « fLaC » : fréquence sur 20 bits, nombre d'échantillons sur 36 bits.
            let b = son.get(18..26)?;
            let infos = u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
            let (frequence, echantillons) = (infos >> 44, infos & ((1 << 36) - 1));
            #[allow(clippy::cast_precision_loss)] // des secondes affichées
            let secondes = echantillons as f64 / frequence as f64;
            (frequence > 0 && echantillons > 0).then(|| (secondes * 100.0).round() / 100.0)
        }
        _ => None,
    }
}

/// Une voix enregistrée par l'utilisateur, gardée aussitôt dans la bibliothèque.
///
/// # Errors
/// Nom invalide, fichier trop gros, format refusé, voix trop courte, ou fichier qui ne s'écrit pas.
pub fn importer(data: &Path, son: &[u8], nom: &str, nom_origine: &str) -> Result<Voix, String> {
    let nom = nom_valide(nom)?;
    if son.len() > TAILLE_MAX {
        return Err(format!(
            "Fichier audio de {} Mo au plus",
            TAILLE_MAX / (1024 * 1024)
        ));
    }
    let formats: Vec<&str> = FORMATS.iter().map(|(f, _)| *f).collect();
    let format = format_audio(son).ok_or_else(|| format!("Format audio accepté : {}", formats.join(", ")))?;
    if let Some(d) = duree_audio(son, format)
        && d < DUREE_MIN
    {
        return Err(format!(
            "Il faut au moins {DUREE_MIN} s de voix (ce fichier en a {d})"
        ));
    }
    let mut infos = Map::new();
    for (k, v) in [
        ("nom", json!(nom)),
        ("gardee", json!(true)),
        ("source", json!("fichier")),
        (
            "nom_origine",
            json!(nom_origine.chars().take(120).collect::<String>()),
        ),
        ("phrase", Value::Null),
        ("description", Value::Null),
        ("langue", Value::Null),
        ("personnage", Value::Null),
        ("seed", Value::Null),
        ("duree_s", json!(0)),
    ] {
        infos.insert(k.to_owned(), v);
    }
    nouvelle(data, son, format, infos)
}

// --- essai de voix ------------------------------------------------------------------------------------------------

/// Une demande d'essai de voix.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeEssai {
    #[serde(default)]
    pub phrase: String,
    #[serde(default)]
    pub description: String,
    /// Le rush du personnage, s'il y en a un.
    #[serde(default)]
    pub personnage: Option<String>,
    #[serde(default)]
    pub seed: Option<u64>,
}

/// Contrôle un essai de voix comme la v2 et rend la commande à mettre en file.
///
/// # Errors
/// Phrase ou description trop longues, ni personnage ni description, personnage qui est un clip ou inconnu, workflow
/// vidéo incomplet, ou filtre AS-18.
pub fn commande_essai(contexte: &Contexte, d: &DemandeEssai, id: String) -> Result<Commande, String> {
    let phrase = d.phrase.split_whitespace().collect::<Vec<_>>().join(" ");
    let phrase = if phrase.is_empty() {
        PHRASE_DEFAUT.to_owned()
    } else {
        phrase
    };
    let description = d.description.split_whitespace().collect::<Vec<_>>().join(" ");
    if phrase.chars().count() > LONGUEUR_PHRASE || phrase.split_whitespace().count() > MOTS_MAX {
        return Err(format!(
            "Phrase d'essai de {MOTS_MAX} mots et {LONGUEUR_PHRASE} caractères au plus"
        ));
    }
    if description.chars().count() > LONGUEUR_DESCRIPTION {
        return Err(format!(
            "Description de {LONGUEUR_DESCRIPTION} caractères au plus"
        ));
    }
    let personnage = d.personnage.clone().filter(|p| !p.is_empty());
    if personnage.is_none() && description.is_empty() {
        return Err("Sans personnage, décris qui parle (par exemple « une femme d'une trentaine d'années, voix douce et un peu grave »)".to_owned());
    }
    if let Some(p) = &personnage {
        if creations::fiche(&contexte.data, p)?
            .get("type")
            .and_then(Value::as_str)
            == Some("clip")
        {
            return Err("Le personnage est une image, pas un clip".to_owned());
        }
        creations::chemin_image(&contexte.data, p)?;
    }
    let mots = phrase
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .count();
    let duree = DUREES
        .iter()
        .find(|(limite, _)| mots <= *limite)
        .map_or(DUREES[2].1, |(_, d)| *d);
    let origine = match &personnage {
        Some(p) => creations::textes_origine(&contexte.data, std::slice::from_ref(p))?,
        None => Vec::new(),
    };
    let mut textes = vec![phrase.as_str(), description.as_str()];
    textes.extend(origine.iter().map(String::as_str));
    securite::verifier(&contexte.dossier, &textes).map_err(|e| e.to_string())?;
    let workflow = montage::workflow_de(contexte, "image_vers_video");
    let wf = workflows::obtenir(&workflow)?;
    if !wf.manquants(&contexte.modeles).is_empty() {
        return Err(format!("Workflow vidéo incomplet : {workflow}"));
    }
    let mut c = Commande::nouvelle(id, "voix", phrase, workflow, "voix");
    c.seed = d.seed;
    c.parent = personnage;
    c.qualite = Some(QUALITE.to_owned());
    c.duree = Some(duree);
    c.description = description;
    Ok(c)
}

/// Le prompt de l'essai V2 de la v2 : gros plan en cabine, la phrase dans sa langue, le silence autour.
pub fn prompt_essai(phrase: &str, description: &str, avec_personnage: bool) -> String {
    let replique: String = phrase.chars().filter(|c| *c != '«' && *c != '»').collect();
    let replique = replique.trim().trim_matches(['"', '“', '”']).trim().to_owned();
    let mut morceaux = Vec::new();
    if avec_personnage {
        morceaux.push("The main character is the person from <Picture 1>.".to_owned());
    } else if !description.is_empty() {
        morceaux.push(format!("The character: {description}."));
    }
    morceaux.push(format!(
        "A close-up portrait of the character in a small recording booth lined with acoustic foam, plain dark background, soft studio light. The character looks at the camera and says: {}",
        h3::balise(&replique)
    ));
    let voix = if avec_personnage && !description.is_empty() {
        format!(", with this voice: {description}")
    } else {
        String::new()
    };
    morceaux.push(format!("The character speaks {}{voix}.", h3::langue(&replique)));
    morceaux.push(SILENCE.to_owned());
    morceaux.join(" ")
}

/// Le graphe de l'essai : `ReferenceToVideo` (le personnage en <Picture 1>, ou aucun), le son seul en sortie.
fn graphe_essai(
    contexte: &Contexte,
    wf: &workflows::Workflow,
    prompt: &str,
    seed: u64,
    duree: u32,
    personnage: Option<&str>,
) -> Result<Value, String> {
    let megapixels = montage::qualites(contexte)
        .into_iter()
        .find(|q| q.cle == QUALITE)
        .map_or(0.4, |q| q.megapixels);
    let reglage = reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&wf.id).and_then(Value::as_object).cloned());
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let mut valeurs = Map::new();
    valeurs.insert("prompt".to_owned(), json!(prompt));
    valeurs.insert("seed".to_owned(), json!(seed));
    valeurs.insert("duree".to_owned(), json!(duree));
    valeurs.insert("megapixels".to_owned(), json!(megapixels));
    valeurs.insert("steps".to_owned(), json!(STEPS));
    valeurs.insert("prefixe".to_owned(), json!("aria_voix"));
    let mut api = wf.preparer(&valeurs, &contexte.modeles, reglage.as_ref(), &filtre)?;
    let i2v = workflows::noeud_unique(&api, "MiniMaxH3ImageToVideo")
        .ok_or_else(|| format!("{} n'a pas de MiniMaxH3ImageToVideo unique", wf.id))?;
    let cv = workflows::noeud_unique(&api, "CreateVideo")
        .ok_or_else(|| format!("{} ne produit pas de vidéo", wf.id))?;
    let (vae, vae_audio) = h3::vaes(&api)?;
    let (largeur, hauteur) = h3::dimensions(1.0, megapixels); // l'image ne compte pas : carré, le plus léger
    let entrees = api
        .pointer(&format!("/{i2v}/inputs"))
        .cloned()
        .ok_or_else(|| "graphe H3 illisible".to_owned())?;
    let audio = api
        .pointer(&format!("/{cv}/inputs/audio"))
        .cloned()
        .ok_or_else(|| "graphe H3 sans son".to_owned())?;
    let mut entrees_ref = json!({
        "clip": entrees.get("clip"), "prompt": entrees.get("prompt"), "width": largeur, "height": hauteur,
        "length": entrees.get("length"), "ref_image_size": "match", "vae": [vae, 0], "audio_vae": [vae_audio, 0]
    });
    let graphe = api
        .as_object_mut()
        .ok_or_else(|| "graphe H3 illisible".to_owned())?;
    if let Some(p) = personnage {
        graphe.insert(
            "aria_personnage".to_owned(),
            json!({"class_type": "LoadImage", "inputs": {"image": p}}),
        );
        if let Some(e) = entrees_ref.as_object_mut() {
            e.insert("ref_images.ref_image_0".to_owned(), json!(["aria_personnage", 0]));
        }
    }
    graphe.insert(
        i2v,
        json!({"class_type": "MiniMaxH3ReferenceToVideo", "inputs": entrees_ref}),
    );
    graphe.insert(
        "aria_voix".to_owned(),
        json!({"class_type": "SaveAudio", "inputs": {"audio": audio, "filename_prefix": "aria_voix"}}),
    );
    // Ni vidéo ni vignette : seul le son compte.
    graphe.retain(|_, n| {
        !matches!(
            n.get("class_type").and_then(Value::as_str),
            Some("SaveVideo" | "SaveImage")
        )
    });
    Ok(h3::elaguer(&api))
}

/// Tourne un essai de voix, enregistré comme brouillon ; son identifiant va dans les images de la commande.
///
/// # Errors
/// Filtre AS-18, workflow absent, ComfyUI qui échoue ou ne rend pas le son.
pub fn executer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    suivi: &Suivi<'_>,
    c: &Commande,
) -> Result<(), String> {
    nettoyer(&contexte.data);
    let origine = creations::textes_origine(&contexte.data, c.parent.as_slice())?;
    let mut textes = vec![c.idee.as_str(), c.description.as_str()];
    textes.extend(origine.iter().map(String::as_str));
    securite::verifier(&contexte.dossier, &textes).map_err(|e| e.to_string())?;
    let wf = workflows::obtenir(&c.workflow)?;
    let prompt = prompt_essai(&c.idee, &c.description, c.parent.is_some());
    let seed = match c.seed {
        Some(s) => s,
        None => plateau::seed_au_hasard()?,
    };
    let debut = Instant::now();
    suivi.etape("Essai de voix");
    suivi.progression(0.0);
    let personnage = match &c.parent {
        Some(p) => {
            let png = fs::read(creations::chemin_image(&contexte.data, p)?).map_err(|e| e.to_string())?;
            Some(generateur.televerser(&format!("aria_{}.png", c.id), &png)?)
        }
        None => None,
    };
    let duree = c.duree.unwrap_or(DUREES[0].1);
    let api = graphe_essai(contexte, &wf, &prompt, seed, duree, personnage.as_deref())?;
    let sorties = generateur.generer(&api, &mut |v, m| suivi.progression_de(v, m), suivi.drapeau())?;
    let son = sorties
        .into_iter()
        .find(|(n, _)| n.to_ascii_lowercase().ends_with(".flac"))
        .map(|(_, o)| o)
        .ok_or_else(|| "ComfyUI n'a pas renvoyé le son de l'essai".to_owned())?;
    let mut infos = Map::new();
    for (k, v) in [
        ("nom", Value::Null),
        ("gardee", json!(false)),
        ("source", json!("essai")),
        ("phrase", json!(c.idee)),
        ("description", json!(c.description)),
        ("langue", json!(h3::langue(&c.idee))),
        ("personnage", json!(c.parent)),
        ("prompt", json!(prompt)),
        ("seed", json!(seed)),
        ("duree", json!(duree)),
        (
            "duree_s",
            json!((debut.elapsed().as_secs_f64() * 100.0).round() / 100.0),
        ),
    ] {
        infos.insert(k.to_owned(), v);
    }
    let v = nouvelle(&contexte.data, &son, "flac", infos)?;
    suivi.image(v.get("id").and_then(Value::as_str).unwrap_or_default().to_owned());
    Ok(())
}

/// Ce que l'interface doit savoir des voix.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsVoix {
    pub phrase_defaut: String,
    pub longueur_phrase: usize,
    pub mots_max: usize,
    pub longueur_description: usize,
    pub longueur_nom: usize,
    pub formats: Vec<String>,
    pub duree_min: f64,
    pub duree_max: f64,
    pub estimation_s: f64,
}

/// Ce que l'interface doit savoir des voix.
pub fn options(contexte: &Contexte) -> OptionsVoix {
    let estimation = montage::qualites(contexte)
        .into_iter()
        .find(|q| q.cle == QUALITE)
        .map_or(0.0, |q| (q.estimation_s * 1.4).round());
    OptionsVoix {
        phrase_defaut: PHRASE_DEFAUT.to_owned(),
        longueur_phrase: LONGUEUR_PHRASE,
        mots_max: MOTS_MAX,
        longueur_description: LONGUEUR_DESCRIPTION,
        longueur_nom: LONGUEUR_NOM,
        formats: FORMATS.iter().map(|(f, _)| (*f).to_owned()).collect(),
        duree_min: DUREE_MIN,
        duree_max: DUREE_MAX,
        estimation_s: estimation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(secondes: u32) -> Vec<u8> {
        let debit: u32 = 16_000 * 2;
        let taille = debit * secondes;
        let mut o = Vec::new();
        o.extend_from_slice(b"RIFF");
        o.extend_from_slice(&(36 + taille).to_le_bytes());
        o.extend_from_slice(b"WAVEfmt ");
        o.extend_from_slice(&16u32.to_le_bytes());
        o.extend_from_slice(&1u16.to_le_bytes()); // PCM
        o.extend_from_slice(&1u16.to_le_bytes()); // mono
        o.extend_from_slice(&16_000u32.to_le_bytes());
        o.extend_from_slice(&debit.to_le_bytes());
        o.extend_from_slice(&2u16.to_le_bytes());
        o.extend_from_slice(&16u16.to_le_bytes());
        o.extend_from_slice(b"data");
        o.extend_from_slice(&taille.to_le_bytes());
        o.resize(o.len() + taille as usize, 0);
        o
    }

    #[test]
    fn importer_garder_supprimer() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path();
        assert_eq!(format_audio(&wav(4)), Some("wav"));
        assert_eq!(duree_audio(&wav(4), "wav"), Some(4.0));
        assert_eq!(format_audio(b"fLaC...."), Some("flac"));
        assert_eq!(format_audio(&[0xFF, 0xFB, 0x90]), Some("mp3"));
        assert_eq!(format_audio(b"<html>"), None);
        assert!(
            importer(data, &wav(2), "Léa", "lea.wav")
                .unwrap_err()
                .contains("au moins 3 s")
        );
        assert!(importer(data, b"<html>", "Léa", "x").is_err());
        assert!(importer(data, &wav(4), "  ", "x").is_err());
        let v = importer(data, &wav(4), "  Léa   douce ", "lea.wav").unwrap();
        let id = v["id"].as_str().unwrap().to_owned();
        assert!(id_valide(&id));
        assert_eq!(v["nom"], json!("Léa douce"));
        assert_eq!(liste(data, false).len(), 1);
        assert_eq!(son(data, &id).unwrap().1, "audio/wav");
        assert!(gardee(data, &id).is_ok());
        garder(data, &id, "Léa").unwrap();
        assert_eq!(lire(data, &id).unwrap()["nom"], json!("Léa"));
        supprimer(data, &id).unwrap();
        assert!(lire(data, &id).is_err());
        assert!(data.join("voix/corbeille").join(format!("{id}.json")).is_file());
        assert!(lire(data, "../x").is_err());
    }

    #[test]
    fn brouillons_effaces_apres_un_jour() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path();
        let mut infos = Map::new();
        infos.insert("gardee".to_owned(), json!(false));
        let v = nouvelle(data, b"fLaC", "flac", infos).unwrap();
        let id = v["id"].as_str().unwrap().to_owned();
        assert!(
            gardee(data, &id).is_err(),
            "un brouillon ne sert pas de référence"
        );
        assert_eq!(liste(data, true).len(), 1);
        assert!(liste(data, false).is_empty());
        let mut vieux = lire(data, &id).unwrap();
        vieux.insert("date".to_owned(), json!("2020-01-01T00:00:00"));
        ecrire(data, &vieux).unwrap();
        assert!(liste(data, true).is_empty());
        assert!(!data.join("voix").join(format!("{id}.flac")).exists());
    }

    #[test]
    fn prompt_de_l_essai() {
        let p = prompt_essai("« Bonjour à toi. »", "voix grave", false);
        assert!(p.starts_with("The character: voix grave."));
        assert!(p.contains("<d>[French] Bonjour à toi.</d>"));
        assert!(p.contains("The character speaks French.") && p.ends_with("close to the microphone."));
        let p = prompt_essai("Hello", "voix douce", true);
        assert!(p.starts_with("The main character is the person from <Picture 1>."));
        assert!(p.contains("with this voice: voix douce"));
    }
}
