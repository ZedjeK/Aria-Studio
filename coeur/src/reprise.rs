//! Reprise d'une v2 installée, pour un essai de la v3 en mode portable : au premier lancement, si l'essai n'a encore
//! aucun emplacement réglé, la v3 cherche la v2 au chemin standard (`%USERPROFILE%\.Aria-Studio`) et pointe vers ses
//! modèles, son ComfyUI et ses llama-server, sans rien copier.
//!
//! La v2 n'est que lue : ses réglages (son `emplacements.modeles` ou `emplacements.comfyui` s'ils sont réglés), la
//! présence de ses dossiers. Tout ce qui s'écrit va dans les réglages de l'essai (`emplacements` : `modeles`, `comfyui`,
//! `llama`, `llama-prism`, et la marque `reprise_v2`). La marque fait qu'une reprise n'a lieu qu'une fois : un
//! emplacement changé ou une reprise annulée ensuite n'est jamais réécrit.

use std::path::{Path, PathBuf};

use chrono::Local;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::catalogue::Catalogue;
use crate::{Dossier, reglages};

/// La section des réglages qui porte les emplacements.
pub const SECTION: &str = "emplacements";
/// La marque d'une reprise faite, dans la section `emplacements` de l'essai.
pub const MARQUE: &str = "reprise_v2";
/// Ce que la reprise sait régler, dans l'ordre où on le montre.
pub const CLES: [&str; 4] = ["modeles", "comfyui", "llama", "llama-prism"];
/// Les sous-dossiers qui font reconnaître le dossier des modèles d'une v2.
const FAMILLES: [&str; 3] = ["image", "video", "llm"];

/// Un dossier de la v2 repris.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Element {
    /// `modeles`, `comfyui`, `llama` ou `llama-prism`.
    pub cle: String,
    pub nom: String,
    pub chemin: String,
}

/// Ce qui a été repris, pour le Journal, le bandeau et Réglages → Emplacements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bilan {
    /// Le dossier de la v2.
    pub depuis: String,
    pub date: String,
    pub repris: Vec<Element>,
    /// Le bandeau du premier lancement a été montré.
    pub annonce: bool,
    /// Ceux qui valent encore (pas changés ni retirés depuis).
    pub en_vigueur: Vec<String>,
}

/// Ce que la reprise a fait à ce lancement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// Rien à faire : pas de v2, rien de reprenable, déjà réglé à la main, ou déjà repris une fois.
    Rien,
    /// Une v2 est là, mais elle tourne (ses ports sont pris) : rien n'est repris, on réessaiera au prochain lancement.
    V2Lancee,
    Reprise(Bilan),
}

fn nom_de(cle: &str) -> &'static str {
    match cle {
        "modeles" => "Modèles (image, vidéo, LLM)",
        "comfyui" => "ComfyUI",
        "llama" => "llama.cpp",
        _ => "llama.cpp PrismML",
    }
}

/// Un chemin complet lu dans une section, s'il y en a un.
fn chemin_de(section: &Map<String, Value>, cle: &str) -> Option<PathBuf> {
    let texte = section.get(cle)?.as_str()?.trim();
    let chemin = PathBuf::from(texte);
    (!texte.is_empty() && chemin.is_absolute()).then_some(chemin)
}

