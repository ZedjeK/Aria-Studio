//! Les modèles sur le disque, comme en v2 (`app/modeles.py`) :
//!
//! ```text
//! modeles\<nature>\<Famille>\modeles\   variantes du modèle de diffusion d'une famille (ex. image\Krea2)
//! modeles\<nature>\<Famille>\loras\     LoRA compatibles avec cette famille
//! modeles\<nature>\_base\<type>\        fichiers de la nature : encodeurs de texte, VAE, LoRA internes…
//! modeles\llm\                          GGUF pour llama.cpp
//! ```
//!
//! Les familles et leur nature (image ou vidéo) viennent des fiches de workflow livrées. ComfyUI apprend ces
//! dossiers par `cache\comfyui_chemins.yaml` ([`ecrire_chemins_comfyui`]), réécrit avant chaque démarrage.
//! Seuls les `.safetensors` (et les `.gguf` des LLM) sont des modèles : jamais de `.ckpt` ni de `.pt`, qui peuvent
//! exécuter du code au chargement.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::fiches::{self, LIVRES};

/// Les natures de modèles.
pub const NATURES: [&str; 2] = ["image", "video"];
/// Le dossier des fichiers communs à une nature.
pub const BASE: &str = "_base";
/// Les types de fichiers communs, dans l'ordre du fichier des chemins de ComfyUI.
pub const TYPES_BASE: [&str; 11] = [
    "checkpoints",
    "diffusion_models",
    "text_encoders",
    "vae",
    "loras",
    "clip_vision",
    "controlnet",
    "upscale_models",
    "model_patches",
    "audio_encoders",
    "latent_upscale_models",
];
/// Les modèles d'image et de vidéo.
pub const EXTENSIONS_IMAGE: [&str; 1] = ["safetensors"];
/// Le dossier des LLM.
pub const DOSSIER_LLM: &str = "llm";

/// La nature d'une tâche de workflow.
fn nature_de_tache(tache: &str) -> &'static str {
    match tache {
        "image_vers_video" | "texte_vers_video" => "video",
        _ => "image",
    }
}

/// Les familles des workflows livrés et leur nature, triées par nom. Une famille dont un workflow est vidéo est
/// vidéo.
pub fn familles() -> BTreeMap<String, &'static str> {
    let mut familles: BTreeMap<String, &'static str> = BTreeMap::new();
    for livre in &LIVRES {
        let Ok(fiche) = fiches::lire(livre.fiche) else {
            continue;
        };
        let (Some(famille), tache) = (
            fiche.get("famille").and_then(Value::as_str),
            fiche.get("tache").and_then(Value::as_str).unwrap_or_default(),
        ) else {
            continue;
        };
        let nature = nature_de_tache(tache);
        let entree = familles.entry(famille.to_owned()).or_insert(nature);
        if nature == "video" {
            *entree = "video";
        }
    }
    familles
}

/// `<nature>/<famille>/<sous>` (`sous` : `modeles` ou `loras`).
///
/// # Errors
/// La famille n'est celle d'aucun workflow livré.
pub fn dossier_famille(famille: &str, sous: &str) -> Result<String, String> {
    let nature = familles()
        .get(famille)
        .copied()
        .ok_or_else(|| format!("Famille inconnue : {famille}"))?;
    Ok(format!("{nature}/{famille}/{sous}"))
}

/// `<nature>/_base/<type>`.
pub fn dossier_base(nature: &str, type_: &str) -> String {
    format!("{nature}/{BASE}/{type_}")
}

/// Tous les dossiers de modèles connus, relatifs au dossier des modèles.
pub fn dossiers() -> Vec<String> {
    let mut liste: Vec<String> = NATURES
        .iter()
        .flat_map(|n| TYPES_BASE.iter().map(move |t| dossier_base(n, t)))
        .collect();
    for (famille, nature) in familles() {
        for sous in ["modeles", "loras"] {
            liste.push(format!("{nature}/{famille}/{sous}"));
        }
    }
    liste.push(DOSSIER_LLM.to_owned());
    liste
}

/// Un dossier de checkpoints de famille (`<nature>/<Famille>/modeles`) ?
pub fn est_checkpoint(dossier: &str) -> bool {
    let morceaux: Vec<&str> = dossier.split('/').collect();
    matches!(morceaux.as_slice(), [nature, famille, "modeles"] if NATURES.contains(nature) && *famille != BASE)
}

