//! Modèles → Ma collection → « Retrouver les sources », comme en v2 (`app/collection.py`) : pour chaque fichier déposé
//! à la main, son SHA-256 (gardé dans `cache\empreintes.json` au format de la v2, recalculé seulement s'il a changé),
//! puis sa source : le catalogue d'Aria (fichiers des fiches et LLM), sinon Civitai par empreinte. Un fichier retrouvé
//! entre au registre avec `retrouve: true` : il garde sa place, mais Aria connaît désormais sa page et sa licence.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use chrono::Local;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::decouvrir::{self, CIVITAI, EntetesDe};
use crate::installations::page_de;
use crate::telechargement::{Contexte, Reseau, empreinte, modifier_registre, registre};
use crate::{Dossier, fiches, journal, modeles, workflows};

/// Un fichier retrouvé, et où.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trouve {
    pub nom: String,
    pub ou: String,
}

/// Une erreur sur un fichier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErreurFichier {
    pub nom: String,
    pub erreur: String,
}

/// Où en est la recherche : `repos`, `en_cours` ou `termine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identification {
    pub etat: String,
    pub total: usize,
    pub fait: usize,
    pub fichier: Option<String>,
    /// `empreinte` ou `civitai`.
    pub phase: Option<String>,
    pub trouves: Vec<Trouve>,
    pub introuvables: Vec<String>,
    pub erreurs: Vec<ErreurFichier>,
}

impl Default for Identification {
    fn default() -> Self {
        Self {
            etat: "repos".to_owned(),
            total: 0,
            fait: 0,
            fichier: None,
            phase: None,
            trouves: Vec::new(),
            introuvables: Vec::new(),
            erreurs: Vec::new(),
        }
    }
}

/// L'état partagé entre la recherche (un fil à part) et l'interface.
#[derive(Debug, Default)]
pub struct Suivi(Mutex<Identification>);

