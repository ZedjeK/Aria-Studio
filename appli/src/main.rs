//! Aria Studio : l'exe Windows. Il prépare le dossier de données, ouvre la fenêtre Tauri et expose à l'interface
//! les seules commandes listées dans build.rs.

// Pas de console derrière la fenêtre, sauf en débogage.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assistant;
mod commandes;
mod decouvrir;
mod demarrage_auto;
mod disque;
mod erreur_fatale;
mod icone;
mod installation;
mod lieux;
mod mcp;
mod mise_a_jour;
mod moteurs;
mod notifications;
mod plateau;
mod pont_v2;
mod reglages_fins;
mod scenario;
mod securite;
mod telechargements;
mod vision;

use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// Un essai portable sur un PC qui a la v2 : ses modèles, son ComfyUI et ses llama-server repris sans rien copier
/// ni rien écrire chez elle (`aria_coeur::reprise`). Rend les emplacements à utiliser, et si une v2 tournait.
fn reprendre_la_v2(emplacements: Emplacements, dossier_exe: &Path, profil: &Dossier) -> (Emplacements, bool) {
    use aria_coeur::reprise::{self, Issue};

    if !emplacements.portable {
        return (emplacements, false);
    }
    let Ok(catalogue) = aria_coeur::catalogue::Catalogue::embarque() else {
        return (emplacements, false);
    };
    let dossier = emplacements.dossier.clone();
    let v2 = profil.racine();
    match reprise::reprendre(
        &dossier,
        v2,
        &catalogue,
        &emplacements.moteurs,
        moteurs::ports_pris,
    ) {
        Ok(Issue::Reprise(bilan)) => {
            let liste: Vec<String> = bilan
                .repris
                .iter()
                .map(|e| format!("{} : {}", e.nom, e.chemin))
                .collect();
            journal::noter(
                &dossier,
                "Version 2 reprise pour l'essai",
                &bilan.depuis,
                journal::Resultat::Ok,
                Some(&format!("lue sans y écrire ; {}", liste.join(" ; "))),
            );
            // Le dossier des modèles est décidé au lancement : on le relit.
            (Emplacements::decider(dossier_exe, profil), false)
        }
        Ok(Issue::V2Lancee) => {
            journal::noter(
                &dossier,
                "Version 2 non reprise",
                &v2.display().to_string(),
                journal::Resultat::Partiel,
                Some("elle tourne en même temps (ports 8195 ou 8095 pris) : ferme-la, puis relance l'essai"),
            );
            (emplacements, true)
        }
        Ok(Issue::Rien) => (emplacements, false),
        Err(e) => {
            journal::noter(
                &dossier,
                "Version 2 non reprise",
                &v2.display().to_string(),
                journal::Resultat::Erreur,
                Some(&e),
            );
            (emplacements, false)
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
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // Lancé par la mise à jour intégrée de la v2 (`--mise-a-jour <pid> <ancien lanceur>`) : installer, relancer et
    // s'arrêter, sans fenêtre ni instance unique.
    if let Some(demande) = aria_coeur::pont_v2::lire_demande(&arguments) {
        std::process::exit(pont_v2::executer(demande, &profil));
    }
    // Comme en v2 : un data\ à côté de l'exe (clé USB…) garde tout à côté de lui (mode portable).
    let dossier_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| profil.racine().to_path_buf());
    let emplacements = Emplacements::decider(&dossier_exe, &profil);
    let dossier = emplacements.dossier.clone();
    dossier.preparer().map_err(|e| e.to_string())?;
    // Lancé par Claude : le relais MCP seul, sans fenêtre, sans journal de lancement ni instance unique.
    if std::env::args().skip(1).any(|a| a == mcp::ARGUMENT) {
        mcp::relayer(&dossier);
        return Ok(());
    }
    // Lancé à l'ouverture de session : l'icône seule, la fenêtre reste rangée jusqu'à ce qu'on l'ouvre.
    let silencieux = identite::EN_FOND && demarrage::est_silencieux(&arguments);
    let lancement = Lancement {
        emplacements,
        dossier_exe,
        profil,
        arguments,
        silencieux,
    };

    tauri::Builder::default()
        // Toujours le premier greffon : relancer l'exe ramène la fenêtre existante au lieu d'en ouvrir une autre.
        // Un second lancement silencieux (ouverture de session) ne fait pas surgir la fenêtre. Ce second lancement
        // s'arrête ici, dans le greffon : rien de ce que fait `demarrer_instance` (journal, canal MCP, moteurs) ne
        // doit avoir eu lieu avant, sinon il écraserait le canal de l'instance qui tourne.
        .plugin(tauri_plugin_single_instance::init(|app, arguments, _dossier| {
            if demarrage::doit_ramener(&arguments) {
                ramener(app);
            }
        }))
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
            mise_a_jour::chercher_mise_a_jour,
            mise_a_jour::voir_notes,
            mise_a_jour::installer_mise_a_jour,
            notifications::essayer_notification,
            moteurs::lire_moteurs,
            moteurs::lire_vram,
            moteurs::installer_moteur,
            moteurs::annuler_installation_moteur,
            moteurs::revenir_moteur,
            moteurs::liberer_carte,
            moteurs::choisir_comfyui,
            plateau::options_plateau,
            plateau::commander_image,
            telechargements::etat_modeles,
            telechargements::lire_telechargements,
            telechargements::telecharger_workflow,
            telechargements::installer_llm,
            telechargements::annuler_telechargement,
            telechargements::collection_modeles,
            telechargements::valider_steps_modele,
            telechargements::regler_lora,
            telechargements::oublier_reglage_lora,
            telechargements::jeter_modele,
            telechargements::corbeille_modeles,
            telechargements::restaurer_modele,
            telechargements::vider_corbeille_modeles,
            telechargements::usages_modeles,
            telechargements::definir_favori,
            telechargements::definir_llm_workflow,
            decouvrir::familles_decouvrir,
            decouvrir::chercher_modeles,
            decouvrir::fichiers_depot,
            decouvrir::preparer_lien,
            decouvrir::telecharger_lien,
            decouvrir::vignette_modele,
            decouvrir::retrouver_sources,
            decouvrir::etat_sources,
            decouvrir::relire_lora,
            scenario::sujets_scenario,
            scenario::structure_scenario,
            scenario::assembler_prompt,
            scenario::ecrire_prompt_scenario,
            scenario::traduire_precisions,
            scenario::decouper_sequence,
            scenario::proposer_directions,
            scenario::expliquer_erreur,
            vision::etat_vision,
            vision::verifier_images,
            vision::choisir_meilleures,
            vision::relancer_avec_prompt,
            scenario::annuler_ecriture,
            scenario::lire_personnages,
            scenario::enregistrer_personnage,
            scenario::supprimer_personnage,
            scenario::conversation_scenario,
            lieux::etat_emplacements,
            lieux::etat_reprise_v2,
            lieux::reprise_v2_vue,
            lieux::annuler_reprise_v2,
            lieux::ouvrir_dossier,
            lieux::parcourir_dossier,
            lieux::choisir_dossier_modeles,
            lieux::etat_cles,
            lieux::enregistrer_cle,
            lieux::supprimer_cle,
            lieux::tester_cle,
            reglages_fins::etat_reglages,
            reglages_fins::modifier_reglages_llm,
            reglages_fins::modifier_reglages_workflow,
            reglages_fins::modifier_reglages_variante,
            reglages_fins::adapter_a_ma_machine,
            reglages_fins::recommander_workflow,
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
            plateau::options_montage,
            plateau::estimer_tournage,
            plateau::commander_clip,
            plateau::video_creation,
            plateau::lire_voix,
            plateau::essayer_voix,
            plateau::garder_voix,
            plateau::supprimer_voix,
            plateau::importer_voix,
            plateau::son_voix,
            mcp::commande_mcp,
            assistant::etat_assistant,
            assistant::plan_assistant,
            assistant::accepter_licences_assistant,
            assistant::telecharger_assistant,
            assistant::terminer_assistant,
            assistant::essai_assistant,
            plateau::lire_sequences,
            plateau::creer_sequence,
            plateau::modifier_sequence,
            plateau::supprimer_sequence,
            plateau::restaurer_sequence,
            plateau::vider_corbeille_sequences,
            plateau::tourner_sequence,
            plateau::lire_castings,
            plateau::lire_casting,
            plateau::image_casting,
            plateau::source_casting,
            plateau::noter_casting,
            plateau::reveler_casting,
            plateau::favori_casting,
            plateau::source_depuis_casting,
            plateau::importer_source_casting,
            plateau::source_casting_depuis_rush,
            plateau::lancer_casting,
        ])
        .setup(move |app| demarrer_instance(app.handle(), lancement).map_err(Into::into))
        .build(tauri::generate_context!())
        .map_err(|e| format!("l'interface n'a pas pu démarrer : {e}"))?
        .run(|app, evenement| {
            if let RunEvent::Exit = evenement {
                // Si le démarrage n'a pas eu lieu (instance déjà là, erreur), il n'y a rien à fermer.
                let Some(etat) = app.try_state::<Etat>() else {
                    return;
                };
                // Les moteurs lancés par Aria s'arrêtent avec elle, et le canal MCP se ferme.
                if let Some(moteurs) = app.try_state::<moteurs::Moteurs>() {
                    moteurs::arreter(&moteurs);
                }
                if let Some(SessionMcp(Some(session))) = app.try_state::<SessionMcp>().as_deref() {
                    aria_coeur::mcp::fermer_canal(&etat.dossier, session);
                }
                journal::noter(&etat.dossier, "Aria fermée", "", journal::Resultat::Ok, None);
            }
        });
    Ok(())
}

