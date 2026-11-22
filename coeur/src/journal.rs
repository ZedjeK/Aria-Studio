//! `journal.json`, dans le dossier de données : les 500 dernières opérations, la plus récente en premier. Jamais de
//! secret dans une entrée.
//!
//! Même format que la version 2 d'Aria Studio, qui partage le même dossier de données :
//!
//! ```json
//! [{"date": "2026-11-14T10:12:03", "action": "Modèle mis à la corbeille", "objet": "image/x.safetensors",
//!   "resultat": "ok", "detail": "…"}]
//! ```
//!
//! Les entrées déjà là sont gardées telles quelles, clés inconnues comprises. Un journal illisible n'est jamais
//! écrasé : il est copié à côté, dans `journal-illisible-AAAAMMJJ-HHMMSS.json`, puis un nouveau journal commence. Un
//! journal qui ne se lit pas pour une autre raison (fichier verrouillé) n'est pas réécrit : l'entrée est perdue,
//! pas l'historique.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Local};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::sauvegardes::copier_sans_ecraser;
use crate::{Dossier, donnees};

/// Le plus d'opérations gardées.
pub const OPERATIONS_MAX: usize = 500;
/// Le détail d'une opération est coupé à cette longueur, en caractères.
pub const DETAIL_MAX: usize = 500;

const PREFIXE_ILLISIBLE: &str = "journal-illisible-";

/// Comment une opération s'est terminée. Dans le fichier : « ok », « erreur », « partiel », « annule ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Resultat {
    #[default]
    Ok,
    Erreur,
    Partiel,
    Annule,
}

/// Une valeur inconnue donne « ok », comme en v2.
impl<'de> Deserialize<'de> for Resultat {
    fn deserialize<D: Deserializer<'de>>(entree: D) -> Result<Self, D::Error> {
        let valeur = Value::deserialize(entree)?;
        Ok(match valeur.as_str() {
            Some("erreur") => Self::Erreur,
            Some("partiel") => Self::Partiel,
            Some("annule") => Self::Annule,
            _ => Self::Ok,
        })
    }
}

/// Une opération du journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    /// Date et heure locales, à la seconde, sans décalage (`2026-11-14T10:12:03`), comme en v2.
    pub date: String,
    /// Ce qui a été fait : « Réglages modifiés », « Mise à jour installée »…
    pub action: String,
    /// Sur quoi : un fichier, une version, une liste de sections ; vide s'il n'y a rien à préciser.
    #[serde(default)]
    pub objet: String,
    #[serde(default)]
    pub resultat: Resultat,
    /// Une précision, coupée à [`DETAIL_MAX`] caractères.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Deux écritures du journal en même temps n'en perdent aucune.
static VERROU: Mutex<()> = Mutex::new(());

/// Note une opération en tête du journal. Ne renvoie pas d'erreur : un journal qui ne s'écrit pas ne doit pas faire
/// échouer l'opération elle-même.
pub fn noter(dossier: &Dossier, action: &str, objet: &str, resultat: Resultat, detail: Option<&str>) {
    noter_a(dossier, action, objet, resultat, detail, Local::now());
}

/// [`noter`], à une date donnée (tests).
pub fn noter_a(
    dossier: &Dossier,
    action: &str,
    objet: &str,
    resultat: Resultat,
    detail: Option<&str>,
    quand: DateTime<Local>,
) {
    let _verrou = VERROU.lock().unwrap_or_else(PoisonError::into_inner);
    let chemin = dossier.journal();
    let Some(mut entrees) = lire_pour_ecrire(&chemin, quand) else {
        return;
    };
    let operation = Operation {
        date: quand.format("%Y-%m-%dT%H:%M:%S").to_string(),
        action: action.to_owned(),
        objet: objet.to_owned(),
        resultat,
        detail: detail.map(|d| d.chars().take(DETAIL_MAX).collect()),
    };
    let Ok(nouvelle) = serde_json::to_value(&operation) else {
        return;
    };
    entrees.insert(0, nouvelle);
    entrees.truncate(OPERATIONS_MAX);
    if let Ok(octets) = texte(&entrees) {
        // Dossier en lecture seule, fichier verrouillé : l'opération a eu lieu quand même.
        let _ = donnees::ecrire_entier(&chemin, &octets);
    }
}

