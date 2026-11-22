//! Où vivent les affaires d'Aria Studio, décidé une fois au lancement, comme en v2 (`app/emplacements.py`).
//!
//! - Par défaut, tout est dans `%USERPROFILE%\.Aria-Studio` : réglages, journal, `data\`, `modeles\`, `moteurs\`.
//! - Mode portable : un dossier `data\` à côté de l'exe (clé USB…), et l'exe n'est pas dans `.Aria-Studio`
//!   lui-même. Tout reste alors à côté de l'exe, et les moteurs vont dans `services\` (nom de la v2). L'appli ne
//!   propose pas de s'installer.
//! - Le dossier des modèles peut être choisi ailleurs : `emplacements.modeles` dans `reglages.json`, un chemin
//!   complet. Un chemin relatif est ignoré.

use std::path::{Path, PathBuf};

use serde::Serialize;

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

/// Un dossier d'Aria tel que Réglages le montre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DossierConnu {
    pub cle: String,
    pub nom: String,
    pub chemin: String,
    pub existe: bool,
}

/// Ce que montre Réglages → Emplacements, comme en v2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatEmplacements {
    pub portable: bool,
    pub dossiers: Vec<DossierConnu>,
    /// Le dossier des modèles choisi dans les réglages, s'il y en a un.
    pub modeles_choisi: Option<String>,
    pub modeles_defaut: String,
    /// Celui qui servira au prochain lancement.
    pub modeles_prochain: String,
    /// Un choix est fait, pris en compte au prochain lancement.
    pub redemarrage: bool,
}

impl Emplacements {
    fn connus(&self) -> [(&'static str, &'static str, PathBuf); 4] {
        [
            ("racine", "Dossier d'Aria", self.dossier.racine().to_path_buf()),
            ("data", "Données", self.data.clone()),
            ("modeles", "Modèles", self.modeles.clone()),
            ("moteurs", "Moteurs", self.moteurs.clone()),
        ]
    }

    /// L'état des dossiers de ce lancement, et le choix du dossier des modèles pour le suivant.
    pub fn etat(&self) -> EtatEmplacements {
        let choisi = reglages::section(&self.dossier, SECTION)
            .and_then(|s| s.get(CLE_MODELES)?.as_str().map(str::to_owned))
            .filter(|c| !c.trim().is_empty());
        let defaut = self.dossier.racine().join(DOSSIER_MODELES);
        let prochain = modeles_choisis(&self.dossier).unwrap_or_else(|| defaut.clone());
        EtatEmplacements {
            portable: self.portable,
            dossiers: self
                .connus()
                .into_iter()
                .map(|(cle, nom, chemin)| DossierConnu {
                    cle: cle.to_owned(),
                    nom: nom.to_owned(),
                    existe: chemin.is_dir(),
                    chemin: chemin.display().to_string(),
                })
                .collect(),
            modeles_choisi: choisi,
            modeles_defaut: defaut.display().to_string(),
            redemarrage: prochain != self.modeles,
            modeles_prochain: prochain.display().to_string(),
        }
    }

    /// Un dossier connu (jamais un chemin venu de l'interface), et un sous-dossier des modèles déjà connu
    /// (« video/MiniMax-H3/modeles ») ; créé s'il manque, pour l'ouvrir.
    ///
    /// # Errors
    /// Dossier ou sous-dossier inconnu, ou impossible à créer.
    pub fn a_ouvrir(&self, cle: &str, sous: Option<&str>) -> Result<PathBuf, String> {
        let base = self
            .connus()
            .into_iter()
            .find(|(c, _, _)| *c == cle)
            .map(|(_, _, chemin)| chemin)
            .ok_or_else(|| format!("Dossier inconnu : {cle}"))?;
        let dossier = match sous.filter(|s| !s.is_empty()) {
            None => base,
            Some(s) if cle == "modeles" && crate::modeles::dossiers().iter().any(|d| d == s) => base.join(s),
            Some(s) => return Err(format!("Sous-dossier inconnu : {s}")),
        };
        std::fs::create_dir_all(&dossier).map_err(|e| format!("{} : {e}", dossier.display()))?;
        Ok(dossier)
    }

