//! Modèles → Rechercher, comme en v2 (`app/decouvrir.py`, `app/sources/`) : modèles image, modèles vidéo, LoRA et
//! LLM sur Civitai et Hugging Face, et la fiche d'un fichier d'après son lien.
//!
//! Les types suivent les familles des fiches de workflow : un modèle image ou vidéo de Civitai n'est proposé que si sa
//! base va dans une famille d'Aria (bloc `civitai` des fiches), et son dossier est déduit de cette famille ; sur
//! Hugging Face, ce sont les dérivés des dépôts d'origine (bloc `hugging_face`). Un résultat déjà installé (même
//! SHA-256, ou même nom de fichier sur Hugging Face) le dit. Seuls les fichiers .safetensors et .gguf sont proposés :
//! les autres formats peuvent exécuter du code au chargement. Le téléchargement passe par la file (`telechargement`).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::modeles::{self, BASE, DOSSIER_LLM, TYPES_BASE};
use crate::telechargement::{Contexte, Fiche, Reseau, registre};
use crate::{suggestions, workflows};

pub const CIVITAI: &str = "https://civitai.com/api/v1";
pub const HF: &str = "https://huggingface.co";
const GO: u64 = 1 << 30;
/// Les types cherchés.
pub const TYPES: [(&str, &str); 4] = [
    ("image", "Modèles image"),
    ("video", "Modèles vidéo"),
    ("lora", "LoRA"),
    ("llm", "LLM (GGUF)"),
];
/// Les formats proposés : les autres (.ckpt, .pt, .bin…) peuvent exécuter du code au chargement.
pub const EXTENSIONS: [&str; 2] = ["safetensors", "gguf"];
/// Ce qu'un LLM prend en plus de son fichier : contexte 4096 et tampons de calcul.
const SURCOUT_LLM: u64 = 3 * GO / 2;
const TYPES_LORA: [&str; 3] = ["LORA", "LoCon", "DoRA"];
/// `nsfwLevel` est un masque : 1 PG, 2 PG-13, 4 R, 8 X, 16 XXX.
const NIVEAUX_ADULTES: u64 = 4 | 8 | 16;
/// Une réponse d'API lue, au plus.
const REPONSE_MAX: u64 = 16 << 20;
/// Le README d'un dépôt lu pour les suggestions, au plus (en caractères).
const README_MAX: usize = 30_000;

/// Les en-têtes d'une source (sa clé, s'il y en a une).
pub type EntetesDe<'a> = &'a (dyn Fn(&str) -> Vec<(String, String)> + Send + Sync);

// --- réseau ------------------------------------------------------------------------------------------------------

fn lire(reseau: &dyn Reseau, adresse: &str, entetes: &[(String, String)]) -> Result<(u16, String), String> {
    let (statut, corps) = reseau
        .ouvrir(adresse, entetes, 0)
        .map_err(|e| format!("Réseau indisponible : {e}"))?;
    let mut texte = String::new();
    corps
        .take(REPONSE_MAX)
        .read_to_string(&mut texte)
        .map_err(|e| format!("Réponse illisible : {e}"))?;
    Ok((statut, texte))
}

fn json_de(quoi: &str, statut: u16, texte: &str) -> Result<Value, String> {
    if statut != 200 {
        return Err(format!("{quoi} a répondu {statut}"));
    }
    serde_json::from_str(texte).map_err(|e| format!("{quoi} : réponse illisible ({e})"))
}

/// Un texte pour une adresse : tout ce qui n'est pas une lettre, un chiffre ou `-_.~` est encodé.
fn encoder(texte: &str) -> String {
    texte
        .bytes()
        .map(|o| {
            if o.is_ascii_alphanumeric() || b"-_.~".contains(&o) {
                char::from(o).to_string()
            } else {
                format!("%{o:02X}")
            }
        })
        .collect()
}

