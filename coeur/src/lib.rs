//! Le cœur d'Aria Studio : ce que toute appli Aria doit tenir, sans rien qui dépende de l'interface.
//!
//! Tout ici se teste sur n'importe quel système, sauf les accès à Windows (registre, DPAPI), marqués
//! `cfg(windows)` et testés par la CI Windows.

pub mod assistant;
pub mod carte;
pub mod casting;
pub mod catalogue;
pub mod cles;
pub mod collection;
pub mod comfy;
pub mod comfy_serveur;
pub mod conversation;
pub mod creations;
pub mod decoupage;
pub mod decouvrir;
pub mod demarrage;
pub mod directions;
pub mod donnees;
pub mod emplacements;
pub mod estimation;
pub mod expliquer;
pub mod fiches;
pub mod garde;
pub mod gguf;
pub mod h3;
pub mod identite;
pub mod images;
pub mod installation;
pub mod installations;
pub mod journal;
pub mod listes;
pub mod llama;
pub mod llama_serveur;
pub mod llm;
pub mod local;
pub mod mcp;
pub mod mises_a_jour;
pub mod modeles;
pub mod montage;
pub mod moteurs;
pub mod optimiseur;
pub mod orchestrateur;
pub mod paliers;
pub mod personnages;
pub mod plateau;
pub mod presence;
pub mod prompt;
pub mod reglages;
pub mod reglages_fins;
pub mod reprise;
pub mod retouche;
pub mod retrouver;
pub mod rushes;
pub mod sauvegardes;
pub mod scenario;
pub mod secrets;
pub mod securite;
pub mod securite_reglages;
pub mod sequences;
pub mod suggestions;
pub mod telechargement;
pub mod usages;
pub mod verification;
pub mod voix;
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
