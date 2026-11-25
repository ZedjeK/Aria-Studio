//! L'icône de notification et son menu, pour une appli qui sert en arrière-plan (`identite::EN_FOND`).
//!
//! Le menu est une petite fenêtre aux couleurs de l'appli (étiquette « menu », vue `MenuIcone.svelte`), ouverte au
//! clic droit près de l'icône et rangée dès qu'elle perd le focus : le menu natif de Windows ne prend pas les
//! couleurs d'Aria. Le clic gauche ramène la fenêtre principale.
//!
//! Un fil relève toutes les quelques secondes ce qui se passe (la file du Plateau, les téléchargements, les moteurs,
//! la VRAM) : l'icône, son infobulle et le bloc « État » du menu en sortent (`aria_coeur::presence`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aria_coeur::moteurs::EtatMoteur;
use aria_coeur::plateau::Plateau;
use aria_coeur::presence::{self, EtatIcone, Lecture, PageMenu, Ton};
use aria_coeur::telechargement::Telechargeur;
use aria_coeur::{carte, identite};
use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, Theme, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

use crate::mise_a_jour::{self, Disponible};
use crate::moteurs::Moteurs;
use crate::{FENETRE, demarrage_auto, ramener};

/// L'identifiant de l'icône de notification.
const ICONE: &str = "principale";

/// L'étiquette de la fenêtre du menu, citée par capabilities\menu.json.
pub const MENU: &str = "menu";

/// L'événement qui demande au menu de relire son état.
const RELIRE: &str = "etat-icone";

/// La largeur du menu ; sa hauteur suit son contenu (commande `taille_menu`).
const LARGEUR_MENU: f64 = 300.0;

/// Le temps entre deux relevés de la file du Plateau, des téléchargements et des installations.
const PAS_DU_RELEVE: Duration = Duration::from_secs(3);

/// Les moteurs (processus lancés, ports) et la VRAM se relisent plus rarement quand personne ne regarde le menu :
/// un relevé tous les `TOURS_MOTEURS` tours.
const TOURS_MOTEURS: u32 = 5;

/// L'état montré par l'icône, partagé entre les commandes.
#[derive(Default)]
pub struct Presence {
    etat: Mutex<EtatIcone>,
    /// Le dernier échec d'une action du menu (libérer la carte, annuler) : orange jusqu'à la prochaine réussite.
    echec: Mutex<Option<String>>,
}

impl Presence {
    fn lire(&self) -> EtatIcone {
        self.etat.lock().map(|etat| etat.clone()).unwrap_or_default()
    }

    fn echec(&self) -> Option<String> {
        self.echec.lock().ok().and_then(|echec| echec.clone())
    }

    fn regler_echec(&self, echec: Option<String>) {
        if let Ok(mut place) = self.echec.lock() {
            *place = echec;
        }
    }
}

/// Ce que le relevé garde d'un tour à l'autre : les moteurs et la VRAM se relisent moins souvent que la file.
#[derive(Default)]
struct Releve {
    moteurs: Vec<EtatMoteur>,
    vram: Option<carte::Vram>,
}

/// Relève ce qu'Aria fait et met l'icône, l'infobulle et le menu à jour. `moteurs_aussi` : relire aussi les moteurs
/// et la VRAM (un programme à lancer et des ports à sonder, à ne pas faire à chaque tour).
fn relire(app: &AppHandle, releve: &mut Releve, moteurs_aussi: bool) {
    if moteurs_aussi {
        if let Some(moteurs) = app.try_state::<Moteurs>() {
            releve.moteurs = crate::moteurs::lecteur_etats(&moteurs)();
        }
        releve.vram = carte::vram_directe();
    }
    let commandes = app
        .try_state::<Arc<Plateau>>()
        .map(|plateau| plateau.commandes())
        .unwrap_or_default();
    let telechargements: Vec<String> = app
        .try_state::<Arc<Telechargeur>>()
        .map(|file| file.taches().into_iter().map(|tache| tache.etat).collect())
        .unwrap_or_default();
    let installation = app
        .try_state::<Moteurs>()
        .and_then(|moteurs| moteurs.installation_en_cours());
    let lecture = Lecture::depuis(
        &releve.moteurs,
        releve.vram,
        &commandes,
        &telechargements,
        installation.as_deref(),
    );
    let echec = app.state::<Presence>().echec();
    changer(app, |etat| etat.lire(&lecture, echec.as_deref()));
}

/// Un relevé complet, hors du fil appelant (l'ouverture du menu, une action qui vient de se terminer).
fn relire_sans_attendre(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || relire(&app, &mut Releve::default(), true));
}