/// Opérations enregistrées, la plus récente en premier ; aucune si le fichier est absent ou illisible. Une entrée
/// qui n'a pas la forme d'une opération est passée.
pub fn lire(dossier: &Dossier) -> Vec<Operation> {
    let Ok(texte) = fs::read_to_string(dossier.journal()) else {
        return Vec::new();
    };
    match serde_json::from_str::<Value>(&texte) {
        Ok(Value::Array(entrees)) => entrees
            .into_iter()
            .filter_map(|e| serde_json::from_value(e).ok())
            .collect(),
        _ => Vec::new(),
    }
}

/// Les entrées actuelles, telles qu'elles sont dans le fichier, pour en ajouter une ; rien si le journal ne doit pas
/// être réécrit.
fn lire_pour_ecrire(chemin: &Path, quand: DateTime<Local>) -> Option<Vec<Value>> {
    match fs::read_to_string(chemin) {
        Ok(texte) => match serde_json::from_str::<Value>(&texte) {
            Ok(Value::Array(entrees)) => Some(entrees.into_iter().filter(Value::is_object).collect()),
            _ => mettre_de_cote(chemin, quand).then(Vec::new),
        },
        Err(e) if e.kind() == ErrorKind::NotFound => Some(Vec::new()),
        Err(e) if e.kind() == ErrorKind::InvalidData => mettre_de_cote(chemin, quand).then(Vec::new),
        Err(_) => None,
    }
}

/// Copie un journal illisible à côté de lui ; faux si la copie n'a pas pu se faire.
fn mettre_de_cote(chemin: &Path, quand: DateTime<Local>) -> bool {
    let Some(dossier) = chemin.parent() else {
        return false;
    };
    let horodatage = quand.format("%Y%m%d-%H%M%S");
    let mut n = 1u32;
    loop {
        let suffixe = if n == 1 { String::new() } else { format!("-{n}") };
        let cible = dossier.join(format!("{PREFIXE_ILLISIBLE}{horodatage}{suffixe}.json"));
        match copier_sans_ecraser(chemin, &cible) {
            Ok(()) => return true,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => n += 1,
            Err(_) => return false,
        }
    }
}

