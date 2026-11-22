//! Modèles → Rechercher : Civitai et Hugging Face (`aria_coeur::decouvrir`), en HTTPS avec les clés de Réglages. Un
//! fichier trouvé ou collé par lien passe par la même file que les workflows (`telechargements`). Les vignettes de
//! Civitai sont lues ici : l'interface ne charge rien d'ailleurs.

use std::io::Read;
use std::sync::Arc;

use aria_coeur::carte;
use aria_coeur::decouvrir::{self, Demande, Famille, FichiersDepot, Proposition, Recherche};
use aria_coeur::retrouver::{self, Identification};
use aria_coeur::telechargement::{Reseau, Tache, Telechargeur};
use tauri::State;
use tauri::ipc::Response;

use crate::telechargements::Https;

/// Le client HTTPS de Rechercher (la file a le sien).
pub struct Sources(pub Https);

/// Ce que l'écran prend au LLM, au plus : au-delà, c'est sans doute un ComfyUI chargé (comme en v2).
const VRAM_OCCUPEE_MAX: u64 = 5 << 29;
/// Une vignette, au plus.
const VIGNETTE_MAX: u64 = 4 << 20;
const GO: u64 = 1 << 30;

async fn hors_interface<T: Send + 'static>(
    file: &State<'_, Arc<Telechargeur>>,
    sources: &State<'_, Arc<Sources>>,
    travail: impl FnOnce(&Telechargeur, &Https) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let file = Arc::clone(file);
    let sources = Arc::clone(sources);
    tauri::async_runtime::spawn_blocking(move || travail(&file, &sources.0))
        .await
        .map_err(|e| format!("Recherche indisponible : {e}"))?
}

/// Les familles proposées pour un type.
#[tauri::command]
pub fn familles_decouvrir(genre: String) -> Result<Vec<Famille>, String> {
    decouvrir::familles(&genre)
}

/// Une page de résultats.
#[tauri::command]
pub async fn chercher_modeles(
    file: State<'_, Arc<Telechargeur>>,
    sources: State<'_, Arc<Sources>>,
    demande: Demande,
) -> Result<Recherche, String> {
    hors_interface(&file, &sources, move |f, https| {
        let c = f.contexte();
        let installes = decouvrir::installes(c);
        decouvrir::chercher(https, &c.entetes, &installes, &demande)
    })
    .await
}

/// La VRAM libre pour un LLM et la VRAM totale, si la carte se lit.
fn vram() -> Option<(u64, u64)> {
    let totale = carte::lire(0).vram_go? * GO;
    let prise = carte::vram_utilisee().unwrap_or(0).min(VRAM_OCCUPEE_MAX);
    Some((totale.saturating_sub(prise), totale))
}

/// Les fichiers d'un dépôt Hugging Face.
#[tauri::command]
pub async fn fichiers_depot(
    file: State<'_, Arc<Telechargeur>>,
    sources: State<'_, Arc<Sources>>,
    depot: String,
    genre: String,
    famille: Option<String>,
) -> Result<FichiersDepot, String> {
    hors_interface(&file, &sources, move |f, https| {
        let c = f.contexte();
        let installes = decouvrir::installes(c);
        let vram = if genre == "llm" { vram() } else { None };
        decouvrir::fichiers_hf(
            https,
            &c.entetes,
            &installes,
            &depot,
            &genre,
            famille.as_deref(),
            vram,
        )
    })
    .await
}

/// Ce qu'un lien collé donne, avant de le mettre en file.
#[tauri::command]
pub async fn preparer_lien(
    file: State<'_, Arc<Telechargeur>>,
    sources: State<'_, Arc<Sources>>,
    lien: String,
) -> Result<Proposition, String> {
    hors_interface(&file, &sources, move |f, https| {
        decouvrir::fiche_depuis_lien(https, &f.contexte().entetes, &lien)
    })
    .await
}

/// Met en file le fichier d'un lien, dans ce dossier (le clic vaut confirmation, comme en v2) : la fiche est relue à
/// la source, jamais prise de l'interface.
#[tauri::command]
pub async fn telecharger_lien(
    file: State<'_, Arc<Telechargeur>>,
    sources: State<'_, Arc<Sources>>,
    lien: String,
    dossier: String,
) -> Result<Tache, String> {
    hors_interface(&file, &sources, move |f, https| {
        let proposition = decouvrir::fiche_depuis_lien(https, &f.contexte().entetes, &lien)?;
        f.ajouter(proposition.fiche, &dossier)
    })
    .await
}

/// Une vignette de Civitai (son CDN seulement), en octets.
#[tauri::command]
pub async fn vignette_modele(
    file: State<'_, Arc<Telechargeur>>,
    sources: State<'_, Arc<Sources>>,
    url: String,
) -> Result<Response, String> {
    if !url.starts_with("https://image.civitai.com/") {
        return Err("Vignette refusée : seules celles de Civitai sont lues".to_owned());
    }
    hors_interface(&file, &sources, move |_, https| {
        let (statut, corps) = https
            .ouvrir(&url, &[], 0)
            .map_err(|e| format!("Vignette indisponible : {e}"))?;
        if statut != 200 {
            return Err(format!("Vignette indisponible ({statut})"));
        }
        let mut octets = Vec::new();
        corps
            .take(VIGNETTE_MAX)
            .read_to_end(&mut octets)
            .map_err(|e| format!("Vignette illisible : {e}"))?;
        Ok(Response::new(octets))
    })
    .await
}

/// Lance « Retrouver les sources » des fichiers déposés à la main, dans un fil à part ; l'interface suit
/// `etat_sources`.
#[tauri::command]
pub fn retrouver_sources(
    file: State<'_, Arc<Telechargeur>>,
    sources: State<'_, Arc<Sources>>,
    suivi: State<'_, Arc<retrouver::Suivi>>,
) -> Result<Identification, String> {
    let c = file.contexte();
    let orphelins = retrouver::orphelins(&c.data, &c.modeles);
    if !suivi.commencer(orphelins.len()) {
        return Err("La recherche des sources est déjà en cours".to_owned());
    }
    let (file, sources, suivi_fil) = (Arc::clone(&file), Arc::clone(&sources), Arc::clone(&suivi));
    std::thread::Builder::new()
        .name("retrouver-sources".to_owned())
        .spawn(move || retrouver::identifier(file.contexte(), &sources.0, &orphelins, &suivi_fil))
        .map_err(|e| format!("Recherche des sources impossible : {e}"))?;
    Ok(suivi.lire())
}

/// Où en est « Retrouver les sources ».
#[tauri::command]
pub fn etat_sources(suivi: State<'_, Arc<retrouver::Suivi>>) -> Identification {
    suivi.lire()
}