/// Le texte d'un morceau d'adresse : les `%XX` décodés.
fn decoder(texte: &str) -> String {
    let octets = texte.as_bytes();
    let mut sortie = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        let code = octets
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (octets[i], code) {
            (b'%', Some(o)) => {
                sortie.push(o);
                i += 3;
            }
            (o, _) => {
                sortie.push(o);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&sortie).into_owned()
}

/// Un chemin pour une adresse : chaque morceau encodé, les `/` gardés.
fn encoder_chemin(chemin: &str) -> String {
    chemin.split('/').map(encoder).collect::<Vec<_>>().join("/")
}

fn adresse(base: &str, params: &[(&str, String)]) -> String {
    let requete: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", encoder(v)))
        .collect();
    format!("{base}?{}", requete.join("&"))
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    v.get(cle).and_then(Value::as_str).map(str::to_owned)
}

fn nombre(v: &Value, cle: &str) -> u64 {
    v.get(cle).and_then(Value::as_u64).unwrap_or(0)
}

fn liste(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect()
}

fn extension(nom: &str) -> String {
    nom.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

/// La taille d'un fichier Civitai (`sizeKB`, décimal).
fn octets_kb(f: &Value) -> u64 {
    let kb = f.get("sizeKB").and_then(Value::as_f64).unwrap_or(0.0).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positif, bien en deçà de 2^53
    let octets = (kb * 1024.0).round() as u64;
    octets
}

// --- familles ----------------------------------------------------------------------------------------------------

/// Une famille d'Aria, vue de Rechercher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Famille {
    pub famille: String,
    pub nature: String,
    /// Ses bases sur Civitai.
    pub bases: Vec<String>,
    /// Un mot exigé dans le nom d'un modèle Civitai (« edit » : la retouche).
    #[serde(skip)]
    pub nom_contient: Option<String>,
    /// Ses dépôts d'origine sur Hugging Face.
    #[serde(skip)]
    pub hf: Vec<String>,
}

/// Les familles d'Aria d'après les fiches des workflows livrés.
pub fn infos_familles() -> BTreeMap<String, Famille> {
    let natures = modeles::familles();
    let mut sortie: BTreeMap<String, Famille> = BTreeMap::new();
    for wf in workflows::catalogue().0 {
        let nom = wf.famille().to_owned();
        let f = sortie.entry(nom.clone()).or_insert_with(|| Famille {
            nature: natures.get(&nom).copied().unwrap_or("image").to_owned(),
            famille: nom,
            bases: Vec::new(),
            nom_contient: None,
            hf: Vec::new(),
        });
        for b in liste(wf.fiche.pointer("/civitai/bases")) {
            if !f.bases.contains(&b) {
                f.bases.push(b);
            }
        }
        if let Some(n) = wf.fiche.pointer("/civitai/nom_contient").and_then(Value::as_str) {
            f.nom_contient = Some(n.to_owned());
        }
        for b in liste(wf.fiche.pointer("/hugging_face/bases")) {
            if !f.hf.contains(&b) {
                f.hf.push(b);
            }
        }
    }
    sortie
}

fn verifier_type(type_: &str) -> Result<(), String> {
    if TYPES.iter().any(|(t, _)| *t == type_) {
        Ok(())
    } else {
        Err(format!("Type inconnu : {type_}"))
    }
}

/// Les familles proposées pour un type : celles de sa nature, toutes pour les LoRA, aucune pour les LLM.
///
/// # Errors
/// Type inconnu.
pub fn familles(type_: &str) -> Result<Vec<Famille>, String> {
    verifier_type(type_)?;
    if type_ == "llm" {
        return Ok(Vec::new());
    }
    Ok(infos_familles()
        .into_values()
        .filter(|f| type_ == "lora" || f.nature == type_)
        .collect())
}

/// La famille d'Aria où va un modèle Civitai de cette base ; une famille qui exige un mot dans le nom passe avant
/// celle qui n'en exige pas (« Qwen » + « edit » : la retouche).
pub fn famille_civitai(
    base: Option<&str>,
    nom_modele: &str,
    infos: &BTreeMap<String, Famille>,
) -> Option<String> {
    let base = base?;
    let mut candidates: Vec<&Famille> = infos
        .values()
        .filter(|f| f.bases.iter().any(|b| b == base))
        .collect();
    candidates.sort_by_key(|f| f.nom_contient.is_none());
    candidates
        .into_iter()
        .find(|f| {
            f.nom_contient
                .as_ref()
                .is_none_or(|n| nom_modele.to_lowercase().contains(&n.to_lowercase()))
        })
        .map(|f| f.famille.clone())
}

/// Le dossier d'un modèle Civitai : `<nature>/<Famille>/loras` ou `…/modeles` d'après sa base ; VAE, ControlNet et
/// agrandisseurs vont côté image.
fn dossier_civitai(
    type_civitai: Option<&str>,
    base: Option<&str>,
    nom_modele: &str,
    infos: &BTreeMap<String, Famille>,
) -> Option<String> {
    match type_civitai {
        Some("VAE") => return Some(modeles::dossier_base("image", "vae")),
        Some("Controlnet") => return Some(modeles::dossier_base("image", "controlnet")),
        Some("Upscaler") => return Some(modeles::dossier_base("image", "upscale_models")),
        _ => {}
    }
    let famille = famille_civitai(base, nom_modele, infos)?;
    let sous = match type_civitai {
        Some(t) if TYPES_LORA.contains(&t) => "loras",
        Some("Checkpoint") => "modeles",
        _ => return None,
    };
    modeles::dossier_famille(&famille, sous).ok()
}

/// La famille d'un dossier `<nature>/<Famille>/…`.
fn famille_du_dossier(dossier: &str) -> Option<String> {
    let mut morceaux = dossier.split('/');
    morceaux.next()?;
    morceaux.next().filter(|f| *f != BASE).map(str::to_owned)
}

// --- ce qui est installé -----------------------------------------------------------------------------------------

/// Ce qui est déjà installé : les empreintes connues et les noms de fichiers.
#[derive(Debug, Clone, Default)]
pub struct Installes {
    pub empreintes: HashSet<String>,
    pub noms: HashSet<String>,
}

/// Les empreintes (registre et cache de la v2) des fichiers présents, et les noms de tous les fichiers de modèles.
pub fn installes(contexte: &Contexte) -> Installes {
    let present = |cle: &str| contexte.modeles.join(cle).is_file();
    let mut empreintes: HashSet<String> = registre(&contexte.data)
        .iter()
        .filter(|(cle, _)| present(cle))
        .filter_map(|(_, i)| i.get("sha256")?.as_str().map(str::to_lowercase))
        .collect();
    let cache = fs::read_to_string(contexte.dossier.cache().join("empreintes.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    if let Some(Value::Object(cache)) = cache {
        empreintes.extend(
            cache
                .iter()
                .filter(|(cle, _)| present(cle))
                .filter_map(|(_, e)| e.get("sha256")?.as_str().map(str::to_lowercase)),
        );
    }
    let noms = modeles::dossiers()
        .iter()
        .flat_map(|d| modeles::fichiers(&contexte.modeles, d, &EXTENSIONS))
        .collect();
    Installes { empreintes, noms }
}

// --- recherche ---------------------------------------------------------------------------------------------------

/// Ce que demande l'interface.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Demande {
    /// `civitai` ou `huggingface`.
    pub source: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub texte: String,
    #[serde(default)]
    pub famille: Option<String>,
    /// `telechargements`, `recents`, `notes` ou `tendances` (Hugging Face).
    #[serde(default)]
    pub tri: String,
    #[serde(default)]
    pub curseur: Option<String>,
    /// Inclure les modèles adultes, que Civitai cache presque tous sans le demander.
    #[serde(default)]
    pub adulte: bool,
}

/// L'image d'exemple d'un modèle Civitai.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Apercu {
    pub url: String,
    pub nsfw: bool,
}

/// Un fichier proposé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FichierTrouve {
    pub nom: String,
    pub taille: u64,
    pub principal: bool,
    pub installe: bool,
    /// Hugging Face : le lien du fichier, son dossier deviné, et pour un GGUF s'il tient dans la VRAM.
    pub lien: Option<String>,
    pub dossier: Option<String>,
    pub vram: Option<String>,
}

/// Une version d'un modèle Civitai.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: u64,
    pub nom: Option<String>,
    pub base: Option<String>,
    pub famille: Option<String>,
    pub mots_declencheurs: Vec<String>,
    pub fichiers: Vec<FichierTrouve>,
    pub lien: String,
    pub dossier: String,
    pub apercu: Option<Apercu>,
}

/// Un modèle trouvé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resultat {
    pub source: String,
    pub id: String,
    pub nom: String,
    pub auteur: Option<String>,
    #[serde(rename = "type")]
    pub type_: String,
    pub famille: Option<String>,
    pub installe: bool,
    pub telechargements: u64,
    pub appreciations: u64,
    pub date: Option<String>,
    pub page: String,
    /// Hugging Face : accès soumis à des conditions.
    pub protege: bool,
    pub apercu: Option<Apercu>,
    pub versions: Vec<Version>,
}

/// Une page de résultats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recherche {
    pub resultats: Vec<Resultat>,
    pub curseur: Option<String>,
}

/// La page d'un modèle Civitai, sur civitai.red s'il a du contenu au-delà de PG-13.
pub fn page_civitai(id_modele: u64, id_version: Option<u64>, niveau: u64) -> String {
    let site = if niveau & NIVEAUX_ADULTES == 0 {
        "https://civitai.com"
    } else {
        "https://civitai.red"
    };
    let version = id_version
        .map(|v| format!("?modelVersionId={v}"))
        .unwrap_or_default();
    format!("{site}/models/{id_modele}{version}")
}

fn apercu(version: &Value) -> Option<Apercu> {
    let image = version
        .get("images")?
        .as_array()?
        .iter()
        .find(|i| i.get("type").and_then(Value::as_str).unwrap_or("image") == "image")?;
    // Le CDN de Civitai sert l'original en pleine résolution : on demande une vignette.
    let url = image
        .get("url")?
        .as_str()?
        .replace("/original=true/", "/width=450/");
    Some(Apercu {
        url,
        nsfw: nombre(image, "nsfwLevel").max(1) >= 4,
    })
}

