//! Les boîtes de dialogue de Windows, pour ce qui doit être dit hors de toute fenêtre de l'appli : l'appli ne peut
//! pas démarrer, ou un avertissement au moment de se relancer.

use aria_coeur::identite;

/// Affiche pourquoi l'appli n'a pas pu démarrer.
#[cfg(windows)]
pub fn afficher(message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::HSTRING;

    let titre = HSTRING::from(identite::NOM);
    let texte = HSTRING::from(format!("{} n'a pas pu démarrer.\n\n{message}", identite::NOM));
    // SAFETY : les deux textes sont des HSTRING valides jusqu'à la fin de l'appel ; aucune fenêtre parente.
    #[allow(unsafe_code)]
    unsafe {
        MessageBoxW(None, &texte, &titre, MB_OK | MB_ICONERROR);
    }
}

/// Hors de Windows (tests, outils) : sur la sortie d'erreur.
#[cfg(not(windows))]
pub fn afficher(message: &str) {
    eprintln!("{} n'a pas pu démarrer : {message}", identite::NOM);
}

/// Prévient d'un problème qui n'empêche pas de continuer (un raccourci non créé, par exemple).
#[cfg(windows)]
pub fn avertir(message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
    use windows::core::HSTRING;

    let titre = HSTRING::from(identite::NOM);
    let texte = HSTRING::from(message);
    // SAFETY : comme pour `afficher`.
    #[allow(unsafe_code)]
    unsafe {
        MessageBoxW(None, &texte, &titre, MB_OK | MB_ICONWARNING);
    }
}

/// Hors de Windows : sur la sortie d'erreur.
#[cfg(not(windows))]
pub fn avertir(message: &str) {
    eprintln!("{} : {message}", identite::NOM);
}
