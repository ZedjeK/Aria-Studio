//! Les réglages de l'appli, dans `reglages.json`. Jamais de secret ici : ils vont dans `secrets\`.
//!
//! Le fichier garde le format de la version 2 d'Aria Studio, qui partage le même dossier de données : un objet JSON
//! avec une section par sujet (`fond`, `installation`, `mises_a_jour`, `emplacements`, `llm`, `loras`…).
//! [`Reglages`] n'en lit que ce que cette version sait déjà gérer :
//!
//! - `fond` : `actif`, `vitesse`, `minimum`, `maximum` ;
//! - `installation.proposee` : proposer de s'installer ;
//! - `mises_a_jour.active` : chercher une nouvelle version au lancement.
//!
//! Toutes les autres sections, et les autres clés des sections lues, sont gardées telles quelles à chaque écriture :
//! une v2 qui reprend la main retrouve tous ses réglages.
//!
//! Comme en v2 : avant chaque écriture qui change quelque chose, l'ancien fichier est copié dans
//! `sauvegardes\reglages-AAAAMMJJ-HHMMSS-micro.json`, et seules les [`SAUVEGARDES_GARDEES`] dernières copies
//! restent ; un fichier illisible n'est jamais écrasé : il est copié dans
//! `sauvegardes\reglages-illisible-AAAAMMJJ-HHMMSS.json` avant d'être réécrit.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, TimeDelta};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::journal::{self, Resultat};
use crate::sauvegardes::copier_sans_ecraser;
use crate::{Dossier, Erreur, donnees};

/// Les sections de `reglages.json`, telles qu'elles sont dans le fichier.
type Sections = Map<String, Value>;

/// Combien de sauvegardes de `reglages.json` restent dans `sauvegardes\` (comme en v2).
pub const SAUVEGARDES_GARDEES: usize = 20;

const SECTION_FOND: &str = "fond";
const SECTION_INSTALLATION: &str = "installation";
const CLE_PROPOSEE: &str = "proposee";
const SECTION_MISES_A_JOUR: &str = "mises_a_jour";
const CLE_ACTIVE: &str = "active";
const PREFIXE_SAUVEGARDE: &str = "reglages-";
const PREFIXE_ILLISIBLE: &str = "reglages-illisible-";

/// Vitesse du fond animé. Dans le fichier : « lente », « normale », « rapide ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Vitesse {
    Lente,
    #[default]
    Normale,
    Rapide,
}

/// Une valeur inconnue, ou qui n'est pas du texte, donne « normale » : un fichier modifié à la main ne casse rien.
impl<'de> Deserialize<'de> for Vitesse {
    fn deserialize<D: Deserializer<'de>>(entree: D) -> Result<Self, D::Error> {
        let valeur = Value::deserialize(entree)?;
        Ok(
            match valeur.as_str().map(|s| s.trim().to_lowercase()).as_deref() {
                Some("lente") => Self::Lente,
                Some("rapide") => Self::Rapide,
                _ => Self::Normale,
            },
        )
    }
}

/// Le fond animé, réglages obligatoires de toute appli Aria : `fond.actif`, `fond.vitesse`, `fond.minimum` et
/// `fond.maximum` (nombre d'éléments tirés au hasard à chaque passage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesFond {
    pub actif: bool,
    pub vitesse: Vitesse,
    pub minimum: i64,
    pub maximum: i64,
}

impl ReglagesFond {
    /// Le moins d'éléments qu'un passage puisse tirer.
    pub const ELEMENTS_MIN: i64 = 1;
    /// Le plus d'éléments qu'un passage puisse tirer.
    pub const ELEMENTS_MAX: i64 = 12;

    /// Bornes ramenées entre 1 et 12, et remises dans l'ordre si le fichier les a inversées.
    #[must_use]
    pub fn assaini(self) -> Self {
        let bas = self.minimum.clamp(Self::ELEMENTS_MIN, Self::ELEMENTS_MAX);
        let haut = self.maximum.clamp(Self::ELEMENTS_MIN, Self::ELEMENTS_MAX);
        Self {
            minimum: bas.min(haut),
            maximum: bas.max(haut),
            ..self
        }
    }
}

impl Default for ReglagesFond {
    fn default() -> Self {
        Self {
            actif: true,
            vitesse: Vitesse::Normale,
            minimum: 2,
            maximum: 5,
        }
    }
}

