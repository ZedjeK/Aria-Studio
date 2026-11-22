//! La page Plateau : créer une image depuis une idée réécrite par le LLM ou depuis un prompt tel quel, suivre la
//! file, voir les dernières créations. Le travail est dans le cœur (`aria_coeur::plateau`, `aria_coeur::creations`) ;
//! ici, les commandes, hors du fil de l'interface quand elles lisent le disque.

use std::sync::Arc;

use aria_coeur::emplacements::Emplacements;
use aria_coeur::plateau::{Commande, Contexte, Demande, Options, Plateau};
use aria_coeur::rushes::{self, EtatRushes};
use aria_coeur::{creations, journal};
use serde_json::Value;
use tauri::State;
use tauri::ipc::Response;

use crate::moteurs::{Moteurs, PORT_LLAMA};

/// Les Rushes de cette page : les images du Plateau (les clips auront leurs Bobines).
const GENRE: &str = "images";

/// Le Plateau de ce lancement : ComfyUI passe par l'orchestrateur des moteurs.
pub fn demarrer(emplacements: &Emplacements, moteurs: &Moteurs) -> Arc<Plateau> {
    Arc::new(Plateau::demarrer(
        Contexte {
            dossier: emplacements.dossier.clone(),
            data: emplacements.data.clone(),
            modeles: emplacements.modeles.clone(),
            moteurs: emplacements.moteurs.clone(),
            port_llama: PORT_LLAMA,
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

/// Les rushes d'une vue (`tous`, `non_ranges`, `archives`, `corbeille`) ou d'un dossier, les plus récents d'abord.
#[tauri::command]
pub async fn lire_rushes(
    plateau: State<'_, Arc<Plateau>>,
    vue: String,
    dossier: Option<String>,
) -> Result<Vec<Value>, String> {
    hors_interface(&plateau, move |p| {
        rushes::liste(&p.contexte().data, &vue, dossier.as_deref(), GENRE)
    })
    .await
}

/// Les compteurs des vues et des dossiers.
#[tauri::command]
pub async fn etat_rushes(plateau: State<'_, Arc<Plateau>>) -> Result<EtatRushes, String> {
    hors_interface(&plateau, |p| rushes::etat(&p.contexte().data, GENRE)).await
}

/// Corbeille, restaurer, archiver, désarchiver ou ranger une sélection ; rend le nombre de rushes modifiés.
#[tauri::command]
pub async fn agir_rushes(
    plateau: State<'_, Arc<Plateau>>,
    ids: Vec<String>,
    action: String,
    dossier: Option<String>,
) -> Result<usize, String> {
    hors_interface(&plateau, move |p| {
        rushes::agir(&p.contexte().data, &ids, &action, dossier.as_deref())
    })
    .await
}

/// La seule suppression définitive : les rushes de la corbeille. L'interface la fait confirmer.
#[tauri::command]
pub async fn vider_corbeille(plateau: State<'_, Arc<Plateau>>) -> Result<usize, String> {
    hors_interface(&plateau, |p| {
        let contexte = p.contexte();
        rushes::vider_corbeille(&contexte.dossier, &contexte.data, GENRE)
    })
    .await
}

/// Crée un dossier de rushes.
#[tauri::command]
pub async fn creer_dossier(plateau: State<'_, Arc<Plateau>>, nom: String) -> Result<String, String> {
    hors_interface(&plateau, move |p| {
        rushes::creer_dossier(&p.contexte().data, &nom, GENRE)
    })
    .await
}

/// Renomme un dossier de rushes, et ses rushes avec lui.
#[tauri::command]
pub async fn renommer_dossier(
    plateau: State<'_, Arc<Plateau>>,
    ancien: String,
    nouveau: String,
) -> Result<String, String> {
    hors_interface(&plateau, move |p| {
        rushes::renommer_dossier(&p.contexte().data, &ancien, &nouveau, GENRE)
    })
    .await
}

/// Supprime un dossier, pas ses images : elles retournent dans « Non rangés ».
#[tauri::command]
pub async fn supprimer_dossier(plateau: State<'_, Arc<Plateau>>, nom: String) -> Result<usize, String> {
    hors_interface(&plateau, move |p| {
        let liberes = rushes::supprimer_dossier(&p.contexte().data, &nom, GENRE)?;
        journal::noter(
            &p.contexte().dossier,
            "Dossier de rushes supprimé",
            &nom,
            journal::Resultat::Ok,
            Some(&format!("{liberes} image(s) retournée(s) dans Non rangés")),
        );
        Ok(liberes)
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
