//! La proposition d'installation (fenêtre « installation », vue `Installation.svelte`) : montrée au lancement d'un
//! exe hors de `programme\`, avant la fenêtre de l'appli. « Installer » range l'appli dans le dossier de données et
//! la relance de là ; « Lancer sans installer » ouvre l'appli sur place ; la croix ferme tout.

use std::path::{Path, PathBuf};
use std::process::Command;

use aria_coeur::installation::{self, Chemins};
use aria_coeur::{Reglages, identite, journal};
use serde::Serialize;
use tauri::window::Color;
use tauri::{AppHandle, Manager, State, Theme, WebviewUrl, WebviewWindowBuilder};

use crate::{Etat, erreur_fatale, ouvrir_principale};

/// L'étiquette de la fenêtre, citée par capabilities\installation.json.
pub const FENETRE_INSTALLATION: &str = "installation";

/// Ouvre la proposition d'installation.
pub fn ouvrir_fenetre(app: &AppHandle) -> tauri::Result<()> {
    let cache_webview = app.state::<Etat>().cache_webview.clone();
    WebviewWindowBuilder::new(app, FENETRE_INSTALLATION, WebviewUrl::default())
        .title(format!("Installer {}", identite::NOM))
        .inner_size(620.0, 560.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .theme(Some(Theme::Dark))
        .background_color(Color(3, 8, 18, 255))
        .data_directory(cache_webview)
        .build()?;
    Ok(())
}

/// Ce que la fenêtre affiche.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfosInstallation {
    nom: &'static str,
    version: &'static str,
    dossier: String,
    version_installee: Option<String>,
    raccourci: Option<String>,
}

/// Le raccourci du Bureau : « Aria Nom.lnk ».
fn chemin_raccourci(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .desktop_dir()
        .ok()
        .map(|bureau| bureau.join(format!("{}.lnk", identite::NOM)))
}

#[tauri::command]
pub fn infos_installation(app: AppHandle, etat: State<'_, Etat>) -> InfosInstallation {
    let programme = etat.dossier.programme();
    InfosInstallation {
        nom: identite::NOM,
        version: identite::VERSION,
        dossier: programme.display().to_string(),
        version_installee: installation::version_installee(&programme),
        raccourci: chemin_raccourci(&app).map(|c| c.display().to_string()),
    }
}

/// Installe, puis relance l'appli depuis `programme\` et arrête ce lancement. En cas d'échec, rien n'a changé et
/// le message dit pourquoi.
#[tauri::command]
pub fn installer(app: AppHandle, etat: State<'_, Etat>, raccourci: bool) -> Result<(), String> {
    // Contrat : un exe élevé ne se lance pas depuis un dossier qu'un programme sans droits peut modifier. Une
    // appli lancée normalement n'a pas besoin des droits administrateur pour se ranger dans le dossier de données.
    if systeme::est_eleve() {
        return Err(
            "L'appli tourne avec les droits administrateur : l'installation se fait sans eux. Ferme-la et \
                    relance l'exe d'un simple double-clic."
                .to_owned(),
        );
    }
    let exe = std::env::current_exe().map_err(|e| format!("L'exe de l'appli est introuvable : {e}"))?;
    let installe = installation::installer(&exe, &Chemins::du_dossier(&etat.dossier)).map_err(|e| e.0)?;
    let mut bilan = format!(
        "version {} installée dans {}",
        identite::VERSION,
        etat.dossier.programme().display()
    );
    if raccourci {
        match chemin_raccourci(&app).map(|lien| systeme::creer_raccourci(&lien, &installe)) {
            Some(Ok(())) => bilan.push_str(", raccourci du Bureau créé"),
            Some(Err(e)) => {
                bilan.push_str(", raccourci non créé");
                erreur_fatale::avertir(&format!(
                    "L'appli est installée, mais le raccourci du Bureau n'a pas pu être créé : {e}"
                ));
            }
            None => bilan.push_str(", raccourci non créé (Bureau introuvable)"),
        }
    }
    journal::consigner(&etat.dossier, "installation", &bilan);
    relancer(&app, &installe)
}