/// Cherche sur Civitai.
///
/// # Errors
/// Type ou famille inconnus, LLM (cherchés sur Hugging Face), ou Civitai qui ne répond pas.
pub fn chercher_civitai(
    reseau: &dyn Reseau,
    entetes: EntetesDe<'_>,
    installes: &Installes,
    d: &Demande,
) -> Result<Recherche, String> {
    verifier_type(&d.type_)?;
    if d.type_ == "llm" {
        return Err("Les LLM se cherchent sur Hugging Face".to_owned());
    }
    let infos = infos_familles();
    let bases: Vec<String> = match &d.famille {
        Some(f) => infos
            .get(f)
            .ok_or_else(|| format!("Famille inconnue : {f}"))?
            .bases
            .clone(),
        None => {
            let mut b: Vec<String> = infos
                .values()
                .filter(|f| d.type_ == "lora" || f.nature == d.type_)
                .flat_map(|f| f.bases.clone())
                .collect();
            b.sort();
            b.dedup();
            b
        }
    };
    if bases.is_empty() {
        return Ok(Recherche {
            resultats: Vec::new(),
            curseur: None,
        });
    }
    let mut params: Vec<(&str, String)> = if d.type_ == "lora" {
        TYPES_LORA.iter().map(|t| ("types", (*t).to_owned())).collect()
    } else {
        vec![("types", "Checkpoint".to_owned())]
    };
    let tri = match d.tri.as_str() {
        "recents" => "Newest",
        "notes" => "Highest Rated",
        _ => "Most Downloaded",
    };
    params.push(("sort", tri.to_owned()));
    params.push(("limit", "20".to_owned()));
    params.extend(bases.iter().map(|b| ("baseModels", b.clone())));
    if !d.texte.trim().is_empty() {
        params.push(("query", d.texte.trim().to_owned()));
    }
    if let Some(c) = d.curseur.as_ref().filter(|c| !c.is_empty()) {
        params.push(("cursor", c.clone()));
    }
    if d.adulte {
        params.push(("nsfw", "true".to_owned()));
    }
    let (statut, corps) = lire(
        reseau,
        &adresse(&format!("{CIVITAI}/models"), &params),
        &entetes("civitai"),
    )?;
    let donnees = json_de("Civitai", statut, &corps)?;
    let mut resultats = Vec::new();
    for m in donnees
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let nom_modele = texte(m, "name").unwrap_or_default();
        let type_civitai = m.get("type").and_then(Value::as_str);
        let mut versions = Vec::new();
        for v in m
            .get("modelVersions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let base = texte(v, "baseModel");
            let Some(dossier) = dossier_civitai(type_civitai, base.as_deref(), &nom_modele, &infos) else {
                continue; // base sans famille dans Aria : pas utilisable ici
            };
            let famille = famille_du_dossier(&dossier);
            if d.famille.is_some() && famille != d.famille {
                continue;
            }
            let fichiers: Vec<FichierTrouve> = v
                .get("files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|f| EXTENSIONS.contains(&extension(&texte(f, "name").unwrap_or_default()).as_str()))
                .map(|f| {
                    let sha = f
                        .pointer("/hashes/SHA256")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_lowercase();
                    FichierTrouve {
                        nom: texte(f, "name").unwrap_or_default(),
                        taille: octets_kb(f),
                        principal: f.get("primary").and_then(Value::as_bool).unwrap_or(false),
                        installe: !sha.is_empty() && installes.empreintes.contains(&sha),
                        lien: None,
                        dossier: None,
                        vram: None,
                    }
                })
                .collect();
            if fichiers.is_empty() {
                continue;
            }
            let id = nombre(v, "id");
            versions.push(Version {
                id,
                nom: texte(v, "name"),
                famille,
                mots_declencheurs: liste(v.get("trainedWords")),
                fichiers,
                lien: format!("https://civitai.com/api/download/models/{id}"),
                dossier,
                apercu: apercu(v),
                base,
            });
        }
        let Some(premiere) = versions.first() else {
            continue; // rien de téléchargeable sans risque, ou rien pour une famille d'Aria
        };
        let id = nombre(m, "id");
        resultats.push(Resultat {
            source: "civitai".to_owned(),
            id: id.to_string(),
            nom: nom_modele,
            auteur: m
                .pointer("/creator/username")
                .and_then(Value::as_str)
                .map(str::to_owned),
            type_: d.type_.clone(),
            famille: premiere.famille.clone(),
            // Le fichier principal seulement : une page Civitai liste aussi l'encodeur ou le VAE qu'on a déjà.
            installe: versions.iter().any(|v| {
                v.fichiers
                    .iter()
                    .any(|f| f.installe && (f.principal || v.fichiers.len() == 1))
            }),
            telechargements: m
                .pointer("/stats/downloadCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            appreciations: m
                .pointer("/stats/thumbsUpCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            date: None,
            page: page_civitai(id, None, nombre(m, "nsfwLevel")),
            protege: false,
            apercu: premiere.apercu.clone(),
            versions,
        });
    }
    let curseur = donnees.pointer("/metadata/nextCursor").and_then(|c| match c {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    });
    Ok(Recherche { resultats, curseur })
}

/// (filtre, famille) de chaque requête Hugging Face : les dérivés des dépôts d'origine des familles d'Aria (tag
/// `base_model`), ceux de la famille choisie seulement ; les LoRA par leur tag `base_model:adapter`.
fn requetes_hf(type_: &str, famille: Option<&str>) -> Result<Vec<(String, Option<String>)>, String> {
    if type_ == "llm" {
        return Ok(vec![("gguf".to_owned(), None)]);
    }
    let infos = infos_familles();
    if let Some(f) = famille
        && !infos.contains_key(f)
    {
        return Err(format!("Famille inconnue : {f}"));
    }
    let prefixe = if type_ == "lora" {
        "base_model:adapter:"
    } else {
        "base_model:"
    };
    Ok(infos
        .values()
        .filter(|f| famille.map_or(type_ == "lora" || f.nature == type_, |c| f.famille == c))
        .flat_map(|f| {
            f.hf.iter()
                .map(|depot| (format!("{prefixe}{depot}"), Some(f.famille.clone())))
        })
        .collect())
}

/// Un modèle de diffusion en safetensors qui n'est pas une LoRA (et l'inverse pour les LoRA).
fn utilisable_hf(m: &Value, type_: &str) -> bool {
    if type_ == "llm" {
        return true;
    }
    let tags = liste(m.get("tags"));
    let lora = tags
        .iter()
        .any(|t| t == "lora" || t.starts_with("base_model:adapter:"));
    if type_ == "lora" {
        lora
    } else {
        !lora && !tags.iter().any(|t| t == "gguf")
    }
}

/// Cherche sur Hugging Face.
///
/// # Errors
/// Type ou famille inconnus, ou Hugging Face qui ne répond pas.
pub fn chercher_hf(reseau: &dyn Reseau, entetes: EntetesDe<'_>, d: &Demande) -> Result<Recherche, String> {
    verifier_type(&d.type_)?;
    let tri = match d.tri.as_str() {
        "recents" => "lastModified",
        "notes" => "likes",
        "tendances" => "trendingScore",
        _ => "downloads",
    };
    let mut vus: Vec<(Value, Option<String>)> = Vec::new();
    for (filtre, famille) in requetes_hf(&d.type_, d.famille.as_deref())? {
        let mut params = vec![
            ("sort", tri.to_owned()),
            ("direction", "-1".to_owned()),
            ("limit", "30".to_owned()),
        ];
        if !d.texte.trim().is_empty() {
            params.push(("search", d.texte.trim().to_owned()));
        }
        params.push(("filter", filtre));
        let (statut, corps) = lire(
            reseau,
            &adresse(&format!("{HF}/api/models"), &params),
            &entetes("huggingface"),
        )?;
        for m in json_de("Hugging Face", statut, &corps)?
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = texte(m, "id").unwrap_or_default();
            if !id.is_empty()
                && utilisable_hf(m, &d.type_)
                && !vus
                    .iter()
                    .any(|(v, _)| texte(v, "id").as_deref() == Some(id.as_str()))
            {
                vus.push((m.clone(), famille.clone()));
            }
        }
    }
    if let Some(cle) = match d.tri.as_str() {
        "notes" => Some("likes"),
        "recents" | "tendances" => None,
        _ => Some("downloads"),
    } {
        vus.sort_by_key(|(m, _)| std::cmp::Reverse(nombre(m, cle)));
    }
    let resultats = vus
        .into_iter()
        .map(|(m, famille)| {
            let id = texte(&m, "id").unwrap_or_default();
            Resultat {
                source: "huggingface".to_owned(),
                auteur: id.split('/').next().map(str::to_owned),
                nom: id.clone(),
                page: format!("{HF}/{id}"),
                type_: d.type_.clone(),
                famille,
                installe: false,
                telechargements: nombre(&m, "downloads"),
                appreciations: nombre(&m, "likes"),
                date: texte(&m, "lastModified"),
                protege: m
                    .get("gated")
                    .is_some_and(|g| g.as_bool() != Some(false) && !g.is_null()),
                apercu: None,
                versions: Vec::new(),
                id,
            }
        })
        .collect();
    Ok(Recherche {
        resultats,
        curseur: None,
    })
}

/// Cherche sur la source demandée.
///
/// # Errors
/// Source inconnue, ou l'erreur de la source.
pub fn chercher(
    reseau: &dyn Reseau,
    entetes: EntetesDe<'_>,
    installes: &Installes,
    d: &Demande,
) -> Result<Recherche, String> {
    match d.source.as_str() {
        "civitai" => chercher_civitai(reseau, entetes, installes, d),
        "huggingface" => chercher_hf(reseau, entetes, d),
        autre => Err(format!("Source inconnue : {autre}")),
    }
}

// --- fichiers d'un dépôt Hugging Face ----------------------------------------------------------------------------

/// Un dépôt Hugging Face : `<propriétaire>/<nom>`.
fn depot_valide(depot: &str) -> bool {
    let morceaux: Vec<&str> = depot.split('/').collect();
    morceaux.len() == 2
        && morceaux.iter().all(|m| {
            !m.is_empty()
                && !m.chars().all(|c| c == '.')
                && m.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        })
}

/// Le dossier d'Aria d'un fichier d'un dépôt : son sous-dossier dans le dépôt d'abord (`vae/`, `loras/`… des dépôts
/// « repackaged » pour ComfyUI), sinon le type cherché et la famille ; rien : à choisir.
pub fn dossier_hf(chemin: &str, type_: &str, famille: Option<&str>) -> Option<String> {
    if type_ == "llm" {
        return Some(DOSSIER_LLM.to_owned());
    }
    let natures = modeles::familles();
    // Les fichiers de base vont du côté de la famille ; sans famille, de celui du type cherché (image par défaut).
    let nature = famille
        .and_then(|f| natures.get(f).copied())
        .unwrap_or(if type_ == "video" { "video" } else { "image" });
    let famille_dossier = |sous: &str| famille.and_then(|f| modeles::dossier_famille(f, sous).ok());
    let parties: Vec<String> = chemin.split('/').map(str::to_lowercase).collect();
    for p in parties.iter().rev().skip(1) {
        match p.as_str() {
            "loras" | "lora" => return famille_dossier("loras"),
            "diffusion_models" | "checkpoints" | "unet" | "transformer" => return famille_dossier("modeles"),
            t if TYPES_BASE.contains(&t) => return Some(modeles::dossier_base(nature, t)),
            _ => {}
        }
    }
    famille_dossier(if type_ == "lora" { "loras" } else { "modeles" })
}

/// Un GGUF qui n'est pas un modèle autonome : projecteur d'images, tête MTP, morceau d'un GGUF découpé.
fn gguf_annexe(chemin: &str) -> bool {
    let bas = chemin.to_lowercase();
    let premier = bas.split('/').next().unwrap_or_default();
    let nom = bas.rsplit('/').next().unwrap_or_default();
    let morceau = nom
        .strip_suffix(".gguf")
        .and_then(|s| s.rsplit_once("-of-"))
        .is_some_and(|(avant, total)| {
            let cinq = |n: &str| n.len() == 5 && n.chars().all(|c| c.is_ascii_digit());
            cinq(total) && avant.rsplit_once('-').is_some_and(|(_, n)| cinq(n))
        });
    bas.contains("mmproj") || morceau || premier.contains("mtp") || nom.starts_with("mtp")
}

/// Un LLM tient-il dans la VRAM libre ?
fn verdict_vram(taille: u64, libre: u64) -> &'static str {
    let besoin = taille + SURCOUT_LLM;
    if besoin.saturating_mul(10) <= libre.saturating_mul(9) {
        "tient"
    } else if besoin <= libre {
        "juste"
    } else {
        "deborde"
    }
}

