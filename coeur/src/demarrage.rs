//! « Démarrer avec Windows » : une valeur « Aria Studio » (comme la v2) dans `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
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

/// Le nom de la valeur dans le registre : « Aria Studio », comme la v2. Une appli qui passe de la v2 à la v3 garde
/// sa case cochée, et n'ouvre pas une seconde entrée de démarrage.
pub const NOM_VALEUR: &str = identite::NOM;

/// Le nom que les versions d'essai de la v3 donnaient à la valeur (celui de l'exe) : encore lu, et retiré à la
/// prochaine écriture. Le même que [`NOM_VALEUR`] pour une appli dont le nom n'a pas d'espace.
pub const NOM_VALEUR_ESSAI: &str = identite::NOM_EXE;

/// Les noms sous lesquels l'appli peut être inscrite : le courant d'abord.
pub fn noms_de_valeur() -> Vec<&'static str> {
    if NOM_VALEUR_ESSAI == NOM_VALEUR {
        vec![NOM_VALEUR]
    } else {
        vec![NOM_VALEUR, NOM_VALEUR_ESSAI]
    }
}

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

/// Un second lancement doit-il ramener la fenêtre de l'instance qui tourne ? Oui, sauf à l'ouverture de session
/// (`--silencieux`) : l'icône est déjà là, la fenêtre reste rangée. `arguments` : la ligne de commande du second
/// lancement, nom de l'exe compris (c'est ce que reçoit le greffon d'instance unique).
pub fn doit_ramener(arguments: &[String]) -> bool {
    !est_silencieux(arguments.iter().skip(1))
}

/// L'accès au registre, sous `HKEY_CURRENT_USER` : sans droits administrateur.
#[cfg(windows)]
pub mod windows {
    use std::io;
    use std::path::Path;

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    use super::{CLE_APPROUVES, CLE_RUN, NOM_VALEUR, commande_pour, est_actif, noms_de_valeur};

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
        noms_de_valeur()
            .into_iter()
            .any(|nom| est_inscrit_sous_le_nom(run, approuves, nom))
    }

    fn est_inscrit_sous_le_nom(run: &str, approuves: &str, nom: &str) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let commande: Option<String> = hkcu.open_subkey(run).ok().and_then(|cle| cle.get_value(nom).ok());
        let approuve = hkcu
            .open_subkey(approuves)
            .ok()
            .and_then(|cle| cle.get_raw_value(nom).ok())
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
        // Une valeur d'une version d'essai ne doit pas lancer l'appli une seconde fois.
        for ancien in noms_de_valeur().into_iter().skip(1) {
            supprimer_valeur(&cle, ancien)?;
        }
        if let Ok(approuve) = hkcu.open_subkey_with_flags(approuves, KEY_SET_VALUE) {
            for nom in noms_de_valeur() {
                supprimer_valeur(&approuve, nom)?;
            }
        }
        Ok(())
    }

    pub(crate) fn desinscrire_sous(run: &str) -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey_with_flags(run, KEY_SET_VALUE) {
            Ok(cle) => noms_de_valeur()
                .into_iter()
                .try_for_each(|nom| supprimer_valeur(&cle, nom)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Supprime la valeur ; sans effet si elle n'existe pas.
    fn supprimer_valeur(cle: &RegKey, nom: &str) -> io::Result<()> {
        match cle.delete_value(nom) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::demarrage::NOM_VALEUR_ESSAI;

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
        fn la_valeur_a_le_nom_de_la_v2() {
            assert_eq!(NOM_VALEUR, "Aria Studio");
        }

        #[test]
        fn une_valeur_d_une_version_d_essai_est_reprise_puis_remplacee() {
            let cles = ClesDeTest::nouvelles("essai");
            let ancien = Path::new(r"C:\Zip\Aria-Studio.exe");
            let (run, _) = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(&cles.run)
                .unwrap();
            run.set_value(NOM_VALEUR_ESSAI, &commande_pour(ancien)).unwrap();
            // Une version d'essai l'avait cochée : la case reste cochée.
            assert!(est_inscrit_sous(&cles.run, &cles.approuves));
            // Cocher de nouveau écrit la valeur « Aria Studio » vers l'exe installé, sans laisser l'ancienne.
            let installe = Path::new(r"C:\Users\moi\.Aria-Studio\programme\Aria-Studio.exe");
            inscrire_sous(&cles.run, &cles.approuves, installe).unwrap();
            let lue: String = run.get_value(NOM_VALEUR).unwrap();
            assert_eq!(lue, commande_pour(installe));
            assert!(run.get_value::<String, _>(NOM_VALEUR_ESSAI).is_err());
            // Décocher retire aussi l'ancienne valeur, si une version d'essai en avait laissé une.
            run.set_value(NOM_VALEUR_ESSAI, &commande_pour(ancien)).unwrap();
            desinscrire_sous(&cles.run).unwrap();
            assert!(!est_inscrit_sous(&cles.run, &cles.approuves));
            assert!(run.get_value::<String, _>(NOM_VALEUR_ESSAI).is_err());
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

    #[test]
    fn le_nom_de_la_valeur_est_celui_de_la_v2_et_l_essai_est_reconnu() {
        assert_eq!(NOM_VALEUR, "Aria Studio");
        assert_eq!(noms_de_valeur(), vec!["Aria Studio", "Aria-Studio"]);
    }

    fn ligne(arguments: &[&str]) -> Vec<String> {
        arguments.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn relancer_l_exe_ramene_la_fenetre() {
        assert!(doit_ramener(&ligne(&["Aria-Essai.exe"])));
        assert!(doit_ramener(&ligne(&["Aria-Essai.exe", "--portable"])));
    }

    #[test]
    fn un_lancement_silencieux_ne_fait_pas_surgir_la_fenetre() {
        assert!(!doit_ramener(&ligne(&["Aria-Essai.exe", "--silencieux"])));
        assert!(!doit_ramener(&ligne(&[
            "Aria-Essai.exe",
            "--portable",
            "--SILENCIEUX"
        ])));
    }

    #[test]
    fn le_nom_de_l_exe_n_est_pas_un_argument() {
        assert!(doit_ramener(&ligne(&["--silencieux"])));
        assert!(doit_ramener(&[]));
    }
}