impl Suivi {
    /// Une copie de l'état.
    pub fn lire(&self) -> Identification {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn modifier(&self, f: impl FnOnce(&mut Identification)) {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// Prépare une recherche sur ces fichiers ; faux si une autre est en cours.
    pub fn commencer(&self, total: usize) -> bool {
        let mut etat = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if etat.etat == "en_cours" {
            return false;
        }
        *etat = Identification {
            etat: "en_cours".to_owned(),
            total,
            ..Identification::default()
        };
        true
    }
}

/// Les fichiers déposés à la main : présents dans un dossier des modèles, absents du registre.
pub fn orphelins(data: &Path, modeles_dir: &Path) -> Vec<String> {
    let r = registre(data);
    modeles::dossiers()
        .iter()
        .flat_map(|d| {
            modeles::fichiers(modeles_dir, d, &decouvrir::EXTENSIONS)
                .into_iter()
                .map(move |n| format!("{d}/{n}"))
        })
        .filter(|cle| !r.contains_key(cle))
        .collect()
}

/// La date de modification d'un fichier, en secondes (comme `st_mtime` en Python).
fn date(meta: &fs::Metadata) -> Option<f64> {
    Some(
        meta.modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs_f64(),
    )
}

/// Le SHA-256 d'un fichier des modèles, repris du cache s'il n'a pas changé (même taille, même date), sinon calculé
/// et gardé pour la fois suivante.
///
/// # Errors
/// Le fichier ne se lit pas.
pub fn empreinte_gardee(dossier: &Dossier, modeles_dir: &Path, cle: &str) -> Result<String, String> {
    let chemin = modeles_dir.join(cle);
    let meta = fs::metadata(&chemin).map_err(|e| e.to_string())?;
    let fichier_cache = dossier.cache().join("empreintes.json");
    let lire_cache = || -> Map<String, Value> {
        fs::read_to_string(&fichier_cache)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    };
    let modifie = date(&meta);
    if let Some(e) = lire_cache().get(cle) {
        let meme_taille = e.get("taille").and_then(Value::as_u64) == Some(meta.len());
        let meme_date = matches!(
            (e.get("date").and_then(Value::as_f64), modifie),
            (Some(a), Some(b)) if (a - b).abs() < 1e-3
        );
        if meme_taille
            && meme_date
            && let Some(sha) = e.get("sha256").and_then(Value::as_str)
        {
            return Ok(sha.to_lowercase());
        }
    }
    let sha = empreinte(&chemin).map_err(|e| e.to_string())?;
    let mut toutes = lire_cache();
    toutes.insert(
        cle.to_owned(),
        json!({"taille": meta.len(), "date": modifie, "sha256": sha}),
    );
    // Le cache n'est qu'un gain de temps : s'il ne s'écrit pas, l'empreinte sera recalculée la prochaine fois.
    if let Some(parent) = fichier_cache.parent()
        && fs::create_dir_all(parent).is_ok()
        && let Ok(texte) = serde_json::to_string_pretty(&Value::Object(toutes))
    {
        let temporaire = fichier_cache.with_extension("json.tmp");
        if fs::write(&temporaire, texte).is_ok() {
            let _ = fs::rename(&temporaire, &fichier_cache);
        }
    }
    Ok(sha)
}

/// Un fichier qu'Aria sait télécharger elle-même.
struct Connu {
    sha256: String,
    url: String,
    page: String,
    licence: Option<String>,
}

/// Tout ce qu'Aria sait télécharger elle-même : fichiers des fiches, LLM du catalogue.
fn connus_du_catalogue() -> Vec<Connu> {
    let mut connus = Vec::new();
    for wf in workflows::catalogue().0 {
        let licence = wf
            .fiche
            .pointer("/licence/nom")
            .and_then(Value::as_str)
            .map(str::to_owned);
        for f in wf
            .fiche
            .get("fichiers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(sha), Some(url)) = (
                f.get("sha256").and_then(Value::as_str),
                f.get("url").and_then(Value::as_str),
            ) {
                connus.push(Connu {
                    sha256: sha.to_lowercase(),
                    url: url.to_owned(),
                    page: page_de(url),
                    licence: licence.clone(),
                });
            }
        }
    }
    for llm in fiches::catalogue_llm().unwrap_or_default() {
        let licence = llm
            .pointer("/licence/nom")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let page = llm
            .get("page")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut fichiers = vec![&llm];
        fichiers.extend(llm.get("vision"));
        fichiers.extend(
            ["consigne", "vision"]
                .iter()
                .filter_map(|k| llm.get("specialiste")?.get(*k)),
        );
        for f in fichiers {
            if let (Some(sha), Some(url)) = (
                f.get("sha256").and_then(Value::as_str),
                f.get("url").and_then(Value::as_str),
            ) {
                connus.push(Connu {
                    sha256: sha.to_lowercase(),
                    url: url.to_owned(),
                    page: page.clone(),
                    licence: licence.clone(),
                });
            }
        }
    }
    connus
}

/// La version Civitai d'un fichier retrouvée par son SHA-256 : l'entrée du registre (rien : Civitai ne le connaît pas).
///
/// # Errors
/// Civitai ne répond pas.
pub fn par_empreinte(
    reseau: &dyn Reseau,
    entetes: EntetesDe<'_>,
    sha256: &str,
) -> Result<Option<Map<String, Value>>, String> {
    if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Empreinte invalide : {sha256}"));
    }
    let (statut, corps) = decouvrir::lire_api(
        reseau,
        &format!("{CIVITAI}/model-versions/by-hash/{sha256}"),
        &entetes("civitai"),
    )?;
    if statut == 404 {
        return Ok(None);
    }
    let version: Value = if statut == 200 {
        serde_json::from_str(&corps).map_err(|e| format!("Civitai : réponse illisible ({e})"))?
    } else {
        return Err(format!("Civitai a répondu {statut}"));
    };
    let fichier = version
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|f| {
            f.pointer("/hashes/SHA256")
                .and_then(Value::as_str)
                .is_some_and(|s| s.eq_ignore_ascii_case(sha256))
        });
    let id_version = version.get("id").and_then(Value::as_u64).unwrap_or(0);
    let type_ = version.pointer("/model/type").and_then(Value::as_str);
    let mut entree = Map::new();
    entree.insert("source".to_owned(), json!("civitai"));
    entree.insert(
        "page".to_owned(),
        json!(decouvrir::page_civitai(
            version.get("modelId").and_then(Value::as_u64).unwrap_or(0),
            Some(id_version),
            version.get("nsfwLevel").and_then(Value::as_u64).unwrap_or(0),
        )),
    );
    entree.insert(
        "url".to_owned(),
        json!(
            fichier
                .and_then(|f| f.get("downloadUrl"))
                .and_then(Value::as_str)
                .map_or_else(
                    || format!("https://civitai.com/api/download/models/{id_version}"),
                    str::to_owned
                )
        ),
    );
    entree.insert("licence".to_owned(), Value::Null);
    let mots: Vec<String> = version
        .get("trainedWords")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
        .collect();
    entree.insert("mots_declencheurs".to_owned(), json!(mots));
    entree.insert(
        "base".to_owned(),
        version.get("baseModel").cloned().unwrap_or(Value::Null),
    );
    if type_.is_some_and(|t| ["LORA", "LoCon", "DoRA"].contains(&t)) {
        let nom = fichier.and_then(|f| f.get("name")).and_then(Value::as_str);
        let (force, images, prompt) = decouvrir::usage_auteur(&version, nom);
        if let Some(force) = force {
            entree.insert("force_auteur".to_owned(), json!(force));
            entree.insert("images_force".to_owned(), json!(images));
        }
        if let Some(prompt) = prompt {
            entree.insert("prompt_exemple".to_owned(), json!(prompt));
        }
    }
    entree.insert(
        "nom_modele".to_owned(),
        version.pointer("/model/name").cloned().unwrap_or(Value::Null),
    );
    entree.insert(
        "version".to_owned(),
        version.get("name").cloned().unwrap_or(Value::Null),
    );
    entree.insert("type".to_owned(), json!(type_));
    Ok(Some(entree))
}