/// Les fichiers d'un dépôt Hugging Face.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FichiersDepot {
    pub fichiers: Vec<FichierTrouve>,
    pub vram_libre_go: Option<f64>,
    pub vram_totale_go: Option<f64>,
}

#[allow(clippy::cast_precision_loss)] // des gigaoctets affichés à 0,1 près
fn en_go(octets: u64) -> f64 {
    (octets as f64 / GO as f64 * 10.0).round() / 10.0
}

/// Les fichiers téléchargeables d'un dépôt ; pour les GGUF, s'ils tiennent dans la VRAM libre (`vram` : libre et
/// totale, en octets, si la carte se lit).
///
/// # Errors
/// Dépôt invalide ou inaccessible.
pub fn fichiers_hf(
    reseau: &dyn Reseau,
    entetes: EntetesDe<'_>,
    installes: &Installes,
    depot: &str,
    type_: &str,
    famille: Option<&str>,
    vram: Option<(u64, u64)>,
) -> Result<FichiersDepot, String> {
    verifier_type(type_)?;
    if !depot_valide(depot) {
        return Err(format!("Dépôt invalide : {depot}"));
    }
    let adresse = format!("{HF}/api/models/{depot}/tree/main?recursive=true");
    let (statut, corps) = lire(reseau, &adresse, &entetes("huggingface"))?;
    if statut != 200 {
        return Err(format!("Dépôt inaccessible ({statut}) : {depot}"));
    }
    let arbre = json_de("Hugging Face", statut, &corps)?;
    let voulue = if type_ == "llm" { "gguf" } else { "safetensors" };
    let mut fichiers: Vec<FichierTrouve> = arbre
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f.get("type").and_then(Value::as_str) == Some("file"))
        .filter_map(|f| {
            let chemin = texte(f, "path")?;
            if extension(&chemin) != voulue || (voulue == "gguf" && gguf_annexe(&chemin)) {
                return None;
            }
            let taille = f
                .get("size")
                .and_then(Value::as_u64)
                .or_else(|| f.pointer("/lfs/size").and_then(Value::as_u64))
                .unwrap_or(0);
            let nom = chemin.rsplit('/').next().unwrap_or(&chemin).to_owned();
            Some(FichierTrouve {
                installe: installes.noms.contains(&nom),
                lien: Some(format!("{HF}/{depot}/blob/main/{chemin}")),
                dossier: dossier_hf(&chemin, type_, famille),
                vram: (voulue == "gguf")
                    .then_some(vram)
                    .flatten()
                    .map(|(libre, _)| verdict_vram(taille, libre).to_owned()),
                principal: false,
                nom: chemin,
                taille,
            })
        })
        .collect();
    fichiers.sort_by_key(|f| f.taille);
    Ok(FichiersDepot {
        fichiers,
        vram_libre_go: vram.map(|(libre, _)| en_go(libre)),
        vram_totale_go: vram.map(|(_, totale)| en_go(totale)),
    })
}

