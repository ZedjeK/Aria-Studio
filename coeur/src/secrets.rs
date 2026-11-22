//! Les secrets de l'appli (jetons, mots de passe, clés), chiffrés par la protection des données de Windows (DPAPI)
//! pour le compte de l'utilisateur, un fichier par secret dans `secrets\` du dossier de données. Jamais en clair
//! dans `reglages.json`, le journal ni les sauvegardes.
//!
//! Contre quoi cette protection ne joue pas (à dire dans le README) : un programme lancé sous le même compte Windows
//! peut déchiffrer ces fichiers.
//!
//! Le chiffrement passe par [`Protection`] : [`Dpapi`] sous Windows, une imitation dans les tests, pour que le
//! rangement se teste sur n'importe quel système.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::PathBuf;

use crate::{Dossier, Erreur, donnees};

/// L'extension des fichiers de secrets.
pub const EXTENSION: &str = "dpapi";

/// Ce qui chiffre et déchiffre : DPAPI sous Windows, une imitation dans les tests.
pub trait Protection {
    /// Chiffre le texte clair.
    ///
    /// # Errors
    /// L'erreur du système si le chiffrement échoue.
    fn proteger(&self, clair: &[u8]) -> io::Result<Vec<u8>>;

    /// Déchiffre ce que [`Protection::proteger`] a produit.
    ///
    /// # Errors
    /// L'erreur du système si le déchiffrement échoue (fichier abîmé, chiffré par un autre compte).
    fn deproteger(&self, chiffre: &[u8]) -> io::Result<Vec<u8>>;
}

/// Le nom devient un nom de fichier : rien qui puisse sortir du dossier des secrets. Lettres minuscules, chiffres
/// et tirets, 64 caractères au plus, ni tiret au début ni à la fin.
pub fn nom_valide(nom: &str) -> bool {
    let octets = nom.as_bytes();
    (1..=64).contains(&octets.len())
        && octets
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && octets.first() != Some(&b'-')
        && octets.last() != Some(&b'-')
}

/// Le coffre des secrets d'une appli.
pub struct Coffre<'p, P: Protection> {
    dossier: PathBuf,
    protection: &'p P,
}

impl<'p, P: Protection> Coffre<'p, P> {
    /// Le coffre de `secrets\`, dans le dossier de données.
    pub fn du_dossier(dossier: &Dossier, protection: &'p P) -> Self {
        Self {
            dossier: dossier.secrets(),
            protection,
        }
    }

    fn chemin(&self, nom: &str) -> Result<PathBuf, Erreur> {
        if nom_valide(nom) {
            Ok(self.dossier.join(format!("{nom}.{EXTENSION}")))
        } else {
            Err(Erreur::NomDeSecret(nom.to_owned()))
        }
    }

    /// Enregistre un secret sous ce nom, en remplaçant l'ancien.
    ///
    /// # Errors
    /// Un nom refusé, un chiffrement ou une écriture qui échoue.
    pub fn enregistrer(&self, nom: &str, secret: &str) -> Result<(), Erreur> {
        let chemin = self.chemin(nom)?;
        let chiffre = self
            .protection
            .proteger(secret.as_bytes())
            .map_err(|e| Erreur::fichier("chiffrement de", &chemin, e))?;
        donnees::ecrire_entier(&chemin, &chiffre)
    }

    /// Le secret enregistré sous ce nom ; rien s'il n'existe pas ou ne se déchiffre pas (il est à redemander).
    ///
    /// # Errors
    /// Seulement un nom refusé.
    pub fn lire(&self, nom: &str) -> Result<Option<String>, Erreur> {
        let chemin = self.chemin(nom)?;
        let secret = fs::read(&chemin)
            .ok()
            .and_then(|chiffre| self.protection.deproteger(&chiffre).ok())
            .and_then(|clair| String::from_utf8(clair).ok());
        Ok(secret)
    }

    /// Supprime le secret ; sans effet s'il n'existe pas.
    ///
    /// # Errors
    /// Un nom refusé, ou une suppression qui échoue.
    pub fn supprimer(&self, nom: &str) -> Result<(), Erreur> {
        let chemin = self.chemin(nom)?;
        match fs::remove_file(&chemin) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(Erreur::fichier("suppression de", chemin, e)),
            _ => Ok(()),
        }
    }
}

/// DPAPI, pour le compte de l'utilisateur, liée à cette appli : un secret d'une autre appli Aria ne se déchiffre
/// pas ici par erreur.
#[cfg(windows)]
pub struct Dpapi;

