//! Le pont de mise à jour depuis la version 2, côté exe : ce que `--mise-a-jour <pid> <ancien lanceur>` demande à
//! Windows (attendre un processus, attendre l'icône de la v2) et le relancement. Les étapes et leurs règles sont dans
//! `aria_coeur::pont_v2`, testées sans Windows.
//!
//! Cet exe est celui du zip décompressé par la v2 dans `cache\mise-a-jour\Aria-Studio-X.Y.Z\` : il n'ouvre aucune
//! fenêtre, ne prend pas la place d'exemplaire unique, installe, relance l'exe installé (ou l'ancien lanceur) et
//! s'arrête.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use aria_coeur::pont_v2::{self, Demande, Systeme};
use aria_coeur::{Dossier, comfy_serveur, identite, journal};

use crate::erreur_fatale;

/// Le système réel : le temps, les processus de Windows, le verrou que la v2 tient pendant que son icône existe.
struct Reel {
    debut: Instant,
    maison: PathBuf,
}

impl Systeme for Reel {
    fn ecoule(&self) -> Duration {
        self.debut.elapsed()
    }

    fn dormir(&self, duree: Duration) {
        std::thread::sleep(duree);
    }

    fn processus_vivant(&self, pid: u32) -> bool {
        comfy_serveur::processus_existe(pid)
    }

    fn attendre_icone(&self, delai: Duration) -> bool {
        let nom = pont_v2::nom_verrou_icone(&self.maison);
        attendre_verrou(&nom, delai)
    }
}

/// Attend au plus `delai` que le verrou nommé soit libre : la v2 le tient tant que son icône est affichée, et son
/// exe reste ouvert jusque-là. Vrai si le verrou est libre (ou n'existe pas), faux si le délai passe ou si Windows
/// refuse : le dossier dira alors lui-même s'il est libre.
#[cfg(windows)]
#[allow(unsafe_code)]
fn attendre_verrou(nom: &str, delai: Duration) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
    use windows::core::HSTRING;

    let nom = HSTRING::from(nom);
    let millisecondes = u32::try_from(delai.as_millis()).unwrap_or(u32::MAX);
    // SAFETY : le nom est un HSTRING valide pendant l'appel ; le verrou obtenu est relâché puis fermé ici, une seule
    // fois, avant de rendre la main.
    unsafe {
        let Ok(verrou) = CreateMutexW(None, false, &nom) else {
            return false;
        };
        // 0 : obtenu ; 0x80 : abandonné par une icône disparue, il est alors à nous aussi.
        let code = WaitForSingleObject(verrou, millisecondes).0;
        let libre = code == 0 || code == 0x80;
        if libre {
            let _ = ReleaseMutex(verrou);
        }
        let _ = CloseHandle(verrou);
        libre
    }
}

/// Hors de Windows (tests, outils) : il n'y a pas d'icône de la v2 à attendre.
#[cfg(not(windows))]
fn attendre_verrou(_nom: &str, _delai: Duration) -> bool {
    true
}

/// Lance un exe depuis son dossier, sans l'attendre.
fn lancer(exe: &Path) -> io::Result<()> {
    let mut commande = Command::new(exe);
    if let Some(dossier) = exe.parent() {
        commande.current_dir(dossier);
    }
    commande.spawn().map(|_| ())
}

/// Dit pourquoi rien n'a été installé : au journal, et dans une boîte de dialogue (il n'y a pas de fenêtre d'appli).
fn signaler(profil: &Dossier, message: &str) {
    journal::noter(
        profil,
        "Mise à jour non installée",
        identite::VERSION,
        journal::Resultat::Erreur,
        Some(message),
    );
    erreur_fatale::avertir(message);
}

/// Relance l'exe installé ; s'il ne part pas, le dit sans perdre l'installation, qui est faite.
fn relancer(installe: &Path, profil: &Dossier) -> i32 {
    match lancer(installe) {
        Ok(()) => 0,
        Err(e) => {
            let message = format!(
                "{} est installée, mais elle n'a pas pu être relancée : {e}. Lance-la depuis {}.",
                identite::NOM,
                installe.display()
            );
            journal::noter(
                profil,
                "Mise à jour installée, relance impossible",
                identite::VERSION,
                journal::Resultat::Partiel,
                Some(&message),
            );
            erreur_fatale::avertir(&message);
            1
        }
    }
}

/// Fait la mise à jour que demande la v2, puis rend le code de sortie de l'exe. Le bilan se note au démarrage
/// suivant (`aria_coeur::pont_v2::bilan_au_demarrage`).
pub fn executer(demande: Result<Demande, String>, profil: &Dossier) -> i32 {
    let demande = match demande {
        Ok(demande) => demande,
        Err(message) => {
            signaler(profil, &message);
            return 1;
        }
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            let message = format!("L'exe de la mise à jour est introuvable : {e}");
            signaler(profil, &message);
            return 1;
        }
    };
    let systeme = Reel {
        debut: Instant::now(),
        maison: profil.racine().to_path_buf(),
    };
    match pont_v2::mettre_a_jour(&demande, profil, &exe, &systeme) {
        Ok(installe) => relancer(&installe, profil),
        Err(refus) => {
            signaler(profil, &refus.message);
            if let Some(ancien) = refus.relancer {
                // La v2 reprend, et c'est elle qui note l'échec à son démarrage ; si elle ne repart pas non plus,
                // le message ci-dessus est déjà dit.
                let _ = lancer(&ancien);
            }
            1
        }
    }
}