/// Lance l'exe installé et arrête celui-ci. La place d'exemplaire unique est rendue d'abord : sinon le nouveau
/// lancement la trouverait prise et s'arrêterait aussitôt.
pub fn relancer(app: &AppHandle, exe: &Path) -> Result<(), String> {
    tauri_plugin_single_instance::destroy(app);
    let mut commande = Command::new(exe);
    if let Some(dossier) = exe.parent() {
        commande.current_dir(dossier);
    }
    commande.spawn().map_err(|e| {
        format!(
            "L'appli est installée mais n'a pas pu être relancée : {e}. Lance-la depuis {}.",
            exe.display()
        )
    })?;
    app.exit(0);
    Ok(())
}

/// Ouvre l'appli sur place. « Ne plus me proposer » est enregistré dans les réglages.
#[tauri::command]
pub fn lancer_sans_installer(
    app: AppHandle,
    etat: State<'_, Etat>,
    ne_plus_proposer: bool,
) -> Result<(), String> {
    if ne_plus_proposer {
        let reglages = Reglages {
            proposer_installation: false,
            ..Reglages::charger(&etat.dossier)
        };
        reglages
            .enregistrer(&etat.dossier)
            .map_err(|e| format!("réglages non enregistrés : {e}"))?;
        journal::consigner(&etat.dossier, "réglages", "proposition d'installation coupée");
    }
    // La fenêtre de l'appli d'abord : fermer la dernière fenêtre arrêterait l'appli.
    ouvrir_principale(&app, true).map_err(|e| e.to_string())?;
    if let Some(fenetre) = app.get_webview_window(FENETRE_INSTALLATION) {
        let _ = fenetre.destroy();
    }
    Ok(())
}

/// Ce que l'installation demande à Windows.
#[cfg(windows)]
mod systeme {
    use std::io;
    use std::path::Path;

    use aria_coeur::identite;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
        IPersistFile,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, IsUserAnAdmin, ShellLink};
    use windows::core::{HSTRING, Interface};

    /// L'appli tourne-t-elle avec les droits administrateur ?
    #[allow(unsafe_code)]
    pub fn est_eleve() -> bool {
        // SAFETY : fonction sans paramètre, qui ne fait que lire le jeton du processus.
        unsafe { IsUserAnAdmin() }.as_bool()
    }

    /// Crée ou remplace le raccourci `lien` vers `cible`, avec l'icône de l'exe. Sur un fil à part, qui prépare
    /// COM pour lui seul.
    pub fn creer_raccourci(lien: &Path, cible: &Path) -> io::Result<()> {
        let (lien, cible) = (HSTRING::from(lien), cible.to_path_buf());
        std::thread::spawn(move || ecrire_raccourci(&lien, &cible))
            .join()
            .map_err(|_| io::Error::other("création du raccourci interrompue"))?
            .map_err(io::Error::other)
    }

    #[allow(unsafe_code)]
    fn ecrire_raccourci(lien: &HSTRING, cible: &Path) -> windows::core::Result<()> {
        // SAFETY : COM est préparé puis libéré sur ce fil, qui ne sert qu'à ça ; les objets COM sont relâchés
        // avant CoUninitialize, à la fin de la fermeture ; les textes sont des HSTRING valides pendant les appels.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let resultat = (|| {
                let raccourci: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                raccourci.SetPath(&HSTRING::from(cible))?;
                if let Some(dossier) = cible.parent() {
                    raccourci.SetWorkingDirectory(&HSTRING::from(dossier))?;
                }
                raccourci.SetIconLocation(&HSTRING::from(cible), 0)?;
                raccourci.SetDescription(&HSTRING::from(identite::NOM))?;
                raccourci.cast::<IPersistFile>()?.Save(lien, true)
            })();
            CoUninitialize();
            resultat
        }
    }
}

/// Hors de Windows (tests, outils) : ni droits administrateur ni raccourci.
#[cfg(not(windows))]
mod systeme {
    use std::io;
    use std::path::Path;

    pub fn est_eleve() -> bool {
        false
    }

    pub fn creer_raccourci(_lien: &Path, _cible: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows seulement"))
    }
}
