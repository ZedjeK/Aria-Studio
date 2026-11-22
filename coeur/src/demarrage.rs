//! « Démarrer avec Windows » : une valeur au nom de l'exe dans `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
//! qui lance l'exe avec [`ARGUMENT_SILENCIEUX`]. Désactivé par défaut, sans droits administrateur. L'état est toujours
//! relu dans le registre, jamais retenu : l'utilisateur peut le changer ailleurs, notamment dans la page
//! « Applications de démarrage » de Windows (`StartupApproved`).
//!
//! Les règles se testent partout ; l'accès au registre ([`windows`]) seulement sous Windows.

use std::path::Path;

use crate::identite;

/// Argument du lancement à l'ouverture de session : l'icône seule, la fenêtre reste rangée.
pub const ARGUMENT_SILENCIEUX: &str = "--silencieux";

/// La clé des programmes lancés à l'ouverture de session.
pub const CLE_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// La clé où Windows note qu'une entrée a été désactivée dans « Applications de démarrage ».
pub const CLE_APPROUVES: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// Le nom de la valeur dans le registre : celui de l'exe.
pub const NOM_VALEUR: &str = identite::NOM_EXE;

/// La ligne de commande enregistrée : l'exe entre guillemets, puis l'argument silencieux.
pub fn commande_pour(exe: &Path) -> String {
    format!("\"{}\" {ARGUMENT_SILENCIEUX}", exe.display())
}

/// L'appli démarrera-t-elle avec Windows ?
///
/// `run` : la commande lue sous [`CLE_RUN`] ; `approuve` : les octets lus sous [`CLE_APPROUVES`]. Premier octet
/// pair, ou pas de valeur (jamais touchée) : activée ; impair : désactivée dans Windows.
pub fn est_actif(run: Option<&str>, approuve: Option<&[u8]>) -> bool {
    let inscrit = run.is_some_and(|commande| !commande.trim().is_empty());
    let autorise = approuve
        .and_then(<[u8]>::first)
        .is_none_or(|octet| octet % 2 == 0);
    inscrit && autorise
}

/// Le lancement a-t-il demandé le mode silencieux (icône seule, fenêtre rangée) ?
pub fn est_silencieux<I, S>(arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    arguments
        .into_iter()
        .any(|a| a.as_ref().eq_ignore_ascii_case(ARGUMENT_SILENCIEUX))
}

/// L'accès au registre, sous `HKEY_CURRENT_USER` : sans droits administrateur.
#[cfg(windows)]
pub mod windows {
    use std::io;
    use std::path::Path;

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    use super::{CLE_APPROUVES, CLE_RUN, NOM_VALEUR, commande_pour, est_actif};

    /// Windows lancera-t-il l'appli à l'ouverture de session ? Relu à chaque fois.
    pub fn est_inscrit() -> bool {
        est_inscrit_sous(CLE_RUN, CLE_APPROUVES)
    }

    /// Lance cet exe à l'ouverture de session. Lève aussi une désactivation faite dans Windows : l'utilisateur
    /// vient de cocher la case.
    ///
    /// # Errors
    /// L'erreur du registre.
    pub fn inscrire(exe: &Path) -> io::Result<()> {
        inscrire_sous(CLE_RUN, CLE_APPROUVES, exe)
    }

    /// Ne lance plus l'appli à l'ouverture de session.
    ///
    /// # Errors
    /// L'erreur du registre.
    pub fn desinscrire() -> io::Result<()> {
        desinscrire_sous(CLE_RUN)
    }

