//! Les LLM qui réécrivent les prompts, comme en v2 (`app/modeles.py`, `app/reglages.py` section LLM,
//! `app/orchestrateur.py::lancement_llm`) : lesquels sont installés, lequel sert par défaut, leurs réglages, leur
//! projecteur d'images, leur moteur (llama.cpp officiel ou PrismML) et de quoi lancer llama-server.
//!
//! Les réglages d'un GGUF sont dans `reglages.json`, section `llm.<fichier>`, comme en v2 ; une clé absente ou d'un
//! mauvais type prend la valeur d'origine.

use std::fs;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::llama::{CONTEXTE_REFLEXION, Lancement};
use crate::modeles::{self, DOSSIER_LLM};
use crate::{Dossier, fiches, gguf, reglages};

/// Le LLM par défaut de la v2 (`config.yaml`, `llama.modele`).
pub const LLM_ORIGINE: &str = "Ternary-Bonsai-2-27B-PTQ1_0.gguf";
/// Les LLM proposés au choix, gagnants du Casting LLM (`config.yaml`, `llama.modeles_au_choix`) : fichier et nom.
pub const FAVORIS: [(&str, &str); 1] = [(LLM_ORIGINE, "Bonsai 2 27B")];
/// Le moteur llama.cpp officiel (catalogue/moteurs.yaml).
pub const MOTEUR_OFFICIEL: &str = "llama";
/// Le fork PrismML, pour les formats ternaires (catalogue/moteurs.yaml).
pub const MOTEUR_PRISM: &str = "llama-prism";
/// Le choix « Automatique » du moteur et de la vision.
pub const AUTO: &str = "auto";
/// La vision coupée à la main.
pub const SANS_VISION: &str = "aucune";
/// L'exécutable de llama-server, dans le dossier de son moteur.
pub const EXECUTABLE: &str = "llama-server.exe";
/// Le contexte d'origine (`config.yaml`, `llama.contexte`).
pub const CONTEXTE_DEFAUT: u32 = 4096;
/// La réflexion d'origine (`config.yaml`, `llama.reflexion`).
pub const REFLEXION_DEFAUT: bool = true;
/// Les GGUF envoyés d'office à PrismML (`config.yaml`, `llama.serveurs_specifiques`).
const SERVEURS_SPECIFIQUES: [(&str, &str); 1] = [(LLM_ORIGINE, MOTEUR_PRISM)];
const SECTION: &str = "llm";

/// Un projecteur d'images : `<famille>-mmproj-….gguf` ou `<famille>.mmproj-…`.
fn motif_mmproj() -> Option<Regex> {
    RegexBuilder::new(r"[-._]mmproj")
        .case_insensitive(true)
        .build()
        .ok()
}

fn est_projecteur(fichier: &str) -> bool {
    fichier.to_lowercase().contains("mmproj")
}

/// Les GGUF de `modeles\llm`, sans les projecteurs d'images : ce ne sont pas des LLM.
pub fn disponibles(modeles: &Path) -> Vec<String> {
    modeles::fichiers(modeles, DOSSIER_LLM, &["gguf"])
        .into_iter()
        .filter(|f| !est_projecteur(f))
        .collect()
}

/// Le LLM retenu par l'utilisateur (`favoris.llama.modele`, écrit par un Casting LLM, comme en v2) ; rien s'il est
/// vide (le témoin gagnant d'un Casting LLM).
fn favori(dossier: &Dossier) -> Option<String> {
    reglages::section(dossier, "favoris")
        .and_then(|f| f.get("llama")?.get("modele")?.as_str().map(str::to_owned))
        .filter(|m| !m.is_empty())
}

