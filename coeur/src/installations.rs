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
    /// Absent et exigé : il reste à télécharger. Un fichier absent mais non exigé (le checkpoint d'origine, quand la
    /// famille en a déjà un autre) ne manque pas.
    pub manque: bool,
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

/// La licence telle que la résume une fiche (bloc `licence`).
pub fn licence(v: Option<&Value>) -> Licence {
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

/// Le fichier `dossier/nom` est-il dans la liste des fichiers qui manquent à un workflow ?
fn dans_les_manquants(manquants: &[Value], dossier: &str, nom: &str) -> bool {
    manquants
        .iter()
        .any(|m| texte(m.get("dossier")) == dossier && texte(m.get("nom")) == nom)
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
                        manque: dans_les_manquants(&manquants, &dossier, &nom),
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

    /// Pose un fichier (un octet) comme s'il était installé.
    fn poser(modeles: &Path, dossier: &str, nom: &str) {
        let chemin = modeles::destination(modeles, dossier, nom).unwrap();
        fs::create_dir_all(chemin.parent().unwrap()).unwrap();
        fs::write(chemin, b"x").unwrap();
    }

    fn est_sha256(empreinte: Option<&str>) -> bool {
        empreinte.is_some_and(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
    }

    fn ids_des_workflows() -> Vec<String> {
        let temp = tempfile::tempdir().unwrap();
        let tous = workflows(temp.path());
        tous.into_iter().map(|w| w.id).collect()
    }

    fn etat(modeles: &Path, id: &str) -> EtatWorkflow {
        let tous = workflows(modeles);
        tous.into_iter().find(|w| w.id == id).unwrap()
    }

    /// Les fichiers d'un workflow qui sont un checkpoint de famille.
    fn checkpoints(wf: &EtatWorkflow) -> Vec<Fichier> {
        let mut liste = Vec::new();
        for f in &wf.fichiers {
            if modeles::est_checkpoint(&f.dossier) {
                liste.push(f.clone());
            }
        }
        liste
    }

    /// Les fichiers d'un workflow qui ne sont pas un checkpoint de famille.
    fn hors_famille(wf: &EtatWorkflow) -> Vec<Fichier> {
        let mut liste = Vec::new();
        for f in &wf.fichiers {
            if !modeles::est_checkpoint(&f.dossier) {
                liste.push(f.clone());
            }
        }
        liste
    }

    #[test]
    fn chaque_fichier_d_un_workflow_se_telecharge() {
        let temp = tempfile::tempdir().unwrap();
        let dossiers = modeles::dossiers();
        for wf in workflows(temp.path()) {
            assert!(!wf.fichiers.is_empty(), "{} sans fichier", wf.id);
            assert!(!checkpoints(&wf).is_empty(), "{} sans checkpoint", wf.id);
            let a_faire = manquants(temp.path(), &wf.id).unwrap();
            assert_eq!(a_faire.len(), wf.fichiers.len(), "{}", wf.id);
            for (fiche, dossier) in &a_faire {
                assert!(dossiers.contains(dossier), "{} : dossier {dossier}", wf.id);
                assert!(fiche.url.starts_with("https://"), "{} : adresse", fiche.nom);
                assert!(fiche.taille > 0, "{} : taille", fiche.nom);
                assert!(est_sha256(fiche.sha256.as_deref()), "{} : sha256", fiche.nom);
            }
        }
    }

    #[test]
    fn un_workflow_vide_est_incomplet_et_tout_manque() {
        let temp = tempfile::tempdir().unwrap();
        for wf in workflows(temp.path()) {
            let total: u64 = wf.fichiers.iter().map(|f| f.taille).sum();
            assert!(!wf.pret, "{} prêt sans rien", wf.id);
            assert_eq!(wf.a_telecharger, total, "{}", wf.id);
            assert!(wf.fichiers.iter().all(|f| !f.present && f.manque), "{}", wf.id);
        }
    }

    #[test]
    fn un_workflow_complet_est_pret_et_ne_demande_rien() {
        for id in ids_des_workflows() {
            let temp = tempfile::tempdir().unwrap();
            let vide = etat(temp.path(), &id);
            for f in &vide.fichiers {
                poser(temp.path(), &f.dossier, &f.nom);
            }
            let complet = etat(temp.path(), &id);
            assert!(complet.pret, "{id}");
            assert_eq!(complet.a_telecharger, 0, "{id}");
            assert!(complet.fichiers.iter().all(|f| f.present && !f.manque), "{id}");
            assert!(manquants(temp.path(), &id).unwrap().is_empty(), "{id}");
        }
    }

    #[test]
    fn un_fichier_qui_manque_rend_le_workflow_incomplet() {
        for id in ids_des_workflows() {
            let vide = etat(tempfile::tempdir().unwrap().path(), &id);
            let a_tour_de_role = hors_famille(&vide);
            assert!(!a_tour_de_role.is_empty(), "{id}");
            for absent in a_tour_de_role {
                let temp = tempfile::tempdir().unwrap();
                for f in &vide.fichiers {
                    if f.nom != absent.nom || f.dossier != absent.dossier {
                        poser(temp.path(), &f.dossier, &f.nom);
                    }
                }
                let wf = etat(temp.path(), &id);
                assert!(!wf.pret, "{id} sans {}", absent.nom);
                assert_eq!(wf.a_telecharger, absent.taille, "{id} sans {}", absent.nom);
                assert_eq!(wf.fichiers.iter().filter(|f| f.manque).count(), 1, "{id}");
                let a_faire = manquants(temp.path(), &id).unwrap();
                assert_eq!(a_faire.len(), 1, "{id} sans {}", absent.nom);
                assert_eq!(a_faire[0].0.nom, absent.nom, "{id}");
            }
        }
    }

    #[test]
    fn le_checkpoint_d_origine_n_est_pas_exige_si_la_famille_en_a_un_autre() {
        for id in ids_des_workflows() {
            let temp = tempfile::tempdir().unwrap();
            let modeles = temp.path();
            let vide = etat(modeles, &id);
            let famille = checkpoints(&vide);
            for f in hors_famille(&vide) {
                poser(modeles, &f.dossier, &f.nom);
            }
            // Seul le checkpoint manque : il se télécharge.
            let sans = etat(modeles, &id);
            let attendu: u64 = famille.iter().map(|f| f.taille).sum();
            assert!(!sans.pret, "{id}");
            assert_eq!(sans.a_telecharger, attendu, "{id}");
            for c in &famille {
                let manque = sans.fichiers.iter().any(|f| f.nom == c.nom && f.manque);
                assert!(manque, "{id} : {} devrait manquer", c.nom);
            }
            // Un autre checkpoint de la famille, déposé à la main : l'origine ne manque plus.
            poser(modeles, &famille[0].dossier, "autre.safetensors");
            let pret = etat(modeles, &id);
            assert!(pret.pret, "{id}");
            assert_eq!(pret.a_telecharger, 0, "{id}");
            assert!(manquants(modeles, &id).unwrap().is_empty(), "{id}");
            // Le fichier d'origine reste absent du disque, mais il ne manque pas.
            assert!(pret.fichiers.iter().any(|f| !f.present), "{id}");
            assert!(pret.fichiers.iter().all(|f| !f.manque), "{id}");
        }
    }

    #[test]
    fn un_autre_checkpoint_ne_cache_pas_un_fichier_qui_manque() {
        for id in ids_des_workflows() {
            let temp = tempfile::tempdir().unwrap();
            let modeles = temp.path();
            let vide = etat(modeles, &id);
            let autres = hors_famille(&vide);
            let absent = &autres[0];
            for f in &autres[1..] {
                poser(modeles, &f.dossier, &f.nom);
            }
            poser(modeles, &checkpoints(&vide)[0].dossier, "autre.safetensors");
            let wf = etat(modeles, &id);
            assert!(!wf.pret, "{id}");
            assert_eq!(wf.a_telecharger, absent.taille, "{id}");
            let a_faire = manquants(modeles, &id).unwrap();
            assert_eq!(a_faire.len(), 1, "{id}");
            assert_eq!(a_faire[0].0.nom, absent.nom, "{id}");
            assert_eq!(wf.fichiers.iter().filter(|f| f.manque).count(), 1, "{id}");
        }
    }

    #[test]
    fn un_telechargement_en_cours_ne_compte_pas_comme_present() {
        for id in ids_des_workflows() {
            let temp = tempfile::tempdir().unwrap();
            let vide = etat(temp.path(), &id);
            for f in &vide.fichiers {
                poser(temp.path(), &f.dossier, &format!("{}.part", f.nom));
            }
            let wf = etat(temp.path(), &id);
            assert!(!wf.pret, "{id}");
            assert_eq!(wf.a_telecharger, vide.a_telecharger, "{id}");
            assert!(wf.fichiers.iter().all(|f| !f.present && f.manque), "{id}");
        }
    }

    #[test]
    fn chaque_llm_du_catalogue_se_telecharge() {
        let temp = tempfile::tempdir().unwrap();
        let catalogue = llm(temp.path());
        assert!(catalogue.iter().any(|l| l.specialiste.is_none()));
        assert!(catalogue.iter().any(|l| l.specialiste.is_some()));
        for l in &catalogue {
            assert!(!l.installe, "{} installé sans rien", l.fichier);
            assert_eq!(l.a_telecharger, l.taille, "{}", l.fichier);
            let a_faire = llm_manquant(temp.path(), &l.fichier).unwrap();
            let total: u64 = a_faire.iter().map(|(f, _)| f.taille).sum();
            assert_eq!(total, l.taille, "{}", l.fichier);
            for (fiche, dossier) in &a_faire {
                assert_eq!(dossier, DOSSIER_LLM);
                assert!(fiche.url.starts_with("https://"), "{} : adresse", fiche.nom);
                assert!(fiche.taille > 0, "{} : taille", fiche.nom);
                assert!(est_sha256(fiche.sha256.as_deref()), "{} : sha256", fiche.nom);
            }
        }
    }

    #[test]
    fn un_llm_n_est_installe_qu_avec_tout_ce_qui_l_accompagne() {
        let vide = tempfile::tempdir().unwrap();
        for l in &llm(vide.path()) {
            let temp = tempfile::tempdir().unwrap();
            let modeles = temp.path();
            let a_faire = llm_manquant(modeles, &l.fichier).unwrap();
            if a_faire.len() > 1 {
                // Le GGUF seul, sans son projecteur d'images ni sa consigne : il reste à installer.
                poser(modeles, DOSSIER_LLM, &l.fichier);
                let tous = llm(modeles);
                let courant = tous.iter().find(|e| e.fichier == l.fichier).unwrap();
                let reste = llm_manquant(modeles, &l.fichier).unwrap();
                let total: u64 = reste.iter().map(|(f, _)| f.taille).sum();
                assert!(!courant.installe, "{}", l.fichier);
                assert_eq!(reste.len(), a_faire.len() - 1, "{}", l.fichier);
                assert_eq!(courant.a_telecharger, total, "{}", l.fichier);
                assert!(courant.a_telecharger > 0, "{}", l.fichier);
            }
            for (fiche, _) in &llm_manquant(modeles, &l.fichier).unwrap() {
                poser(modeles, DOSSIER_LLM, &fiche.nom);
            }
            let tous = llm(modeles);
            let complet = tous.iter().find(|e| e.fichier == l.fichier).unwrap();
            assert!(complet.installe, "{}", l.fichier);
            assert_eq!(complet.a_telecharger, 0, "{}", l.fichier);
            let plus_rien = llm_manquant(modeles, &l.fichier).unwrap();
            assert!(plus_rien.is_empty(), "{}", l.fichier);
        }
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
