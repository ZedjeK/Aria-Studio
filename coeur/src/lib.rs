//! Le cœur d'Aria Studio : ce que toute appli Aria doit tenir, sans rien qui dépende de l'interface.
//!
//! Tout ici se teste sur n'importe quel système, sauf les accès à Windows (registre, DPAPI), marqués
//! `cfg(windows)` et testés par la CI Windows.

pub mod carte;
pub mod catalogue;
pub mod cles;
pub mod collection;
pub mod comfy;
pub mod comfy_serveur;
pub mod creations;
pub mod demarrage;
pub mod donnees;
pub mod emplacements;
pub mod fiches;
pub mod garde;
pub mod gguf;
pub mod identite;
pub mod images;
pub mod installation;
pub mod installations;
pub mod journal;
pub mod llama;
pub mod llama_serveur;
pub mod llm;
pub mod local;
pub mod mises_a_jour;
pub mod modeles;
pub mod moteurs;
pub mod orchestrateur;
pub mod plateau;
pub mod presence;
pub mod prompt;
pub mod reglages;
pub mod reglages_fins;
pub mod retouche;
pub mod rushes;
pub mod sauvegardes;
pub mod secrets;
pub mod securite;
pub mod securite_reglages;
pub mod telechargement;
pub mod workflows;

pub use donnees::Dossier;
pub use reglages::{Reglages, ReglagesFond, Vitesse};

use std::path::PathBuf;

/// Les erreurs du cœur, en français : elles peuvent être montrées telles quelles à l'utilisateur.
#[derive(Debug, thiserror::Error)]
pub enum Erreur {
    /// Ni `USERPROFILE` ni `HOME` : impossible de savoir où ranger le dossier de données.
    #[error("le dossier de l'utilisateur est introuvable (ni USERPROFILE ni HOME)")]
    ProfilIntrouvable,

    /// Une lecture ou une écriture a échoué ; le chemin dit où.
    #[error("{action} {chemin} : {source}")]
    Fichier {
        action: &'static str,
        chemin: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Le fichier à sauvegarder n'existe pas.
    #[error("fichier à sauvegarder introuvable : {0}")]
    AbsentASauvegarder(PathBuf),

    /// Un nom de secret qui pourrait sortir du dossier des secrets.
    #[error("nom de secret refusé : « {0} » (lettres minuscules, chiffres et tirets, 64 caractères au plus)")]
    NomDeSecret(String),
}

impl Erreur {
    /// Raccourci pour une erreur de fichier.
    pub(crate) fn fichier(action: &'static str, chemin: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Fichier {
            action,
            chemin: chemin.into(),
            source,
        }
    }
}
