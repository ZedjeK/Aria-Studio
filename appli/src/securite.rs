//! Réglages → Sécurité : les listes du filtre AS-18, les ajouts et les phrases témoins de l'utilisateur. Le travail est
//! dans le cœur (`aria_coeur::securite_reglages`) ; ici, les commandes, hors du fil de l'interface (le filtre se
//! reconstruit à chaque appel).

use aria_coeur::Dossier;
use aria_coeur::securite_reglages::{self, Ajout, Essai, EtatSecurite};
use tauri::State;

use crate::Etat;

async fn hors_interface<T: Send + 'static>(
    etat: &State<'_, Etat>,
    travail: impl FnOnce(&Dossier) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let dossier = etat.dossier.clone();
    tauri::async_runtime::spawn_blocking(move || travail(&dossier))
        .await
        .map_err(|e| format!("Sécurité indisponible : {e}"))?
}

/// Les listes (socle et ajouts) et les phrases témoins rejouées.
#[tauri::command]
pub async fn etat_securite(etat: State<'_, Etat>) -> Result<EtatSecurite, String> {
    hors_interface(&etat, |d| Ok(securite_reglages::etat(d))).await
}

/// Le verdict du filtre sur une phrase, sans rien générer.
#[tauri::command]
pub async fn essayer_securite(etat: State<'_, Etat>, texte: String) -> Result<Essai, String> {
    hors_interface(&etat, move |d| securite_reglages::essayer(d, &texte)).await
}

/// Un terme de plus dans une liste ; à confirmer s'il ferait bloquer une phrase témoin qui doit passer.
#[tauri::command]
pub async fn ajouter_terme_securite(
    etat: State<'_, Etat>,
    liste: String,
    terme: String,
    forcer: bool,
) -> Result<Ajout, String> {
    hors_interface(&etat, move |d| {
        securite_reglages::ajouter(d, &liste, &terme, forcer)
    })
    .await
}

/// Retire un des ajouts de l'utilisateur ; le socle ne se retire pas.
#[tauri::command]
pub async fn retirer_terme_securite(
    etat: State<'_, Etat>,
    liste: String,
    terme: String,
) -> Result<EtatSecurite, String> {
    hors_interface(&etat, move |d| securite_reglages::retirer(d, &liste, &terme)).await
}

/// Une phrase témoin de plus, qui doit bloquer ou passer.
#[tauri::command]
pub async fn ajouter_temoin_securite(
    etat: State<'_, Etat>,
    texte: String,
    attendu: String,
) -> Result<EtatSecurite, String> {
    hors_interface(&etat, move |d| {
        securite_reglages::ajouter_temoin(d, &texte, &attendu)
    })
    .await
}

/// Retire une des phrases témoins de l'utilisateur.
#[tauri::command]
pub async fn retirer_temoin_securite(etat: State<'_, Etat>, texte: String) -> Result<EtatSecurite, String> {
    hors_interface(&etat, move |d| securite_reglages::retirer_temoin(d, &texte)).await
}
