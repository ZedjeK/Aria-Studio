//! Le dossier de données, `%USERPROFILE%\.Aria-Studio`, et ce qu'il contient. Tout ce que l'appli écrit va là :
//! rien dans `%APPDATA%`, `%LOCALAPPDATA%` ni `%TEMP%`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{Erreur, identite};

/// Les réglages.
pub const FICHIER_REGLAGES: &str = "reglages.json";
/// Les 500 dernières opérations.
pub const FICHIER_JOURNAL: &str = "journal.json";
/// Une copie datée de chaque fichier avant qu'il soit modifié.
pub const DOSSIER_SAUVEGARDES: &str = "sauvegardes";
/// Le cache de l'interface, dont celui de WebView2.
pub const DOSSIER_CACHE: &str = "cache";
/// Les secrets, chiffrés par DPAPI pour le compte de l'utilisateur.
pub const DOSSIER_SECRETS: &str = "secrets";
/// L'exe installé et ce qu'il livre, pour une appli qui s'installe.
pub const DOSSIER_PROGRAMME: &str = "programme";

/// Le dossier de données d'une appli, et les chemins de ce qu'il contient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dossier {
    racine: PathBuf,
}

impl Dossier {
    /// Le dossier de données dans le profil de l'utilisateur : `USERPROFILE` sous Windows, `HOME` ailleurs.
    ///
    /// # Errors
    /// [`Erreur::ProfilIntrouvable`] si aucune des deux variables n'est définie.
    pub fn du_profil() -> Result<Self, Erreur> {
        let profil = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .filter(|p| !p.is_empty())
            .ok_or(Erreur::ProfilIntrouvable)?;
        Ok(Self::a(PathBuf::from(profil).join(identite::DOSSIER_DONNEES)))
    }

    /// Un dossier de données à un endroit donné (tests, outils).
    pub fn a(racine: impl Into<PathBuf>) -> Self {
        Self {
            racine: racine.into(),
        }
    }

    /// Crée le dossier et ses sous-dossiers s'ils manquent.
    ///
    /// # Errors
    /// [`Erreur::Fichier`] si un dossier ne peut pas être créé.
    pub fn preparer(&self) -> Result<(), Erreur> {
        for dossier in [
            self.racine.clone(),
            self.sauvegardes(),
            self.cache(),
            self.secrets(),
        ] {
            fs::create_dir_all(&dossier).map_err(|e| Erreur::fichier("création de", dossier, e))?;
        }
        Ok(())
    }

    /// La racine du dossier de données.
    pub fn racine(&self) -> &Path {
        &self.racine
    }

    /// `reglages.json`.
    pub fn reglages(&self) -> PathBuf {
        self.racine.join(FICHIER_REGLAGES)
    }

    /// `journal.json`.
    pub fn journal(&self) -> PathBuf {
        self.racine.join(FICHIER_JOURNAL)
    }

    /// `sauvegardes\`.
    pub fn sauvegardes(&self) -> PathBuf {
        self.racine.join(DOSSIER_SAUVEGARDES)
    }

    /// `cache\`.
    pub fn cache(&self) -> PathBuf {
        self.racine.join(DOSSIER_CACHE)
    }

    /// `cache\webview\` : le dossier de WebView2, qui sinon irait dans `%LOCALAPPDATA%`.
    pub fn cache_webview(&self) -> PathBuf {
        self.cache().join("webview")
    }

    /// `secrets\`.
    pub fn secrets(&self) -> PathBuf {
        self.racine.join(DOSSIER_SECRETS)
    }

    /// `programme\`.
    pub fn programme(&self) -> PathBuf {
        self.racine.join(DOSSIER_PROGRAMME)
    }
}

/// Écrit un fichier en entier sans jamais le laisser à moitié écrit : à côté d'abord, puis remplacement.
///
/// # Errors
/// [`Erreur::Fichier`] si l'écriture ou le remplacement échoue ; le fichier d'origine reste alors intact.
pub fn ecrire_entier(chemin: &Path, contenu: &[u8]) -> Result<(), Erreur> {
    if let Some(parent) = chemin.parent() {
        fs::create_dir_all(parent).map_err(|e| Erreur::fichier("création de", parent, e))?;
    }
    let mut provisoire = chemin.as_os_str().to_owned();
    provisoire.push(".tmp");
    let provisoire = PathBuf::from(provisoire);
    let resultat = (|| {
        let mut fichier = fs::File::create(&provisoire)?;
        fichier.write_all(contenu)?;
        fichier.sync_all()
    })();
    resultat.map_err(|e| Erreur::fichier("écriture de", &provisoire, e))?;
    // Sous Windows, rename remplace la cible existante (MoveFileEx avec MOVEFILE_REPLACE_EXISTING).
    fs::rename(&provisoire, chemin).map_err(|e| Erreur::fichier("remplacement de", chemin, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparer_cree_les_sous_dossiers() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path().join(".Aria-Essai"));
        dossier.preparer().unwrap();
        for d in [
            dossier.racine().to_path_buf(),
            dossier.sauvegardes(),
            dossier.cache(),
            dossier.secrets(),
        ] {
            assert!(d.is_dir(), "{} manque", d.display());
        }
        // Le dossier programme\ n'existe que pour une appli installée.
        assert!(!dossier.programme().exists());
    }

    #[test]
    fn preparer_deux_fois_ne_casse_rien() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        dossier.preparer().unwrap();
        dossier.preparer().unwrap();
    }

    #[test]
    fn le_cache_de_webview2_est_dans_le_dossier_de_donnees() {
        let dossier = Dossier::a("/x/.Aria-Essai");
        assert!(dossier.cache_webview().starts_with(dossier.racine()));
    }

    #[test]
    fn ecrire_entier_remplace_sans_laisser_de_fichier_provisoire() {
        let temp = tempfile::tempdir().unwrap();
        let chemin = temp.path().join("a.json");
        ecrire_entier(&chemin, b"un").unwrap();
        ecrire_entier(&chemin, b"deux").unwrap();
        assert_eq!(fs::read_to_string(&chemin).unwrap(), "deux");
        assert!(!temp.path().join("a.json.tmp").exists());
    }
}
