//! Les notifications Windows de fin de rendu : une image, une retouche, un clip, une séquence ou un casting se
//! termine, et Windows le dit, même fenêtre réduite ou appli rangée dans son icône. Un clic sur la notification
//! ramène la fenêtre d'Aria. Les mots sont dans le cœur (`aria_coeur::notifications`), le réglage dans
//! `reglages.json` (`notifications.active` et `notifications.son`).
//!
//! Tout reste local : une notification Windows ne passe par aucun réseau, et rien n'est écrit hors du dossier de
//! données. Les appels à Windows (WinRT) passent par la crate `windows`, déjà dans l'exe : pas de dépendance de plus.

use std::sync::{Arc, OnceLock};

use aria_coeur::notifications::{self, Annonce};
use aria_coeur::plateau::Commande;
use aria_coeur::{Dossier, Reglages, identite};
use tauri::{AppHandle, Manager};

use crate::FENETRE;

/// L'appli une fois ouverte : la file du Plateau démarre avant la fenêtre, elle ne peut donc pas la tenir d'avance.
pub type Poignee = Arc<OnceLock<AppHandle>>;

/// L'identité sous laquelle Windows montre la notification. Aria n'a pas de raccourci enregistré dans Windows (ce
/// serait une écriture hors du dossier de données) : on prend celle de PowerShell, que Windows connaît toujours.
/// L'en-tête de la notification porte donc ce nom ; son titre, lui, dit Aria Studio.
#[cfg(windows)]
const APP_ID: &str = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// À la fin d'une commande de la file : prévient l'utilisateur, si le réglage le veut et si la fenêtre d'Aria n'est
/// pas déjà devant lui (il voit alors la file). Une annulation ne prévient pas.
pub fn prevenir(poignee: &Poignee, dossier: &Dossier, commande: &Commande) {
    let (Some(app), Some(annonce)) = (poignee.get(), notifications::annonce(commande)) else {
        return;
    };
    let reglages = Reglages::charger(dossier).notifications;
    if reglages.active && !fenetre_devant(app) {
        // Une notification qui ne part pas ne gâche pas le rendu : la création est faite, la file le montre.
        let _ = afficher(app, &annonce, reglages.son);
    }
}

/// Vrai si la fenêtre d'Aria est visible, au premier plan et pas réduite.
fn fenetre_devant(app: &AppHandle) -> bool {
    app.get_webview_window(FENETRE).is_some_and(|fenetre| {
        fenetre.is_visible().unwrap_or(false)
            && fenetre.is_focused().unwrap_or(false)
            && !fenetre.is_minimized().unwrap_or(false)
    })
}

/// « Essayer » (Paramètres › Notifications) : montre une notification d'essai, avec ou sans son, même si les
/// notifications sont décochées ou si la fenêtre est devant. Windows peut la retenir (Assistant de concentration,
/// notifications coupées pour l'appli) : l'essai ne le sait pas.
#[tauri::command]
pub async fn essayer_notification(app: AppHandle, son: bool) -> Result<(), String> {
    let annonce = Annonce {
        titre: format!("{} : essai", identite::NOM),
        texte: "Voilà à quoi ressemblera la fin d'un rendu.".to_owned(),
    };
    tauri::async_runtime::spawn_blocking(move || afficher(&app, &annonce, son))
        .await
        .map_err(|e| format!("l'essai a été interrompu : {e}"))?
}

/// Montre la notification ; un clic ramène la fenêtre d'Aria.
#[cfg(windows)]
fn afficher(app: &AppHandle, annonce: &Annonce, son: bool) -> Result<(), String> {
    toast(app, annonce, son).map_err(|e| format!("la notification n'a pas pu être montrée : {e}"))
}

#[cfg(not(windows))]
fn afficher(_app: &AppHandle, _annonce: &Annonce, _son: bool) -> Result<(), String> {
    Err("les notifications ne sont prises en charge que sous Windows".to_owned())
}

/// Le toast Windows : un titre, une ligne de texte, le son par défaut de Windows ou le silence.
#[cfg(windows)]
fn toast(app: &AppHandle, annonce: &Annonce, son: bool) -> windows::core::Result<()> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::core::{HSTRING, IInspectable};

    let document = XmlDocument::new()?;
    let element = |nom: &str| document.CreateElement(&HSTRING::from(nom));
    let notification = element("toast")?;
    let visuel = element("visual")?;
    let liaison = element("binding")?;
    liaison.SetAttribute(&HSTRING::from("template"), &HSTRING::from("ToastGeneric"))?;
    for (rang, texte) in [("1", annonce.titre.as_str()), ("2", annonce.texte.as_str())] {
        let ligne = element("text")?;
        ligne.SetAttribute(&HSTRING::from("id"), &HSTRING::from(rang))?;
        ligne.SetInnerText(&HSTRING::from(texte))?;
        liaison.AppendChild(&ligne)?;
    }
    visuel.AppendChild(&liaison)?;
    notification.AppendChild(&visuel)?;
    if !son {
        let audio = element("audio")?;
        audio.SetAttribute(&HSTRING::from("silent"), &HSTRING::from("true"))?;
        notification.AppendChild(&audio)?;
    }
    document.AppendChild(&notification)?;
    let avis = ToastNotification::CreateToastNotification(&document)?;
    let app = app.clone();
    let au_clic = TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, _| {
        crate::ramener(&app);
        Ok(())
    });
    avis.Activated(&au_clic)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&avis)
}
