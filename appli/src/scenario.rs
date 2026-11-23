//! Le Scénario : les listes guidées et l'assemblage du prompt (`aria_coeur::listes`), l'écriture du prompt et la
//! traduction des précisions par le LLM (`aria_coeur::scenario`). Le travail est dans le cœur ; ici, les commandes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use aria_coeur::conversation::{self, DemandeConversation, Tour};
use aria_coeur::decoupage::{self, Decoupe, DemandeDecoupage};
use aria_coeur::directions::{self, DemandeDirections, Directions};
use aria_coeur::emplacements::Emplacements;
use aria_coeur::listes::{self, Assemblage, DemandeAssemblage, Structure, Sujet};
use aria_coeur::personnages::{self, Personnage};
use aria_coeur::plateau::{EcritureAnnulable, Plateau};
use aria_coeur::scenario::{self, DemandeEcriture, Ecrit};
use tauri::State;

use crate::moteurs::Moteurs;

/// Une écriture du Scénario, que ✕ Annuler arrête ; une nouvelle écriture repart sans annulation.
fn ecriture(moteurs: &Moteurs) -> EcritureAnnulable {
    moteurs.ecriture_annulee.store(false, Ordering::Relaxed);
    EcritureAnnulable {
        orchestrateur: Arc::clone(&moteurs.orchestrateur),
        annule: Arc::clone(&moteurs.ecriture_annulee),
    }
}

/// ✕ Annuler : l'écriture en cours s'arrête (le LLM cesse d'écrire, la connexion est coupée).
#[tauri::command]
pub fn annuler_ecriture(moteurs: State<'_, Moteurs>) {
    moteurs.ecriture_annulee.store(true, Ordering::Relaxed);
}

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

/// « ✨ Écrire le prompt » : le LLM écrit le prompt d'un atelier pour le modèle cible (il prend la carte, comme au
/// Plateau).
#[tauri::command]
pub async fn ecrire_prompt_scenario(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
    demande: DemandeEcriture,
) -> Result<Ecrit, String> {
    let contexte = plateau.contexte().clone();
    let mut orchestrateur = ecriture(&moteurs);
    tauri::async_runtime::spawn_blocking(move || {
        scenario::ecrire_prompt(&contexte, &mut orchestrateur, &demande)
    })
    .await
    .map_err(|e| format!("Scénario indisponible : {e}"))?
}

/// Traduit les précisions du Scénario en anglais.
#[tauri::command]
pub async fn traduire_precisions(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
    texte: String,
    llm: Option<String>,
) -> Result<String, String> {
    let contexte = plateau.contexte().clone();
    let mut orchestrateur = ecriture(&moteurs);
    tauri::async_runtime::spawn_blocking(move || {
        scenario::traduire_precisions(&contexte, &mut orchestrateur, &texte, llm.as_deref())
    })
    .await
    .map_err(|e| format!("Scénario indisponible : {e}"))?
}

/// Atelier Séquence : le LLM écrit les prompts de tous les plans (✨ Tout écrire), ou d'un seul (✨ d'un plan), puis
/// les garde-fous du cœur les contrôlent.
#[tauri::command]
pub async fn decouper_sequence(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
    demande: DemandeDecoupage,
) -> Result<Decoupe, String> {
    let contexte = plateau.contexte().clone();
    let mut orchestrateur = ecriture(&moteurs);
    tauri::async_runtime::spawn_blocking(move || decoupage::decouper(&contexte, &mut orchestrateur, &demande))
        .await
        .map_err(|e| format!("Scénario indisponible : {e}"))?
}

/// « Proposer des directions » : à partir d'une envie vague, le LLM propose trois directions distinctes, avant
/// d'écrire le prompt.
#[tauri::command]
pub async fn proposer_directions(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
    demande: DemandeDirections,
) -> Result<Directions, String> {
    let contexte = plateau.contexte().clone();
    let mut orchestrateur = ecriture(&moteurs);
    tauri::async_runtime::spawn_blocking(move || {
        directions::proposer(&contexte, &mut orchestrateur, &demande)
    })
    .await
    .map_err(|e| format!("Scénario indisponible : {e}"))?
}

/// Les personnages enregistrés.
#[tauri::command]
pub fn lire_personnages(lieux: State<'_, Emplacements>) -> Result<Vec<Personnage>, String> {
    personnages::liste(&lieux.data)
}

/// Enregistre un personnage (ses choix du bloc « personnage » et son âge précis).
#[tauri::command]
pub fn enregistrer_personnage(
    lieux: State<'_, Emplacements>,
    nom: String,
    choix: BTreeMap<String, String>,
    age: Option<u32>,
) -> Result<Personnage, String> {
    personnages::enregistrer(&lieux.dossier, &lieux.data, &nom, &choix, age)
}

/// Supprime un personnage.
#[tauri::command]
pub fn supprimer_personnage(lieux: State<'_, Emplacements>, nom: String) -> Result<(), String> {
    personnages::supprimer(&lieux.data, &nom)
}

/// « 💬 M'aider à préciser » : un tour de conversation (questions, fiche « Ce que j'ai compris », prompt).
#[tauri::command]
pub async fn conversation_scenario(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
    demande: DemandeConversation,
) -> Result<Tour, String> {
    let contexte = plateau.contexte().clone();
    let mut orchestrateur = ecriture(&moteurs);
    tauri::async_runtime::spawn_blocking(move || conversation::tour(&contexte, &mut orchestrateur, &demande))
        .await
        .map_err(|e| format!("Scénario indisponible : {e}"))?
}