/// Les réglages que cette version sait gérer. Ce type est aussi celui qu'échangent le cœur et l'interface : ses
/// noms de champs sont ceux de l'interface, pas ceux du fichier (voir le haut du module).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    /// Fond animé ; absent du fichier, c'est celui par défaut.
    pub fond: ReglagesFond,

    /// Proposer de s'installer quand l'exe est lancé hors de `programme\` (zip décompressé). Décoché par « Ne plus
    /// me proposer » ; se recoche dans les réglages. Dans le fichier : `installation.proposee`.
    #[serde(rename = "proposerInstallation")]
    pub proposer_installation: bool,

    /// Chercher une nouvelle version au lancement : une requête vers l'API GitHub. Décoché : plus aucune connexion
    /// pour ça. Dans le fichier : `mises_a_jour.active`.
    #[serde(rename = "chercherMisesAJour")]
    pub chercher_mises_a_jour: bool,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            fond: ReglagesFond::default(),
            proposer_installation: true,
            chercher_mises_a_jour: true,
        }
    }
}

impl Reglages {
    /// Valeurs ramenées dans leurs bornes.
    #[must_use]
    pub fn assaini(self) -> Self {
        Self {
            fond: self.fond.assaini(),
            ..self
        }
    }

    /// Réglages lus dans le texte d'un `reglages.json` ; ceux par défaut s'il est illisible.
    pub fn depuis_json(texte: &str) -> Self {
        match serde_json::from_str::<Value>(texte) {
            Ok(Value::Object(sections)) => Self::depuis_sections(&sections),
            _ => Self::default(),
        }
    }

    /// Réglages lus dans les sections du fichier ; une valeur absente ou d'un mauvais type prend sa valeur par
    /// défaut.
    fn depuis_sections(sections: &Sections) -> Self {
        let defaut = Self::default();
        let fond = sections
            .get(SECTION_FOND)
            .and_then(|v| serde_json::from_value::<ReglagesFond>(v.clone()).ok())
            .unwrap_or_default();
        Self {
            fond,
            proposer_installation: booleen(sections, SECTION_INSTALLATION, CLE_PROPOSEE)
                .unwrap_or(defaut.proposer_installation),
            chercher_mises_a_jour: booleen(sections, SECTION_MISES_A_JOUR, CLE_ACTIVE)
                .unwrap_or(defaut.chercher_mises_a_jour),
        }
        .assaini()
    }

    /// Verse ces réglages, assainis, dans les sections du fichier, sans toucher au reste.
    fn appliquer(&self, sections: &mut Sections) -> Result<(), serde_json::Error> {
        let propre = self.clone().assaini();
        if let Value::Object(fond) = serde_json::to_value(propre.fond)? {
            fusionner(sections, SECTION_FOND, fond);
        }
        fusionner(
            sections,
            SECTION_INSTALLATION,
            une_cle(CLE_PROPOSEE, propre.proposer_installation),
        );
        fusionner(
            sections,
            SECTION_MISES_A_JOUR,
            une_cle(CLE_ACTIVE, propre.chercher_mises_a_jour),
        );
        Ok(())
    }

    /// Réglages enregistrés ; ceux par défaut si le fichier est absent ou illisible.
    pub fn charger(dossier: &Dossier) -> Self {
        match lire(&dossier.reglages()) {
            Ok(Contenu::Lisible(sections)) => Self::depuis_sections(&sections),
            _ => Self::default(),
        }
    }

    /// Enregistre les réglages, assainis, en gardant tout le reste du fichier. Rien n'est écrit si rien ne change ;
    /// sinon l'ancien fichier est sauvegardé d'abord, et le nouveau n'est jamais laissé à moitié écrit.
    ///
    /// # Errors
    /// [`Erreur::Fichier`] si la lecture, la sauvegarde ou l'écriture échoue ; l'ancien fichier reste alors intact.
    pub fn enregistrer(&self, dossier: &Dossier) -> Result<(), Erreur> {
        let chemin = dossier.reglages();
        let (mut sections, avant) = match lire(&chemin)? {
            Contenu::Absent => (Sections::new(), None),
            Contenu::Lisible(sections) => (sections.clone(), Some(sections)),
            Contenu::Illisible => {
                mettre_de_cote(dossier, &chemin, Local::now())?;
                (Sections::new(), None)
            }
        };
        self.appliquer(&mut sections)
            .map_err(|e| Erreur::fichier("préparation de", &chemin, e.into()))?;
        if avant.as_ref() == Some(&sections) {
            return Ok(());
        }
        if avant.is_some() {
            sauvegarder(dossier, &chemin, Local::now())?;
        }
        let octets = texte(&sections).map_err(|e| Erreur::fichier("préparation de", &chemin, e.into()))?;
        donnees::ecrire_entier(&chemin, &octets)?;
        // Comme en v2 : le journal dit quelles sections ont changé.
        let changees: Vec<&str> = sections
            .iter()
            .filter(|(cle, valeur)| avant.as_ref().and_then(|a| a.get(cle.as_str())) != Some(*valeur))
            .map(|(cle, _)| cle.as_str())
            .collect();
        journal::noter(
            dossier,
            "Réglages modifiés",
            &changees.join(", "),
            Resultat::Ok,
            None,
        );
        Ok(())
    }
}