/// Les emplacements réglés dans le `reglages.json` de la v2, lus sans passer par le module des réglages (qui pourrait
/// sauvegarder ou réécrire le fichier) : rien n'est jamais écrit chez la v2.
fn emplacements_v2(v2: &Path) -> Map<String, Value> {
    std::fs::read_to_string(v2.join("reglages.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| v.get(SECTION)?.as_object().cloned())
        .unwrap_or_default()
}

/// Le dossier des modèles d'une v2 : celui de ses réglages, sinon `modeles\` ; reconnu à ses familles.
fn modeles_v2(v2: &Path, reglages_v2: &Map<String, Value>) -> Option<PathBuf> {
    let dossier = chemin_de(reglages_v2, "modeles").unwrap_or_else(|| v2.join("modeles"));
    FAMILLES
        .iter()
        .any(|f| dossier.join(f).is_dir())
        .then_some(dossier)
}

/// Un moteur de la v2 installé : son dossier, reconnu à la preuve du catalogue. ComfyUI : celui de ses réglages s'il
/// en a choisi un, avec `python_embeded` en plus.
fn moteur_v2(
    v2: &Path,
    reglages_v2: &Map<String, Value>,
    catalogue: &Catalogue,
    id: &str,
) -> Option<PathBuf> {
    let moteur = catalogue.moteur(id)?;
    let dossier = if id == "comfyui" {
        chemin_de(reglages_v2, "comfyui").unwrap_or_else(|| v2.join("moteurs").join(&moteur.dossier))
    } else {
        v2.join("moteurs").join(&moteur.dossier)
    };
    let pret = dossier.join(&moteur.preuve).is_file()
        && (id != "comfyui" || dossier.join("python_embeded").is_dir());
    pret.then_some(dossier)
}

/// Ce qu'une v2 installée en `v2` permet de reprendre, en lecture seule. Un moteur que l'essai a déjà dans
/// `moteurs_essai` (installé ou copié à la main) n'est pas repris : le sien l'emporte.
pub fn trouver(v2: &Path, catalogue: &Catalogue, moteurs_essai: &Path) -> Vec<Element> {
    if !v2.is_dir() {
        return Vec::new();
    }
    let reglages_v2 = emplacements_v2(v2);
    let mut trouves = Vec::new();
    if let Some(d) = modeles_v2(v2, &reglages_v2) {
        trouves.push(("modeles", d));
    }
    for id in ["comfyui", "llama", "llama-prism"] {
        let deja = catalogue
            .moteur(id)
            .is_some_and(|m| moteurs_essai.join(&m.dossier).join(&m.preuve).is_file());
        if !deja && let Some(d) = moteur_v2(v2, &reglages_v2, catalogue, id) {
            trouves.push((id, d));
        }
    }
    trouves
        .into_iter()
        .map(|(cle, chemin)| Element {
            cle: cle.to_owned(),
            nom: nom_de(cle).to_owned(),
            chemin: chemin.display().to_string(),
        })
        .collect()
}

/// Au lancement d'un essai portable : reprend la v2 si l'essai n'a encore rien réglé et n'a jamais rien repris.
/// `v2_lancee` n'est appelée que s'il y a quelque chose à reprendre.
///
/// # Errors
/// Les réglages de l'essai ne s'écrivent pas.
pub fn reprendre(
    essai: &Dossier,
    v2: &Path,
    catalogue: &Catalogue,
    moteurs_essai: &Path,
    v2_lancee: impl FnOnce() -> bool,
) -> Result<Issue, String> {
    let section = reglages::section(essai, SECTION).unwrap_or_default();
    let deja_regle = CLES.iter().any(|c| chemin_de(&section, c).is_some());
    if section.contains_key(MARQUE) || deja_regle {
        return Ok(Issue::Rien);
    }
    let repris = trouver(v2, catalogue, moteurs_essai);
    if repris.is_empty() {
        return Ok(Issue::Rien);
    }
    if v2_lancee() {
        return Ok(Issue::V2Lancee);
    }
    let date = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let depuis = v2.display().to_string();
    let marque = json!({
        "depuis": depuis,
        "date": date,
        "repris": repris.iter().map(|e| json!({"cle": e.cle, "chemin": e.chemin})).collect::<Vec<_>>(),
        "annonce": false,
    });
    reglages::modifier_section(essai, SECTION, |s| {
        for e in &repris {
            s.insert(e.cle.clone(), json!(e.chemin));
        }
        s.insert(MARQUE.to_owned(), marque);
    })
    .map_err(|e| e.to_string())?;
    let en_vigueur = repris.iter().map(|e| e.cle.clone()).collect();
    Ok(Issue::Reprise(Bilan {
        depuis,
        date,
        repris,
        annonce: false,
        en_vigueur,
    }))
}

/// La reprise faite dans cet essai, s'il y en a eu une.
pub fn bilan(essai: &Dossier) -> Option<Bilan> {
    let section = reglages::section(essai, SECTION)?;
    let marque = section.get(MARQUE)?.as_object()?;
    let texte = |c: &str| {
        marque
            .get(c)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let repris: Vec<Element> = marque
        .get("repris")
        .and_then(Value::as_array)
        .map(|liste| {
            liste
                .iter()
                .filter_map(|e| {
                    let cle = e.get("cle")?.as_str()?;
                    Some(Element {
                        cle: cle.to_owned(),
                        nom: nom_de(cle).to_owned(),
                        chemin: e.get("chemin")?.as_str()?.to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let en_vigueur = repris
        .iter()
        .filter(|e| section.get(&e.cle).and_then(Value::as_str) == Some(e.chemin.as_str()))
        .map(|e| e.cle.clone())
        .collect();
    Some(Bilan {
        depuis: texte("depuis"),
        date: texte("date"),
        annonce: marque.get("annonce").and_then(Value::as_bool).unwrap_or(true),
        repris,
        en_vigueur,
    })
}

/// Le bandeau du premier lancement a été vu : il ne revient plus.
///
/// # Errors
/// Les réglages de l'essai ne s'écrivent pas.
pub fn marquer_annoncee(essai: &Dossier) -> Result<(), String> {
    reglages::modifier_section(essai, SECTION, |s| {
        if let Some(m) = s.get_mut(MARQUE).and_then(Value::as_object_mut) {
            m.insert("annonce".to_owned(), json!(true));
        }
    })
    .map_err(|e| e.to_string())
}

/// Ne plus utiliser la v2 : les emplacements repris et encore en vigueur sont retirés (l'essai revient à ses propres
/// dossiers au prochain lancement). La marque reste : la reprise ne se refait pas toute seule.
///
/// # Errors
/// Les réglages de l'essai ne s'écrivent pas.
pub fn annuler(essai: &Dossier) -> Result<Vec<String>, String> {
    let Some(b) = bilan(essai) else {
        return Ok(Vec::new());
    };
    let retires = b.en_vigueur.clone();
    reglages::modifier_section(essai, SECTION, |s| {
        for cle in &retires {
            s.remove(cle);
        }
        if let Some(m) = s.get_mut(MARQUE).and_then(Value::as_object_mut) {
            m.insert("annonce".to_owned(), json!(true));
            m.insert(
                "annulee".to_owned(),
                json!(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(retires)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    /// Une v2 installée, imitée : modèles, ComfyUI portable, llama.cpp ; ses réglages et ses données.
    fn fausse_v2(racine: &Path) {
        fs::create_dir_all(racine.join("modeles").join("image").join("Krea")).unwrap();
        fs::write(
            racine
                .join("modeles")
                .join("image")
                .join("Krea")
                .join("krea.safetensors"),
            b"poids",
        )
        .unwrap();
        fs::create_dir_all(racine.join("modeles").join("llm")).unwrap();
        let comfy = racine.join("moteurs").join("comfyui");
        fs::create_dir_all(comfy.join("python_embeded")).unwrap();
        fs::create_dir_all(comfy.join("ComfyUI")).unwrap();
        fs::write(comfy.join("ComfyUI").join("main.py"), b"print()").unwrap();
        let llama = racine.join("moteurs").join("llama");
        fs::create_dir_all(&llama).unwrap();
        fs::write(llama.join("llama-server.exe"), b"exe").unwrap();
        fs::write(racine.join("reglages.json"), r#"{"favoris": {"generation": {}}}"#).unwrap();
        fs::create_dir_all(racine.join("data").join("creations")).unwrap();
        fs::write(racine.join("data").join("creations").join("a.json"), b"{}").unwrap();
        fs::write(racine.join("journal.json"), b"[]").unwrap();
    }

    /// Tout ce que contient un dossier : chemin relatif, contenu des fichiers (rien pour un dossier).
    fn photo(racine: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
        let mut sortie = BTreeMap::new();
        let mut a_voir = vec![racine.to_path_buf()];
        while let Some(d) = a_voir.pop() {
            for e in fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                let rel = p.strip_prefix(racine).unwrap().display().to_string();
                if p.is_dir() {
                    sortie.insert(rel, None);
                    a_voir.push(p);
                } else {
                    sortie.insert(rel, Some(fs::read(&p).unwrap()));
                }
            }
        }
        sortie
    }

    struct Lieux {
        _temp: tempfile::TempDir,
        v2: PathBuf,
        essai: Dossier,
        services: PathBuf,
    }

    fn lieux() -> Lieux {
        let temp = tempfile::tempdir().unwrap();
        let v2 = temp.path().join(".Aria-Studio");
        fausse_v2(&v2);
        let racine_essai = temp.path().join("Essai");
        fs::create_dir_all(racine_essai.join("data")).unwrap();
        let essai = Dossier::a(&racine_essai);
        Lieux {
            services: racine_essai.join("services"),
            _temp: temp,
            v2,
            essai,
        }
    }

    #[test]
    fn reprend_une_v2_sans_jamais_y_ecrire() {
        let l = lieux();
        let cat = Catalogue::embarque().unwrap();
        let avant = photo(&l.v2);
        let issue = reprendre(&l.essai, &l.v2, &cat, &l.services, || false).unwrap();
        let Issue::Reprise(b) = issue else {
            panic!("reprise attendue : {issue:?}")
        };
        let cles: Vec<&str> = b.repris.iter().map(|e| e.cle.as_str()).collect();
        // llama-prism n'est pas installé dans cette v2 : il n'est pas repris.
        assert_eq!(cles, ["modeles", "comfyui", "llama"]);
        assert_eq!(photo(&l.v2), avant, "la reprise a écrit dans la v2");
        let s = reglages::section(&l.essai, SECTION).unwrap();
        assert_eq!(s["modeles"], json!(l.v2.join("modeles").display().to_string()));
        assert_eq!(
            s["comfyui"],
            json!(l.v2.join("moteurs").join("comfyui").display().to_string())
        );
        assert_eq!(
            s["llama"],
            json!(l.v2.join("moteurs").join("llama").display().to_string())
        );
        // Lire le bilan, l'annoncer, l'annuler : toujours rien chez la v2.
        let b = bilan(&l.essai).unwrap();
        assert!(!b.annonce);
        assert_eq!(b.en_vigueur, ["modeles", "comfyui", "llama"]);
        marquer_annoncee(&l.essai).unwrap();
        assert!(bilan(&l.essai).unwrap().annonce);
        assert_eq!(photo(&l.v2), avant);
    }

    #[test]
    fn une_seule_fois_et_jamais_par_dessus_un_choix() {
        let l = lieux();
        let cat = Catalogue::embarque().unwrap();
        // Un emplacement déjà réglé à la main : rien n'est repris.
        reglages::modifier_section(&l.essai, SECTION, |s| {
            s.insert(
                "modeles".to_owned(),
                json!(l.v2.join("ailleurs").display().to_string()),
            );
        })
        .unwrap();
        assert_eq!(
            reprendre(&l.essai, &l.v2, &cat, &l.services, || false).unwrap(),
            Issue::Rien
        );
        // Une reprise annulée ne se refait pas au lancement suivant.
        let l = lieux();
        assert!(matches!(
            reprendre(&l.essai, &l.v2, &cat, &l.services, || false).unwrap(),
            Issue::Reprise(_)
        ));
        reglages::modifier_section(&l.essai, SECTION, |s| {
            s.insert("comfyui".to_owned(), json!("D:\\ComfyUI"));
        })
        .unwrap();
        assert_eq!(annuler(&l.essai).unwrap(), ["modeles", "llama"]);
        let s = reglages::section(&l.essai, SECTION).unwrap();
        assert!(!s.contains_key("modeles") && !s.contains_key("llama"));
        assert_eq!(s["comfyui"], json!("D:\\ComfyUI"), "un choix fait ensuite reste");
        reglages::modifier_section(&l.essai, SECTION, |s| {
            s.remove("comfyui");
        })
        .unwrap();
        assert_eq!(
            reprendre(&l.essai, &l.v2, &cat, &l.services, || false).unwrap(),
            Issue::Rien
        );
    }

    #[test]
    fn rien_sans_v2_et_rien_tant_qu_elle_tourne() {
        let l = lieux();
        let cat = Catalogue::embarque().unwrap();
        let ailleurs = l.v2.with_file_name("Pas-de-v2");
        assert_eq!(
            reprendre(&l.essai, &ailleurs, &cat, &l.services, || false).unwrap(),
            Issue::Rien
        );
        assert_eq!(
            reprendre(&l.essai, &l.v2, &cat, &l.services, || true).unwrap(),
            Issue::V2Lancee
        );
        // Rien n'a été marqué : la reprise se fera au prochain lancement, la v2 fermée.
        assert!(bilan(&l.essai).is_none());
        assert!(matches!(
            reprendre(&l.essai, &l.v2, &cat, &l.services, || false).unwrap(),
            Issue::Reprise(_)
        ));
    }

    #[test]
    fn le_moteur_de_l_essai_et_les_choix_de_la_v2_l_emportent() {
        let l = lieux();
        let cat = Catalogue::embarque().unwrap();
        // L'essai a déjà son llama.cpp : celui de la v2 n'est pas repris.
        fs::create_dir_all(l.services.join("llama")).unwrap();
        fs::write(l.services.join("llama").join("llama-server.exe"), b"exe").unwrap();
        // La v2 avait choisi un autre dossier des modèles.
        let autres = l.v2.with_file_name("Mes-modeles");
        fs::create_dir_all(autres.join("video")).unwrap();
        fs::write(
            l.v2.join("reglages.json"),
            serde_json::to_string(&json!({"emplacements": {"modeles": autres.display().to_string()}}))
                .unwrap(),
        )
        .unwrap();
        let trouves = trouver(&l.v2, &cat, &l.services);
        let cles: Vec<&str> = trouves.iter().map(|e| e.cle.as_str()).collect();
        assert_eq!(cles, ["modeles", "comfyui"]);
        assert_eq!(trouves[0].chemin, autres.display().to_string());
    }
}