/// Le texte du fichier, comme l'écrit la v2 : indenté d'un espace, accents en clair.
fn texte(entrees: &[Value]) -> Result<Vec<u8>, serde_json::Error> {
    let mut sortie = Vec::new();
    let format = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut serialiseur = serde_json::Serializer::with_formatter(&mut sortie, format);
    entrees.serialize(&mut serialiseur)?;
    Ok(sortie)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Un journal tel que l'écrit la v2.4, la plus récente en premier.
    const JOURNAL_V2: &str = r#"[
 {"date": "2026-11-14T10:12:03", "action": "Modèle mis à la corbeille", "objet": "image/x.safetensors",
  "resultat": "ok"},
 {"date": "2026-11-14T09:00:00", "action": "Téléchargement", "objet": "flux.gguf", "resultat": "erreur",
  "detail": "connexion coupée", "inconnue": 7},
 "pas une entrée",
 {"date": "2026-11-13T08:00:00", "action": "Migration des réglages", "objet": "aria.yaml", "resultat": "ok"}
]"#;

    fn date() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 22, 15, 4, 5).unwrap()
    }

    fn dossier() -> (tempfile::TempDir, Dossier) {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        (temp, dossier)
    }

    fn brut(dossier: &Dossier) -> Vec<Value> {
        serde_json::from_str(&fs::read_to_string(dossier.journal()).unwrap()).unwrap()
    }

    #[test]
    fn noter_ajoute_en_tete() {
        let (_temp, dossier) = dossier();
        noter(&dossier, "Aria lancée", "premier", Resultat::Ok, None);
        noter(&dossier, "Réglages modifiés", "second", Resultat::Ok, None);
        let ops = lire(&dossier);
        assert_eq!(ops.len(), 2);
        assert_eq!(
            (ops[0].action.as_str(), ops[1].objet.as_str()),
            ("Réglages modifiés", "premier")
        );
    }

    #[test]
    fn une_entree_a_le_format_de_la_v2() {
        let (_temp, dossier) = dossier();
        noter_a(
            &dossier,
            "Téléchargement",
            "flux.gguf",
            Resultat::Erreur,
            Some("coupé"),
            date(),
        );
        let entrees = brut(&dossier);
        assert_eq!(
            entrees[0],
            serde_json::json!({
                "date": "2026-11-22T15:04:05",
                "action": "Téléchargement",
                "objet": "flux.gguf",
                "resultat": "erreur",
                "detail": "coupé"
            })
        );
        noter_a(&dossier, "Aria lancée", "", Resultat::Ok, None, date());
        assert!(brut(&dossier)[0].get("detail").is_none());
        assert!(fs::read_to_string(dossier.journal()).unwrap().contains("\n {"));
    }

    #[test]
    fn le_journal_de_la_v2_est_lu() {
        let (_temp, dossier) = dossier();
        fs::write(dossier.journal(), JOURNAL_V2).unwrap();
        let ops = lire(&dossier);
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].action, "Modèle mis à la corbeille");
        assert_eq!(ops[1].resultat, Resultat::Erreur);
        assert_eq!(ops[1].detail.as_deref(), Some("connexion coupée"));
    }

    #[test]
    fn les_entrees_de_la_v2_sont_gardees_telles_quelles() {
        let (_temp, dossier) = dossier();
        fs::write(dossier.journal(), JOURNAL_V2).unwrap();
        let avant: Vec<Value> = serde_json::from_str(JOURNAL_V2).unwrap();
        noter_a(
            &dossier,
            "Aria lancée",
            "Aria Studio 3.0.0",
            Resultat::Ok,
            None,
            date(),
        );
        let apres = brut(&dossier);
        let objets: Vec<&Value> = avant.iter().filter(|e| e.is_object()).collect();
        assert_eq!(apres.len(), objets.len() + 1);
        for (i, entree) in objets.iter().enumerate() {
            assert_eq!(apres[i + 1], **entree, "entrée {i}");
        }
        assert_eq!(apres[2]["inconnue"], Value::from(7));
    }

    #[test]
    fn le_journal_garde_les_500_dernieres() {
        let (_temp, dossier) = dossier();
        for n in 0..OPERATIONS_MAX + 3 {
            noter(&dossier, "Essai", &n.to_string(), Resultat::Ok, None);
        }
        let ops = lire(&dossier);
        assert_eq!(ops.len(), OPERATIONS_MAX);
        assert_eq!(ops[0].objet, (OPERATIONS_MAX + 2).to_string());
        assert_eq!(ops[OPERATIONS_MAX - 1].objet, "3");
    }

    #[test]
    fn un_journal_illisible_est_mis_de_cote_puis_recommence() {
        let (temp, dossier) = dossier();
        fs::write(dossier.journal(), "{ abîmé").unwrap();
        assert!(lire(&dossier).is_empty());
        noter_a(&dossier, "Essai", "repart", Resultat::Ok, None, date());
        assert_eq!(lire(&dossier).len(), 1);
        let copie = temp.path().join("journal-illisible-20261122-150405.json");
        assert_eq!(fs::read_to_string(copie).unwrap(), "{ abîmé");
    }

    #[test]
    fn un_detail_trop_long_est_coupe() {
        let (_temp, dossier) = dossier();
        let long = "é".repeat(DETAIL_MAX + 20);
        noter(&dossier, "Essai", "", Resultat::Ok, Some(&long));
        let detail = lire(&dossier)[0].detail.clone().unwrap();
        assert_eq!(detail.chars().count(), DETAIL_MAX);
    }

    #[test]
    fn un_resultat_inconnu_donne_ok() {
        let op: Operation =
            serde_json::from_str(r#"{"date": "d", "action": "a", "resultat": "bizarre"}"#).unwrap();
        assert_eq!(op.resultat, Resultat::Ok);
        assert_eq!(op.objet, "");
    }

    #[test]
    fn un_dossier_impossible_ne_fait_pas_echouer() {
        let temp = tempfile::tempdir().unwrap();
        let fichier = temp.path().join("pas-un-dossier");
        fs::write(&fichier, "x").unwrap();
        noter(
            &Dossier::a(fichier.join("dessous")),
            "Essai",
            "rien",
            Resultat::Ok,
            None,
        );
    }
}
