//! « Démarrer avec Windows », côté appli : l'état relu dans le registre à chaque fois, et la case des réglages et
//! du menu de l'icône. Seulement pour une appli qui sert en arrière-plan (`identite::EN_FOND`).

use aria_coeur::{identite, journal};
use tauri::{AppHandle, Emitter, State};

use crate::Etat;
use crate::icone::MENU;

/// Windows lancera-t-il l'appli à l'ouverture de session ?
pub fn est_actif() -> bool {
    #[cfg(windows)]
    {
        identite::EN_FOND && aria_coeur::demarrage::windows::est_inscrit()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn inscrire(actif: bool) -> Result<(), String> {
    if !identite::EN_FOND {
        return Err("cette appli ne démarre pas avec Windows".to_owned());
    }
    #[cfg(windows)]
    {
        use aria_coeur::demarrage::windows;
        let resultat = if actif {
            std::env::current_exe().and_then(|exe| windows::inscrire(&exe))
        } else {
            windows::desinscrire()
        };
        resultat.map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = actif;
        Err("Windows seulement".to_owned())
    }
}

#[tauri::command]
pub fn lire_demarrage_auto() -> bool {
    est_actif()
}

/// Coche ou décoche « Démarrer avec Windows », et renvoie l'état relu ensuite.
#[tauri::command]
pub fn regler_demarrage_auto(app: AppHandle, etat: State<'_, Etat>, actif: bool) -> Result<bool, String> {
    inscrire(actif).map_err(|e| format!("le démarrage avec Windows n'a pas pu être changé : {e}"))?;
    let bilan = if actif {
        "démarrage avec Windows activé"
    } else {
        "démarrage avec Windows désactivé"
    };
    journal::consigner(&etat.dossier, "démarrage auto", bilan);
    let _ = app.emit_to(MENU, "etat-icone", ());
    Ok(est_actif())
}
