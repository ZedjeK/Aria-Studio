//! L'icône de notification et son menu, pour une appli qui sert en arrière-plan (`identite::EN_FOND`).
//!
//! Le menu est une petite fenêtre aux couleurs de l'appli (étiquette « menu », vue `MenuIcone.svelte`), ouverte au
//! clic droit près de l'icône et rangée dès qu'elle perd le focus : le menu natif de Windows ne prend pas les
//! couleurs d'Aria. Le clic gauche ramène la fenêtre principale.

use std::path::PathBuf;
use std::sync::Mutex;

use aria_coeur::identite;
use aria_coeur::presence::{self, EtatIcone, Ton};
use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, Theme, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

use crate::mise_a_jour::{self, Disponible};
use crate::{FENETRE, demarrage_auto, ramener};

/// L'identifiant de l'icône de notification.
const ICONE: &str = "principale";

/// L'étiquette de la fenêtre du menu, citée par capabilities\menu.json.
pub const MENU: &str = "menu";

/// L'événement qui demande au menu de relire son état.
const RELIRE: &str = "etat-icone";

/// La largeur du menu ; sa hauteur suit son contenu (commande `taille_menu`).
const LARGEUR_MENU: f64 = 300.0;

/// L'état montré par l'icône, partagé entre les commandes.
#[derive(Default)]
pub struct Presence {
    etat: Mutex<EtatIcone>,
}

impl Presence {
    fn lire(&self) -> EtatIcone {
        self.etat.lock().map(|etat| etat.clone()).unwrap_or_default()
    }
}

/// Change ce que montre l'icône, puis redessine l'icône, l'infobulle et le menu. Sans effet si l'appli n'a pas
/// d'icône.
pub fn changer(app: &AppHandle, modifier: impl FnOnce(&mut EtatIcone)) {
    let Some(presence) = app.try_state::<Presence>() else {
        return;
    };
    let etat = {
        let Ok(mut etat) = presence.etat.lock() else {
            return;
        };
        modifier(&mut etat);
        etat.clone()
    };
    if let Some(icone) = app.tray_by_id(ICONE) {
        // L'icône et l'infobulle sont de l'affichage : un échec ne doit rien arrêter.
        if let Some(image) = image(app, &etat) {
            let _ = icone.set_icon(Some(image));
        }
        let _ = icone.set_tooltip(Some(etat.infobulle(identite::NOM)));
    }
    let _ = app.emit_to(MENU, RELIRE, ());
}

/// Le logo de l'appli, délavé ou non, avec sa pastille.
fn image(app: &AppHandle, etat: &EtatIcone) -> Option<Image<'static>> {
    let logo = app.default_window_icon()?;
    let rgba = presence::composer_icone(
        logo.rgba(),
        logo.width(),
        logo.height(),
        etat.delavee(),
        etat.pastille(),
    );
    Some(Image::new_owned(rgba, logo.width(), logo.height()))
}