/// Les fichiers d'un dossier de modèles (pas ses sous-dossiers) dont l'extension est dans `extensions`, triés.
/// Un téléchargement en cours (`.part`) n'en est pas un.
pub fn fichiers(modeles: &Path, dossier: &str, extensions: &[&str]) -> Vec<String> {
    let Ok(entrees) = fs::read_dir(modeles.join(dossier)) else {
        return Vec::new();
    };
    let mut noms: Vec<String> = entrees
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            n.rsplit_once('.')
                .is_some_and(|(_, ext)| extensions.contains(&ext.to_lowercase().as_str()))
        })
        .collect();
    noms.sort();
    noms
}

/// Où ranger un fichier de modèle : le dossier doit être connu et le nom ne doit pas en sortir. Rien n'est créé.
///
/// # Errors
/// Dossier inconnu, ou nom refusé.
pub fn destination(modeles: &Path, dossier: &str, nom: &str) -> Result<PathBuf, String> {
    if !dossiers().iter().any(|d| d == dossier) {
        return Err(format!("Dossier inconnu : {dossier}"));
    }
    if nom.is_empty() || nom == "." || nom == ".." || nom.contains(['/', '\\', ':']) {
        return Err(format!("Nom de fichier invalide : {nom}"));
    }
    Ok(modeles.join(dossier).join(nom))
}

/// Un texte entre apostrophes en YAML (une apostrophe s'y double ; rien d'autre n'est interprété).
fn entre_apostrophes(texte: &str) -> String {
    format!("'{}'", texte.replace('\'', "''"))
}