/// Inscrit un fichier retrouvé au registre.
fn inscrire(
    contexte: &Contexte,
    cle: &str,
    mut entree: Map<String, Value>,
    sha256: &str,
) -> Result<(), String> {
    let taille = fs::metadata(contexte.modeles.join(cle))
        .map(|m| m.len())
        .unwrap_or(0);
    entree.entry("mots_declencheurs").or_insert_with(|| json!([]));
    entree.entry("base").or_insert(Value::Null);
    entree.insert("taille".to_owned(), json!(taille));
    entree.insert("sha256".to_owned(), json!(sha256));
    entree.insert(
        "date".to_owned(),
        json!(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
    );
    entree.insert("retrouve".to_owned(), json!(true));
    modifier_registre(&contexte.data, |r| {
        r.insert(cle.to_owned(), Value::Object(entree));
    })
    .map_err(|e| format!("registre des modèles : {e}"))
}

/// La source d'un fichier : où il a été retrouvé (rien : introuvable).
fn source_de(
    contexte: &Contexte,
    reseau: &dyn Reseau,
    connus: &[Connu],
    cle: &str,
    suivi: &Suivi,
) -> Result<Option<String>, String> {
    let sha = empreinte_gardee(&contexte.dossier, &contexte.modeles, cle)?;
    if let Some(connu) = connus.iter().find(|c| c.sha256 == sha) {
        let mut entree = Map::new();
        entree.insert("source".to_owned(), json!("huggingface"));
        entree.insert("page".to_owned(), json!(connu.page));
        entree.insert("url".to_owned(), json!(connu.url));
        entree.insert("licence".to_owned(), json!(connu.licence));
        inscrire(contexte, cle, entree, &sha)?;
        return Ok(Some("catalogue d'Aria".to_owned()));
    }
    suivi.modifier(|e| e.phase = Some("civitai".to_owned()));
    match par_empreinte(reseau, &contexte.entetes, &sha)? {
        Some(entree) => {
            let modele = entree
                .get("nom_modele")
                .and_then(Value::as_str)
                .unwrap_or("modèle")
                .to_owned();
            inscrire(contexte, cle, entree, &sha)?;
            Ok(Some(format!("Civitai : {modele}")))
        }
        None => Ok(None),
    }
}

/// Retrouve la source de chaque fichier déposé à la main, en suivant l'avancement dans `suivi` (déjà commencé).
pub fn identifier(contexte: &Contexte, reseau: &dyn Reseau, orphelins: &[String], suivi: &Suivi) {
    let connus = connus_du_catalogue();
    for (i, cle) in orphelins.iter().enumerate() {
        let nom = cle.rsplit('/').next().unwrap_or(cle).to_owned();
        suivi.modifier(|e| {
            e.fichier = Some(nom.clone());
            e.fait = i;
            e.phase = Some("empreinte".to_owned());
        });
        let resultat = source_de(contexte, reseau, &connus, cle, suivi);
        suivi.modifier(|e| match resultat {
            Ok(Some(ou)) => e.trouves.push(Trouve { nom, ou }),
            Ok(None) => e.introuvables.push(nom),
            Err(erreur) => e.erreurs.push(ErreurFichier { nom, erreur }),
        });
    }
    let fin = suivi.lire();
    journal::noter(
        &contexte.dossier,
        "Sources des modèles recherchées",
        &format!("{} fichier(s)", orphelins.len()),
        if fin.erreurs.is_empty() {
            journal::Resultat::Ok
        } else {
            journal::Resultat::Partiel
        },
        Some(&format!(
            "{} retrouvé(s), {} introuvable(s), {} erreur(s)",
            fin.trouves.len(),
            fin.introuvables.len(),
            fin.erreurs.len()
        )),
    );
    suivi.modifier(|e| {
        e.etat = "termine".to_owned();
        e.fait = orphelins.len();
        e.fichier = None;
        e.phase = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor, Read};
    use std::path::PathBuf;

    struct Faux(String);

    impl Reseau for Faux {
        fn ouvrir(&self, adresse: &str, _: &[(String, String)], _: u64) -> io::Result<(u16, Box<dyn Read>)> {
            if adresse.ends_with(&self.0) {
                let version = json!({
                    "id": 70, "modelId": 7, "name": "v1", "baseModel": "Krea 2", "trainedWords": ["neon"],
                    "model": {"type": "LORA", "name": "Neon"},
                    "files": [{"name": "neon.safetensors", "hashes": {"SHA256": self.0.to_uppercase()},
                               "downloadUrl": "https://civitai.com/api/download/models/70"}],
                    "images": [{"meta": {"additionalResources": [{"type": "lora", "strength": 0.9}]}}]
                });
                return Ok((200, Box::new(Cursor::new(version.to_string().into_bytes()))));
            }
            Ok((404, Box::new(Cursor::new(Vec::new()))))
        }
    }

    fn contexte(racine: &Path) -> Contexte {
        Contexte {
            dossier: Dossier::a(racine),
            data: racine.join("data"),
            modeles: racine.join("modeles"),
            entetes: Box::new(|_| Vec::new()),
            espace_libre: Box::new(|_| None),
        }
    }

    fn poser(c: &Contexte, cle: &str, octets: &[u8]) -> PathBuf {
        let chemin = c.modeles.join(cle);
        fs::create_dir_all(chemin.parent().unwrap()).unwrap();
        fs::write(&chemin, octets).unwrap();
        chemin
    }

    #[test]
    fn retrouve_par_civitai_et_garde_les_empreintes() {
        let temp = tempfile::tempdir().unwrap();
        let c = contexte(temp.path());
        let lora = format!(
            "{}/neon.safetensors",
            workflows::obtenir("krea2_turbo").unwrap().dossier_loras()
        );
        poser(&c, &lora, b"poids de la lora");
        poser(&c, "llm/inconnu.gguf", b"rien");
        poser(&c, "llm/note.txt", b"pas un modele");
        let sha_lora = empreinte(&c.modeles.join(&lora)).unwrap();
        let a_chercher = orphelins(&c.data, &c.modeles);
        assert_eq!(a_chercher.len(), 2);
        let suivi = Suivi::default();
        assert!(suivi.commencer(a_chercher.len()));
        assert!(!suivi.commencer(1), "une seule recherche à la fois");
        identifier(&c, &Faux(sha_lora.clone()), &a_chercher, &suivi);
        let fin = suivi.lire();
        assert_eq!((fin.etat.as_str(), fin.fait), ("termine", 2));
        assert_eq!(
            fin.trouves,
            vec![Trouve {
                nom: "neon.safetensors".to_owned(),
                ou: "Civitai : Neon".to_owned()
            }]
        );
        assert_eq!(fin.introuvables, vec!["inconnu.gguf".to_owned()]);
        let r = registre(&c.data);
        assert_eq!(r[&lora]["retrouve"], json!(true));
        assert_eq!(r[&lora]["force_auteur"], json!(0.9));
        assert_eq!(r[&lora]["mots_declencheurs"], json!(["neon"]));
        assert_eq!(
            orphelins(&c.data, &c.modeles),
            vec!["llm/inconnu.gguf".to_owned()]
        );
        // L'empreinte est gardée : un fichier modifié est recalculé.
        let cache: Value =
            serde_json::from_str(&fs::read_to_string(c.dossier.cache().join("empreintes.json")).unwrap())
                .unwrap();
        assert_eq!(cache[&lora]["sha256"], json!(sha_lora));
        assert_eq!(empreinte_gardee(&c.dossier, &c.modeles, &lora).unwrap(), sha_lora);
        poser(&c, &lora, b"autre contenu");
        assert_ne!(empreinte_gardee(&c.dossier, &c.modeles, &lora).unwrap(), sha_lora);
        assert_eq!(
            journal::lire(&c.dossier)[0].action,
            "Sources des modèles recherchées"
        );
    }

    #[test]
    fn empreinte_invalide_refusee() {
        assert!(par_empreinte(&Faux(String::new()), &|_| Vec::new(), "../x").is_err());
    }
}