/// Le LLM par défaut, comme `llm_defaut` de la v2 : celui retenu par un Casting LLM, sinon celui de la v2, s'il est
/// installé ; sinon le premier favori installé, sinon le premier GGUF installé qui n'est pas un spécialiste. Rien
/// d'installé : le retenu (ou celui de la v2), et le lancement dira qu'il manque.
pub fn defaut(dossier: &Dossier, modeles: &Path) -> String {
    let presents = disponibles(modeles);
    let premier = favori(dossier).unwrap_or_else(|| LLM_ORIGINE.to_owned());
    std::iter::once(premier.clone())
        .chain(FAVORIS.iter().map(|(f, _)| (*f).to_owned()))
        .chain(
            presents
                .iter()
                .filter(|f| fiches::specialiste(f).is_none())
                .cloned(),
        )
        .find(|f| presents.contains(f))
        .unwrap_or(premier)
}

/// Le GGUF demandé, ou le LLM par défaut.
pub fn nom(dossier: &Dossier, modeles: &Path, modele: Option<&str>) -> String {
    modele
        .filter(|m| !m.is_empty())
        .map_or_else(|| defaut(dossier, modeles), str::to_owned)
}

/// Le LLM choisi pour ce workflow (`reglages.json`, `divers.llm_par_workflow.<workflow>`, comme en v2) s'il est
/// installé, sinon le LLM par défaut.
pub fn de_workflow(dossier: &Dossier, modeles: &Path, workflow: &str) -> String {
    reglages::section(dossier, "divers")
        .and_then(|d| {
            d.get("llm_par_workflow")?
                .get(workflow)?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|choisi| disponibles(modeles).contains(choisi))
        .unwrap_or_else(|| defaut(dossier, modeles))
}

/// Un nom de fichier seul, qui ne sort pas de `modeles\llm`.
///
/// # Errors
/// Le nom est vide ou contient un chemin.
pub fn nom_de_fichier(nom: &str) -> Result<&str, String> {
    let interdit =
        nom.is_empty() || nom.contains(['/', '\\', ':']) || nom == "." || nom == ".." || nom.trim() != nom;
    if interdit {
        Err(format!("Nom de GGUF invalide : {nom}"))
    } else {
        Ok(nom)
    }
}

/// Les réglages d'un GGUF, tels que l'onglet Réglages de la v2 les écrit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValeursLlm {
    /// Idée → prompt quand la réflexion est coupée, et les tours de la conversation.
    pub temperature_reecriture: f64,
    pub temperature_retouche: f64,
    pub temperature_traduction: f64,
    pub top_p: f64,
    pub top_k: i64,
    pub min_p: f64,
    pub presence_penalty: f64,
    /// Idée → prompt et découpage, quand la réflexion est active.
    pub temperature_reflexion: f64,
    pub top_p_reflexion: f64,
    pub top_k_reflexion: i64,
    pub min_p_reflexion: f64,
    pub presence_penalty_reflexion: f64,
    pub max_tokens: u32,
    pub contexte: u32,
    pub reflexion: bool,
    /// `auto`, `llama` ou `llama-prism`.
    pub moteur: String,
    /// `auto`, `aucune` ou le fichier d'un projecteur.
    pub vision: String,
}

impl Default for ValeursLlm {
    /// Les valeurs d'origine de la v2 : celles de llama-server pour l'échantillonnage.
    fn default() -> Self {
        Self {
            temperature_reecriture: 0.7,
            temperature_retouche: 0.2,
            temperature_traduction: 0.2,
            top_p: 0.95,
            top_k: 40,
            min_p: 0.05,
            presence_penalty: 0.0,
            temperature_reflexion: 0.7,
            top_p_reflexion: 0.95,
            top_k_reflexion: 40,
            min_p_reflexion: 0.05,
            presence_penalty_reflexion: 0.0,
            max_tokens: 512,
            contexte: CONTEXTE_DEFAUT,
            reflexion: REFLEXION_DEFAUT,
            moteur: AUTO.to_owned(),
            vision: AUTO.to_owned(),
        }
    }
}