/// Une section de `reglages.json`, telle qu'elle est dans le fichier : pour les réglages que [`Reglages`] ne gère
/// pas encore (emplacements, favoris…). Rien si le fichier est absent ou illisible, ou si la section n'est pas un
/// objet.
pub fn section(dossier: &Dossier, nom: &str) -> Option<Map<String, Value>> {
    match lire(&dossier.reglages()) {
        Ok(Contenu::Lisible(mut sections)) => match sections.remove(nom) {
            Some(Value::Object(section)) => Some(section),
            _ => None,
        },
        _ => None,
    }
}

/// Ce que contient `reglages.json`.
enum Contenu {
    Absent,
    Lisible(Sections),
    /// Pas du JSON, pas un objet, ou pas de l'UTF-8.
    Illisible,
}

fn lire(chemin: &Path) -> Result<Contenu, Erreur> {
    match fs::read_to_string(chemin) {
        Ok(texte) => Ok(match serde_json::from_str::<Value>(&texte) {
            Ok(Value::Object(sections)) => Contenu::Lisible(sections),
            _ => Contenu::Illisible,
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Contenu::Absent),
        Err(e) if e.kind() == ErrorKind::InvalidData => Ok(Contenu::Illisible),
        Err(e) => Err(Erreur::fichier("lecture de", chemin, e)),
    }
}

/// Un booléen rangé dans une section ; rien s'il manque ou n'est pas un booléen.
fn booleen(sections: &Sections, section: &str, cle: &str) -> Option<bool> {
    sections.get(section)?.get(cle)?.as_bool()
}

fn une_cle(cle: &str, valeur: bool) -> Sections {
    let mut objet = Sections::new();
    objet.insert(cle.to_owned(), Value::Bool(valeur));
    objet
}

/// Pose des clés dans une section, en gardant ses autres clés. Une section qui n'est pas un objet est remplacée.
fn fusionner(sections: &mut Sections, section: &str, valeurs: Sections) {
    let entree = sections
        .entry(section)
        .or_insert_with(|| Value::Object(Sections::new()));
    if !entree.is_object() {
        *entree = Value::Object(Sections::new());
    }
    if let Value::Object(objet) = entree {
        objet.extend(valeurs);
    }
}

/// Le texte du fichier, comme l'écrit la v2 : indenté de quatre espaces, accents en clair.
fn texte(sections: &Sections) -> Result<Vec<u8>, serde_json::Error> {
    let mut sortie = Vec::new();
    let format = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serialiseur = serde_json::Serializer::with_formatter(&mut sortie, format);
    sections.serialize(&mut serialiseur)?;
    Ok(sortie)
}

/// Copie `reglages.json` dans `sauvegardes\reglages-AAAAMMJJ-HHMMSS-micro.json`, puis ne garde que les
/// [`SAUVEGARDES_GARDEES`] dernières copies.
fn sauvegarder(dossier: &Dossier, chemin: &Path, quand: DateTime<Local>) -> Result<PathBuf, Erreur> {
    let destination = dossier.sauvegardes();
    fs::create_dir_all(&destination).map_err(|e| Erreur::fichier("création de", &destination, e))?;
    let mut quand = quand;
    loop {
        let cible = destination.join(format!(
            "{PREFIXE_SAUVEGARDE}{}.json",
            quand.format("%Y%m%d-%H%M%S-%6f")
        ));
        match copier_sans_ecraser(chemin, &cible) {
            Ok(()) => {
                elaguer(&destination);
                return Ok(cible);
            }
            // Deux sauvegardes dans la même microseconde : la suivante prend la microseconde d'après.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => quand += TimeDelta::microseconds(1),
            Err(e) => return Err(Erreur::fichier("sauvegarde vers", cible, e)),
        }
    }
}

/// Copie un `reglages.json` illisible dans `sauvegardes\reglages-illisible-AAAAMMJJ-HHMMSS.json` avant qu'il soit
/// réécrit.
fn mettre_de_cote(dossier: &Dossier, chemin: &Path, quand: DateTime<Local>) -> Result<PathBuf, Erreur> {
    let destination = dossier.sauvegardes();
    fs::create_dir_all(&destination).map_err(|e| Erreur::fichier("création de", &destination, e))?;
    let horodatage = quand.format("%Y%m%d-%H%M%S");
    let mut n = 1u32;
    loop {
        let suffixe = if n == 1 { String::new() } else { format!("-{n}") };
        let cible = destination.join(format!("{PREFIXE_ILLISIBLE}{horodatage}{suffixe}.json"));
        match copier_sans_ecraser(chemin, &cible) {
            Ok(()) => return Ok(cible),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(Erreur::fichier("mise de côté vers", cible, e)),
        }
    }
}

/// Efface les sauvegardes de `reglages.json` au-delà des [`SAUVEGARDES_GARDEES`] plus récentes. Seules les copies
/// au nom `reglages-AAAAMMJJ-HHMMSS-micro.json` sont comptées : rien d'autre dans `sauvegardes\` n'est touché.
fn elaguer(destination: &Path) {
    let Ok(entrees) = fs::read_dir(destination) else {
        return;
    };
    let mut noms: Vec<String> = entrees
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|nom| est_sauvegarde(nom))
        .collect();
    // Le nom porte la date à la microseconde : l'ordre alphabétique est l'ordre chronologique.
    noms.sort_unstable();
    for nom in noms.iter().rev().skip(SAUVEGARDES_GARDEES) {
        // Une copie qui ne s'efface pas reste : le ménage reprend à la prochaine sauvegarde.
        let _ = fs::remove_file(destination.join(nom));
    }
}