/// Crée l'icône de notification et la fenêtre de son menu, cachée.
pub fn creer(app: &AppHandle, cache_webview: PathBuf) -> tauri::Result<()> {
    let etat = app.state::<Presence>().lire();
    let mut icone = TrayIconBuilder::with_id(ICONE)
        .tooltip(etat.infobulle(identite::NOM))
        .on_tray_icon_event(|icone, evenement| {
            if let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = evenement
            {
                match button {
                    MouseButton::Left => ramener(icone.app_handle()),
                    MouseButton::Right => ouvrir_menu(icone.app_handle(), position),
                    MouseButton::Middle => {}
                }
            }
        });
    if let Some(image) = image(app, &etat) {
        icone = icone.icon(image);
    }
    icone.build(app)?;

    let menu = WebviewWindowBuilder::new(app, MENU, WebviewUrl::default())
        .title(identite::NOM)
        .inner_size(LARGEUR_MENU, 240.0)
        .resizable(false)
        .decorations(false)
        .shadow(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .theme(Some(Theme::Dark))
        .background_color(Color(3, 8, 18, 255))
        .data_directory(cache_webview)
        .build()?;
    let fenetre = menu.clone();
    menu.on_window_event(move |evenement| {
        // Comme un menu : un clic ailleurs le range.
        if let WindowEvent::Focused(false) = evenement {
            let _ = fenetre.hide();
        }
    });
    Ok(())
}

/// Ouvre le menu près du clic : au-dessus et à gauche, ou de l'autre côté si l'écran s'arrête là (barre des
/// tâches en haut ou à gauche).
fn ouvrir_menu(app: &AppHandle, clic: PhysicalPosition<f64>) {
    let Some(menu) = app.get_webview_window(MENU) else {
        return;
    };
    let _ = app.emit_to(MENU, RELIRE, ());
    let taille = menu.outer_size().unwrap_or_default();
    let (largeur, hauteur) = (f64::from(taille.width), f64::from(taille.height));
    let (mut x, mut y) = (clic.x - largeur, clic.y - hauteur);
    if let Ok(Some(ecran)) = app.monitor_from_point(clic.x, clic.y) {
        let zone = ecran.work_area();
        let (gauche, haut) = (f64::from(zone.position.x), f64::from(zone.position.y));
        let droite = gauche + f64::from(zone.size.width);
        let bas = haut + f64::from(zone.size.height);
        if x < gauche {
            x = clic.x;
        }
        if y < haut {
            y = clic.y;
        }
        x = x.clamp(gauche, (droite - largeur).max(gauche));
        y = y.clamp(haut, (bas - hauteur).max(haut));
    }
    let _ = menu.set_position(PhysicalPosition::new(x, y));
    let _ = menu.show();
    let _ = menu.set_focus();
}

/// Une ligne du bloc « État ».
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LigneMenu {
    ton: &'static str,
    texte: String,
}

/// Ce que le menu affiche.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatMenu {
    nom: &'static str,
    lignes: Vec<LigneMenu>,
    demarrage_auto: bool,
    /// La version disponible : la ligne « Voir la mise à jour » n'apparaît qu'avec elle.
    mise_a_jour: Option<String>,
}

#[tauri::command]
pub fn etat_menu(presence: State<'_, Presence>, disponible: State<'_, Disponible>) -> EtatMenu {
    let lignes = presence
        .lire()
        .lignes()
        .into_iter()
        .map(|ligne| LigneMenu {
            ton: match ligne.ton {
                Ton::Repos => "repos",
                Ton::Actif => "actif",
                Ton::Attention => "attention",
                Ton::Information => "information",
            },
            texte: ligne.texte,
        })
        .collect();
    EtatMenu {
        nom: identite::NOM,
        lignes,
        demarrage_auto: demarrage_auto::est_actif(),
        mise_a_jour: mise_a_jour::mise_a_jour_disponible(disponible).map(|m| m.version),
    }
}

/// Les lignes du menu qui agissent.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionMenu {
    Ouvrir,
    Parametres,
    MiseAJour,
    Quitter,
}

#[tauri::command]
pub fn action_menu(app: AppHandle, action: ActionMenu) {
    if let Some(menu) = app.get_webview_window(MENU) {
        let _ = menu.hide();
    }
    match action {
        ActionMenu::Ouvrir => ramener(&app),
        ActionMenu::Parametres => {
            ramener(&app);
            let _ = app.emit_to(FENETRE, "aller-a", "parametres");
        }
        ActionMenu::MiseAJour => {
            mise_a_jour::montrer(&app);
        }
        ActionMenu::Quitter => app.exit(0),
    }
}

/// La hauteur du contenu du menu, mesurée par l'interface : la fenêtre la prend.
#[tauri::command]
pub fn taille_menu(app: AppHandle, hauteur: f64) -> Result<(), String> {
    let menu = app
        .get_webview_window(MENU)
        .ok_or_else(|| "fenêtre du menu introuvable".to_owned())?;
    menu.set_size(LogicalSize::new(LARGEUR_MENU, hauteur.clamp(80.0, 640.0)))
        .map_err(|e| e.to_string())
}