#[cfg(windows)]
mod dpapi {
    use std::io;

    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };
    use windows::core::PCWSTR;

    use super::{Dpapi, Protection};
    use crate::identite;

    /// Un bloc d'octets à donner à DPAPI, qui ne fait que le lire.
    fn bloc(octets: &[u8]) -> io::Result<CRYPT_INTEGER_BLOB> {
        let taille = u32::try_from(octets.len()).map_err(|_| io::Error::other("secret trop long"))?;
        Ok(CRYPT_INTEGER_BLOB {
            cbData: taille,
            pbData: octets.as_ptr().cast_mut(),
        })
    }

    /// Recopie ce que DPAPI a alloué, puis le rend au système.
    #[allow(unsafe_code)]
    fn recopier(sortie: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if sortie.pbData.is_null() {
            return Vec::new();
        }
        let taille = usize::try_from(sortie.cbData).unwrap_or(0);
        // SAFETY : DPAPI a rempli `sortie` avec un bloc de `cbData` octets alloué par LocalAlloc, lu ici une seule
        // fois puis libéré par LocalFree, comme le demande sa documentation.
        unsafe {
            let copie = std::slice::from_raw_parts(sortie.pbData, taille).to_vec();
            let _ = LocalFree(Some(HLOCAL(sortie.pbData.cast())));
            copie
        }
    }

    impl Protection for Dpapi {
        #[allow(unsafe_code)]
        fn proteger(&self, clair: &[u8]) -> io::Result<Vec<u8>> {
            let entree = bloc(clair)?;
            let entropie = bloc(identite::NOM_EXE.as_bytes())?;
            let mut sortie = CRYPT_INTEGER_BLOB::default();
            // SAFETY : les blocs d'entrée pointent sur des tranches vivantes pendant l'appel, que DPAPI ne modifie
            // pas ; la sortie est un bloc vide que DPAPI remplit, rendu par `recopier`.
            unsafe {
                CryptProtectData(
                    &raw const entree,
                    PCWSTR::null(),
                    Some(&raw const entropie),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &raw mut sortie,
                )
            }
            .map_err(io::Error::other)?;
            Ok(recopier(sortie))
        }

        #[allow(unsafe_code)]
        fn deproteger(&self, chiffre: &[u8]) -> io::Result<Vec<u8>> {
            let entree = bloc(chiffre)?;
            let entropie = bloc(identite::NOM_EXE.as_bytes())?;
            let mut sortie = CRYPT_INTEGER_BLOB::default();
            // SAFETY : comme pour `proteger` ; aucune description demandée en retour.
            unsafe {
                CryptUnprotectData(
                    &raw const entree,
                    None,
                    Some(&raw const entropie),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &raw mut sortie,
                )
            }
            .map_err(io::Error::other)?;
            Ok(recopier(sortie))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::Dossier;
        use crate::secrets::Coffre;

        #[test]
        fn dpapi_chiffre_et_dechiffre() {
            let chiffre = Dpapi.proteger("jeton secret é".as_bytes()).unwrap();
            assert!(!chiffre.windows(5).any(|w| w == b"jeton"));
            assert_eq!(Dpapi.deproteger(&chiffre).unwrap(), "jeton secret é".as_bytes());
        }

        #[test]
        fn dpapi_refuse_un_bloc_abime() {
            let mut chiffre = Dpapi.proteger(b"abc").unwrap();
            if let Some(dernier) = chiffre.last_mut() {
                *dernier ^= 0xff;
            }
            assert!(Dpapi.deproteger(&chiffre).is_err());
        }

        #[test]
        fn le_coffre_marche_avec_dpapi() {
            let temp = tempfile::tempdir().unwrap();
            let dossier = Dossier::a(temp.path());
            let coffre = Coffre::du_dossier(&dossier, &Dpapi);
            coffre.enregistrer("cle", "valeur").unwrap();
            assert_eq!(coffre.lire("cle").unwrap().as_deref(), Some("valeur"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une imitation réversible, pour tester le rangement sans Windows.
    struct Imitation;

    impl Protection for Imitation {
        fn proteger(&self, clair: &[u8]) -> io::Result<Vec<u8>> {
            Ok(clair.iter().map(|o| o ^ 0x5a).collect())
        }
        fn deproteger(&self, chiffre: &[u8]) -> io::Result<Vec<u8>> {
            if chiffre.first() == Some(&0) {
                return Err(io::Error::other("abîmé"));
            }
            Ok(chiffre.iter().map(|o| o ^ 0x5a).collect())
        }
    }

    #[test]
    fn enregistrer_lire_supprimer() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let coffre = Coffre::du_dossier(&dossier, &Imitation);
        assert_eq!(coffre.lire("jeton").unwrap(), None);
        coffre.enregistrer("jeton", "s3cr3t").unwrap();
        assert_eq!(coffre.lire("jeton").unwrap().as_deref(), Some("s3cr3t"));
        let fichier = dossier.secrets().join("jeton.dpapi");
        assert_ne!(
            fs::read(&fichier).unwrap(),
            b"s3cr3t",
            "jamais en clair sur le disque"
        );
        coffre.supprimer("jeton").unwrap();
        coffre.supprimer("jeton").unwrap();
        assert_eq!(coffre.lire("jeton").unwrap(), None);
    }

    #[test]
    fn un_fichier_abime_se_lit_comme_absent() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        fs::create_dir_all(dossier.secrets()).unwrap();
        fs::write(dossier.secrets().join("cle.dpapi"), [0u8, 1, 2]).unwrap();
        assert_eq!(
            Coffre::du_dossier(&dossier, &Imitation).lire("cle").unwrap(),
            None
        );
    }

    #[test]
    fn un_nom_qui_sortirait_du_dossier_est_refuse() {
        for nom in ["", "../x", "a/b", "A", "-a", "a-", "é", &"a".repeat(65)] {
            assert!(!nom_valide(nom), "{nom}");
        }
        for nom in ["a", "jeton-github", "cle2", &"a".repeat(64)] {
            assert!(nom_valide(nom), "{nom}");
        }
        let temp = tempfile::tempdir().unwrap();
        let coffre = Coffre::du_dossier(&Dossier::a(temp.path()), &Imitation);
        assert!(matches!(
            coffre.enregistrer("../x", "s"),
            Err(Erreur::NomDeSecret(_))
        ));
    }
}