// --- fiche d'après un lien ---------------------------------------------------------------------------------------

/// Ce qu'un lien donne, avant la mise en file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposition {
    pub fiche: Fiche,
    /// Le dossier deviné, à confirmer ; rien : à choisir.
    pub dossier: Option<String>,
    /// La source demande une clé que Réglages n'a pas.
    pub jeton_requis: bool,
}

/// (hôte, chemin, requête) d'une adresse HTTPS.
fn decouper_lien(lien: &str) -> Option<(String, String, String)> {
    let reste = lien.trim().strip_prefix("https://")?;
    let (hote, chemin) = reste.split_once('/').unwrap_or((reste, ""));
    let (chemin, requete) = chemin.split_once('?').unwrap_or((chemin, ""));
    let chemin = chemin.split('#').next().unwrap_or_default();
    let requete = requete.split('#').next().unwrap_or_default();
    Some((hote.to_lowercase(), chemin.to_owned(), requete.to_owned()))
}

fn de_l_hote(hote: &str, domaine: &str) -> bool {
    hote == domaine || hote.ends_with(&format!(".{domaine}"))
}

fn parametre<'a>(requete: &'a str, nom: &str) -> Option<&'a str> {
    requete
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| *k == nom)
        .map(|(_, v)| v)
}

fn chiffres(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// La fiche du fichier d'un lien Hugging Face (`…/<propriétaire>/<dépôt>/(blob|resolve)/<branche>/<fichier>`) ou
/// Civitai (`…/models/<id>[?modelVersionId=<v>]`, `…/api/download/models/<v>`).
///
/// # Errors
/// Lien d'une autre source, format refusé, ou source qui ne connaît pas le fichier.
pub fn fiche_depuis_lien(
    reseau: &dyn Reseau,
    entetes: EntetesDe<'_>,
    lien: &str,
) -> Result<Proposition, String> {
    let non_reconnu =
        || "Lien non reconnu : seuls Hugging Face et Civitai (https://) sont pris en charge".to_owned();
    let (hote, chemin, requete) = decouper_lien(lien).ok_or_else(non_reconnu)?;
    let proposition = if de_l_hote(&hote, "huggingface.co") {
        fiche_hf(reseau, entetes, &chemin)?
    } else if de_l_hote(&hote, "civitai.com") || de_l_hote(&hote, "civitai.red") {
        fiche_civitai(reseau, entetes, &chemin, &requete)?
    } else {
        return Err(non_reconnu());
    };
    if !EXTENSIONS.contains(&extension(&proposition.fiche.nom).as_str()) {
        return Err(format!(
            "Format refusé ({}) : seuls .safetensors et .gguf sont acceptés, les autres formats peuvent exécuter du \
             code au chargement",
            proposition.fiche.nom
        ));
    }
    Ok(proposition)
}

/// Ce que l'auteur fait de sa LoRA dans ses images d'exemple : Civitai n'a pas de champ « force conseillée », mais
/// leurs métadonnées donnent la force utilisée (la médiane, au vingtième) et un prompt. Rend (force, images, prompt).
pub fn usage_auteur(version: &Value, nom_fichier: Option<&str>) -> (Option<f64>, u32, Option<String>) {
    let souche = |nom: &str| {
        let nom = nom.rsplit('/').next().unwrap_or(nom).to_lowercase();
        nom.rsplit_once('.').map_or(nom.clone(), |(s, _)| s.to_owned())
    };
    let voulue = nom_fichier.map(souche).unwrap_or_default();
    let mut forces: Vec<f64> = Vec::new();
    let mut prompt = None;
    for image in version
        .get("images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(meta) = image.get("meta").filter(|m| m.is_object()) else {
            continue;
        };
        if prompt.is_none()
            && let Some(p) = meta
                .get("prompt")
                .and_then(Value::as_str)
                .filter(|p| !p.trim().is_empty())
        {
            let p: String = p.split_whitespace().collect::<Vec<_>>().join(" ");
            prompt = Some(p.chars().take(500).collect());
        }
        let mut loras: Vec<&Value> = meta
            .get("additionalResources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|r| {
                r.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| ["lora", "locon", "dora"].contains(&t.to_lowercase().as_str()))
            })
            .collect();
        if loras.len() > 1 {
            // Plusieurs LoRA sur l'image : celle qui porte le nom du fichier.
            loras.retain(|r| {
                !voulue.is_empty()
                    && r.get("name").and_then(Value::as_str).map(souche) == Some(voulue.clone())
            });
        }
        if let Some(r) = loras.first() {
            let force = r
                .get("strength")
                .or_else(|| r.get("weight"))
                .and_then(|f| match f {
                    Value::Number(n) => n.as_f64(),
                    Value::String(s) => s.trim().parse().ok(),
                    _ => None,
                });
            if let Some(f) = force.filter(|f| *f > 0.0 && *f <= 2.0) {
                forces.push(f);
            }
        }
    }
    forces.sort_by(f64::total_cmp);
    let n = forces.len();
    let mediane = match n {
        0 => None,
        _ if n % 2 == 1 => forces.get(n / 2).copied(),
        _ => Some((forces[n / 2 - 1] + forces[n / 2]) / 2.0),
    };
    let images = u32::try_from(n).unwrap_or(u32::MAX);
    (mediane.map(|m| (m * 20.0).round() / 20.0), images, prompt)
}