fn reel(section: &Map<String, Value>, cle: &str, defaut: f64, min: f64, max: f64) -> f64 {
    section
        .get(cle)
        .filter(|v| !v.is_boolean())
        .and_then(Value::as_f64)
        .filter(|v| (min..=max).contains(v))
        .unwrap_or(defaut)
}

fn entier(section: &Map<String, Value>, cle: &str, defaut: i64, min: i64, max: i64) -> i64 {
    section
        .get(cle)
        .and_then(Value::as_i64)
        .filter(|v| (min..=max).contains(v))
        .unwrap_or(defaut)
}

fn entier_u32(section: &Map<String, Value>, cle: &str, defaut: u32, min: u32, max: u32) -> u32 {
    section
        .get(cle)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| (min..=max).contains(v))
        .unwrap_or(defaut)
}

impl ValeursLlm {
    /// Les réglages lus dans une section `llm.<fichier>` ; hors bornes ou d'un mauvais type : la valeur d'origine.
    pub fn depuis(section: &Map<String, Value>) -> Self {
        let d = Self::default();
        let temperature = |cle: &str, defaut: f64| reel(section, cle, defaut, 0.0, 2.0);
        let moteur = section
            .get("moteur")
            .and_then(Value::as_str)
            .filter(|m| [AUTO, MOTEUR_OFFICIEL, MOTEUR_PRISM].contains(m))
            .map_or(d.moteur.clone(), str::to_owned);
        let vision = section
            .get("vision")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map_or(d.vision.clone(), str::to_owned);
        Self {
            temperature_reecriture: temperature("temperature_reecriture", d.temperature_reecriture),
            temperature_retouche: temperature("temperature_retouche", d.temperature_retouche),
            temperature_traduction: temperature("temperature_traduction", d.temperature_traduction),
            top_p: reel(section, "top_p", d.top_p, 0.0, 1.0),
            top_k: entier(section, "top_k", d.top_k, 0, 200),
            min_p: reel(section, "min_p", d.min_p, 0.0, 1.0),
            presence_penalty: reel(section, "presence_penalty", d.presence_penalty, -2.0, 2.0),
            temperature_reflexion: temperature("temperature_reflexion", d.temperature_reflexion),
            top_p_reflexion: reel(section, "top_p_reflexion", d.top_p_reflexion, 0.0, 1.0),
            top_k_reflexion: entier(section, "top_k_reflexion", d.top_k_reflexion, 0, 200),
            min_p_reflexion: reel(section, "min_p_reflexion", d.min_p_reflexion, 0.0, 1.0),
            presence_penalty_reflexion: reel(
                section,
                "presence_penalty_reflexion",
                d.presence_penalty_reflexion,
                -2.0,
                2.0,
            ),
            max_tokens: entier_u32(section, "max_tokens", d.max_tokens, 64, 4096),
            contexte: entier_u32(section, "contexte", d.contexte, 1024, 262_144),
            reflexion: section
                .get("reflexion")
                .and_then(Value::as_bool)
                .unwrap_or(d.reflexion),
            moteur,
            vision,
        }
    }

    /// L'échantillonnage d'une demande qui ne réfléchit pas (traductions, tours de la conversation).
    pub fn echantillonnage(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("top_p".to_owned(), json!(self.top_p));
        m.insert("top_k".to_owned(), json!(self.top_k));
        m.insert("min_p".to_owned(), json!(self.min_p));
        m.insert("presence_penalty".to_owned(), json!(self.presence_penalty));
        m
    }

    /// Température et échantillonnage de l'écriture d'un prompt : le jeu « avec réflexion » quand la demande
    /// réfléchit vraiment, sinon le jeu « sans réflexion ». Les modèles recommandent un jeu par mode.
    pub fn ecriture(&self, reflexion: bool) -> (f64, Map<String, Value>) {
        if !reflexion {
            return (self.temperature_reecriture, self.echantillonnage());
        }
        let mut m = Map::new();
        m.insert("top_p".to_owned(), json!(self.top_p_reflexion));
        m.insert("top_k".to_owned(), json!(self.top_k_reflexion));
        m.insert("min_p".to_owned(), json!(self.min_p_reflexion));
        m.insert(
            "presence_penalty".to_owned(),
            json!(self.presence_penalty_reflexion),
        );
        (self.temperature_reflexion, m)
    }
}

