//! Ce qu'Aria sait télécharger elle-même, comme en v2 (`app/collection.py` : `telecharger_manquants`, `installer_llm`) :
//! les fichiers de chaque workflow livré (adresses, tailles et empreintes figées dans sa fiche) et les LLM du
//! catalogue (avec la consigne et le projecteur d'images d'un spécialiste). Le clic, après lecture de la licence,
//! vaut accord.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::modeles::{self, DOSSIER_LLM};
use crate::telechargement::Fiche;
use crate::{fiches, workflows};

/// La licence d'un modèle, telle que sa fiche la résume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Licence {
    pub nom: String,
    pub url: String,
    pub resume: String,
    pub commercial: bool,
}

/// Un fichier d'un workflow ou d'un LLM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fichier {
    pub nom: String,
    pub dossier: String,
    pub taille: u64,
    pub present: bool,
}

/// Un workflow livré et ce qui lui manque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatWorkflow {
    pub id: String,
    pub nom: String,
    pub tache: String,
    pub famille: String,
    pub licence: Licence,
    pub fichiers: Vec<Fichier>,
    /// Octets encore à télécharger.
    pub a_telecharger: u64,
    pub pret: bool,
}

/// Un LLM du catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatLlm {
    pub fichier: String,
    pub nom: String,
    pub page: String,
    /// `creation` ou `retouche` pour un spécialiste.
    pub specialiste: Option<String>,
    pub vision: bool,
    pub licence: Licence,
    pub taille: u64,
    pub a_telecharger: u64,
    pub installe: bool,
}

