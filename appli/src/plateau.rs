//! La page Plateau : créer une image depuis une idée réécrite par le LLM ou depuis un prompt tel quel, suivre la
//! file, voir les dernières créations. Le travail est dans le cœur (`aria_coeur::plateau`, `aria_coeur::creations`) ;
//! ici, les commandes, hors du fil de l'interface quand elles lisent le disque.

use std::sync::Arc;

use aria_coeur::emplacements::Emplacements;
use aria_coeur::images;
use aria_coeur::montage::{self, DemandeClip, OptionsMontage};
use aria_coeur::plateau::{Commande, Contexte, Demande, Options, Plateau};
use aria_coeur::retouche::{self, DemandeAgrandissement, DemandeRetouche};
use aria_coeur::rushes::{self, EtatRushes};
use aria_coeur::voix;
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

/// Les workflows de retouche, les formats et les LLM.
#[tauri::command]
pub async fn options_retouche(plateau: State<'_, Arc<Plateau>>) -> Result<Options, String> {
    hors_interface(&plateau, |p| Ok(retouche::options(p.contexte()))).await
}

/// Contrôle une demande de retouche et la met en file.
#[tauri::command]
pub async fn commander_retouche(
    plateau: State<'_, Arc<Plateau>>,
    demande: DemandeRetouche,
) -> Result<Commande, String> {
    hors_interface(&plateau, move |p| p.commander_retouche(&demande)).await
}

/// Contrôle une demande d'agrandissement et la met en file.
#[tauri::command]
pub async fn commander_agrandissement(
    plateau: State<'_, Arc<Plateau>>,
    demande: DemandeAgrandissement,
) -> Result<Commande, String> {
    hors_interface(&plateau, move |p| p.commander_agrandissement(&demande)).await
}

/// Ajoute une image personnelle (data URL) aux créations ; rend sa fiche.
#[tauri::command]
pub async fn importer_image(
    plateau: State<'_, Arc<Plateau>>,
    image: String,
    nom: String,
) -> Result<Value, String> {
    hors_interface(&plateau, move |p| {
        let octets = images::depuis_data_url(&image)?;
        let id = p.importer(&octets, &nom)?;
        creations::fiche(&p.contexte().data, &id)
    })
    .await
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

/// Ce que propose le Montage : résolutions, durées, workflows prêts pour animer et pour partir d'un texte.
#[tauri::command]
pub async fn options_montage(plateau: State<'_, Arc<Plateau>>) -> Result<OptionsMontage, String> {
    hors_interface(&plateau, |p| Ok(montage::options(p.contexte()))).await
}

/// Contrôle une demande de clip et la met en file.
#[tauri::command]
pub async fn commander_clip(
    plateau: State<'_, Arc<Plateau>>,
    demande: DemandeClip,
) -> Result<Commande, String> {
    hors_interface(&plateau, move |p| p.commander_clip(&demande)).await
}

/// La vidéo d'un clip, en octets.
#[tauri::command]
pub async fn video_creation(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<Response, String> {
    hors_interface(&plateau, move |p| {
        let chemin = creations::chemin_video(&p.contexte().data, &id)?;
        std::fs::read(&chemin)
            .map(Response::new)
            .map_err(|e| format!("vidéo de {id} illisible : {e}"))
    })
    .await
}

/// Les voix de la bibliothèque (et les brouillons si demandé), et ce que l'interface doit savoir d'elles.
#[tauri::command]
pub async fn lire_voix(
    plateau: State<'_, Arc<Plateau>>,
    brouillons: bool,
) -> Result<(Vec<voix::Voix>, voix::OptionsVoix), String> {
    hors_interface(&plateau, move |p| {
        let c = p.contexte();
        Ok((voix::liste(&c.data, brouillons), voix::options(c)))
    })
    .await
}

/// Lance un essai de voix (un gros plan en cabine, dont seul le son est gardé).
#[tauri::command]
pub async fn essayer_voix(
    plateau: State<'_, Arc<Plateau>>,
    demande: voix::DemandeEssai,
) -> Result<Commande, String> {
    hors_interface(&plateau, move |p| p.commander_voix(&demande)).await
}

/// Garde un essai sous un nom, ou renomme une voix.
#[tauri::command]
pub async fn garder_voix(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    nom: String,
) -> Result<voix::Voix, String> {
    hors_interface(&plateau, move |p| voix::garder(&p.contexte().data, &id, &nom)).await
}

/// Met une voix à la corbeille.
#[tauri::command]
pub async fn supprimer_voix(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<(), String> {
    hors_interface(&plateau, move |p| {
        voix::supprimer(&p.contexte().data, &id).map(|_| ())
    })
    .await
}

/// Une voix enregistrée par l'utilisateur (data URL), gardée aussitôt.
#[tauri::command]
pub async fn importer_voix(
    plateau: State<'_, Arc<Plateau>>,
    son: String,
    nom: String,
    nom_origine: String,
) -> Result<voix::Voix, String> {
    hors_interface(&plateau, move |p| {
        let octets = images::depuis_data_url(&son).map_err(|_| "Fichier audio illisible".to_owned())?;
        voix::importer(&p.contexte().data, &octets, &nom, &nom_origine)
    })
    .await
}

/// Le son d'une voix, en octets.
#[tauri::command]
pub async fn son_voix(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<Response, String> {
    hors_interface(&plateau, move |p| {
        let (chemin, _) = voix::son(&p.contexte().data, &id)?;
        std::fs::read(&chemin)
            .map(Response::new)
            .map_err(|e| format!("son de la voix {id} illisible : {e}"))
    })
    .await
}
