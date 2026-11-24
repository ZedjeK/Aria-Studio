//! Mise à jour intégrée, côté appli : une requête vers GitHub au lancement (si le réglage le permet) ou quand
//! l'utilisateur clique « Vérifier maintenant », la pastille bleue, le bandeau et « Voir la mise à jour » quand
//! une version plus récente existe, puis le dialogue de la fenêtre de l'appli (`dialogues/MiseAJour.svelte`),
//! qui télécharge, vérifie et installe seulement sur demande.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use aria_coeur::mises_a_jour::{self, MiseAJour, Recherche};
use aria_coeur::{Dossier, Reglages, identite, journal};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::{Etat, FENETRE, icone, installation};

/// La mise à jour trouvée au lancement, s'il y en a une.
#[derive(Default)]
pub struct Disponible(Mutex<Option<MiseAJour>>);

impl Disponible {
    fn lire(&self) -> Option<MiseAJour> {
        self.0.lock().ok().and_then(|m| m.clone())
    }
}

/// Le client HTTPS : HTTPS seulement, certificats et proxy de Windows, délais bornés.
pub(crate) fn client(delai: Duration) -> Agent {
    Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(delai))
        .user_agent(format!("{}/{}", identite::NOM_EXE, identite::VERSION))
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

/// Une seule requête anonyme vers `releases/latest` du dépôt de l'appli, la même au lancement et à la demande.
/// Rend le texte de la réponse, ou pourquoi il n'y en a pas.
fn lire_derniere_release() -> Result<String, String> {
    let mut reponse = client(Duration::from_secs(30))
        .get(mises_a_jour::adresse_derniere_release())
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => "aucune version n'est publiée pour le moment".to_owned(),
            ureq::Error::StatusCode(403 | 429) => {
                "GitHub limite les requêtes anonymes : réessaie dans quelques minutes".to_owned()
            }
            ureq::Error::StatusCode(statut) => format!("GitHub a répondu {statut}"),
            autre => format!("GitHub est injoignable ({autre})"),
        })?;
    reponse
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("la réponse de GitHub est illisible ({e})"))
}

/// Garde la mise à jour trouvée, change l'icône et prévient la fenêtre de l'appli.
fn annoncer(app: &AppHandle, maj: MiseAJour) {
    let version = mises_a_jour::ecrire_version(maj.version);
    if let Ok(mut place) = app.state::<Disponible>().0.lock() {
        *place = Some(maj);
    }
    icone::changer(app, |etat| {
        etat.information = Some(format!("mise à jour {version} disponible"));
    });
    let _ = app.emit_to(FENETRE, "mise-a-jour", version);
}

/// Au lancement : vide le dossier de travail, puis, si le réglage le permet, cherche une mise à jour sans bloquer
/// l'appli. Une seule requête vers GitHub ; rien n'est téléchargé.
pub fn chercher_au_lancement(app: &AppHandle, dossier: &Dossier) {
    mises_a_jour::nettoyer(&mises_a_jour::dossier_travail(dossier));
    if !Reglages::charger(dossier).chercher_mises_a_jour {
        return;
    }
    let Some(actuelle) = mises_a_jour::lire_actuelle(identite::VERSION) else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(maj) = lire_derniere_release()
            .ok()
            .and_then(|json| mises_a_jour::analyser(&json, actuelle))
        else {
            return;
        };
        annoncer(&app, maj);
    });
}

/// « Vérifier maintenant » : la même requête que le lancement, faite à la demande de l'utilisateur, que le
/// réglage du lancement soit coché ou non. Rend la version trouvée, ou rien quand l'appli est à jour ; l'erreur dit
/// ce qui a manqué. Rien n'est téléchargé ni installé : l'installation reste au dialogue, sur demande.
#[tauri::command]
pub async fn chercher_mise_a_jour(app: AppHandle) -> Result<Option<InfosMiseAJour>, String> {
    let actuelle = mises_a_jour::lire_actuelle(identite::VERSION)
        .ok_or_else(|| "la version de l'appli est illisible".to_owned())?;
    let json = tauri::async_runtime::spawn_blocking(lire_derniere_release)
        .await
        .map_err(|e| format!("la recherche a été interrompue : {e}"))??;
    match mises_a_jour::rechercher(&json, actuelle) {
        Recherche::Disponible(maj) => {
            let infos = infos(&maj);
            annoncer(&app, maj);
            Ok(Some(infos))
        }
        Recherche::AJour => Ok(None),
        Recherche::Inexploitable(version) => Err(format!(
            "La version {} est publiée, mais son zip n'est pas exploitable : rien n'est proposé",
            mises_a_jour::ecrire_version(version)
        )),
        Recherche::Illisible => Err("la réponse de GitHub n'est pas une release lisible".to_owned()),
    }
}

