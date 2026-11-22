//! Ce qu'il faut pour parler aux moteurs et les lancer : HTTP sur la boucle locale seulement, et des processus sans
//! fenêtre de console, avec leur journal dans `data\logs\`.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use ureq::Agent;

/// Un client HTTP pour les moteurs, sur `127.0.0.1` : jamais de proxy (la clé de llama-server ne doit pas sortir),
/// pas de redirection, et une réponse d'erreur rendue telle quelle pour lire son corps.
pub fn client(delai: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(delai))
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .build()
        .into()
}

/// Le client d'une réponse en flux (génération d'un LLM) : pas de limite d'ensemble, mais un silence de plus de
/// `silence` entre deux lectures l'interrompt (comme en v2). Même règles que [`client`] sinon.
pub fn client_flux(silence: Duration) -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_per_read(Some(silence))
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .build()
        .into()
}

/// Le journal d'un moteur, `data\logs\<nom>.log`, vidé à chaque lancement.
///
/// # Errors
/// Le dossier ou le fichier ne se crée pas.
pub fn journal(data: &Path, nom: &str) -> io::Result<(PathBuf, File)> {
    let dossier = data.join("logs");
    fs::create_dir_all(&dossier)?;
    let chemin = dossier.join(format!("{nom}.log"));
    let fichier = File::create(&chemin)?;
    Ok((chemin, fichier))
}

/// Lance un programme sans fenêtre de console, depuis `dossier`, sorties dans `journal`, avec des variables
/// d'environnement en plus de celles d'Aria.
///
/// # Errors
/// Le programme ne se lance pas.
pub fn lancer(
    programme: &Path,
    arguments: &[OsString],
    dossier: Option<&Path>,
    journal: &File,
    environnement: &[(&str, &str)],
) -> io::Result<Child> {
    let mut commande = Command::new(programme);
    commande
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(journal.try_clone()?))
        .stderr(Stdio::from(journal.try_clone()?));
    if let Some(dossier) = dossier {
        commande.current_dir(dossier);
    }
    for (cle, valeur) in environnement {
        commande.env(cle, valeur);
    }
    sans_console(&mut commande);
    commande.spawn()
}

#[cfg(windows)]
fn sans_console(commande: &mut Command) {
    use std::os::windows::process::CommandExt;
    /// Pas de fenêtre de console pour un moteur lancé par Aria.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    commande.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn sans_console(_commande: &mut Command) {}

/// Arrête un processus lancé par Aria et attend sa fin. (Sous Windows, il n'y a pas d'arrêt plus doux qu'on puisse
/// demander à un programme sans fenêtre : c'est ce que faisait déjà la v2.)
pub fn arreter(processus: &mut Child) {
    // Déjà arrêté : kill échoue, ce qui est sans conséquence.
    let _ = processus.kill();
    let _ = processus.wait();
}

/// Le processus tourne-t-il encore ?
pub fn tourne(processus: &mut Child) -> bool {
    matches!(processus.try_wait(), Ok(None))
}

/// Le code de sortie d'un processus terminé, en texte.
pub fn code(processus: &mut Child) -> String {
    match processus.try_wait() {
        Ok(Some(statut)) => statut
            .code()
            .map_or_else(|| "inconnu".to_owned(), |c| c.to_string()),
        _ => "inconnu".to_owned(),
    }
}

/// Un programme écoute-t-il sur ce port de la boucle locale ?
pub fn port_pris(port: u16, delai: Duration) -> bool {
    let adresse = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&adresse, delai).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn un_port_qui_ecoute_est_vu_pris() {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ecoute.local_addr().unwrap().port();
        assert!(port_pris(port, Duration::from_millis(500)));
        drop(ecoute);
        assert!(!port_pris(port, Duration::from_millis(200)));
    }

    #[test]
    fn le_journal_est_vide_a_chaque_lancement() {
        let temp = tempfile::tempdir().unwrap();
        let (chemin, _) = journal(temp.path(), "llama").unwrap();
        fs::write(&chemin, "ancien").unwrap();
        let (meme, _) = journal(temp.path(), "llama").unwrap();
        assert_eq!(meme, temp.path().join("logs").join("llama.log"));
        assert_eq!(fs::read_to_string(meme).unwrap(), "");
    }
}