/// Le fil du relevé : il tourne tant que l'appli tourne.
fn surveiller(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut releve = Releve::default();
        let mut tour: u32 = 0;
        loop {
            let menu_ouvert = app
                .get_webview_window(MENU)
                .is_some_and(|menu| menu.is_visible().unwrap_or(false));
            relire(
                &app,
                &mut releve,
                menu_ouvert || tour.is_multiple_of(TOURS_MOTEURS),
            );
            tour = tour.wrapping_add(1);
            std::thread::sleep(PAS_DU_RELEVE);
        }
    });
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
        let avant = etat.clone();
        modifier(&mut etat);
        // Rien n'a changé (le relevé repasse toutes les quelques secondes) : ni redessin, ni message au menu.
        if *etat == avant {
            return;
        }
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

    surveiller(app);

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
    // Le menu s'ouvre sur l'état d'avant, et se met à jour dès que le relevé est fait.
    relire_sans_attendre(app);
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
    /// Une génération tourne : « Annuler la génération en cours » n'agit que dans ce cas.
    generation_en_cours: bool,
}

#[tauri::command]
pub fn etat_menu(
    presence: State<'_, Presence>,
    disponible: State<'_, Disponible>,
    plateau: State<'_, Arc<Plateau>>,
) -> EtatMenu {
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
        generation_en_cours: presence::commande_a_annuler(&plateau.commandes()).is_some(),
    }
}

/// Les lignes du menu qui agissent.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionMenu {
    Ouvrir,
    /// « Agir sans ouvrir » : annule la commande du Plateau qui tourne.
    AnnulerGeneration,
    /// « Agir sans ouvrir » : ComfyUI vide sa VRAM, llama-server s'arrête.
    LibererCarte,
    /// « Aller à » : ramène la fenêtre sur cette page.
    Aller(PageMenu),
    MiseAJour,
    Quitter,
}

#[tauri::command]
pub fn action_menu(app: AppHandle, action: ActionMenu) {
    if let Some(menu) = app.get_webview_window(MENU) {
        let _ = menu.hide();
    }
    match action {
        ActionMenu::Ouvrir => {
            app.state::<Presence>().regler_echec(None);
            ramener(&app);
        }
        ActionMenu::AnnulerGeneration => annuler_la_generation(&app),
        ActionMenu::LibererCarte => liberer_la_carte(&app),
        ActionMenu::Aller(page) => {
            ramener(&app);
            let _ = app.emit_to(FENETRE, "aller-a", page.identifiant());
        }
        ActionMenu::MiseAJour => {
            mise_a_jour::montrer(&app);
        }
        ActionMenu::Quitter => app.exit(0),
    }
}

/// Annule la commande du Plateau qui tourne (la même annulation que le ✕ de la file).
fn annuler_la_generation(app: &AppHandle) {
    let Some(plateau) = app.try_state::<Arc<Plateau>>() else {
        return;
    };
    let Some(id) = presence::commande_a_annuler(&plateau.commandes()) else {
        return;
    };
    let echec = plateau.annuler(&id).err();
    app.state::<Presence>().regler_echec(echec);
    relire_sans_attendre(app);
}

/// Rend la carte graphique (la même action que « Libérer la carte » de la page Moteurs). Elle attend la fin d'un
/// rendu en cours : hors du fil de l'interface.
fn liberer_la_carte(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let echec = app
            .try_state::<Moteurs>()
            .and_then(|moteurs| moteurs.liberer().err())
            .map(|e| format!("libération de la carte impossible : {e}"));
        app.state::<Presence>().regler_echec(echec);
        relire(&app, &mut Releve::default(), true);
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lire(json: &str) -> Option<ActionMenu> {
        serde_json::from_str(json).ok()
    }

    #[test]
    fn le_menu_comprend_les_actions_que_l_interface_envoie() {
        assert!(matches!(lire("\"ouvrir\""), Some(ActionMenu::Ouvrir)));
        assert!(matches!(
            lire("\"annulerGeneration\""),
            Some(ActionMenu::AnnulerGeneration)
        ));
        assert!(matches!(lire("\"libererCarte\""), Some(ActionMenu::LibererCarte)));
        assert!(matches!(lire("\"miseAJour\""), Some(ActionMenu::MiseAJour)));
        assert!(matches!(lire("\"quitter\""), Some(ActionMenu::Quitter)));
        assert!(matches!(
            lire("{\"aller\":\"montage\"}"),
            Some(ActionMenu::Aller(PageMenu::Montage))
        ));
    }

    #[test]
    fn une_action_inconnue_est_refusee() {
        assert!(lire("\"parametres\"").is_none());
        assert!(lire("{\"aller\":\"journal\"}").is_none());
    }

    #[test]
    fn l_etat_du_menu_part_en_camel_case() {
        let etat = EtatMenu {
            nom: "Aria Studio",
            lignes: vec![LigneMenu {
                ton: "actif",
                texte: "ComfyUI : en marche".to_owned(),
            }],
            demarrage_auto: false,
            mise_a_jour: None,
            generation_en_cours: true,
        };
        let json = serde_json::to_value(&etat).unwrap_or_default();
        assert_eq!(json["generationEnCours"], true);
        assert_eq!(json["demarrageAuto"], false);
        assert_eq!(json["lignes"][0]["texte"], "ComfyUI : en marche");
    }
}