fn fiche_civitai(
    reseau: &dyn Reseau,
    entetes: EntetesDe<'_>,
    chemin: &str,
    requete: &str,
) -> Result<Proposition, String> {
    let morceaux: Vec<&str> = chemin.split('/').filter(|m| !m.is_empty()).collect();
    let attendu = || "Lien Civitai attendu : https://civitai.com/models/<id>".to_owned();
    let (id_modele, id_version) = match morceaux.as_slice() {
        ["api", "download", "models", v, ..] if chiffres(v) => (None, Some((*v).to_owned())),
        ["models", m, ..] if chiffres(m) => (
            Some((*m).to_owned()),
            parametre(requete, "modelVersionId")
                .filter(|v| chiffres(v))
                .map(str::to_owned),
        ),
        _ => return Err(attendu()),
    };
    let cles = entetes("civitai");
    let id_version = match (id_version, id_modele) {
        (Some(v), _) => v,
        (None, Some(m)) => {
            let (statut, corps) = lire(reseau, &format!("{CIVITAI}/models/{m}"), &cles)?;
            if statut == 404 {
                return Err(format!("Modèle Civitai introuvable : {m}"));
            }
            let modele = json_de("Civitai", statut, &corps)?;
            // La plus récente.
            modele
                .pointer("/modelVersions/0/id")
                .and_then(Value::as_u64)
                .ok_or_else(|| "Ce modèle n'a aucune version téléchargeable".to_owned())?
                .to_string()
        }
        (None, None) => return Err(attendu()),
    };
    let (statut, corps) = lire(reseau, &format!("{CIVITAI}/model-versions/{id_version}"), &cles)?;
    if statut == 404 {
        return Err(format!("Version Civitai introuvable : {id_version}"));
    }
    let version = json_de("Civitai", statut, &corps)?;
    let fichiers: Vec<&Value> = version
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    let f = fichiers
        .iter()
        .find(|f| f.get("primary").and_then(Value::as_bool) == Some(true))
        .or_else(|| fichiers.first())
        .ok_or_else(|| "Cette version ne contient aucun fichier".to_owned())?;
    let nom = texte(f, "name").unwrap_or_default();
    let type_civitai = version.pointer("/model/type").and_then(Value::as_str);
    let base = texte(&version, "baseModel");
    let lora = type_civitai.is_some_and(|t| TYPES_LORA.contains(&t));
    let (force_auteur, images_force, prompt_exemple) = if lora {
        usage_auteur(&version, Some(&nom))
    } else {
        (None, 0, None)
    };
    let nom_version = texte(&version, "name").unwrap_or_default();
    let description = texte(&version, "description").unwrap_or_default();
    let textes = [
        ("nom du fichier", nom.as_str()),
        ("nom de la version", nom_version.as_str()),
        ("description de la version", description.as_str()),
    ];
    let nom_modele = version
        .pointer("/model/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let protege = version.get("availability").and_then(Value::as_str) == Some("EarlyAccess");
    Ok(Proposition {
        dossier: dossier_civitai(type_civitai, base.as_deref(), nom_modele, &infos_familles()),
        jeton_requis: protege && cles.is_empty(),
        fiche: Fiche {
            source: "civitai".to_owned(),
            taille: octets_kb(f),
            sha256: f
                .pointer("/hashes/SHA256")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase),
            url: texte(f, "downloadUrl")
                .filter(|u| u.starts_with("https://"))
                .unwrap_or_else(|| format!("https://civitai.com/api/download/models/{id_version}")),
            page: page_civitai(
                nombre(&version, "modelId"),
                Some(nombre(&version, "id")),
                nombre(&version, "nsfwLevel"),
            ),
            // Civitai n'expose pas de licence nommée : voir la page du modèle.
            licence: None,
            mots_declencheurs: liste(version.get("trainedWords"))
                .into_iter()
                .map(|m| m.trim().to_owned())
                .filter(|m| !m.is_empty())
                .collect(),
            base,
            force_auteur,
            images_force,
            prompt_exemple,
            steps_suggeres: (!lora).then(|| suggestions::steps(&textes)),
            cfg_suggere: (!lora).then(|| suggestions::cfg(&textes)),
            nom,
        },
    })
}

/// Le dossier deviné d'un fichier d'un lien Hugging Face : `llm` pour un GGUF, le type de base de son sous-dossier
/// côté image ; ce qui dépend d'une famille est à choisir.
fn deviner_dossier_hf(chemin: &str) -> Option<String> {
    if extension(chemin) == "gguf" {
        return Some(DOSSIER_LLM.to_owned());
    }
    chemin.split('/').rev().skip(1).find_map(|partie| {
        let partie = partie.to_lowercase();
        let partie = match partie.as_str() {
            "unet" => "diffusion_models",
            "clip" => "text_encoders",
            "lora" => "loras",
            autre => autre,
        };
        (TYPES_BASE.contains(&partie) && !["diffusion_models", "loras", "checkpoints"].contains(&partie))
            .then(|| modeles::dossier_base("image", partie))
    })
}

fn fiche_hf(reseau: &dyn Reseau, entetes: EntetesDe<'_>, chemin: &str) -> Result<Proposition, String> {
    let morceaux: Vec<&str> = chemin.split('/').filter(|m| !m.is_empty()).collect();
    let [proprietaire, nom_depot, genre, revision, fichier @ ..] = morceaux.as_slice() else {
        return Err(attendu_hf());
    };
    if !["blob", "resolve"].contains(genre) || fichier.is_empty() {
        return Err(attendu_hf());
    }
    let depot = format!("{proprietaire}/{nom_depot}");
    if !depot_valide(&depot) {
        return Err(format!("Dépôt invalide : {depot}"));
    }
    let chemin_fichier = fichier.iter().map(|m| decoder(m)).collect::<Vec<_>>().join("/");
    let cles = entetes("huggingface");
    let (statut, corps) = lire(reseau, &format!("{HF}/api/models/{depot}"), &cles)?;
    if statut == 401 || statut == 404 {
        return Err(format!("Dépôt introuvable ou privé : {depot}"));
    }
    let infos = json_de("Hugging Face", statut, &corps)?;
    let (dossier_parent, _) = chemin_fichier
        .rsplit_once('/')
        .unwrap_or(("", chemin_fichier.as_str()));
    let revision = encoder(&decoder(revision));
    let arbre = if dossier_parent.is_empty() {
        format!("{HF}/api/models/{depot}/tree/{revision}")
    } else {
        format!(
            "{HF}/api/models/{depot}/tree/{revision}/{}",
            encoder_chemin(dossier_parent)
        )
    };
    let (statut, corps) = lire(reseau, &arbre, &cles)?;
    let entrees = json_de("Hugging Face", statut, &corps)?;
    let f = entrees
        .as_array()
        .into_iter()
        .flatten()
        .find(|f| {
            f.get("type").and_then(Value::as_str) == Some("file")
                && texte(f, "path").as_deref() == Some(chemin_fichier.as_str())
        })
        .ok_or_else(|| format!("Fichier introuvable dans {depot} : {chemin_fichier}"))?;
    let nom = chemin_fichier
        .rsplit('/')
        .next()
        .unwrap_or(&chemin_fichier)
        .to_owned();
    // La fiche du dépôt (README) : texte libre, au mieux.
    let readme = lire(reseau, &format!("{HF}/{depot}/raw/{revision}/README.md"), &cles)
        .ok()
        .filter(|(s, _)| *s == 200)
        .map(|(_, t)| t.chars().take(README_MAX).collect::<String>())
        .unwrap_or_default();
    let mut textes = vec![("nom du fichier", nom.as_str())];
    if !readme.is_empty() {
        textes.push(("README du dépôt", readme.as_str()));
    }
    let licence = infos
        .pointer("/cardData/license")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            liste(infos.get("tags"))
                .into_iter()
                .find_map(|t| t.strip_prefix("license:").map(str::to_owned))
        });
    let protege = infos
        .get("gated")
        .is_some_and(|g| !g.is_null() && g.as_bool() != Some(false));
    Ok(Proposition {
        dossier: deviner_dossier_hf(&chemin_fichier),
        jeton_requis: protege && cles.is_empty(),
        fiche: Fiche {
            source: "huggingface".to_owned(),
            taille: f
                .get("size")
                .and_then(Value::as_u64)
                .or_else(|| f.pointer("/lfs/size").and_then(Value::as_u64))
                .unwrap_or(0),
            // Absente pour les petits fichiers hors LFS.
            sha256: f
                .pointer("/lfs/oid")
                .and_then(Value::as_str)
                .map(str::to_lowercase),
            url: format!(
                "{HF}/{depot}/resolve/{revision}/{}",
                encoder_chemin(&chemin_fichier)
            ),
            page: format!("{HF}/{depot}"),
            licence,
            steps_suggeres: Some(suggestions::steps(&textes)),
            cfg_suggere: Some(suggestions::cfg(&textes)),
            nom,
            ..Fiche::default()
        },
    })
}

