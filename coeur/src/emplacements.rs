//! Où vivent les affaires d'Aria Studio, décidé une fois au lancement, comme en v2 (`app/emplacements.py`).
//!
//! - Par défaut, tout est dans `%USERPROFILE%\.Aria-Studio` : réglages, journal, `data\`, `modeles\`, `moteurs\`.
//! - Mode portable : un dossier `data\` à côté de l'exe (clé USB…), et l'exe n'est pas dans `.Aria-Studio`
//!   lui-même. Tout reste alors à côté de l'exe, et les moteurs vont dans `services\` (nom de la v2). L'appli ne
//!   propose pas de s'installer.
//! - Le dossier des modèles peut être choisi ailleurs : `emplacements.modeles` dans `reglages.json`, un chemin
//!   complet. Un chemin relatif est ignoré.

use std::path::{Path, PathBuf};

use crate::installation::meme_dossier;
use crate::{Dossier, reglages};

/// Ce qui se crée et se lit : séquences, rushes, voix, journaux des moteurs.
pub const DOSSIER_DATA: &str = "data";
/// Les modèles, quand l'utilisateur n'en a pas choisi d'autre.
pub const DOSSIER_MODELES: &str = "modeles";
/// ComfyUI portable et llama-server.
pub const DOSSIER_MOTEURS: &str = "moteurs";
/// Les moteurs en mode portable.
pub const DOSSIER_MOTEURS_PORTABLE: &str = "services";

const SECTION: &str = "emplacements";
const CLE_MODELES: &str = "modeles";

/// Les dossiers de l'appli pour ce lancement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emplacements {
    /// Le dossier de données : celui du profil, ou celui de l'exe en mode portable.
    pub dossier: Dossier,
    /// Vrai si tout vit à côté de l'exe.
    pub portable: bool,
    pub data: PathBuf,
    pub modeles: PathBuf,
    pub moteurs: PathBuf,
}

impl Emplacements {
    /// Décide des dossiers d'après la place de l'exe. `profil` est le dossier de données du profil
    /// ([`Dossier::du_profil`]).
    pub fn decider(dossier_exe: &Path, profil: &Dossier) -> Self {
        let portable = est_portable(dossier_exe, profil);
        let dossier = if portable {
            Dossier::a(dossier_exe)
        } else {
            profil.clone()
        };
        let racine = dossier.racine().to_path_buf();
        let modeles = modeles_choisis(&dossier).unwrap_or_else(|| racine.join(DOSSIER_MODELES));
        let moteurs = racine.join(if portable {
            DOSSIER_MOTEURS_PORTABLE
        } else {
            DOSSIER_MOTEURS
        });
        Self {
            data: racine.join(DOSSIER_DATA),
            modeles,
            moteurs,
            portable,
            dossier,
        }
    }
}

/// Un `data\` à côté de l'exe, et l'exe ailleurs que dans le dossier du profil lui-même.
fn est_portable(dossier_exe: &Path, profil: &Dossier) -> bool {
    dossier_exe.join(DOSSIER_DATA).is_dir() && !meme_dossier(dossier_exe, profil.racine())
}

/// Le dossier des modèles choisi dans les réglages, s'il est donné en chemin complet.
fn modeles_choisis(dossier: &Dossier) -> Option<PathBuf> {
    let section = reglages::section(dossier, SECTION)?;
    let chemin = PathBuf::from(section.get(CLE_MODELES)?.as_str()?.trim());
    chemin.is_absolute().then_some(chemin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Essai {
        _temp: tempfile::TempDir,
        exe: PathBuf,
        profil: Dossier,
    }

    fn essai() -> Essai {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("cle-usb").join("Aria-Studio");
        fs::create_dir_all(&exe).unwrap();
        let profil = Dossier::a(temp.path().join("profil").join(".Aria-Studio"));
        fs::create_dir_all(profil.racine()).unwrap();
        Essai {
            _temp: temp,
            exe,
            profil,
        }
    }

    #[test]
    fn par_defaut_tout_est_dans_le_profil() {
        let e = essai();
        let lieux = Emplacements::decider(&e.exe, &e.profil);
        assert!(!lieux.portable);
        assert_eq!(lieux.dossier, e.profil);
        assert_eq!(lieux.data, e.profil.racine().join("data"));
        assert_eq!(lieux.modeles, e.profil.racine().join("modeles"));
        assert_eq!(lieux.moteurs, e.profil.racine().join("moteurs"));
    }

    #[test]
    fn un_dossier_data_a_cote_de_l_exe_rend_portable() {
        let e = essai();
        fs::create_dir(e.exe.join("data")).unwrap();
        let lieux = Emplacements::decider(&e.exe, &e.profil);
        assert!(lieux.portable);
        assert_eq!(lieux.dossier.racine(), e.exe.as_path());
        assert_eq!(lieux.data, e.exe.join("data"));
        assert_eq!(lieux.modeles, e.exe.join("modeles"));
        assert_eq!(lieux.moteurs, e.exe.join("services"));
    }

    #[test]
    fn l_exe_dans_le_dossier_du_profil_n_est_pas_portable() {
        let e = essai();
        fs::create_dir(e.profil.racine().join("data")).unwrap();
        let lieux = Emplacements::decider(e.profil.racine(), &e.profil);
        assert!(!lieux.portable);
        assert_eq!(lieux.moteurs, e.profil.racine().join("moteurs"));
    }

    #[test]
    fn le_dossier_des_modeles_choisi_est_suivi() {
        let e = essai();
        let choisi = e.exe.parent().unwrap().join("modeles-ailleurs");
        let reglages = serde_json::json!({ "emplacements": { "modeles": choisi.to_string_lossy() } });
        fs::write(e.profil.reglages(), reglages.to_string()).unwrap();
        let lieux = Emplacements::decider(&e.exe, &e.profil);
        assert_eq!(lieux.modeles, choisi);
    }

    #[test]
    fn un_dossier_des_modeles_relatif_ou_vide_est_ignore() {
        let e = essai();
        for valeur in ["modeles-relatifs", "", "  "] {
            let reglages = serde_json::json!({ "emplacements": { "modeles": valeur } });
            fs::write(e.profil.reglages(), reglages.to_string()).unwrap();
            let lieux = Emplacements::decider(&e.exe, &e.profil);
            assert_eq!(lieux.modeles, e.profil.racine().join("modeles"), "{valeur:?}");
        }
    }

    #[test]
    fn en_mode_portable_les_reglages_lus_sont_ceux_d_a_cote_de_l_exe() {
        let e = essai();
        fs::create_dir(e.exe.join("data")).unwrap();
        let choisi = e.exe.join("mes-modeles");
        let reglages = serde_json::json!({ "emplacements": { "modeles": choisi.to_string_lossy() } });
        fs::write(e.exe.join("reglages.json"), reglages.to_string()).unwrap();
        let lieux = Emplacements::decider(&e.exe, &e.profil);
        assert_eq!(lieux.modeles, choisi);
    }
}