    pub(crate) fn est_inscrit_sous(run: &str, approuves: &str) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let commande: Option<String> = hkcu
            .open_subkey(run)
            .ok()
            .and_then(|cle| cle.get_value(NOM_VALEUR).ok());
        let approuve = hkcu
            .open_subkey(approuves)
            .ok()
            .and_then(|cle| cle.get_raw_value(NOM_VALEUR).ok())
            .map(|valeur| valeur.bytes.to_vec());
        est_actif(commande.as_deref(), approuve.as_deref())
    }

    pub(crate) fn inscrire_sous(run: &str, approuves: &str, exe: &Path) -> io::Result<()> {
        if !exe.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "chemin de l'exe attendu en entier",
            ));
        }
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (cle, _) = hkcu.create_subkey(run)?;
        cle.set_value(NOM_VALEUR, &commande_pour(exe))?;
        if let Ok(approuve) = hkcu.open_subkey_with_flags(approuves, KEY_SET_VALUE) {
            supprimer_valeur(&approuve)?;
        }
        Ok(())
    }

    pub(crate) fn desinscrire_sous(run: &str) -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey_with_flags(run, KEY_SET_VALUE) {
            Ok(cle) => supprimer_valeur(&cle),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Supprime la valeur ; sans effet si elle n'existe pas.
    fn supprimer_valeur(cle: &RegKey) -> io::Result<()> {
        match cle.delete_value(NOM_VALEUR) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Des clés à part sous HKCU, jamais celles que lit Windows ; effacées à la fin du test.
        struct ClesDeTest {
            racine: String,
            run: String,
            approuves: String,
        }

        impl ClesDeTest {
            fn nouvelles(nom: &str) -> Self {
                let racine = format!(r"Software\Aria-Essais-{}-{nom}", std::process::id());
                Self {
                    run: format!(r"{racine}\Run"),
                    approuves: format!(r"{racine}\Approuves"),
                    racine,
                }
            }
        }

        impl Drop for ClesDeTest {
            fn drop(&mut self) {
                let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.racine);
            }
        }

        #[test]
        fn inscrire_puis_desinscrire() {
            let cles = ClesDeTest::nouvelles("cycle");
            let exe = Path::new(r"C:\Essai\Aria-Essai.exe");
            assert!(!est_inscrit_sous(&cles.run, &cles.approuves));
            inscrire_sous(&cles.run, &cles.approuves, exe).unwrap();
            assert!(est_inscrit_sous(&cles.run, &cles.approuves));
            let lue: String = RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey(&cles.run)
                .unwrap()
                .get_value(NOM_VALEUR)
                .unwrap();
            assert_eq!(lue, commande_pour(exe));
            desinscrire_sous(&cles.run).unwrap();
            desinscrire_sous(&cles.run).unwrap();
            assert!(!est_inscrit_sous(&cles.run, &cles.approuves));
        }

        #[test]
        fn cocher_leve_la_desactivation_faite_dans_windows() {
            let cles = ClesDeTest::nouvelles("approuve");
            let exe = Path::new(r"C:\Essai\Aria-Essai.exe");
            inscrire_sous(&cles.run, &cles.approuves, exe).unwrap();
            let (approuve, _) = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(&cles.approuves)
                .unwrap();
            let desactivee = winreg::RegValue {
                vtype: winreg::enums::RegType::REG_BINARY,
                bytes: vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into(),
            };
            approuve.set_raw_value(NOM_VALEUR, &desactivee).unwrap();
            assert!(!est_inscrit_sous(&cles.run, &cles.approuves));
            inscrire_sous(&cles.run, &cles.approuves, exe).unwrap();
            assert!(est_inscrit_sous(&cles.run, &cles.approuves));
        }

        #[test]
        fn un_chemin_relatif_est_refuse() {
            let cles = ClesDeTest::nouvelles("relatif");
            assert!(inscrire_sous(&cles.run, &cles.approuves, Path::new("Aria-Essai.exe")).is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_commande_met_l_exe_entre_guillemets() {
        let commande = commande_pour(Path::new(r"C:\Users\moi\.Aria-Essai\programme\Aria-Essai.exe"));
        assert_eq!(
            commande,
            r#""C:\Users\moi\.Aria-Essai\programme\Aria-Essai.exe" --silencieux"#
        );
    }

    #[test]
    fn sans_entree_dans_run_rien_ne_demarre() {
        assert!(!est_actif(None, None));
        assert!(!est_actif(Some("  "), None));
    }

    #[test]
    fn une_entree_jamais_touchee_dans_windows_est_active() {
        assert!(est_actif(Some("\"a.exe\" --silencieux"), None));
        assert!(est_actif(Some("\"a.exe\""), Some(&[])));
    }

    #[test]
    fn startup_approved_impair_desactive() {
        assert!(est_actif(Some("a"), Some(&[2, 0, 0])));
        assert!(!est_actif(Some("a"), Some(&[3, 0, 0])));
    }

    #[test]
    fn le_mode_silencieux_se_lit_dans_les_arguments() {
        assert!(est_silencieux(["Aria-Essai.exe", "--silencieux"]));
        assert!(est_silencieux(["Aria-Essai.exe", "--SILENCIEUX"]));
        assert!(!est_silencieux(["Aria-Essai.exe"]));
    }
}
