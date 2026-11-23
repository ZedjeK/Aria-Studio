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
use aria_coeur::{casting, sequences, voix};
use aria_coeur::{creations, journal};
use serde_json::Value;
use tauri::State;
use tauri::ipc::Response;

use crate::moteurs::{Moteurs, PORT_LLAMA};

/// Le genre par défaut : les Rushes du Plateau (images) ; les Bobines du Montage passent `clips`.
const GENRE: &str = "images";

fn genre(choisi: Option<String>) -> String {
    choisi.unwrap_or_else(|| GENRE.to_owned())
}

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
    dossier: Option<String>,
) -> Result<Value, String> {
    hors_interface(&plateau, move |p| {
        let octets = images::depuis_data_url(&image)?;
        let id = p.importer(&octets, &nom, dossier.as_deref())?;
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
    genre: Option<String>,
) -> Result<Vec<Value>, String> {
    let genre = self::genre(genre);
    hors_interface(&plateau, move |p| {
        rushes::liste(&p.contexte().data, &vue, dossier.as_deref(), &genre)
    })
    .await
}

/// Les compteurs des vues et des dossiers.
#[tauri::command]
pub async fn etat_rushes(
    plateau: State<'_, Arc<Plateau>>,
    genre: Option<String>,
) -> Result<EtatRushes, String> {
    let genre = self::genre(genre);
    hors_interface(&plateau, move |p| rushes::etat(&p.contexte().data, &genre)).await
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
pub async fn vider_corbeille(
    plateau: State<'_, Arc<Plateau>>,
    genre: Option<String>,
) -> Result<usize, String> {
    let genre = self::genre(genre);
    hors_interface(&plateau, move |p| {
        let contexte = p.contexte();
        rushes::vider_corbeille(&contexte.dossier, &contexte.data, &genre)
    })
    .await
}

/// Crée un dossier de rushes.
#[tauri::command]
pub async fn creer_dossier(
    plateau: State<'_, Arc<Plateau>>,
    nom: String,
    genre: Option<String>,
) -> Result<String, String> {
    let genre = self::genre(genre);
    hors_interface(&plateau, move |p| {
        rushes::creer_dossier(&p.contexte().data, &nom, &genre)
    })
    .await
}

/// Renomme un dossier de rushes, et ses rushes avec lui.
#[tauri::command]
pub async fn renommer_dossier(
    plateau: State<'_, Arc<Plateau>>,
    ancien: String,
    nouveau: String,
    genre: Option<String>,
) -> Result<String, String> {
    let genre = self::genre(genre);
    hors_interface(&plateau, move |p| {
        rushes::renommer_dossier(&p.contexte().data, &ancien, &nouveau, &genre)
    })
    .await
}

/// Supprime un dossier, pas ses images : elles retournent dans « Non rangés ».
#[tauri::command]
pub async fn supprimer_dossier(
    plateau: State<'_, Arc<Plateau>>,
    nom: String,
    genre: Option<String>,
) -> Result<usize, String> {
    let genre = self::genre(genre);
    hors_interface(&plateau, move |p| {
        let liberes = rushes::supprimer_dossier(&p.contexte().data, &nom, &genre)?;
        let (action, quoi) = if genre == "clips" {
            ("Dossier des Bobines supprimé", "clip(s) retourné(s)")
        } else {
            ("Dossier de rushes supprimé", "image(s) retournée(s)")
        };
        journal::noter(
            &p.contexte().dossier,
            action,
            &nom,
            journal::Resultat::Ok,
            Some(&format!("{liberes} {quoi} dans Non rangés")),
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

/// Refuse de toucher à une séquence pendant son tournage : le tournage la réécrit plan après plan.
fn hors_tournage(p: &Plateau, id: &str) -> Result<(), String> {
    if p.sequences_en_tournage().iter().any(|s| s == id) {
        Err("Cette séquence est en tournage : attends la fin, ou annule-le dans la file".to_owned())
    } else {
        Ok(())
    }
}

/// Les séquences, celles de la corbeille, celles en tournage, et les options de l'écran.
#[tauri::command]
pub async fn lire_sequences(plateau: State<'_, Arc<Plateau>>) -> Result<sequences::EtatSequences, String> {
    hors_interface(&plateau, move |p| {
        Ok(sequences::etat(p.contexte(), p.sequences_en_tournage()))
    })
    .await
}

/// Crée une séquence (au format de la v2).
#[tauri::command]
pub async fn creer_sequence(plateau: State<'_, Arc<Plateau>>, donnees: Value) -> Result<Value, String> {
    hors_interface(&plateau, move |p| sequences::creer(p.contexte(), &donnees)).await
}

/// Enregistre les changements d'une séquence ; les plans touchés seront retournés.
#[tauri::command]
pub async fn modifier_sequence(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    donnees: Value,
) -> Result<Value, String> {
    hors_interface(&plateau, move |p| {
        hors_tournage(p, &id)?;
        sequences::modifier(p.contexte(), &id, &donnees)
    })
    .await
}

/// Met une séquence à la corbeille (ses clips restent dans les Bobines).
#[tauri::command]
pub async fn supprimer_sequence(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<(), String> {
    hors_interface(&plateau, move |p| {
        hors_tournage(p, &id)?;
        sequences::supprimer(p.contexte(), &id)
    })
    .await
}

/// Ramène une séquence de la corbeille.
#[tauri::command]
pub async fn restaurer_sequence(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<Value, String> {
    hors_interface(&plateau, move |p| sequences::restaurer(p.contexte(), &id)).await
}

/// Vide la corbeille des séquences ; rend le nombre de séquences effacées.
#[tauri::command]
pub async fn vider_corbeille_sequences(plateau: State<'_, Arc<Plateau>>) -> Result<usize, String> {
    hors_interface(&plateau, move |p| sequences::vider_corbeille(p.contexte())).await
}

/// Tourne les plans demandés (et ceux qui en dépendent), puis assemble le film.
#[tauri::command]
pub async fn tourner_sequence(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    plans: Vec<usize>,
) -> Result<Commande, String> {
    hors_interface(&plateau, move |p| p.commander_sequence(&id, &plans)).await
}

/// Le Casting : les castings faits, et ce que ferait un casting de chaque tâche.
#[tauri::command]
pub async fn lire_castings(plateau: State<'_, Arc<Plateau>>) -> Result<casting::EtatCasting, String> {
    hors_interface(&plateau, move |p| casting::etat(p.contexte())).await
}

/// Les résultats d'un casting, à l'aveugle tant qu'il n'est pas révélé.
#[tauri::command]
pub async fn lire_casting(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<Value, String> {
    hors_interface(&plateau, move |p| casting::vue(p.contexte(), &id)).await
}

/// Une image (ou un clip) d'un casting, en octets.
#[tauri::command]
pub async fn image_casting(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    cle: String,
    fichier: String,
) -> Result<Response, String> {
    hors_interface(&plateau, move |p| {
        let chemin = casting::chemin_image(&p.contexte().data, &id, &cle, &fichier)?;
        std::fs::read(&chemin)
            .map(Response::new)
            .map_err(|e| format!("{fichier} illisible : {e}"))
    })
    .await
}

/// Une image de départ des castings de retouche et d'image vers vidéo, en octets.
#[tauri::command]
pub async fn source_casting(plateau: State<'_, Arc<Plateau>>, nom: String) -> Result<Response, String> {
    hors_interface(&plateau, move |p| {
        casting::source(&p.contexte().data, &nom)
            .map(Response::new)
            .ok_or_else(|| format!("Image de départ inconnue : {nom}"))
    })
    .await
}

/// Note une image de 1 à 5 ; rend le casting.
#[tauri::command]
pub async fn noter_casting(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    cle: String,
    fichier: String,
    note: u8,
) -> Result<Value, String> {
    hors_interface(&plateau, move |p| {
        casting::noter(&p.contexte().data, &id, &cle, &fichier, note)?;
        casting::vue(p.contexte(), &id)
    })
    .await
}

/// Révèle les modèles d'un casting (ça ne se défait pas) ; rend le casting.
#[tauri::command]
pub async fn reveler_casting(plateau: State<'_, Arc<Plateau>>, id: String) -> Result<Value, String> {
    hors_interface(&plateau, move |p| {
        casting::reveler(&p.contexte().data, &id)?;
        casting::vue(p.contexte(), &id)
    })
    .await
}

/// Fait d'un modèle du casting le modèle par défaut de sa tâche ; rend le casting.
#[tauri::command]
pub async fn favori_casting(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    workflow: String,
) -> Result<Value, String> {
    hors_interface(&plateau, move |p| {
        casting::choisir_favori(p.contexte(), &id, &workflow)?;
        casting::vue(p.contexte(), &id)
    })
    .await
}

/// Fige une image d'un casting texte vers image comme image de départ des castings de retouche ; rend le casting.
#[tauri::command]
pub async fn source_depuis_casting(
    plateau: State<'_, Arc<Plateau>>,
    id: String,
    cle: String,
    fichier: String,
) -> Result<Value, String> {
    hors_interface(&plateau, move |p| {
        casting::utiliser_comme_source(&p.contexte().data, &id, &cle, &fichier)?;
        casting::vue(p.contexte(), &id)
    })
    .await
}
