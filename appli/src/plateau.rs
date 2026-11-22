//! La page Plateau : créer une image depuis un prompt, suivre la file, voir les dernières créations. Le travail est
//! dans le cœur (`aria_coeur::plateau`, `aria_coeur::creations`) ; ici, les commandes, hors du fil de l'interface
//! quand elles lisent le disque.

use std::sync::Arc;

use aria_coeur::creations;
use aria_coeur::emplacements::Emplacements;
use aria_coeur::plateau::{Commande, Contexte, Demande, Options, Plateau};
use serde_json::Value;
use tauri::State;
use tauri::ipc::Response;

use crate::moteurs::Moteurs;

/// Les créations montrées par la page, au plus.
const CREATIONS_MAX: usize = 60;

/// Le Plateau de ce lancement : ComfyUI passe par l'orchestrateur des moteurs.
pub fn demarrer(emplacements: &Emplacements, moteurs: &Moteurs) -> Arc<Plateau> {
    Arc::new(Plateau::demarrer(
        Contexte {
            dossier: emplacements.dossier.clone(),
            data: emplacements.data.clone(),
            modeles: emplacements.modeles.clone(),
        },
        Arc::clone(&moteurs.orchestrateur),
    ))
}

async fn hors_interface<T: Send + 'static>(
    plateau: &State<'_, Arc<Plateau>>,
    travail: impl FnOnce(&Plateau) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let plateau = Arc::clone(plateau);
    tauri::async_runtime::spawn_blocking(move || travail(&plateau))
        .await
        .map_err(|e| format!("Plateau indisponible : {e}"))?
}

/// Workflows prêts, checkpoints, formats : relu à chaque appel.
#[tauri::command]
pub async fn options_plateau(plateau: State<'_, Arc<Plateau>>) -> Result<Options, String> {
    hors_interface(&plateau, |p| Ok(p.options())).await
}

/// Contrôle la demande et la met en file.
#[tauri::command]
pub async fn commander_image(plateau: State<'_, Arc<Plateau>>, demande: Demande) -> Result<Commande, String> {
    hors_interface(&plateau, move |p| p.commander(&demande)).await
}

/// Les commandes de ce lancement, la plus récente en premier.
#[tauri::command]
pub fn lire_commandes(plateau: State<'_, Arc<Plateau>>) -> Vec<Commande> {
    plateau.commandes()
}

/// ✕ Annuler.
#[tauri::command]
pub fn annuler_commande(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<Commande, String> {
    plateau.annuler(&id)
}

/// Valide les steps d'un checkpoint : il devient utilisable dans les workflows de sa famille.
#[tauri::command]
pub async fn valider_checkpoint(
    plateau: State<'_, Arc<Plateau>>,
    workflow: String,
    checkpoint: String,
    steps: u64,
) -> Result<(), String> {
    hors_interface(&plateau, move |p| {
        p.valider_checkpoint(&workflow, &checkpoint, steps)
    })
    .await
}

/// Les fiches des dernières créations, la plus récente en premier.
#[tauri::command]
pub async fn lire_creations(plateau: State<'_, Arc<Plateau>>) -> Result<Vec<Value>, String> {
    hors_interface(&plateau, |p| {
        Ok(creations::liste(&p.contexte().data, CREATIONS_MAX))
    })
    .await
}

/// Le PNG d'une création, en octets.
#[tauri::command]
pub async fn image_creation(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<Response, String> {
    hors_interface(&plateau, move |p| {
        let chemin = creations::chemin_image(&p.contexte().data, &id)?;
        std::fs::read(&chemin)
            .map(Response::new)
            .map_err(|e| format!("image de {id} illisible : {e}"))
    })
    .await
}
