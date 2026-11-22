//! Aria Studio : l'exe Windows. Il prépare le dossier de données, ouvre la fenêtre Tauri et expose à l'interface
//! les seules commandes listées dans build.rs.

// Pas de console derrière la fenêtre, sauf en débogage.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commandes;
mod demarrage_auto;
mod disque;
mod erreur_fatale;
mod icone;
mod installation;
mod mise_a_jour;
mod moteurs;
mod plateau;
mod securite;

use std::path::{Path, PathBuf};

use aria_coeur::emplacements::Emplacements;
use aria_coeur::{Dossier, Reglages, demarrage, identite, journal};
use tauri::window::Color;
use tauri::{AppHandle, Manager, RunEvent, Theme, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// L'étiquette de la fenêtre principale, citée par capabilities\principale.json.
pub const FENETRE: &str = "principale";

/// Ce que les commandes partagent.
pub struct Etat {
    pub dossier: Dossier,
    /// Contrat : WebView2 range ses données dans le dossier de l'appli, jamais dans %LOCALAPPDATA%.
    pub cache_webview: PathBuf,
}

/// Les clés Civitai et Hugging Face de la v2 (Gestionnaire d'identification Windows) recopiées dans `secrets\`, au
/// journal ce qui a été repris ou a échoué. L'entrée du Gestionnaire reste en place pour une v2 restaurée.
#[cfg(windows)]
fn reprendre_les_cles(dossier: &Dossier) {
    use aria_coeur::cles::{self, GestionnaireIdentification, Reprise};
    use aria_coeur::secrets::{Coffre, Dpapi};

    let coffre = Coffre::du_dossier(dossier, &Dpapi);
    for (source, reprise) in cles::reprendre_de_la_v2(&coffre, &GestionnaireIdentification) {
        match reprise {
            Reprise::Reprise => journal::noter(
                dossier,
                "Clé reprise du coffre Windows",
                source.nom(),
                journal::Resultat::Ok,
                Some("l'entrée du coffre Windows reste en place pour la version 2"),
            ),
            Reprise::Echec(message) => journal::noter(
                dossier,
                "Clé non reprise du coffre Windows",
                source.nom(),
                journal::Resultat::Erreur,
                Some(&message),
            ),
            Reprise::DejaLa | Reprise::Absente => {}
        }
    }
}

/// Hors de Windows, il n'y a ni Gestionnaire d'identification ni DPAPI : rien à reprendre.
#[cfg(not(windows))]
fn reprendre_les_cles(_dossier: &Dossier) {}

fn main() {
    if let Err(message) = lancer() {
        erreur_fatale::afficher(&message);
        std::process::exit(1);
    }
}

fn lancer() -> Result<(), String> {
    let profil = Dossier::du_profil().map_err(|e| e.to_string())?;
    // Comme en v2 : un data\ à côté de l'exe (clé USB…) garde tout à côté de lui (mode portable).
    let dossier_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| profil.racine().to_path_buf());
    let emplacements = Emplacements::decider(&dossier_exe, &profil);
    let dossier = emplacements.dossier.clone();
    dossier.preparer().map_err(|e| e.to_string())?;
    let portable = if emplacements.portable { " (portable)" } else { "" };
    journal::noter(
        &dossier,
        "Aria lancée",
        &format!("{} {}{portable}", identite::NOM, identite::VERSION),
        journal::Resultat::Ok,
        None,
    );
    reprendre_les_cles(&dossier);
    // Restes d'un remplacement précédent, maintenant que l'ancienne version est arrêtée.
    aria_coeur::installation::nettoyer(&dossier.programme());
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // Lancé à l'ouverture de session : l'icône seule, la fenêtre reste rangée jusqu'à ce qu'on l'ouvre.
    let silencieux = identite::EN_FOND && demarrage::est_silencieux(&arguments);
    // Exe lancé hors de programme\ (zip décompressé) : proposition de s'installer, avant la fenêtre de l'appli.
    // Jamais en mode portable, comme en v2.
    let proposer = !silencieux
        && !emplacements.portable
        && std::env::current_exe().is_ok_and(|exe| {
            aria_coeur::installation::doit_proposer(&exe, &dossier, &Reglages::charger(&dossier), &arguments)
        });
    let cache_webview = dossier.cache_webview();
    let moteurs = moteurs::Moteurs::new(&emplacements)?;
    moteurs::reprendre(&moteurs);
    let plateau = plateau::demarrer(&emplacements, &moteurs);

    tauri::Builder::default()
        // Toujours le premier greffon : relancer l'exe ramène la fenêtre existante au lieu d'en ouvrir une autre.
        // Un second lancement silencieux (ouverture de session) ne fait pas surgir la fenêtre.
        .plugin(tauri_plugin_single_instance::init(|app, arguments, _dossier| {
            if !demarrage::est_silencieux(arguments.iter().skip(1)) {
                ramener(app);
            }
        }))
        .manage(Etat {
            dossier,
            cache_webview,
        })
        .manage(icone::Presence::default())
        .manage(mise_a_jour::Disponible::default())
        .manage(moteurs)
        .manage(plateau)
        .invoke_handler(tauri::generate_handler![
            commandes::identite,
            commandes::lire_reglages,
            commandes::enregistrer_reglages,
            commandes::mesurer_disque,
            commandes::lire_journal,
            commandes::quitter,
            demarrage_auto::lire_demarrage_auto,
            demarrage_auto::regler_demarrage_auto,
            icone::etat_menu,
            icone::action_menu,
            icone::taille_menu,
            installation::infos_installation,
            installation::installer,
            installation::lancer_sans_installer,
            mise_a_jour::mise_a_jour_disponible,
            mise_a_jour::voir_notes,
            mise_a_jour::installer_mise_a_jour,
            moteurs::lire_moteurs,
            moteurs::installer_moteur,
            moteurs::annuler_installation_moteur,
            moteurs::revenir_moteur,
            moteurs::liberer_carte,
            plateau::options_plateau,
            plateau::commander_image,
            securite::etat_securite,
            securite::essayer_securite,
            securite::ajouter_terme_securite,
            securite::retirer_terme_securite,
            securite::ajouter_temoin_securite,
            securite::retirer_temoin_securite,
            plateau::options_retouche,
            plateau::commander_retouche,
            plateau::commander_agrandissement,
            plateau::importer_image,
            plateau::lire_commandes,
            plateau::annuler_commande,
            plateau::valider_checkpoint,
            plateau::lire_rushes,
            plateau::etat_rushes,
            plateau::agir_rushes,
            plateau::vider_corbeille,
            plateau::creer_dossier,
            plateau::renommer_dossier,
            plateau::supprimer_dossier,
            plateau::image_creation,
        ])
        .setup(move |app| {
            if proposer {
                installation::ouvrir_fenetre(app.handle())?;
            } else {
                ouvrir_principale(app.handle(), !silencieux)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .map_err(|e| format!("l'interface n'a pas pu démarrer : {e}"))?
        .run(|app, evenement| {
            if let RunEvent::Exit = evenement {
                // Les moteurs lancés par Aria s'arrêtent avec elle.
                moteurs::arreter(&app.state::<moteurs::Moteurs>());
                journal::noter(
                    &app.state::<Etat>().dossier,
                    "Aria fermée",
                    "",
                    journal::Resultat::Ok,
                    None,
                );
            }
        });
    Ok(())
}

/// Ouvre la fenêtre de l'appli et, pour une appli qui sert en arrière-plan, son icône de notification.
pub fn ouvrir_principale(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    let cache_webview = app.state::<Etat>().cache_webview.clone();
    let fenetre = WebviewWindowBuilder::new(app, FENETRE, WebviewUrl::default())
        .title(identite::NOM)
        .inner_size(1100.0, 740.0)
        .min_inner_size(900.0, 600.0)
        .center()
        // Sans cadre de Windows : la barre de titre est celle de l'interface (BarreTitre.svelte).
        .decorations(false)
        .visible(visible)
        .theme(Some(Theme::Dark))
        // La couleur du fond de l'appli : pas d'éclair blanc avant le premier affichage.
        .background_color(Color(3, 8, 18, 255))
        .data_directory(cache_webview.clone())
        .build()?;
    if identite::EN_FOND {
        icone::creer(app, cache_webview)?;
        // Fermer la fenêtre ne suffit pas à arrêter l'appli : elle reste dans son icône.
        let rangee = fenetre.clone();
        fenetre.on_window_event(move |evenement| {
            if let WindowEvent::CloseRequested { api, .. } = evenement {
                api.prevent_close();
                let _ = rangee.hide();
            }
        });
    }
    // Une requête vers GitHub, seulement si le réglage le permet ; aucune installation sans accord.
    let dossier = app.state::<Etat>().dossier.clone();
    mise_a_jour::chercher_au_lancement(app, &dossier);
    Ok(())
}

/// Ramène au premier plan la fenêtre ouverte (celle de l'appli, ou la proposition d'installation), même réduite
/// ou cachée.
fn ramener(app: &AppHandle) {
    let fenetre = app
        .get_webview_window(FENETRE)
        .or_else(|| app.get_webview_window(installation::FENETRE_INSTALLATION));
    if let Some(fenetre) = fenetre {
        // Chaque étape est indépendante : si l'une échoue, les autres ramènent quand même la fenêtre.
        let _ = fenetre.unminimize();
        let _ = fenetre.show();
        let _ = fenetre.set_focus();
    }
}
