//! Les clés d'accès aux sources de modèles : Civitai et Hugging Face.
//!
//! Elles sont chiffrées par DPAPI dans `secrets\` du dossier de données ([`crate::secrets::Coffre`]), comme le veut
//! le contrat des applis Aria. Repli : les variables d'environnement `CIVITAI_TOKEN` et `HF_TOKEN`. Une clé ne
//! repart jamais vers l'interface, ni dans le journal.
//!
//! La version 2 d'Aria Studio les rangeait dans le Gestionnaire d'identification Windows, sous les cibles
//! `Aria-Studio/civitai` et `Aria-Studio/huggingface`. Au lancement, [`reprendre_de_la_v2`] recopie dans `secrets\`
//! chaque clé qui n'y est pas encore, et **laisse l'entrée du Gestionnaire en place** : une v2 restaurée depuis
//! `programme.precedent\` retrouve ainsi ses clés. Cette entrée ne sera retirée qu'avec l'accord de l'utilisateur.

use std::io;

use crate::secrets::{Coffre, Protection};

/// La longueur d'une clé, au plus (comme en v2).
pub const LONGUEUR_MAX: usize = 512;

/// Le début des cibles de la v2 dans le Gestionnaire d'identification Windows.
const PREFIXE_CIBLE_V2: &str = "Aria-Studio/";

/// Une source de modèles qui demande une clé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Civitai,
    HuggingFace,
}

impl Source {
    /// Toutes les sources, dans l'ordre de la page Réglages.
    pub const TOUTES: [Self; 2] = [Self::Civitai, Self::HuggingFace];

    /// Le nom affiché.
    pub fn nom(self) -> &'static str {
        match self {
            Self::Civitai => "Civitai",
            Self::HuggingFace => "Hugging Face",
        }
    }

    /// Le nom du secret dans `secrets\`, et la fin de la cible de la v2.
    pub fn nom_secret(self) -> &'static str {
        match self {
            Self::Civitai => "civitai",
            Self::HuggingFace => "huggingface",
        }
    }

    /// La variable d'environnement de repli.
    pub fn variable(self) -> &'static str {
        match self {
            Self::Civitai => "CIVITAI_TOKEN",
            Self::HuggingFace => "HF_TOKEN",
        }
    }

    /// La cible de la v2 dans le Gestionnaire d'identification Windows.
    pub fn cible_v2(self) -> String {
        format!("{PREFIXE_CIBLE_V2}{}", self.nom_secret())
    }
}

/// Une clé telle qu'elle sera enregistrée : sans espace autour, ni vide, ni trop longue, sans espace dedans.
///
/// # Errors
/// Un message à montrer tel quel, qui ne cite jamais la clé.
pub fn valider(valeur: &str) -> Result<String, String> {
    let cle = valeur.trim();
    if cle.is_empty() || cle.chars().count() > LONGUEUR_MAX || cle.chars().any(char::is_whitespace) {
        return Err("Colle la clé seule, sans espace ni retour à la ligne".to_owned());
    }
    Ok(cle.to_owned())
}

/// La clé à utiliser pour une source : celle de `secrets\`, sinon la variable d'environnement.
pub fn cle<P: Protection>(coffre: &Coffre<'_, P>, source: Source) -> Option<String> {
    cle_avec(coffre, source, |nom| std::env::var(nom).ok())
}

/// [`cle`], avec une lecture des variables d'environnement donnée (tests).
fn cle_avec<P: Protection>(
    coffre: &Coffre<'_, P>,
    source: Source,
    variable: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    coffre
        .lire(source.nom_secret())
        .ok()
        .flatten()
        .or_else(|| variable(source.variable()))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Le Gestionnaire d'identification Windows, lu seulement : jamais d'écriture ni de suppression ici.
pub trait AncienCoffre {
    /// La valeur rangée sous cette cible ; rien si elle n'existe pas.
    ///
    /// # Errors
    /// Le coffre ne répond pas, ou la valeur n'est pas du texte.
    fn lire(&self, cible: &str) -> io::Result<Option<String>>;
}

/// Ce que la reprise a fait pour une source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reprise {
    /// La clé était déjà dans `secrets\` : rien n'a été touché.
    DejaLa,
    /// La clé de la v2 a été recopiée dans `secrets\`.
    Reprise,
    /// Pas de clé dans le Gestionnaire d'identification.
    Absente,
    /// La reprise a échoué ; le message ne cite jamais la clé.
    Echec(String),
}

/// Recopie dans `secrets\` les clés que la v2 avait rangées dans le Gestionnaire d'identification Windows, sans les
/// retirer de celui-ci. Une clé déjà dans `secrets\` n'est jamais remplacée.
pub fn reprendre_de_la_v2<P: Protection, A: AncienCoffre>(
    coffre: &Coffre<'_, P>,
    ancien: &A,
) -> Vec<(Source, Reprise)> {
    Source::TOUTES
        .iter()
        .map(|&source| (source, reprendre(coffre, ancien, source)))
        .collect()
}