    /// Enregistre le dossier des modèles dans `reglages.json` (rien : revenir au dossier par défaut). Les fichiers ne
    /// sont pas déplacés ; le nouveau dossier vaut au prochain lancement d'Aria.
    ///
    /// # Errors
    /// Chemin relatif, fichier, dossier interdit (données, moteurs), impossible d'y écrire, ou `reglages.json` qui ne
    /// s'écrit pas.
    pub fn choisir_modeles(&self, chemin: Option<&str>) -> Result<EtatEmplacements, String> {
        let choisi = match chemin.map(str::trim).filter(|c| !c.is_empty()) {
            None => None,
            Some(c) => {
                let dossier = PathBuf::from(c);
                if !dossier.is_absolute() {
                    return Err("Donne un chemin complet, par exemple D:\\IA\\Aria-modeles".to_owned());
                }
                if dossier.exists() && !dossier.is_dir() {
                    return Err(format!("{} est un fichier, pas un dossier", dossier.display()));
                }
                for interdit in [&self.data, &self.moteurs] {
                    if dossier.starts_with(interdit) {
                        return Err(format!(
                            "Les modèles ne peuvent pas aller dans {}",
                            interdit.display()
                        ));
                    }
                }
                let essai = dossier.join(".aria-ecriture");
                std::fs::create_dir_all(&dossier)
                    .and_then(|()| std::fs::write(&essai, "ok"))
                    .and_then(|()| std::fs::remove_file(&essai))
                    .map_err(|e| format!("Impossible d'écrire dans {} : {e}", dossier.display()))?;
                Some(dossier.display().to_string())
            }
        };
        reglages::modifier_section(&self.dossier, SECTION, |section| match &choisi {
            Some(c) => {
                section.insert(CLE_MODELES.to_owned(), serde_json::Value::String(c.clone()));
            }
            None => {
                section.remove(CLE_MODELES);
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(self.etat())
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

#[cfg(test)]
mod tests_reglages {
    use super::*;
    use std::fs;

    #[test]
    fn etat_et_choix_du_dossier_des_modeles() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("exe");
        fs::create_dir_all(&exe).unwrap();
        let profil = Dossier::a(temp.path().join("profil"));
        fs::create_dir_all(profil.racine()).unwrap();
        let lieux = Emplacements::decider(&exe, &profil);
        let e = lieux.etat();
        assert!(!e.portable);
        assert_eq!(e.dossiers.len(), 4);
        assert_eq!(e.dossiers[1].nom, "Données");
        assert!(!e.redemarrage);
        assert!(e.modeles_choisi.is_none());

        assert!(
            lieux
                .choisir_modeles(Some("relatif"))
                .unwrap_err()
                .starts_with("Donne un chemin complet")
        );
        let dans_data = lieux.data.join("modeles");
        assert!(
            lieux
                .choisir_modeles(Some(&dans_data.display().to_string()))
                .unwrap_err()
                .starts_with("Les modèles ne peuvent pas aller")
        );
        let fichier = temp.path().join("un-fichier");
        fs::write(&fichier, "x").unwrap();
        assert!(
            lieux
                .choisir_modeles(Some(&fichier.display().to_string()))
                .unwrap_err()
                .ends_with("pas un dossier")
        );

        let ailleurs = temp.path().join("D").join("modeles");
        let e = lieux
            .choisir_modeles(Some(&ailleurs.display().to_string()))
            .unwrap();
        assert!(e.redemarrage);
        assert_eq!(e.modeles_prochain, ailleurs.display().to_string());
        assert!(ailleurs.is_dir() && !ailleurs.join(".aria-ecriture").exists());
        // Le lancement suivant le prend.
        assert_eq!(Emplacements::decider(&exe, &profil).modeles, ailleurs);
        let e = lieux.choisir_modeles(None).unwrap();
        assert!(!e.redemarrage && e.modeles_choisi.is_none());
    }

    #[test]
    fn seuls_les_dossiers_connus_s_ouvrent() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("exe");
        fs::create_dir_all(&exe).unwrap();
        let profil = Dossier::a(temp.path().join("profil"));
        let lieux = Emplacements::decider(&exe, &profil);
        assert_eq!(lieux.a_ouvrir("data", None).unwrap(), lieux.data);
        assert!(lieux.data.is_dir());
        assert!(lieux.a_ouvrir("C:\\Windows", None).is_err());
        assert!(lieux.a_ouvrir("modeles", Some("../..")).is_err());
        assert!(lieux.a_ouvrir("moteurs", Some("comfyui")).is_err());
        let llm = lieux.a_ouvrir("modeles", Some("llm")).unwrap();
        assert_eq!(llm, lieux.modeles.join("llm"));
    }
}