fn texte(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn licence(v: Option<&Value>) -> Licence {
    Licence {
        nom: texte(v.and_then(|l| l.get("nom"))),
        url: texte(v.and_then(|l| l.get("url"))),
        resume: texte(v.and_then(|l| l.get("resume"))),
        commercial: v
            .and_then(|l| l.get("commercial"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// La page d'un dépôt Hugging Face d'après l'adresse d'un de ses fichiers (`…/resolve/main/x`).
pub fn page_de(url: &str) -> String {
    url.split_once("/resolve/")
        .or_else(|| url.split_once("/blob/"))
        .map_or_else(|| url.to_owned(), |(page, _)| page.to_owned())
}

fn present(modeles: &Path, dossier: &str, nom: &str) -> bool {
    modeles::destination(modeles, dossier, nom).is_ok_and(|p| p.is_file())
}

/// Les workflows livrés, leurs fichiers et ce qui leur manque (le checkpoint d'origine n'est pas exigé si la famille
/// en a déjà un, comme en v2).
pub fn workflows(modeles: &Path) -> Vec<EtatWorkflow> {
    workflows::catalogue()
        .0
        .iter()
        .map(|wf| {
            let manquants = wf.manquants(modeles);
            let fichiers: Vec<Fichier> = wf
                .fiche
                .get("fichiers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|f| {
                    let nom = texte(f.get("nom"));
                    let dossier = texte(f.get("dossier"));
                    Fichier {
                        present: present(modeles, &dossier, &nom),
                        taille: f.get("taille").and_then(Value::as_u64).unwrap_or(0),
                        nom,
                        dossier,
                    }
                })
                .collect();
            EtatWorkflow {
                id: wf.id.clone(),
                nom: texte(wf.fiche.get("nom")),
                tache: wf.tache().to_owned(),
                famille: wf.famille().to_owned(),
                licence: licence(wf.fiche.get("licence")),
                a_telecharger: manquants
                    .iter()
                    .filter_map(|f| f.get("taille").and_then(Value::as_u64))
                    .sum(),
                pret: manquants.is_empty(),
                fichiers,
            }
        })
        .collect()
}

/// Ce qu'il faut mettre en file pour qu'un workflow soit prêt : une fiche de téléchargement et son dossier par
/// fichier manquant.
///
/// # Errors
/// Workflow inconnu.
pub fn manquants(modeles: &Path, id: &str) -> Result<Vec<(Fiche, String)>, String> {
    let wf = workflows::obtenir(id)?;
    let licence = licence(wf.fiche.get("licence")).nom;
    Ok(wf
        .manquants(modeles)
        .iter()
        .map(|f| {
            let url = texte(f.get("url"));
            (
                Fiche {
                    source: "huggingface".to_owned(),
                    nom: texte(f.get("nom")),
                    taille: f.get("taille").and_then(Value::as_u64).unwrap_or(0),
                    sha256: f.get("sha256").and_then(Value::as_str).map(str::to_owned),
                    page: page_de(&url),
                    url,
                    licence: Some(licence.clone()),
                    ..Fiche::default()
                },
                texte(f.get("dossier")),
            )
        })
        .collect())
}

/// Les fichiers d'un LLM du catalogue : le GGUF et sa vision, et pour un spécialiste sa consigne et sa vision.
fn fichiers_llm(fiche: &Value) -> Vec<Value> {
    let mut liste = vec![serde_json::json!({
        "nom": fiche.get("id"),
        "url": fiche.get("url"),
        "taille": fiche.get("taille"),
        "sha256": fiche.get("sha256"),
    })];
    if let Some(v) = fiche.get("vision") {
        liste.push(v.clone());
    }
    for cle in ["consigne", "vision"] {
        if let Some(v) = fiche.get("specialiste").and_then(|s| s.get(cle)) {
            liste.push(v.clone());
        }
    }
    liste
}

/// Les LLM du catalogue et ce qui leur manque.
pub fn llm(modeles: &Path) -> Vec<EtatLlm> {
    fiches::catalogue_llm()
        .unwrap_or_default()
        .iter()
        .map(|fiche| {
            let fichiers = fichiers_llm(fiche);
            let taille = |f: &Value| f.get("taille").and_then(Value::as_u64).unwrap_or(0);
            let manque: Vec<&Value> = fichiers
                .iter()
                .filter(|f| !present(modeles, DOSSIER_LLM, &texte(f.get("nom"))))
                .collect();
            let specialiste = fiche.get("specialiste");
            EtatLlm {
                fichier: texte(fiche.get("id")),
                nom: texte(fiche.get("nom")),
                page: texte(fiche.get("page")),
                specialiste: specialiste
                    .and_then(|s| s.get("usage"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                vision: fiche.get("vision").is_some() || specialiste.and_then(|s| s.get("vision")).is_some(),
                licence: licence(fiche.get("licence")),
                taille: fichiers.iter().map(taille).sum(),
                a_telecharger: manque.iter().map(|f| taille(f)).sum(),
                installe: manque.is_empty(),
            }
        })
        .collect()
}

/// Ce qu'il faut mettre en file pour installer un LLM du catalogue.
///
/// # Errors
/// LLM inconnu du catalogue.
pub fn llm_manquant(modeles: &Path, fichier: &str) -> Result<Vec<(Fiche, String)>, String> {
    let fiche = fiches::fiche_llm(fichier).ok_or_else(|| format!("LLM inconnu du catalogue : {fichier}"))?;
    let licence = licence(fiche.get("licence")).nom;
    let page = texte(fiche.get("page"));
    Ok(fichiers_llm(&fiche)
        .iter()
        .filter(|f| !present(modeles, DOSSIER_LLM, &texte(f.get("nom"))))
        .map(|f| {
            (
                Fiche {
                    source: "huggingface".to_owned(),
                    nom: texte(f.get("nom")),
                    taille: f.get("taille").and_then(Value::as_u64).unwrap_or(0),
                    sha256: f.get("sha256").and_then(Value::as_str).map(str::to_owned),
                    url: texte(f.get("url")),
                    page: page.clone(),
                    licence: Some(licence.clone()),
                    ..Fiche::default()
                },
                DOSSIER_LLM.to_owned(),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn workflows_et_ce_qui_leur_manque() {
        let temp = tempfile::tempdir().unwrap();
        let modeles = temp.path();
        let etat = workflows(modeles);
        assert_eq!(etat.len(), 5);
        let krea = etat.iter().find(|w| w.id == "krea2_turbo").unwrap();
        assert!(!krea.pret);
        assert!(krea.a_telecharger > 0);
        assert!(krea.fichiers.iter().all(|f| !f.present));
        assert!(!krea.licence.nom.is_empty());
        let a_faire = manquants(modeles, "krea2_turbo").unwrap();
        assert_eq!(a_faire.len(), krea.fichiers.len());
        let (fiche, dossier) = &a_faire[0];
        assert!(fiche.url.starts_with("https://"));
        assert!(fiche.sha256.as_ref().is_some_and(|s| s.len() == 64));
        assert!(!fiche.page.contains("/resolve/"));
        // Un fichier posé ne manque plus.
        let chemin = modeles::destination(modeles, dossier, &fiche.nom).unwrap();
        fs::create_dir_all(chemin.parent().unwrap()).unwrap();
        fs::write(&chemin, b"x").unwrap();
        assert_eq!(
            manquants(modeles, "krea2_turbo").unwrap().len(),
            a_faire.len() - 1
        );
        assert!(manquants(modeles, "inconnu").is_err());
    }

    #[test]
    fn llm_du_catalogue() {
        let temp = tempfile::tempdir().unwrap();
        let etat = llm(temp.path());
        let bonsai = etat
            .iter()
            .find(|l| l.fichier == "Ternary-Bonsai-2-27B-PTQ1_0.gguf")
            .unwrap();
        assert!(!bonsai.installe && bonsai.vision && bonsai.specialiste.is_none());
        assert_eq!(bonsai.a_telecharger, bonsai.taille);
        let t2i = etat
            .iter()
            .find(|l| l.fichier == "Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf")
            .unwrap();
        assert_eq!(t2i.specialiste.as_deref(), Some("creation"));
        let a_faire = llm_manquant(temp.path(), "Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf").unwrap();
        assert_eq!(a_faire.len(), 2, "le GGUF et sa consigne");
        assert!(a_faire.iter().all(|(_, d)| d == "llm"));
        assert!(llm_manquant(temp.path(), "inconnu.gguf").is_err());
    }

    #[test]
    fn page_d_un_depot() {
        assert_eq!(
            page_de("https://huggingface.co/Comfy-Org/x/resolve/main/split_files/vae/a.safetensors"),
            "https://huggingface.co/Comfy-Org/x"
        );
        assert_eq!(page_de("https://exemple.org/a"), "https://exemple.org/a");
    }
}