fn attendu_hf() -> String {
    "Lien Hugging Face attendu vers un fichier : …/<propriétaire>/<dépôt>/blob/<branche>/<fichier>".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    use std::sync::Mutex;

    /// Un faux site : des réponses par début d'adresse, et les adresses demandées.
    #[derive(Default)]
    struct Faux {
        reponses: Vec<(String, u16, String)>,
        demandes: Mutex<Vec<String>>,
    }

    impl Faux {
        fn avec(mut self, debut: &str, statut: u16, corps: &Value) -> Self {
            self.reponses.push((debut.to_owned(), statut, corps.to_string()));
            self
        }

        fn demandes(&self) -> Vec<String> {
            self.demandes.lock().unwrap().clone()
        }
    }

    impl Reseau for Faux {
        fn ouvrir(&self, adresse: &str, _: &[(String, String)], _: u64) -> io::Result<(u16, Box<dyn Read>)> {
            self.demandes.lock().unwrap().push(adresse.to_owned());
            let (statut, corps) = self
                .reponses
                .iter()
                .find(|(debut, _, _)| adresse.starts_with(debut.as_str()))
                .map_or((404, String::new()), |(_, s, c)| (*s, c.clone()));
            Ok((statut, Box::new(Cursor::new(corps.into_bytes()))))
        }
    }

    fn sans_cle(_: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    fn demande(source: &str, type_: &str) -> Demande {
        Demande {
            source: source.to_owned(),
            type_: type_.to_owned(),
            ..Demande::default()
        }
    }

    #[test]
    fn familles_des_fiches() {
        let infos = infos_familles();
        let krea = &infos["Krea2"];
        assert_eq!(krea.nature, "image");
        assert!(krea.bases.contains(&"Krea 2".to_owned()));
        assert!(krea.hf.contains(&"krea/Krea-2-Turbo".to_owned()));
        assert_eq!(
            famille_civitai(Some("Krea 2"), "x", &infos).as_deref(),
            Some("Krea2")
        );
        assert!(famille_civitai(Some("SD 1.5"), "x", &infos).is_none());
        assert!(familles("llm").unwrap().is_empty());
        assert!(familles("lora").unwrap().len() >= familles("image").unwrap().len());
        assert!(familles("autre").is_err());
    }

    #[test]
    fn recherche_civitai() {
        let reponse = serde_json::json!({
            "items": [
                {"id": 7, "name": "Neon", "type": "LORA", "nsfwLevel": 1, "creator": {"username": "ana"},
                 "stats": {"downloadCount": 12, "thumbsUpCount": 3},
                 "modelVersions": [
                    {"id": 70, "name": "v1", "baseModel": "Krea 2", "trainedWords": ["neon"],
                     "images": [{"type": "image", "url": "https://image.civitai.com/a/original=true/1.jpeg", "nsfwLevel": 8}],
                     "files": [{"name": "neon.safetensors", "sizeKB": 2.0, "primary": true, "hashes": {"SHA256": "ABC"}},
                               {"name": "neon.pt", "sizeKB": 1.0}]},
                    {"id": 71, "baseModel": "SD 1.5", "files": [{"name": "x.safetensors", "sizeKB": 1.0}]}
                 ]},
                {"id": 8, "name": "Ancien", "type": "LORA", "modelVersions": [{"id": 80, "baseModel": "SD 1.5",
                 "files": [{"name": "y.safetensors", "sizeKB": 1.0}]}]}
            ],
            "metadata": {"nextCursor": "suite"}
        });
        let faux = Faux::default().avec(CIVITAI, 200, &reponse);
        let installes = Installes {
            empreintes: HashSet::from(["abc".to_owned()]),
            ..Installes::default()
        };
        let mut d = demande("civitai", "lora");
        d.texte = "néon".to_owned();
        d.adulte = true;
        let r = chercher(&faux, &sans_cle, &installes, &d).unwrap();
        assert_eq!(r.curseur.as_deref(), Some("suite"));
        assert_eq!(r.resultats.len(), 1);
        let neon = &r.resultats[0];
        assert!(neon.installe);
        assert_eq!(neon.famille.as_deref(), Some("Krea2"));
        assert_eq!(neon.versions.len(), 1);
        let v = &neon.versions[0];
        assert_eq!(v.fichiers.len(), 1, "le .pt n'est pas proposé");
        assert_eq!(v.fichiers[0].taille, 2048);
        assert!(v.dossier.ends_with("/Krea2/loras"));
        let apercu = v.apercu.as_ref().unwrap();
        assert!(apercu.nsfw && apercu.url.contains("/width=450/"));
        let url = &faux.demandes()[0];
        assert!(
            url.contains("types=LORA") && url.contains("baseModels=Krea%202") && url.contains("nsfw=true")
        );
        assert!(url.contains("query=n%C3%A9on"));
        assert!(chercher(&faux, &sans_cle, &installes, &demande("civitai", "llm")).is_err());
        let mut inconnue = demande("civitai", "image");
        inconnue.famille = Some("Inconnue".to_owned());
        assert!(chercher(&faux, &sans_cle, &installes, &inconnue).is_err());
    }

    #[test]
    fn recherche_hugging_face() {
        let reponse = serde_json::json!([
            {"id": "a/llm-gguf", "downloads": 5, "likes": 1, "gated": false},
            {"id": "b/autre-gguf", "downloads": 50, "likes": 0, "gated": "manual"}
        ]);
        let faux = Faux::default().avec(HF, 200, &reponse);
        let r = chercher(
            &faux,
            &sans_cle,
            &Installes::default(),
            &demande("huggingface", "llm"),
        )
        .unwrap();
        assert_eq!(
            r.resultats.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["b/autre-gguf", "a/llm-gguf"]
        );
        assert!(r.resultats[0].protege && !r.resultats[1].protege);
        assert!(faux.demandes()[0].contains("filter=gguf"));
        // Les LoRA d'une famille : son tag d'adaptateur, une requête par dépôt d'origine.
        let faux = Faux::default().avec(
            HF,
            200,
            &serde_json::json!([{"id": "c/style", "tags": ["lora"]}, {"id": "c/base"}]),
        );
        let mut d = demande("huggingface", "lora");
        d.famille = Some("Krea2".to_owned());
        let r = chercher(&faux, &sans_cle, &Installes::default(), &d).unwrap();
        assert_eq!(r.resultats.len(), 1);
        assert!(faux.demandes()[0].contains("filter=base_model%3Aadapter%3Akrea%2FKrea-2-"));
    }

    #[test]
    fn fichiers_d_un_depot() {
        let arbre = serde_json::json!([
            {"type": "file", "path": "q4.gguf", "size": 4 * GO},
            {"type": "file", "path": "mmproj-f16.gguf", "size": 1},
            {"type": "file", "path": "gros-00001-of-00002.gguf", "size": 1},
            {"type": "file", "path": "q8.gguf", "lfs": {"size": 9 * GO}},
            {"type": "file", "path": "README.md", "size": 1},
            {"type": "directory", "path": "x.gguf"}
        ]);
        let faux = Faux::default().avec(&format!("{HF}/api/models/org/depot/tree/main"), 200, &arbre);
        let installes = Installes {
            noms: HashSet::from(["q4.gguf".to_owned()]),
            ..Installes::default()
        };
        let r = fichiers_hf(
            &faux,
            &sans_cle,
            &installes,
            "org/depot",
            "llm",
            None,
            Some((8 * GO, 12 * GO)),
        )
        .unwrap();
        assert_eq!(
            r.fichiers.iter().map(|f| f.nom.as_str()).collect::<Vec<_>>(),
            ["q4.gguf", "q8.gguf"]
        );
        assert!(r.fichiers[0].installe);
        assert_eq!(r.fichiers[0].vram.as_deref(), Some("tient"));
        assert_eq!(r.fichiers[1].vram.as_deref(), Some("deborde"));
        assert_eq!(r.fichiers[0].dossier.as_deref(), Some("llm"));
        assert_eq!((r.vram_libre_go, r.vram_totale_go), (Some(8.0), Some(12.0)));
        assert!(fichiers_hf(&faux, &sans_cle, &installes, "../x", "llm", None, None).is_err());
        assert!(fichiers_hf(&faux, &sans_cle, &installes, "org/absent", "llm", None, None).is_err());
    }

    #[test]
    fn dossiers_hugging_face() {
        assert_eq!(
            dossier_hf("split_files/vae/a.safetensors", "image", None).as_deref(),
            Some("image/_base/vae")
        );
        let krea = dossier_hf("diffusion_models/k.safetensors", "image", Some("Krea2")).unwrap();
        assert!(krea.ends_with("/Krea2/modeles"));
        assert!(dossier_hf("diffusion_models/k.safetensors", "image", None).is_none());
        assert!(
            dossier_hf("k.safetensors", "lora", Some("Krea2"))
                .unwrap()
                .ends_with("/loras")
        );
        assert_eq!(
            deviner_dossier_hf("split_files/clip/t5.safetensors").as_deref(),
            Some("image/_base/text_encoders")
        );
        assert!(deviner_dossier_hf("unet/k.safetensors").is_none());
        assert_eq!(deviner_dossier_hf("x/m.Q4.gguf").as_deref(), Some("llm"));
    }

    #[test]
    fn fiche_d_une_lora_civitai() {
        let version = serde_json::json!({
            "id": 70, "modelId": 7, "name": "v1 8 steps", "baseModel": "Krea 2", "nsfwLevel": 4,
            "trainedWords": [" neon ", ""], "model": {"type": "LORA", "name": "Neon"},
            "files": [{"name": "neon.safetensors", "sizeKB": 1.0, "primary": true, "hashes": {"SHA256": "ABC"},
                       "downloadUrl": "https://civitai.com/api/download/models/70"}],
            "images": [
                {"meta": {"prompt": "  neon   city ", "additionalResources": [{"type": "lora", "strength": 0.8}]}},
                {"meta": {"additionalResources": [{"type": "LORA", "name": "neon.safetensors", "weight": "0.6"},
                                                  {"type": "lora", "name": "autre", "strength": 1.9}]}},
                {"meta": {"additionalResources": [{"type": "lora", "strength": 5}]}}
            ]
        });
        let faux = Faux::default()
            .avec(&format!("{CIVITAI}/model-versions/70"), 200, &version)
            .avec(
                &format!("{CIVITAI}/models/7"),
                200,
                &serde_json::json!({"modelVersions": [{"id": 70}]}),
            );
        let p = fiche_depuis_lien(&faux, &sans_cle, "https://civitai.red/models/7/neon").unwrap();
        assert_eq!(p.fiche.nom, "neon.safetensors");
        assert_eq!(p.fiche.sha256.as_deref(), Some("abc"));
        assert_eq!(p.fiche.mots_declencheurs, vec!["neon".to_owned()]);
        assert_eq!((p.fiche.force_auteur, p.fiche.images_force), (Some(0.7), 2));
        assert_eq!(p.fiche.prompt_exemple.as_deref(), Some("neon city"));
        assert!(p.fiche.steps_suggeres.is_none(), "une LoRA n'a pas de steps");
        assert_eq!(p.fiche.page, "https://civitai.red/models/7?modelVersionId=70");
        assert!(p.dossier.unwrap().ends_with("/Krea2/loras"));
        let p = fiche_depuis_lien(&faux, &sans_cle, "https://civitai.com/api/download/models/70").unwrap();
        assert_eq!(p.fiche.taille, 1024);
        assert!(fiche_depuis_lien(&faux, &sans_cle, "https://civitai.com/user/ana").is_err());
        assert!(fiche_depuis_lien(&faux, &sans_cle, "https://exemple.org/models/7").is_err());
        assert!(fiche_depuis_lien(&faux, &sans_cle, "http://civitai.com/models/7").is_err());
    }

    #[test]
    fn fiche_d_un_checkpoint_hugging_face() {
        let faux = Faux::default()
            .avec(&format!("{HF}/api/models/org/depot/tree/main/split_files"), 200, &serde_json::json!([
                {"type": "file", "path": "split_files/k turbo.safetensors", "size": 10, "lfs": {"oid": "DEF", "size": 10}}
            ]))
            .avec(&format!("{HF}/api/models/org/depot"), 200, &serde_json::json!({"gated": "auto", "tags": ["license:apache-2.0"]}))
            .avec(&format!("{HF}/org/depot/raw/main/README.md"), 200, &Value::String("Use 8 steps, CFG 1".to_owned()));
        let p = fiche_depuis_lien(
            &faux,
            &sans_cle,
            "https://huggingface.co/org/depot/blob/main/split_files/k%20turbo.safetensors",
        )
        .unwrap();
        assert_eq!(p.fiche.nom, "k turbo.safetensors");
        assert_eq!(
            p.fiche.url,
            "https://huggingface.co/org/depot/resolve/main/split_files/k%20turbo.safetensors"
        );
        assert_eq!(
            (p.fiche.sha256.as_deref(), p.fiche.licence.as_deref()),
            (Some("def"), Some("apache-2.0"))
        );
        assert!(p.jeton_requis);
        assert_eq!(p.fiche.steps_suggeres.unwrap()["valeurs"], serde_json::json!([8]));
        assert!(p.dossier.is_none(), "un modèle de diffusion dépend d'une famille");
        assert!(fiche_depuis_lien(&faux, &sans_cle, "https://huggingface.co/org/depot").is_err());
    }

    #[test]
    fn adresses() {
        assert_eq!(encoder("Krea 2/é"), "Krea%202%2F%C3%A9");
        assert_eq!(encoder_chemin("a b/c"), "a%20b/c");
        assert_eq!(decoder("k%20t%C3%A9%2"), "k té%2");
        assert!(depot_valide("org/Mon-Depot_1.0") && !depot_valide("org/..") && !depot_valide("a/b/c"));
        assert!(
            gguf_annexe("x-00001-of-00003.gguf")
                && !gguf_annexe("x-Q4_K_M.gguf")
                && gguf_annexe("mtp/x.gguf")
        );
        assert_eq!(verdict_vram(GO, 10 * GO), "tient");
        assert_eq!(page_civitai(1, None, 2), "https://civitai.com/models/1");
    }
}