fn section_du(dossier: &Dossier, nom: &str) -> Map<String, Value> {
    reglages::section(dossier, SECTION)
        .and_then(|mut s| s.remove(nom))
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}

/// Les réglages de ce GGUF.
pub fn valeurs(dossier: &Dossier, nom: &str) -> ValeursLlm {
    ValeursLlm::depuis(&section_du(dossier, nom))
}

/// La consigne réécrite par l'utilisateur pour ce GGUF et cet usage (`reecriture`, `retouche`…), ou rien.
pub fn consigne(dossier: &Dossier, nom: &str, usage: &str) -> Option<String> {
    section_du(dossier, nom)
        .get("consignes")?
        .get(usage)?
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .map(str::to_owned)
}

/// Les projecteurs d'images de `modeles\llm`, du plus léger au plus lourd (Q8_0 avant BF16).
pub fn projecteurs(modeles: &Path) -> Vec<String> {
    let dossier = modeles.join(DOSSIER_LLM);
    let mut trouves: Vec<(u64, String)> = modeles::fichiers(modeles, DOSSIER_LLM, &["gguf"])
        .into_iter()
        .filter(|f| motif_mmproj().is_some_and(|m| m.is_match(f)))
        .map(|f| {
            let taille = fs::metadata(dossier.join(&f)).map_or(u64::MAX, |m| m.len());
            (taille, f)
        })
        .collect();
    trouves.sort();
    trouves.into_iter().map(|(_, f)| f).collect()
}

/// Le début de nom qu'un projecteur partage avec les GGUF qu'il sert : « Ternary-Bonsai-2-27B ».
pub fn famille(projecteur: &str) -> &str {
    motif_mmproj()
        .and_then(|m| m.find(projecteur))
        .map_or(projecteur, |trouve| &projecteur[..trouve.start()])
}

/// Le projecteur de ce GGUF et la raison : le choix de Réglages l'emporte ; en automatique, le plus léger de la
/// même famille.
pub fn projecteur(dossier: &Dossier, modeles: &Path, nom: &str) -> (Option<String>, &'static str) {
    let choisi = valeurs(dossier, nom).vision;
    if choisi == SANS_VISION {
        return (None, "coupée à la main");
    }
    let presents = projecteurs(modeles);
    if presents.contains(&choisi) {
        return (Some(choisi), "choisi à la main");
    }
    // Un choix dont le fichier a disparu revient à l'automatique.
    let bas = nom.to_lowercase();
    presents
        .into_iter()
        .find(|p| bas.starts_with(&famille(p).to_lowercase()))
        .map_or((None, "aucun projecteur de sa famille dans modeles/llm"), |p| {
            (Some(p), "projecteur de la même famille")
        })
}

/// Le moteur de ce GGUF et la raison : le choix de Réglages l'emporte ; en automatique, un GGUF désigné, sinon les
/// types de ses tenseurs (un type que l'officiel ne connaît pas vient d'un fork : PrismML).
pub fn moteur(dossier: &Dossier, modeles: &Path, nom: &str) -> (String, String) {
    let choisi = valeurs(dossier, nom).moteur;
    if choisi != AUTO {
        return (choisi, "choisi à la main".to_owned());
    }
    if let Some((_, m)) = SERVEURS_SPECIFIQUES.iter().find(|(f, _)| *f == nom) {
        return ((*m).to_owned(), "désigné d'office".to_owned());
    }
    match gguf::types_inconnus(&modeles.join(DOSSIER_LLM).join(nom)) {
        Err(e) => (
            MOTEUR_OFFICIEL.to_owned(),
            format!("en-tête illisible ({e}) : moteur officiel"),
        ),
        Ok(inconnus) if !inconnus.is_empty() => {
            let types: Vec<String> = inconnus.iter().map(u32::to_string).collect();
            (
                MOTEUR_PRISM.to_owned(),
                format!("format hors llama.cpp officiel (type {})", types.join(", ")),
            )
        }
        Ok(_) => (MOTEUR_OFFICIEL.to_owned(), "format standard".to_owned()),
    }
}

