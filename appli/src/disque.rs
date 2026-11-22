//! La place sur un disque, demandée à Windows. Lecture seule.

use std::io;
use std::path::Path;

use serde::Serialize;

/// Place libre (pour l'utilisateur) et taille du disque, en octets.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Espace {
    pub libre: u64,
    pub total: u64,
}

/// La place sur le disque qui porte `chemin`.
///
/// # Errors
/// L'erreur de Windows si le disque ne répond pas ou si le chemin n'existe pas.
#[cfg(windows)]
pub fn espace(chemin: &Path) -> io::Result<Espace> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let large: Vec<u16> = chemin.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut libre, mut total) = (0u64, 0u64);
    // SAFETY : `large` se termine par un zéro et vit jusqu'à la fin de l'appel ; les deux sorties pointent vers
    // des u64 locaux valides ; Windows n'écrit rien d'autre.
    #[allow(unsafe_code)]
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(large.as_ptr()),
            Some(&raw mut libre),
            Some(&raw mut total),
            None,
        )?;
    }
    Ok(Espace { libre, total })
}

/// Hors de Windows, la mesure n'existe pas : l'appli ne vise que Windows.
#[cfg(not(windows))]
pub fn espace(_chemin: &Path) -> io::Result<Espace> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "mesure du disque : Windows seulement",
    ))
}