/// `reglages-AAAAMMJJ-HHMMSS-micro.json`, le nom que donnent la v2 et cette version à une sauvegarde.
fn est_sauvegarde(nom: &str) -> bool {
    let Some(milieu) = nom
        .strip_prefix(PREFIXE_SAUVEGARDE)
        .and_then(|n| n.strip_suffix(".json"))
    else {
        return false;
    };
    let morceaux: Vec<&str> = milieu.split('-').collect();
    match morceaux.as_slice() {
        [jour, heure, micro] => chiffres(jour, 8) && chiffres(heure, 6) && chiffres(micro, 6),
        _ => false,
    }
}

fn chiffres(texte: &str, longueur: usize) -> bool {
    texte.len() == longueur && texte.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un reglages.json tel que l'écrit la v2.4 : les sections que cette version gère, et les autres.
    const FICHIER_V2: &str = r#"{
    "migration": {"date": "2026-11-13T10:00:00", "depuis": ["aria.yaml"]},
    "emplacements": {"modeles": "D:\\IA\\modèles"},
    "favoris": {"generation": {"texte_vers_image": "krea2_turbo"}},
    "assistant": {"termine": true, "licences": ["flux"]},
    "installation": {"proposee": false, "autre": 1},
    "fond": {"actif": true, "vitesse": "rapide", "minimum": 3, "maximum": 9},
    "notifications": {"active": true, "son": false},
    "mises_a_jour": {"active": false},
    "loras": {"style.safetensors": {"declencheurs": "néon", "force": 0.8}},
    "llm": {"qwen.gguf": {"temperature": 0.7, "contexte": 8192}},
    "workflows": {"krea2_turbo": {"etapes": 8}},
    "securite": {"sensibles": ["mot"], "temoins": [{"texte": "essai", "attendu": "bloque"}]}
}"#;

    fn dossier_avec(texte: &str) -> (tempfile::TempDir, Dossier) {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        fs::write(dossier.reglages(), texte).unwrap();
        (temp, dossier)
    }

    fn fichier(dossier: &Dossier) -> Value {
        serde_json::from_str(&fs::read_to_string(dossier.reglages()).unwrap()).unwrap()
    }

    fn sauvegardes(dossier: &Dossier) -> Vec<String> {
        let mut noms: Vec<String> = fs::read_dir(dossier.sauvegardes())
            .map(|entrees| {
                entrees
                    .map(|e| e.unwrap().file_name().into_string().unwrap())
                    .collect()
            })
            .unwrap_or_default();
        noms.sort();
        noms
    }

    #[test]
    fn un_fichier_absent_donne_les_reglages_par_defaut() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(Reglages::charger(&Dossier::a(temp.path())), Reglages::default());
    }

    #[test]
    fn un_fichier_illisible_donne_les_reglages_par_defaut() {
        assert_eq!(Reglages::depuis_json("{ pas du json"), Reglages::default());
        assert_eq!(Reglages::depuis_json("[]"), Reglages::default());
        let (_temp, dossier) = dossier_avec("{ pas du json");
        assert_eq!(Reglages::charger(&dossier), Reglages::default());
    }

    #[test]
    fn enregistrer_puis_charger_rend_les_memes_reglages() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let reglages = Reglages {
            fond: ReglagesFond {
                actif: false,
                vitesse: Vitesse::Rapide,
                minimum: 3,
                maximum: 9,
            },
            proposer_installation: false,
            chercher_mises_a_jour: false,
        };
        reglages.enregistrer(&dossier).unwrap();
        assert_eq!(Reglages::charger(&dossier), reglages);
    }

    #[test]
    fn les_reglages_de_la_v2_sont_lus() {
        let (_temp, dossier) = dossier_avec(FICHIER_V2);
        let r = Reglages::charger(&dossier);
        assert!(!r.proposer_installation);
        assert!(!r.chercher_mises_a_jour);
        assert_eq!(
            r.fond,
            ReglagesFond {
                actif: true,
                vitesse: Vitesse::Rapide,
                minimum: 3,
                maximum: 9,
            }
        );
    }

    #[test]
    fn le_fichier_garde_le_format_de_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        Reglages::default().enregistrer(&dossier).unwrap();
        let v = fichier(&dossier);
        assert_eq!(v["installation"]["proposee"], Value::Bool(true));
        assert_eq!(v["mises_a_jour"]["active"], Value::Bool(true));
        assert_eq!(v["fond"]["vitesse"], Value::from("normale"));
        assert!(v.get("proposerInstallation").is_none());
        assert!(v.get("chercherMisesAJour").is_none());
        let texte = fs::read_to_string(dossier.reglages()).unwrap();
        assert!(
            texte.contains("\n    \"fond\""),
            "indentation de quatre espaces attendue : {texte}"
        );
    }

    #[test]
    fn les_autres_sections_de_la_v2_sont_gardees() {
        let (_temp, dossier) = dossier_avec(FICHIER_V2);
        let avant: Value = serde_json::from_str(FICHIER_V2).unwrap();
        let reglages = Reglages {
            proposer_installation: true,
            chercher_mises_a_jour: true,
            ..Reglages::charger(&dossier)
        };
        reglages.enregistrer(&dossier).unwrap();
        let apres = fichier(&dossier);
        for section in [
            "migration",
            "emplacements",
            "favoris",
            "assistant",
            "notifications",
            "loras",
            "llm",
            "workflows",
            "securite",
        ] {
            assert_eq!(apres[section], avant[section], "section {section}");
        }
        assert_eq!(apres["installation"]["proposee"], Value::Bool(true));
        assert_eq!(apres["installation"]["autre"], Value::from(1));
        assert_eq!(apres["mises_a_jour"]["active"], Value::Bool(true));
        assert_eq!(apres["fond"], avant["fond"]);
    }

    #[test]
    fn une_sauvegarde_au_format_de_la_v2_est_faite_avant_d_ecrire() {
        let (_temp, dossier) = dossier_avec(FICHIER_V2);
        let reglages = Reglages {
            chercher_mises_a_jour: true,
            ..Reglages::charger(&dossier)
        };
        reglages.enregistrer(&dossier).unwrap();
        let noms = sauvegardes(&dossier);
        assert_eq!(noms.len(), 1, "{noms:?}");
        let nom = &noms[0];
        assert!(est_sauvegarde(nom), "{nom}");
        assert_eq!(
            fs::read_to_string(dossier.sauvegardes().join(nom)).unwrap(),
            FICHIER_V2
        );
    }

    #[test]
    fn rien_n_est_ecrit_si_rien_ne_change() {
        let (_temp, dossier) = dossier_avec(FICHIER_V2);
        let reglages = Reglages::charger(&dossier);
        reglages.enregistrer(&dossier).unwrap();
        assert_eq!(fs::read_to_string(dossier.reglages()).unwrap(), FICHIER_V2);
        assert!(sauvegardes(&dossier).is_empty());
    }

    #[test]
    fn un_changement_est_note_au_journal_avec_ses_sections() {
        let (_temp, dossier) = dossier_avec(FICHIER_V2);
        let reglages = Reglages {
            chercher_mises_a_jour: true,
            ..Reglages::charger(&dossier)
        };
        reglages.enregistrer(&dossier).unwrap();
        let ops = journal::lire(&dossier);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].action, "Réglages modifiés");
        assert_eq!(ops[0].objet, "mises_a_jour");
        reglages.enregistrer(&dossier).unwrap();
        assert_eq!(journal::lire(&dossier).len(), 1, "rien n'a changé : rien de noté");
    }

    #[test]
    fn un_fichier_illisible_est_mis_de_cote_avant_d_etre_reecrit() {
        let (_temp, dossier) = dossier_avec("{ pas du json");
        Reglages::default().enregistrer(&dossier).unwrap();
        let noms = sauvegardes(&dossier);
        assert_eq!(noms.len(), 1, "{noms:?}");
        assert!(noms[0].starts_with(PREFIXE_ILLISIBLE), "{noms:?}");
        assert_eq!(
            fs::read_to_string(dossier.sauvegardes().join(&noms[0])).unwrap(),
            "{ pas du json"
        );
        assert_eq!(Reglages::charger(&dossier), Reglages::default());
        assert!(fichier(&dossier).is_object());
    }

    #[test]
    fn seules_les_vingt_dernieres_sauvegardes_restent() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path();
        for i in 0..25 {
            fs::write(
                destination.join(format!("reglages-20261101-1000{i:02}-000000.json")),
                "{}",
            )
            .unwrap();
        }
        fs::write(destination.join("reglages-illisible-20261101-100000.json"), "x").unwrap();
        fs::write(destination.join("2026-11-21_19-05-00_journal.json"), "[]").unwrap();
        elaguer(destination);
        let mut restes: Vec<String> = fs::read_dir(destination)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        restes.sort();
        let gardees: Vec<&String> = restes.iter().filter(|n| est_sauvegarde(n)).collect();
        assert_eq!(gardees.len(), SAUVEGARDES_GARDEES);
        assert_eq!(gardees[0], "reglages-20261101-100005-000000.json");
        assert!(restes.contains(&"reglages-illisible-20261101-100000.json".to_owned()));
        assert!(restes.contains(&"2026-11-21_19-05-00_journal.json".to_owned()));
    }

    #[test]
    fn le_nom_d_une_sauvegarde_est_reconnu() {
        assert!(est_sauvegarde("reglages-20261122-141005-123456.json"));
        assert!(!est_sauvegarde("reglages-illisible-20261122-141005.json"));
        assert!(!est_sauvegarde("reglages-20261122-141005.json"));
        assert!(!est_sauvegarde("2026-11-22_14-10-05_reglages.json"));
        assert!(!est_sauvegarde("reglages-2026112a-141005-123456.json"));
    }

    #[test]
    fn les_bornes_du_fond_sont_ramenees_et_remises_dans_l_ordre() {
        let r = Reglages::depuis_json(r#"{ "fond": { "minimum": 40, "maximum": -3 } }"#);
        assert_eq!((r.fond.minimum, r.fond.maximum), (1, 12));
        let r = Reglages::depuis_json(r#"{ "fond": { "minimum": 8, "maximum": 4 } }"#);
        assert_eq!((r.fond.minimum, r.fond.maximum), (4, 8));
    }

    #[test]
    fn une_vitesse_inconnue_donne_normale() {
        for valeur in ["\"turbo\"", "12", "null", "\" RAPIDE \""] {
            let texte = String::from(r#"{ "fond": { "vitesse": "#) + valeur + " } }";
            let r = Reglages::depuis_json(&texte);
            let attendue = if valeur.contains("RAPIDE") {
                Vitesse::Rapide
            } else {
                Vitesse::Normale
            };
            assert_eq!(r.fond.vitesse, attendue, "{valeur}");
        }
    }

    #[test]
    fn un_champ_manquant_ou_d_un_mauvais_type_prend_sa_valeur_par_defaut() {
        let r = Reglages::depuis_json(r#"{ "mises_a_jour": { "active": false } }"#);
        assert!(!r.chercher_mises_a_jour);
        assert!(r.proposer_installation);
        assert_eq!(r.fond, ReglagesFond::default());
        let r = Reglages::depuis_json(r#"{ "installation": { "proposee": "non" }, "mises_a_jour": 3 }"#);
        assert!(r.proposer_installation);
        assert!(r.chercher_mises_a_jour);
    }
}
