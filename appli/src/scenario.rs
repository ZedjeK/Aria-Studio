//! Le Scénario : les listes guidées et l'assemblage du prompt (`aria_coeur::listes`). Le travail est dans le cœur ;
//! ici, les commandes.

use aria_coeur::emplacements::Emplacements;
use aria_coeur::listes::{self, Assemblage, DemandeAssemblage, Structure, Sujet};
use tauri::State;

/// Les sujets du Scénario.
#[tauri::command]
pub fn sujets_scenario() -> Vec<Sujet> {
    listes::sujets()
}

/// Blocs, accordéons, valeurs et incompatibilités d'un sujet.
#[tauri::command]
pub fn structure_scenario(sujet: String) -> Result<Structure, String> {
    listes::structure(&sujet)
}

/// Le prompt assemblé à partir des choix, après contrôle des règles et du filtre AS-18.
#[tauri::command]
pub async fn assembler_prompt(
    lieux: State<'_, Emplacements>,
    demande: DemandeAssemblage,
) -> Result<Assemblage, String> {
    let dossier = lieux.dossier.clone();
    tauri::async_runtime::spawn_blocking(move || listes::assembler(&dossier, &demande))
        .await
        .map_err(|e| format!("Scénario indisponible : {e}"))?
}
