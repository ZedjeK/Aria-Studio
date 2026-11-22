//! Sauvegardes : une copie datée de chaque fichier de l'utilisateur, faite avant de le modifier, dans
//! `sauvegardes\` du dossier de données. Rien n'est jamais supprimé ni écrasé ici ; restaurer, c'est recopier.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::{Dossier, Erreur};

/// Copie le fichier avant sa modification et renvoie le chemin de la copie.
///
/// # Errors
/// Une erreur si la copie échoue : sans sauvegarde, la modification ne doit pas avoir lieu.
pub fn sauvegarder(dossier: &Dossier, fichier: &Path) -> Result<PathBuf, Erreur> {
    sauvegarder_a(&dossier.sauvegardes(), fichier, Local::now())
}

/// [`sauvegarder`], dans un dossier et à une date donnés (tests).
///
/// # Errors
/// [`Erreur::AbsentASauvegarder`] si le fichier n'existe pas, [`Erreur::Fichier`] si la copie échoue.
pub fn sauvegarder_a(dossier: &Path, fichier: &Path, quand: DateTime<Local>) -> Result<PathBuf, Erreur> {
    if !fichier.is_file() {
        return Err(Erreur::AbsentASauvegarder(fichier.to_path_buf()));
    }
    fs::create_dir_all(dossier).map_err(|e| Erreur::fichier("création de", dossier, e))?;
    let nom = fichier
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let horodatage = quand.format("%Y-%m-%d_%H-%M-%S");
    let (base, extension) = match nom.rsplit_once('.') {
        Some((b, e)) if !b.is_empty() => (b.to_owned(), format!(".{e}")),
        _ => (nom.clone(), String::new()),
    };
    // Deux sauvegardes du même fichier dans la même seconde : un suffixe, jamais un écrasement.
    for n in 1u32.. {
        let suffixe = if n == 1 { String::new() } else { format!("_{n}") };
        let cible = dossier.join(format!("{horodatage}_{base}{suffixe}{extension}"));
        match copier_sans_ecraser(fichier, &cible) {
            Ok(()) => return Ok(cible),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(Erreur::fichier("sauvegarde vers", cible, e)),
        }
    }
    unreachable!("la boucle ne se termine que par un retour")
}

/// Copie seulement si la cible n'existe pas encore, sans course possible entre le test et la création.
fn copier_sans_ecraser(source: &Path, cible: &Path) -> io::Result<()> {
    let mut sortie = OpenOptions::new().write(true).create_new(true).open(cible)?;
    let mut entree = fs::File::open(source)?;
    io::copy(&mut entree, &mut sortie)?;
    sortie.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn date() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 21, 19, 5, 0).unwrap()
    }

    #[test]
    fn la_copie_est_datee_et_identique() {
        let temp = tempfile::tempdir().unwrap();
        let fichier = temp.path().join("hosts.txt");
        fs::write(&fichier, "127.0.0.1 localhost").unwrap();
        let copie = sauvegarder_a(&temp.path().join("sauvegardes"), &fichier, date()).unwrap();
        assert!(
            copie.ends_with("2026-11-21_19-05-00_hosts.txt"),
            "{}",
            copie.display()
        );
        assert_eq!(fs::read(&copie).unwrap(), fs::read(&fichier).unwrap());
    }

    #[test]
    fn deux_copies_dans_la_meme_seconde_ne_s_ecrasent_pas() {
        let temp = tempfile::tempdir().unwrap();
        let fichier = temp.path().join("a.json");
        fs::write(&fichier, "1").unwrap();
        let dossier = temp.path().join("sauvegardes");
        let premiere = sauvegarder_a(&dossier, &fichier, date()).unwrap();
        fs::write(&fichier, "2").unwrap();
        let seconde = sauvegarder_a(&dossier, &fichier, date()).unwrap();
        assert!(
            seconde.ends_with("2026-11-21_19-05-00_a_2.json"),
            "{}",
            seconde.display()
        );
        assert_eq!(fs::read_to_string(premiere).unwrap(), "1");
        assert_eq!(fs::read_to_string(seconde).unwrap(), "2");
    }

    #[test]
    fn un_fichier_absent_est_refuse() {
        let temp = tempfile::tempdir().unwrap();
        let erreur = sauvegarder_a(temp.path(), &temp.path().join("absent"), date()).unwrap_err();
        assert!(matches!(erreur, Erreur::AbsentASauvegarder(_)));
    }

    #[test]
    fn un_fichier_sans_extension_garde_son_nom() {
        let temp = tempfile::tempdir().unwrap();
        let fichier = temp.path().join("hosts");
        fs::write(&fichier, "x").unwrap();
        let copie = sauvegarder_a(&temp.path().join("s"), &fichier, date()).unwrap();
        assert!(
            copie.ends_with("2026-11-21_19-05-00_hosts"),
            "{}",
            copie.display()
        );
    }
}