/// Écrit `cible` (`cache\comfyui_chemins.yaml`) : une racine par nature, `aria_image` (par défaut) et
/// `aria_video`, avec pour chaque type de fichier le dossier `_base` de la nature et, pour les modèles de diffusion
/// et les LoRA, les dossiers de chaque famille de cette nature. Les dossiers de famille sont créés.
///
/// # Errors
/// Un dossier ou le fichier ne se crée pas.
pub fn ecrire_chemins_comfyui(modeles: &Path, cible: &Path) -> io::Result<PathBuf> {
    let familles = familles();
    for (famille, nature) in &familles {
        for sous in ["modeles", "loras"] {
            fs::create_dir_all(modeles.join(nature).join(famille).join(sous))?;
        }
    }
    let mut texte =
        String::from("# Écrit par Aria Studio avant chaque démarrage de ComfyUI : ne pas modifier.\n");
    for nature in NATURES {
        let siennes: Vec<&String> = familles
            .iter()
            .filter(|(_, n)| **n == nature)
            .map(|(f, _)| f)
            .collect();
        texte.push_str(&format!("aria_{nature}:\n"));
        let racine = modeles.join(nature);
        texte.push_str(&format!(
            "  base_path: {}\n",
            entre_apostrophes(&racine.to_string_lossy())
        ));
        if nature == "image" {
            texte.push_str("  is_default: true\n");
        }
        for type_ in TYPES_BASE {
            let sous = match type_ {
                "diffusion_models" => Some("modeles"),
                "loras" => Some("loras"),
                _ => None,
            };
            match sous {
                Some(sous) => {
                    texte.push_str(&format!("  {type_}: |\n    {BASE}/{type_}/\n"));
                    for famille in &siennes {
                        texte.push_str(&format!("    {famille}/{sous}/\n"));
                    }
                }
                None => texte.push_str(&format!("  {type_}: {BASE}/{type_}/\n")),
            }
        }
    }
    if let Some(parent) = cible.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(cible, texte)?;
    Ok(cible.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_familles_livrees_et_leur_nature() {
        let f = familles();
        assert_eq!(f.get("Krea2").copied(), Some("image"));
        assert_eq!(f.get("Qwen-Image2.1").copied(), Some("image"));
        assert_eq!(f.get("MiniMax-H3").copied(), Some("video"));
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn chaque_famille_de_son_cote() {
        assert_eq!(
            dossier_famille("Krea2", "modeles").unwrap(),
            "image/Krea2/modeles"
        );
        assert_eq!(
            dossier_famille("MiniMax-H3", "loras").unwrap(),
            "video/MiniMax-H3/loras"
        );
        assert!(
            dossier_famille("Inconnue", "modeles")
                .unwrap_err()
                .contains("Famille inconnue")
        );
        assert!(est_checkpoint("image/Krea2/modeles"));
        assert!(!est_checkpoint("image/Krea2/loras"));
        assert!(!est_checkpoint("image/_base/checkpoints"));
        assert!(!est_checkpoint("llm"));
    }

    #[test]
    fn les_fiches_rangent_chaque_fichier_du_cote_de_leur_famille() {
        let natures = familles();
        for livre in &LIVRES {
            let fiche = fiches::lire(livre.fiche).unwrap();
            let nature = natures[fiche["famille"].as_str().unwrap()];
            for f in fiche["fichiers"].as_array().unwrap() {
                let dossier = f["dossier"].as_str().unwrap();
                assert!(dossiers().iter().any(|d| d == dossier), "{dossier}");
                assert!(dossier.starts_with(nature), "{} : {dossier}", livre.id);
            }
        }
    }

    #[test]
    fn la_destination_refuse_ce_qui_sortirait() {
        let racine = Path::new("/m");
        assert_eq!(
            destination(racine, "image/_base/vae", "vae.safetensors").unwrap(),
            racine.join("image/_base/vae").join("vae.safetensors")
        );
        assert!(
            destination(racine, "image/ailleurs", "a")
                .unwrap_err()
                .starts_with("Dossier inconnu")
        );
        for nom in ["", ".", "..", "a/b", "a\\b", "C:a"] {
            assert!(
                destination(racine, "llm", nom)
                    .unwrap_err()
                    .starts_with("Nom de fichier invalide"),
                "{nom}"
            );
        }
    }

    #[test]
    fn seuls_les_safetensors_sont_des_modeles() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = temp.path().join("image/Krea2/modeles");
        fs::create_dir_all(dossier.join("sous")).unwrap();
        for nom in [
            "b.safetensors",
            "a.SAFETENSORS",
            "c.ckpt",
            "d.pt",
            "e.safetensors.part",
            "f.yaml",
        ] {
            fs::write(dossier.join(nom), "x").unwrap();
        }
        assert_eq!(
            fichiers(temp.path(), "image/Krea2/modeles", &EXTENSIONS_IMAGE),
            vec!["a.SAFETENSORS", "b.safetensors"]
        );
    }

    #[test]
    fn une_racine_par_nature_pour_comfyui() {
        let temp = tempfile::tempdir().unwrap();
        let modeles = temp.path().join("modèles d'essai");
        let cible = temp.path().join("cache").join("comfyui_chemins.yaml");
        ecrire_chemins_comfyui(&modeles, &cible).unwrap();
        let lu: Value = serde_saphyr::from_str(&fs::read_to_string(&cible).unwrap()).unwrap();
        let racines: Vec<&String> = lu.as_object().unwrap().keys().collect();
        assert_eq!(racines, vec!["aria_image", "aria_video"]);
        let image = &lu["aria_image"];
        assert_eq!(
            image["base_path"],
            modeles.join("image").to_string_lossy().as_ref()
        );
        assert_eq!(image["is_default"], true);
        assert!(lu["aria_video"].get("is_default").is_none());
        let diffusion = image["diffusion_models"].as_str().unwrap();
        let lignes: Vec<&str> = diffusion.lines().collect();
        assert_eq!(lignes[0], "_base/diffusion_models/");
        assert!(
            lignes.contains(&"Krea2/modeles/") && lignes.contains(&"Qwen-Image2.1/modeles/"),
            "{diffusion}"
        );
        assert!(!diffusion.contains("MiniMax-H3"));
        assert!(
            image["loras"]
                .as_str()
                .unwrap()
                .lines()
                .any(|l| l == "Krea2/loras/")
        );
        assert_eq!(lu["aria_video"]["vae"], "_base/vae/");
        assert!(
            lu["aria_video"]["diffusion_models"]
                .as_str()
                .unwrap()
                .contains("MiniMax-H3/modeles/")
        );
        assert_eq!(image.as_object().unwrap().len(), 2 + TYPES_BASE.len());
        assert!(modeles.join("video").join("MiniMax-H3").join("modeles").is_dir());
        assert!(modeles.join("image").join("Krea2").join("loras").is_dir());
    }
}
