//! Réglages → LLM et workflows : les réglages fins de chaque GGUF (et ses consignes réécrites) et de chaque workflow
//! (et de ses checkpoints). Le travail est dans le cœur (`aria_coeur::reglages_fins`) ; ici, les commandes, hors du
//! fil de l'interface (elles lisent les modèles et l'en-tête des GGUF).

use std::sync::Arc;

use aria_coeur::plateau::{Contexte, Plateau};
use aria_coeur::reglages_fins::{self, EtatLlm, EtatReglages, EtatWorkflow};
use serde_json::{Map, Value};
use tauri::State;

async fn hors_interface<T: Send + 'static>(
    plateau: &State<'_, Arc<Plateau>>,
    travail: impl FnOnce(&Contexte) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let contexte = plateau.contexte().clone();
    tauri::async_runtime::spawn_blocking(move || travail(&contexte))
        .await
        .map_err(|e| format!("Réglages indisponibles : {e}"))?
}

/// Les LLM installés, les workflows et leurs réglages.
#[tauri::command]
pub async fn etat_reglages(plateau: State<'_, Arc<Plateau>>) -> Result<EtatReglages, String> {
    hors_interface(&plateau, |c| Ok(reglages_fins::etat(&c.dossier, &c.modeles))).await
}

/// Modifie les réglages et les consignes d'un GGUF ; une valeur nulle revient à celle d'origine.
#[tauri::command]
pub async fn modifier_reglages_llm(
    plateau: State<'_, Arc<Plateau>>,
    fichier: String,
    changements: Map<String, Value>,
    consignes: Map<String, Value>,
) -> Result<EtatLlm, String> {
    hors_interface(&plateau, move |c| {
        reglages_fins::modifier_llm(&c.dossier, &c.modeles, &fichier, &changements, &consignes)
    })
    .await
}

/// Modifie les réglages d'un workflow.
#[tauri::command]
pub async fn modifier_reglages_workflow(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    changements: Map<String, Value>,
) -> Result<EtatWorkflow, String> {
    hors_interface(&plateau, move |c| {
        reglages_fins::modifier_workflow(&c.dossier, &c.modeles, &id, &changements)
    })
    .await
}

/// Modifie les réglages d'un checkpoint de la famille d'un workflow.
#[tauri::command]
pub async fn modifier_reglages_variante(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    fichier: String,
    changements: Map<String, Value>,
) -> Result<EtatWorkflow, String> {
    hors_interface(&plateau, move |c| {
        reglages_fins::modifier_variante(&c.dossier, &c.modeles, &id, &fichier, &changements)
    })
    .await
}