/// Le canal MCP de ce lancement, gardé pour le fermer à la sortie (rien si le canal n'a pas pu s'ouvrir).
struct SessionMcp(Option<aria_coeur::mcp::Session>);

/// Ce que `lancer` a décidé avant que Tauri ne démarre ; seule l'instance qui tourne le met en œuvre.
struct Lancement {
    emplacements: Emplacements,
    dossier_exe: PathBuf,
    profil: Dossier,
    arguments: Vec<String>,
    silencieux: bool,
}

/// Le démarrage de l'instance, une fois le greffon d'instance unique passé : journal, reprise de la v2, moteurs,
/// Plateau, téléchargements, canal MCP, puis la fenêtre (la proposition d'installation, ou celle de l'appli).
fn demarrer_instance(app: &AppHandle, lancement: Lancement) -> Result<(), String> {
    let Lancement {
        emplacements,
        dossier_exe,
        profil,
        arguments,
        silencieux,
    } = lancement;
    let dossier = emplacements.dossier.clone();
    let portable = if emplacements.portable { " (portable)" } else { "" };
    journal::noter(
        &dossier,
        "Aria lancée",
        &format!("{} {}{portable}", identite::NOM, identite::VERSION),
        journal::Resultat::Ok,
        None,
    );
    // Après une mise à jour lancée par la v2 : la version attendue tourne-t-elle ? Avant le ménage du cache.
    aria_coeur::pont_v2::bilan_au_demarrage(&dossier, identite::VERSION);
    reprendre_les_cles(&dossier);
    let (emplacements, v2_lancee) = reprendre_la_v2(emplacements, &dossier_exe, &profil);
    // Restes d'un remplacement précédent, maintenant que l'ancienne version est arrêtée.
    aria_coeur::installation::nettoyer(&dossier.programme());
    // Exe lancé hors de programme (zip décompressé) : proposition de s'installer, avant la fenêtre de l'appli.
    // Jamais en mode portable, comme en v2.
    let proposer = !silencieux
        && !emplacements.portable
        && std::env::current_exe().is_ok_and(|exe| {
            aria_coeur::installation::doit_proposer(&exe, &dossier, &Reglages::charger(&dossier), &arguments)
        });
    let cache_webview = dossier.cache_webview();
    let moteurs = moteurs::Moteurs::new(&emplacements)?;
    moteurs::reprendre(&moteurs);
    let poignee = notifications::Poignee::default();
    let plateau = plateau::demarrer(&emplacements, &moteurs, Arc::clone(&poignee));
    let telechargeur = telechargements::demarrer(&emplacements);
    // Le canal local du serveur MCP pour Claude (127.0.0.1, jeton neuf à chaque lancement).
    let session_mcp = mcp::ouvrir(&dossier, &plateau, &moteurs);

    app.manage(emplacements);
    app.manage(lieux::RepriseAuLancement { v2_lancee });
    app.manage(Etat {
        dossier,
        cache_webview,
    });
    app.manage(icone::Presence::default());
    app.manage(mise_a_jour::Disponible::default());
    app.manage(moteurs);
    app.manage(plateau);
    app.manage(telechargeur);
    app.manage(Arc::new(aria_coeur::retrouver::Suivi::default()));
    app.manage(Arc::new(decouvrir::Sources(telechargements::Https::new())));
    app.manage(SessionMcp(session_mcp));
    // La file du Plateau peut prévenir dès que l'appli existe ; la poignée ne se pose qu'une fois.
    let _ = poignee.set(app.clone());
    if proposer {
        installation::ouvrir_fenetre(app).map_err(|e| e.to_string())
    } else {
        ouvrir_principale(app, !silencieux).map_err(|e| e.to_string())
    }
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
