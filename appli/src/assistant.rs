//! L'assistant de premier lancement, comme en v2 (`/api/assistant*`) : packs, licences, téléchargement par la file
//! habituelle, favoris et image d'essai. Le travail est dans le cœur (`aria_coeur::assistant`) ; ici, les états de
//! l'appli (Plateau, file des téléchargements, moteurs) et les commandes.

use std::sync::Arc;

use aria_coeur::assistant::{self, EtatAssistant, Fin, Plan};
use aria_coeur::plateau::{Commande, Plateau};
use aria_coeur::telechargement::{self, Telechargeur};
use serde::Serialize;
use tauri::State;

use crate::moteurs::{Moteurs, lecteur_etats};

/// Ce que l'assistant a mis dans la file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Envoi {
    /// Les téléchargements ajoutés.
    pub ajoutes: Vec<String>,
    /// Les fichiers que la file avait déjà.
    pub deja_en_file: Vec<String>,
}

async fn hors_interface<T: Send + 'static>(
    travail: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(travail)
        .await
        .map_err(|e| format!("Assistant indisponible : {e}"))?
}

/// La place libre sur le disque des modèles (ou de son premier dossier parent qui existe).
fn libre(file: &Telechargeur) -> Option<u64> {
    let c = file.contexte();
    (c.espace_libre)(&telechargement::premier_existant(&c.modeles))
}

/// L'assistant : s'ouvrir au lancement, la date de fin, et les packs.
#[tauri::command]
pub async fn etat_assistant(
    plateau: State<'_, Arc<Plateau>>,
    moteurs: State<'_, Moteurs>,
) -> Result<EtatAssistant, String> {
    let plateau = Arc::clone(&plateau);
    let etats = lecteur_etats(&moteurs);
    hors_interface(move || {
        let c = plateau.contexte();
        assistant::etat(&c.dossier, &c.modeles, &etats())
    })
    .await
}

/// Ce que coûte une sélection de packs.
#[tauri::command]
pub async fn plan_assistant(
    plateau: State<'_, Arc<Plateau>>,
    file: State<'_, Arc<Telechargeur>>,
    packs: Vec<String>,
) -> Result<Plan, String> {
    let (plateau, file) = (Arc::clone(&plateau), Arc::clone(&file));
    hors_interface(move || {
        let c = plateau.contexte();
        assistant::plan(&c.dossier, &c.modeles, &packs, libre(&file))
    })
    .await
}

/// « J'ai lu et j'accepte » pour chaque pack coché (daté dans `reglages.json`).
#[tauri::command]
pub async fn accepter_licences_assistant(
    plateau: State<'_, Arc<Plateau>>,
    file: State<'_, Arc<Telechargeur>>,
    packs: Vec<String>,
) -> Result<Plan, String> {
    let (plateau, file) = (Arc::clone(&plateau), Arc::clone(&file));
    hors_interface(move || {
        let c = plateau.contexte();
        assistant::accepter(&c.dossier, &packs)?;
        assistant::plan(&c.dossier, &c.modeles, &packs, libre(&file))
    })
    .await
}

/// Met les fichiers manquants des packs dans la file ; refusé tant qu'une licence n'est pas acceptée.
#[tauri::command]
pub async fn telecharger_assistant(
    plateau: State<'_, Arc<Plateau>>,
    file: State<'_, Arc<Telechargeur>>,
    packs: Vec<String>,
) -> Result<Envoi, String> {
    let (plateau, file) = (Arc::clone(&plateau), Arc::clone(&file));
    hors_interface(move || {
        let c = plateau.contexte();
        let a_faire = assistant::a_telecharger(&c.dossier, &c.modeles, &packs, libre(&file))?;
        let mut envoi = Envoi {
            ajoutes: Vec::new(),
            deja_en_file: Vec::new(),
        };
        for (fiche, dossier) in a_faire {
            let nom = fiche.nom.clone();
            match file.ajouter(fiche, &dossier) {
                Ok(tache) => envoi.ajoutes.push(tache.id),
                Err(e) if e == "Ce fichier est déjà dans la file" => envoi.deja_en_file.push(nom),
                Err(e) => return Err(format!("{nom} : {e}")),
            }
        }
        Ok(envoi)
    })
    .await
}

/// Règle les favoris sur ce qui est installé et note la fin de l'assistant.
#[tauri::command]
pub async fn terminer_assistant(plateau: State<'_, Arc<Plateau>>, packs: Vec<String>) -> Result<Fin, String> {
    let plateau = Arc::clone(&plateau);
    hors_interface(move || {
        let c = plateau.contexte();
        assistant::terminer(&c.dossier, &c.modeles, &packs)
    })
    .await
}

/// Une image rapide (peu de steps) pour vérifier que toute la chaîne tourne ; suivie par la file du Plateau.
#[tauri::command]
pub async fn essai_assistant(plateau: State<'_, Arc<Plateau>>) -> Result<Commande, String> {
    let plateau = Arc::clone(&plateau);
    hors_interface(move || {
        let c = plateau.contexte();
        let (demande, imposees) = assistant::demande_essai(&c.dossier, &c.modeles)?;
        plateau.commander_avec(&demande, imposees)
    })
    .await
}
