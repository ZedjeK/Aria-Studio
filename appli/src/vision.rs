//! Le LLM regarde ce qui est sorti (`aria_coeur::verification`) : vérifier une image, choisir les meilleures d'un
//! lot, relancer avec le prompt corrigé. Le travail est dans le cœur ; ici, les commandes.

use std::sync::Arc;

use aria_coeur::plateau::{Commande, Plateau};
use aria_coeur::verification::{self, Choix, EtatVision, Relance, Verifications};
use tauri::State;

use crate::moteurs::Moteurs;
use crate::scenario::ecriture;

/// Le LLM qui regarderait les images (pour ce modèle), ou pourquoi il n'y en a pas.
#[tauri::command]
pub async fn etat_vision(
    plateau: State<'_, Arc<Plateau>>,
    workflow: Option<String>,
) -> Result<EtatVision, String> {
    let contexte = plateau.contexte().clone();
    tauri::async_runtime::spawn_blocking(move || verification::etat_vision(&contexte, workflow.as_deref()))
        .await
        .map_err(|e| format!("Vision indisponible : {e}"))
}

/// « Vérifier l'image » : le LLM qui voit compare chaque image à l'intention, et propose un prompt corrigé (✕ Annuler
/// l'arrête).
#[tauri::command]
pub async fn verifier_images(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
    ids: Vec<String>,
    llm: Option<String>,
) -> Result<Verifications, String> {
    let contexte = plateau.contexte().clone();
    let mut orchestrateur = ecriture(&moteurs);
    tauri::async_runtime::spawn_blocking(move || {
        verification::verifier(&contexte, &mut orchestrateur, &ids, llm.as_deref(), |_, _| true)
    })
    .await
    .map_err(|e| format!("Vision indisponible : {e}"))?
}

/// « Choisir les meilleures » : le LLM qui voit compare un lot et retient celles qui respectent le mieux l'intention.
#[tauri::command]
pub async fn choisir_meilleures(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
    ids: Vec<String>,
    llm: Option<String>,
) -> Result<Choix, String> {
    let contexte = plateau.contexte().clone();
    let mut orchestrateur = ecriture(&moteurs);
    tauri::async_runtime::spawn_blocking(move || {
        verification::choisir(&contexte, &mut orchestrateur, &ids, llm.as_deref())
    })
    .await
    .map_err(|e| format!("Vision indisponible : {e}"))?
}

/// « Relancer avec ce prompt » : la même demande que la création (image du Plateau ou clip du Montage), avec ce
/// prompt, remise en file.
#[tauri::command]
pub async fn relancer_avec_prompt(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    prompt: String,
) -> Result<Commande, String> {
    let plateau = Arc::clone(&plateau);
    tauri::async_runtime::spawn_blocking(move || {
        match verification::relance(plateau.contexte(), &id, &prompt)? {
            Relance::Image(demande) => plateau.commander(&demande),
            Relance::Clip(demande) => plateau.commander_clip(&demande),
        }
    })
    .await
    .map_err(|e| format!("Plateau indisponible : {e}"))?
}