/// Ce que l'interface affiche d'une mise à jour.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfosMiseAJour {
    nom: &'static str,
    actuelle: &'static str,
    pub version: String,
    taille: u64,
}

fn infos(maj: &MiseAJour) -> InfosMiseAJour {
    InfosMiseAJour {
        nom: identite::NOM,
        actuelle: identite::VERSION,
        version: mises_a_jour::ecrire_version(maj.version),
        taille: maj.paquet.taille,
    }
}

#[tauri::command]
pub fn mise_a_jour_disponible(disponible: State<'_, Disponible>) -> Option<InfosMiseAJour> {
    disponible.lire().as_ref().map(infos)
}

/// Montre la mise à jour : la fenêtre de l'appli revient au premier plan et ouvre son dialogue.
pub fn montrer(app: &AppHandle) {
    crate::ramener(app);
    let _ = app.emit_to(FENETRE, "aller-a", "mise-a-jour");
}

/// Ouvre la page de la release dans le navigateur : seulement une page du dépôt de l'appli.
#[tauri::command]
pub fn voir_notes(disponible: State<'_, Disponible>) -> Result<(), String> {
    let maj = disponible
        .lire()
        .ok_or_else(|| "aucune mise à jour connue".to_owned())?;
    if !maj.page.starts_with(&format!("{}/", mises_a_jour::depot())) {
        return Err("adresse de la page refusée".to_owned());
    }
    Command::new("explorer")
        .arg(&maj.page)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("le navigateur n'a pas pu être ouvert : {e}"))
}

/// Télécharge, vérifie et installe la mise à jour là où tourne l'exe, puis relance l'appli. À la moindre erreur,
/// rien n'a changé et le message dit pourquoi.
#[tauri::command]
pub async fn installer_mise_a_jour(app: AppHandle) -> Result<(), String> {
    let maj = app
        .state::<Disponible>()
        .lire()
        .ok_or_else(|| "aucune mise à jour connue".to_owned())?;
    let dossier = app.state::<Etat>().dossier.clone();
    let exe = std::env::current_exe().map_err(|e| format!("l'exe de l'appli est introuvable : {e}"))?;
    let installe = tauri::async_runtime::spawn_blocking(move || installer(&maj, &dossier, &exe))
        .await
        .map_err(|e| format!("mise à jour interrompue : {e}"))??;
    installation::relancer(&app, &installe)
}

fn installer(maj: &MiseAJour, dossier: &Dossier, exe: &std::path::Path) -> Result<PathBuf, String> {
    if !maj
        .paquet
        .adresse
        .starts_with(&mises_a_jour::prefixe_telechargement())
    {
        return Err("adresse de téléchargement refusée : elle n'est pas une release de ce dépôt".to_owned());
    }
    let dossier_exe = exe
        .parent()
        .ok_or_else(|| "le dossier de l'exe est introuvable".to_owned())?;
    let travail = mises_a_jour::dossier_travail(dossier);
    std::fs::create_dir_all(&travail).map_err(|e| format!("dossier de travail : {e}"))?;
    let zip = travail.join(&maj.paquet.nom);
    let mut reponse = client(Duration::from_secs(600))
        .get(&maj.paquet.adresse)
        .call()
        .map_err(|e| format!("téléchargement impossible : {e}"))?;
    mises_a_jour::recevoir(
        reponse.body_mut().with_config().limit(maj.paquet.taille).reader(),
        &maj.paquet,
        &zip,
    )
    .map_err(|e| format!("téléchargement refusé : {e}"))?;
    let versions = dossier
        .sauvegardes()
        .join(aria_coeur::installation::DOSSIER_VERSIONS);
    let installe = mises_a_jour::installer_zip(&zip, dossier_exe, &versions, &travail).map_err(|e| e.0)?;
    journal::noter(
        dossier,
        "Mise à jour installée",
        &format!(
            "{} → {}",
            identite::VERSION,
            mises_a_jour::ecrire_version(maj.version)
        ),
        journal::Resultat::Ok,
        Some("l'ancienne version est gardée dans les sauvegardes"),
    );
    Ok(installe)
}