fn reprendre<P: Protection, A: AncienCoffre>(coffre: &Coffre<'_, P>, ancien: &A, source: Source) -> Reprise {
    match coffre.lire(source.nom_secret()) {
        Ok(Some(_)) => return Reprise::DejaLa,
        Ok(None) => {}
        Err(e) => return Reprise::Echec(format!("secrets\\ illisible : {e}")),
    }
    let valeur = match ancien.lire(&source.cible_v2()) {
        Ok(Some(valeur)) => valeur,
        Ok(None) => return Reprise::Absente,
        Err(e) => return Reprise::Echec(format!("le coffre Windows n'a pas rendu la clé : {e}")),
    };
    let Ok(cle) = valider(&valeur) else {
        return Reprise::Echec(
            "la clé de la v2 n'a pas la forme attendue : elle reste dans le coffre Windows".to_owned(),
        );
    };
    match coffre.enregistrer(source.nom_secret(), &cle) {
        Ok(()) => Reprise::Reprise,
        Err(e) => Reprise::Echec(format!("clé non enregistrée dans secrets\\ : {e}")),
    }
}

/// Le Gestionnaire d'identification Windows (`CredReadW`).
#[cfg(windows)]
pub struct GestionnaireIdentification;

#[cfg(windows)]
mod gestionnaire {
    use std::io;

    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW};
    use windows::core::{HRESULT, PCWSTR};

    use super::{AncienCoffre, GestionnaireIdentification};

    impl AncienCoffre for GestionnaireIdentification {
        #[allow(unsafe_code)]
        fn lire(&self, cible: &str) -> io::Result<Option<String>> {
            let large: Vec<u16> = cible.encode_utf16().chain(Some(0)).collect();
            let mut identifiant: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY : `large` est une chaîne UTF-16 terminée par un zéro, vivante pendant l'appel, que CredReadW ne
            // fait que lire ; `identifiant` reçoit une structure allouée par le système, libérée plus bas.
            let lu = unsafe {
                CredReadW(
                    PCWSTR(large.as_ptr()),
                    CRED_TYPE_GENERIC,
                    None,
                    &raw mut identifiant,
                )
            };
            if let Err(e) = lu {
                if e.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
                    return Ok(None);
                }
                return Err(io::Error::other(e));
            }
            if identifiant.is_null() {
                return Ok(None);
            }
            // SAFETY : CredReadW a réussi et rendu une structure valide ; son bloc de `CredentialBlobSize` octets est
            // recopié une seule fois, puis la structure entière est rendue au système par CredFree, comme le demande
            // sa documentation. Rien n'est lu après la libération.
            let octets = unsafe {
                let lue = &*identifiant;
                let taille = usize::try_from(lue.CredentialBlobSize).unwrap_or(0);
                let copie = if lue.CredentialBlob.is_null() || taille == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(lue.CredentialBlob, taille).to_vec()
                };
                CredFree(identifiant.cast_const().cast());
                copie
            };
            // La v2 écrivait la clé en UTF-8.
            String::from_utf8(octets)
                .map(Some)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "la valeur n'est pas du texte UTF-8"))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn une_cible_absente_se_lit_comme_absente() {
            let cible = format!("Aria-Studio-essai/absente-{}", std::process::id());
            assert_eq!(GestionnaireIdentification.lire(&cible).unwrap(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dossier;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Un chiffrement d'essai : le texte tel quel, pour tester le rangement sur tout système.
    struct EnClair;

    impl Protection for EnClair {
        fn proteger(&self, clair: &[u8]) -> io::Result<Vec<u8>> {
            Ok(clair.to_vec())
        }
        fn deproteger(&self, chiffre: &[u8]) -> io::Result<Vec<u8>> {
            Ok(chiffre.to_vec())
        }
    }

    /// Un Gestionnaire d'identification d'essai, qui retient ce qu'on lui demande.
    #[derive(Default)]
    struct CoffreV2 {
        valeurs: HashMap<String, String>,
        en_panne: bool,
        lectures: RefCell<Vec<String>>,
    }

    impl AncienCoffre for CoffreV2 {
        fn lire(&self, cible: &str) -> io::Result<Option<String>> {
            self.lectures.borrow_mut().push(cible.to_owned());
            if self.en_panne {
                return Err(io::Error::other("coffre indisponible"));
            }
            Ok(self.valeurs.get(cible).cloned())
        }
    }

    fn v2(paires: &[(&str, &str)]) -> CoffreV2 {
        CoffreV2 {
            valeurs: paires
                .iter()
                .map(|(c, v)| ((*c).to_owned(), (*v).to_owned()))
                .collect(),
            ..CoffreV2::default()
        }
    }

    #[test]
    fn les_cibles_sont_celles_de_la_v2() {
        assert_eq!(Source::Civitai.cible_v2(), "Aria-Studio/civitai");
        assert_eq!(Source::HuggingFace.cible_v2(), "Aria-Studio/huggingface");
        assert_eq!(Source::Civitai.variable(), "CIVITAI_TOKEN");
        assert_eq!(Source::HuggingFace.variable(), "HF_TOKEN");
    }

    #[test]
    fn une_cle_de_la_v2_est_reprise_dans_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let coffre = Coffre::du_dossier(&Dossier::a(temp.path()), &EnClair);
        let ancien = v2(&[("Aria-Studio/civitai", "cle-civitai")]);
        let bilan = reprendre_de_la_v2(&coffre, &ancien);
        assert_eq!(
            bilan,
            vec![
                (Source::Civitai, Reprise::Reprise),
                (Source::HuggingFace, Reprise::Absente)
            ]
        );
        assert_eq!(coffre.lire("civitai").unwrap().as_deref(), Some("cle-civitai"));
        assert_eq!(coffre.lire("huggingface").unwrap(), None);
    }

    #[test]
    fn une_cle_deja_dans_secrets_n_est_ni_remplacee_ni_redemandee() {
        let temp = tempfile::tempdir().unwrap();
        let coffre = Coffre::du_dossier(&Dossier::a(temp.path()), &EnClair);
        coffre.enregistrer("civitai", "cle-neuve").unwrap();
        let ancien = v2(&[("Aria-Studio/civitai", "cle-v2")]);
        let bilan = reprendre_de_la_v2(&coffre, &ancien);
        assert_eq!(bilan[0], (Source::Civitai, Reprise::DejaLa));
        assert_eq!(coffre.lire("civitai").unwrap().as_deref(), Some("cle-neuve"));
        assert!(
            !ancien
                .lectures
                .borrow()
                .contains(&"Aria-Studio/civitai".to_owned())
        );
    }

    #[test]
    fn un_coffre_windows_en_panne_est_rapporte_sans_rien_ecrire() {
        let temp = tempfile::tempdir().unwrap();
        let coffre = Coffre::du_dossier(&Dossier::a(temp.path()), &EnClair);
        let ancien = CoffreV2 {
            en_panne: true,
            ..CoffreV2::default()
        };
        for (_, reprise) in reprendre_de_la_v2(&coffre, &ancien) {
            assert!(matches!(reprise, Reprise::Echec(_)), "{reprise:?}");
        }
        assert_eq!(coffre.lire("civitai").unwrap(), None);
    }

    #[test]
    fn une_cle_v2_mal_formee_reste_dans_le_coffre_windows_et_n_est_pas_citee() {
        let temp = tempfile::tempdir().unwrap();
        let coffre = Coffre::du_dossier(&Dossier::a(temp.path()), &EnClair);
        let ancien = v2(&[("Aria-Studio/huggingface", "hf avec espace")]);
        let bilan = reprendre_de_la_v2(&coffre, &ancien);
        let Reprise::Echec(message) = &bilan[1].1 else {
            panic!("attendu : un échec, reçu {:?}", bilan[1]);
        };
        assert!(!message.contains("hf avec espace"), "{message}");
        assert_eq!(coffre.lire("huggingface").unwrap(), None);
    }

    #[test]
    fn la_cle_de_secrets_passe_avant_la_variable() {
        let temp = tempfile::tempdir().unwrap();
        let coffre = Coffre::du_dossier(&Dossier::a(temp.path()), &EnClair);
        let variable = |nom: &str| (nom == "HF_TOKEN").then(|| " depuis-la-variable ".to_owned());
        assert_eq!(
            cle_avec(&coffre, Source::HuggingFace, variable).as_deref(),
            Some("depuis-la-variable")
        );
        coffre.enregistrer("huggingface", "depuis-secrets").unwrap();
        assert_eq!(
            cle_avec(&coffre, Source::HuggingFace, variable).as_deref(),
            Some("depuis-secrets")
        );
        assert_eq!(cle_avec(&coffre, Source::Civitai, variable), None);
    }

    #[test]
    fn une_cle_se_valide_comme_en_v2() {
        assert_eq!(valider("  abc123  ").unwrap(), "abc123");
        for mauvaise in ["", "   ", "a b", "a\nb", &"x".repeat(LONGUEUR_MAX + 1)] {
            assert!(valider(mauvaise).is_err(), "{mauvaise:?}");
        }
        assert!(valider(&"x".repeat(LONGUEUR_MAX)).is_ok());
    }
}
