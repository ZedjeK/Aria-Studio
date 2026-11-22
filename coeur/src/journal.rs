//! `journal.json`, dans le dossier de données : les 500 dernières opérations, de la plus ancienne à la plus récente.
//! Jamais de secret dans un bilan.

use std::fs;
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Local, SecondsFormat};
use serde::{Deserialize, Serialize};

use crate::{Dossier, donnees};

/// Le plus d'opérations gardées.
pub const OPERATIONS_MAX: usize = 500;

/// Une opération du journal : quand, quoi, et son bilan en une ligne.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    /// Date et heure locales, avec leur décalage (`2026-11-21T19:05:00+01:00`).
    pub date: String,
    /// Le genre d'opération : « lancement », « réglages », « sauvegarde »…
    #[serde(rename = "type")]
    pub genre: String,
    /// Ce qui s'est passé, en une ligne.
    pub bilan: String,
}

/// Deux écritures du journal en même temps n'en perdent aucune.
static VERROU: Mutex<()> = Mutex::new(());

/// Ajoute une opération au journal. Ne renvoie pas d'erreur : un journal qui ne s'écrit pas ne doit pas faire
/// échouer l'opération elle-même.
pub fn consigner(dossier: &Dossier, genre: &str, bilan: &str) {
    consigner_a(dossier, genre, bilan, Local::now());
}

/// [`consigner`], à une date donnée (tests).
pub fn consigner_a(dossier: &Dossier, genre: &str, bilan: &str, quand: DateTime<Local>) {
    let _verrou = VERROU.lock().unwrap_or_else(PoisonError::into_inner);
    let mut operations = lire(dossier);
    operations.push(Operation {
        date: quand.to_rfc3339_opts(SecondsFormat::Secs, false),
        genre: genre.to_owned(),
        bilan: bilan.to_owned(),
    });
    if operations.len() > OPERATIONS_MAX {
        operations.drain(..operations.len() - OPERATIONS_MAX);
    }
    if let Ok(texte) = serde_json::to_string_pretty(&operations) {
        // Dossier en lecture seule, fichier verrouillé : l'opération a eu lieu quand même.
        let _ = donnees::ecrire_entier(&dossier.journal(), texte.as_bytes());
    }
}

/// Opérations enregistrées, de la plus ancienne à la plus récente ; aucune si le fichier est absent ou illisible.
pub fn lire(dossier: &Dossier) -> Vec<Operation> {
    fs::read_to_string(dossier.journal())
        .ok()
        .and_then(|texte| serde_json::from_str(&texte).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consigner_ajoute_a_la_fin() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        consigner(&dossier, "lancement", "premier");
        consigner(&dossier, "réglages", "second");
        let ops = lire(&dossier);
        assert_eq!(ops.len(), 2);
        assert_eq!(
            (ops[0].genre.as_str(), ops[1].bilan.as_str()),
            ("lancement", "second")
        );
    }

    #[test]
    fn le_journal_garde_les_500_dernieres() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        for n in 0..OPERATIONS_MAX + 3 {
            consigner(&dossier, "essai", &n.to_string());
        }
        let ops = lire(&dossier);
        assert_eq!(ops.len(), OPERATIONS_MAX);
        assert_eq!(ops[0].bilan, "3");
    }

    #[test]
    fn un_journal_illisible_repart_de_zero() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        fs::write(dossier.journal(), "{ abîmé").unwrap();
        assert!(lire(&dossier).is_empty());
        consigner(&dossier, "essai", "repart");
        assert_eq!(lire(&dossier).len(), 1);
    }

    #[test]
    fn la_date_porte_son_decalage() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        consigner(&dossier, "essai", "date");
        let date = &lire(&dossier)[0].date;
        assert!(DateTime::parse_from_rfc3339(date).is_ok(), "{date}");
    }

    #[test]
    fn un_dossier_impossible_ne_fait_pas_echouer() {
        let temp = tempfile::tempdir().unwrap();
        let fichier = temp.path().join("pas-un-dossier");
        fs::write(&fichier, "x").unwrap();
        consigner(&Dossier::a(fichier.join("dessous")), "essai", "rien");
    }
}