/// Les dossiers dont dépend le lancement d'un LLM.
#[derive(Debug, Clone)]
pub struct Lieux {
    pub dossier: Dossier,
    pub modeles: PathBuf,
    pub moteurs: PathBuf,
    pub port: u16,
}

/// Comment lancer ce GGUF (le LLM par défaut si rien n'est demandé) : serveur, contexte, réflexion et vision, d'après
/// ses réglages. La vision est chargée d'office quand le LLM en a une ; avec la réflexion ou la vision, le contexte
/// monte à 16 384 au moins. Un LLM spécialisé a ses propres réglages d'entraînement.
///
/// # Errors
/// Nom de GGUF invalide.
pub fn lancement(lieux: &Lieux, modele: Option<&str>) -> Result<Lancement, String> {
    let nom = nom(&lieux.dossier, &lieux.modeles, modele);
    nom_de_fichier(&nom)?;
    let regle = valeurs(&lieux.dossier, &nom);
    let llm = lieux.modeles.join(DOSSIER_LLM);
    let mut contexte = regle.contexte;
    let mut reflexion = regle.reflexion;
    let mut vision = projecteur(&lieux.dossier, &lieux.modeles, &nom)
        .0
        .map(|p| llm.join(p));
    if reflexion || vision.is_some() {
        contexte = contexte.max(CONTEXTE_REFLEXION);
    }
    if let Some(s) = fiches::specialiste(&nom) {
        let du_specialiste = s
            .get("contexte")
            .and_then(Value::as_u64)
            .and_then(|c| u32::try_from(c).ok())
            .unwrap_or(0);
        contexte = contexte.max(du_specialiste);
        reflexion = s.get("reflexion").and_then(Value::as_bool).unwrap_or(reflexion);
        if let Some(projecteur) = s.pointer("/vision/nom").and_then(Value::as_str) {
            vision = Some(llm.join(nom_de_fichier(projecteur)?));
        }
    }
    let (moteur, _) = moteur(&lieux.dossier, &lieux.modeles, &nom);
    Ok(Lancement {
        // Le llama.cpp choisi ailleurs (celui d'une v2 repris pour un essai), sinon celui d'Aria.
        serveur: crate::emplacements::moteur_externe(&lieux.dossier, &moteur)
            .unwrap_or_else(|| lieux.moteurs.join(&moteur))
            .join(EXECUTABLE),
        modele: llm.join(&nom),
        port: lieux.port,
        contexte,
        reflexion,
        vision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gguf::essai;

    const T2I: &str = "Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf";

    struct Banc {
        _temp: tempfile::TempDir,
        lieux: Lieux,
    }

    impl Banc {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let lieux = Lieux {
                dossier: Dossier::a(temp.path()),
                modeles: temp.path().join("modeles"),
                moteurs: temp.path().join("moteurs"),
                port: 8095,
            };
            fs::create_dir_all(lieux.modeles.join(DOSSIER_LLM)).unwrap();
            Self { _temp: temp, lieux }
        }

        fn poser(&self, nom: &str, octets: &[u8]) {
            fs::write(self.lieux.modeles.join(DOSSIER_LLM).join(nom), octets).unwrap();
        }

        fn regler(&self, json: &str) {
            fs::write(self.lieux.dossier.reglages(), json).unwrap();
        }

        fn modeles(&self) -> &Path {
            &self.lieux.modeles
        }
    }

    #[test]
    fn llm_installes_et_defaut() {
        let b = Banc::new();
        assert!(disponibles(b.modeles()).is_empty());
        assert_eq!(defaut(&b.lieux.dossier, b.modeles()), LLM_ORIGINE);
        b.poser(T2I, b"x");
        b.poser("mistral.gguf", b"x");
        b.poser("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf", b"x");
        b.poser("notes.txt", b"x");
        assert_eq!(
            disponibles(b.modeles()),
            vec![T2I.to_owned(), "mistral.gguf".to_owned()]
        );
        // Un spécialiste ne sert pas de LLM par défaut.
        assert_eq!(defaut(&b.lieux.dossier, b.modeles()), "mistral.gguf");
        b.poser(LLM_ORIGINE, b"x");
        assert_eq!(defaut(&b.lieux.dossier, b.modeles()), LLM_ORIGINE);
        assert_eq!(
            nom(&b.lieux.dossier, b.modeles(), Some("mistral.gguf")),
            "mistral.gguf"
        );
        assert_eq!(nom(&b.lieux.dossier, b.modeles(), None), LLM_ORIGINE);
    }

    #[test]
    fn llm_retenu_par_un_casting() {
        let b = Banc::new();
        let retenir = |modele: &str| {
            reglages::modifier_section(&b.lieux.dossier, "favoris", |f| {
                f.insert("llama".to_owned(), json!({"modele": modele}));
            })
            .unwrap();
        };
        // Retenu mais pas installé : rien d'installé, c'est lui que le lancement dira manquant.
        retenir("mistral.gguf");
        assert_eq!(defaut(&b.lieux.dossier, b.modeles()), "mistral.gguf");
        b.poser(LLM_ORIGINE, b"x");
        assert_eq!(defaut(&b.lieux.dossier, b.modeles()), LLM_ORIGINE);
        // Installé : il passe devant celui de la v2, partout où le LLM par défaut sert.
        b.poser("mistral.gguf", b"x");
        assert_eq!(defaut(&b.lieux.dossier, b.modeles()), "mistral.gguf");
        assert_eq!(nom(&b.lieux.dossier, b.modeles(), None), "mistral.gguf");
        assert_eq!(
            de_workflow(&b.lieux.dossier, b.modeles(), "krea2_turbo"),
            "mistral.gguf"
        );
        // Le témoin gagnant (pas de LLM) laisse un modèle vide : le défaut de l'appli reprend.
        retenir("");
        assert_eq!(defaut(&b.lieux.dossier, b.modeles()), LLM_ORIGINE);
    }

    #[test]
    fn llm_choisi_pour_un_workflow() {
        let b = Banc::new();
        b.poser("mistral.gguf", b"x");
        b.poser("qwen3.gguf", b"x");
        let d = &b.lieux.dossier;
        assert_eq!(de_workflow(d, b.modeles(), "krea2_turbo"), "mistral.gguf");
        b.regler(r#"{"divers": {"llm_par_workflow": {"krea2_turbo": "qwen3.gguf", "qwen_image_2_1": "absent.gguf"}}}"#);
        assert_eq!(de_workflow(d, b.modeles(), "krea2_turbo"), "qwen3.gguf");
        assert_eq!(de_workflow(d, b.modeles(), "qwen_image_2_1"), "mistral.gguf");
    }

    #[test]
    fn noms_de_fichier() {
        assert!(nom_de_fichier("bonsai.gguf").is_ok());
        for mauvais in ["", "..", "../x.gguf", "a\\b.gguf", "C:x.gguf", " x.gguf"] {
            assert!(nom_de_fichier(mauvais).is_err(), "{mauvais}");
        }
    }

    #[test]
    fn reglages_lus_comme_la_v2() {
        let b = Banc::new();
        assert_eq!(valeurs(&b.lieux.dossier, "x.gguf"), ValeursLlm::default());
        b.regler(
            r#"{"llm": {"x.gguf": {"temperature_reecriture": 0.4, "top_k": 20, "reflexion": false,
                "contexte": 8192, "max_tokens": 99999, "top_p": "haut", "moteur": "llama-prism",
                "consignes": {"reecriture": "Ma consigne", "retouche": "  "}}}}"#,
        );
        let v = valeurs(&b.lieux.dossier, "x.gguf");
        assert!((v.temperature_reecriture - 0.4).abs() < f64::EPSILON);
        assert_eq!(v.top_k, 20);
        assert!(!v.reflexion);
        assert_eq!(v.contexte, 8192);
        // Hors bornes ou d'un mauvais type : la valeur d'origine.
        assert_eq!(v.max_tokens, 512);
        assert!((v.top_p - 0.95).abs() < f64::EPSILON);
        assert_eq!(v.moteur, MOTEUR_PRISM);
        assert_eq!(
            consigne(&b.lieux.dossier, "x.gguf", "reecriture").as_deref(),
            Some("Ma consigne")
        );
        assert!(consigne(&b.lieux.dossier, "x.gguf", "retouche").is_none());
        assert!(consigne(&b.lieux.dossier, "y.gguf", "reecriture").is_none());
    }

    #[test]
    fn un_jeu_d_echantillonnage_par_mode() {
        let v = ValeursLlm {
            temperature_reflexion: 1.0,
            top_k_reflexion: 20,
            presence_penalty: 1.5,
            ..ValeursLlm::default()
        };
        let (t, sans) = v.ecriture(false);
        assert!((t - 0.7).abs() < f64::EPSILON);
        assert_eq!(sans["presence_penalty"], json!(1.5));
        let (t, avec) = v.ecriture(true);
        assert!((t - 1.0).abs() < f64::EPSILON);
        assert_eq!(avec["top_k"], json!(20));
        assert_eq!(avec["presence_penalty"], json!(0.0));
        assert_eq!(v.echantillonnage().len(), 4);
    }

    #[test]
    fn projecteur_de_la_meme_famille() {
        let b = Banc::new();
        let d = &b.lieux.dossier;
        assert_eq!(
            famille("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf"),
            "Ternary-Bonsai-2-27B"
        );
        assert_eq!(
            famille("Qwen-Image-2.1-PE-I2I.mmproj-bf16.gguf"),
            "Qwen-Image-2.1-PE-I2I"
        );
        assert_eq!(projecteur(d, b.modeles(), LLM_ORIGINE).0, None);
        b.poser("Ternary-Bonsai-2-27B-mmproj-BF16.gguf", &[0; 20]);
        b.poser("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf", &[0; 10]);
        b.poser("Autre-mmproj-Q8_0.gguf", &[0; 5]);
        assert_eq!(
            projecteurs(b.modeles()),
            vec![
                "Autre-mmproj-Q8_0.gguf".to_owned(),
                "Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf".to_owned(),
                "Ternary-Bonsai-2-27B-mmproj-BF16.gguf".to_owned()
            ]
        );
        // Le plus léger de la famille, aussi pour un dérivé (Heretic).
        let heretic = "Ternary-Bonsai-2-27B-Heretic-PTQ1_0.gguf";
        assert_eq!(
            projecteur(d, b.modeles(), heretic),
            (
                Some("Ternary-Bonsai-2-27B-mmproj-Q8_0.gguf".to_owned()),
                "projecteur de la même famille"
            )
        );
        assert_eq!(projecteur(d, b.modeles(), "mistral.gguf").0, None);
        b.regler(r#"{"llm": {"mistral.gguf": {"vision": "Autre-mmproj-Q8_0.gguf"}}, "x": 1}"#);
        assert_eq!(projecteur(d, b.modeles(), "mistral.gguf").1, "choisi à la main");
        b.regler(&format!(r#"{{"llm": {{"{heretic}": {{"vision": "aucune"}}}}}}"#));
        assert_eq!(projecteur(d, b.modeles(), heretic), (None, "coupée à la main"));
    }

    #[test]
    fn moteur_d_apres_le_format() {
        let b = Banc::new();
        let d = &b.lieux.dossier;
        b.poser("standard.gguf", &essai::gguf(&[1, 12]));
        b.poser("ternaire.gguf", &essai::gguf(&[142, 1]));
        b.poser("abime.gguf", b"pas un gguf");
        assert_eq!(moteur(d, b.modeles(), "standard.gguf").0, MOTEUR_OFFICIEL);
        let (m, raison) = moteur(d, b.modeles(), "ternaire.gguf");
        assert_eq!(m, MOTEUR_PRISM);
        assert_eq!(raison, "format hors llama.cpp officiel (type 142)");
        assert_eq!(moteur(d, b.modeles(), "abime.gguf").0, MOTEUR_OFFICIEL);
        assert_eq!(moteur(d, b.modeles(), LLM_ORIGINE).0, MOTEUR_PRISM);
        b.regler(r#"{"llm": {"ternaire.gguf": {"moteur": "llama"}}}"#);
        assert_eq!(
            moteur(d, b.modeles(), "ternaire.gguf"),
            (MOTEUR_OFFICIEL.to_owned(), "choisi à la main".to_owned())
        );
    }

    #[test]
    fn lancement_comme_la_v2() {
        let b = Banc::new();
        b.poser("standard.gguf", &essai::gguf(&[1]));
        b.regler(r#"{"llm": {"standard.gguf": {"reflexion": false, "contexte": 8192}}}"#);
        let l = lancement(&b.lieux, Some("standard.gguf")).unwrap();
        assert_eq!(l.serveur, b.lieux.moteurs.join("llama").join(EXECUTABLE));
        assert_eq!(l.modele, b.modeles().join("llm").join("standard.gguf"));
        assert_eq!(
            (l.port, l.contexte, l.reflexion, l.vision.clone()),
            (8095, 8192, false, None)
        );

        // Réflexion : le contexte monte à 16 384 au moins.
        b.regler(r#"{"llm": {"standard.gguf": {"contexte": 8192}}}"#);
        let l = lancement(&b.lieux, Some("standard.gguf")).unwrap();
        assert_eq!((l.contexte, l.reflexion), (CONTEXTE_REFLEXION, true));

        // Le LLM par défaut, ternaire : PrismML.
        b.poser(LLM_ORIGINE, &essai::gguf(&[143]));
        let l = lancement(&b.lieux, None).unwrap();
        assert_eq!(l.serveur, b.lieux.moteurs.join("llama-prism").join(EXECUTABLE));
        assert_eq!(l.modele, b.modeles().join("llm").join(LLM_ORIGINE));

        // Un llama.cpp pris ailleurs (celui d'une v2 reprise pour un essai) : son llama-server.
        let ailleurs = b.lieux.moteurs.with_file_name("v2-llama-prism");
        b.regler(&format!(
            r#"{{"emplacements": {{"llama-prism": {}}}}}"#,
            serde_json::to_string(&ailleurs.display().to_string()).unwrap()
        ));
        let l = lancement(&b.lieux, None).unwrap();
        assert_eq!(l.serveur, ailleurs.join(EXECUTABLE));
        b.regler("{}");

        // Un spécialiste : ses réglages d'entraînement.
        b.poser(T2I, &essai::gguf(&[12]));
        b.regler(&format!(
            r#"{{"llm": {{"{T2I}": {{"reflexion": false, "contexte": 2048}}}}}}"#
        ));
        let l = lancement(&b.lieux, Some(T2I)).unwrap();
        assert_eq!((l.contexte, l.reflexion), (16384, true));
        assert!(lancement(&b.lieux, Some("../x.gguf")).is_err());
    }
}
