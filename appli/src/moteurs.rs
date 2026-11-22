//! L'onglet Moteurs : état des moteurs et de la carte, installation en arrière-plan avec son suivi, annulation d'un
//! téléchargement, retour à la version précédente, libération de la carte. Le travail est dans le cœur
//! (`aria_coeur::moteurs`, `aria_coeur::orchestrateur`) ; ici, le réseau vers GitHub, les fils et les commandes.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aria_coeur::carte::{self, Carte};
use aria_coeur::catalogue::Catalogue;
use aria_coeur::comfy::{self, DOSSIER_TRAVAIL};
use aria_coeur::comfy_serveur::ServeurComfy;
use aria_coeur::emplacements::Emplacements;
use aria_coeur::garde::Garde;
use aria_coeur::llama_serveur::ServeurLlama;
use aria_coeur::moteurs::{self, EtatMoteur, Extracteurs, Fiche, Reseau};
use aria_coeur::orchestrateur::Orchestrateur;
use aria_coeur::{Dossier, identite, journal, local, reglages};
use serde::Serialize;
use tauri::State;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Le port de ComfyUI, sur `127.0.0.1` (comme en v2).
pub const PORT_COMFYUI: u16 = 8195;
/// Le port de llama-server, sur `127.0.0.1` (comme en v2).
pub const PORT_LLAMA: u16 = 8095;

/// Le port d'un moteur, d'après le nom que lui donne le catalogue.
fn port(nom: &str) -> Option<u16> {
    match nom {
        "comfyui" => Some(PORT_COMFYUI),
        "llama" => Some(PORT_LLAMA),
        _ => None,
    }
}

fn port_occupe(nom: &str) -> bool {
    port(nom).is_some_and(|p| local::port_pris(p, Duration::from_millis(100)))
}

/// L'installation en cours ou la dernière, pour le suivi dans l'onglet.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tache {
    pub id: String,
    pub nom: String,
    pub etape: String,
    pub fait: u64,
    pub total: u64,
    pub erreur: Option<String>,
    pub fini: bool,
}

/// Ce que l'onglet Moteurs affiche.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatMoteurs {
    pub carte: Carte,
    pub moteurs: Vec<EtatMoteur>,
    pub tache: Option<Tache>,
}

/// L'état partagé des moteurs.
pub struct Moteurs {
    catalogue: Catalogue,
    dossier_moteurs: PathBuf,
    dossier: Dossier,
    pub orchestrateur: Arc<Mutex<Orchestrateur>>,
    pub garde: Arc<Garde>,
    tache: Arc<Mutex<Option<Tache>>>,
    annuler: Arc<AtomicBool>,
}

impl Moteurs {
    /// Les moteurs de ce lancement, d'après les emplacements décidés.
    ///
    /// # Errors
    /// Le catalogue embarqué ne se lit pas (les tests du cœur le vérifient).
    pub fn new(emplacements: &Emplacements) -> Result<Self, String> {
        let catalogue = Catalogue::embarque()?;
        let dossier = emplacements.dossier.clone();
        let comfyui = comfyui_externe(&dossier).unwrap_or_else(|| {
            let sous_dossier = catalogue
                .moteur("comfyui")
                .map_or("comfyui", |m| m.dossier.as_str());
            emplacements.moteurs.join(sous_dossier)
        });
        let comfy = ServeurComfy::new(
            comfy::Lancement {
                dossier: comfyui,
                port: PORT_COMFYUI,
                chemins_modeles: dossier.cache().join("comfyui_chemins.yaml"),
                travail: dossier.cache().join(DOSSIER_TRAVAIL),
            },
            emplacements.data.clone(),
        );
        let llama = ServeurLlama::new(PORT_LLAMA, emplacements.data.clone());
        let garde = Arc::new(Garde::default());
        Ok(Self {
            catalogue,
            dossier_moteurs: emplacements.moteurs.clone(),
            dossier,
            orchestrateur: Arc::new(Mutex::new(Orchestrateur::new(comfy, llama, Arc::clone(&garde)))),
            garde,
            tache: Arc::new(Mutex::new(None)),
            annuler: Arc::new(AtomicBool::new(false)),
        })
    }
}

/// Le ComfyUI de l'utilisateur, s'il en a choisi un (`emplacements.comfyui` de `reglages.json`, comme en v2).
fn comfyui_externe(dossier: &Dossier) -> Option<PathBuf> {
    let section = reglages::section(dossier, "emplacements")?;
    let chemin = section.get("comfyui")?.as_str()?.trim();
    (!chemin.is_empty()).then(|| PathBuf::from(chemin))
}

/// Les outils d'extraction des 7z de ce PC, et leur lancement.
#[cfg(windows)]
fn outils() -> (Extracteurs, moteurs::systeme::Systeme) {
    (moteurs::systeme::extracteurs(), moteurs::systeme::Systeme)
}

/// Hors de Windows, il n'y a ni tar de Windows ni 7-Zip à chercher.
#[cfg(not(windows))]
fn outils() -> (Extracteurs, SansOutil) {
    (Extracteurs::default(), SansOutil)
}

#[cfg(not(windows))]
struct SansOutil;

#[cfg(not(windows))]
impl moteurs::Lanceur for SansOutil {
    fn lancer(&self, _commande: &[std::ffi::OsString]) -> io::Result<moteurs::Sortie> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "seulement sous Windows",
        ))
    }
}

/// GitHub en HTTPS, avec les certificats et le proxy de Windows (comme les mises à jour) ; les redirections vers
/// le serveur de fichiers de GitHub sont suivies, l'empreinte SHA-256 fait foi.
struct Github(Agent);

impl Github {
    fn new() -> Self {
        Self(
            Agent::config_builder()
                .https_only(true)
                .timeout_connect(Some(Duration::from_secs(30)))
                .timeout_per_read(Some(Duration::from_secs(120)))
                .http_status_as_error(false)
                .user_agent(format!("{}/{}", identite::NOM_EXE, identite::VERSION))
                .tls_config(
                    TlsConfig::builder()
                        .provider(TlsProvider::NativeTls)
                        .root_certs(RootCerts::PlatformVerifier)
                        .build(),
                )
                .build()
                .into(),
        )
    }
}

impl Reseau for Github {
    fn ouvrir(&self, adresse: &str, depuis: u64) -> io::Result<(u16, Box<dyn Read>)> {
        let mut requete = self.0.get(adresse);
        if depuis > 0 {
            requete = requete.header("Range", format!("bytes={depuis}-"));
        }
        let reponse = requete.call().map_err(io::Error::other)?;
        let statut = reponse.status().as_u16();
        Ok((statut, Box::new(reponse.into_body().into_reader())))
    }
}

/// L'état des moteurs et de la carte.
#[tauri::command]
pub async fn lire_moteurs(moteurs: State<'_, Moteurs>) -> Result<EtatMoteurs, String> {
    let catalogue = moteurs.catalogue.clone();
    let dossier_moteurs = moteurs.dossier_moteurs.clone();
    let dossier = moteurs.dossier.clone();
    let tache = moteurs.tache.lock().ok().and_then(|t| t.clone());
    // NVML, tar --version et les ports : à faire hors du fil de l'interface.
    tauri::async_runtime::spawn_blocking(move || {
        let externe = comfyui_externe(&dossier);
        let (extracteurs, _) = outils();
        EtatMoteurs {
            carte: carte::lire(catalogue.pilote_min),
            moteurs: moteurs::etat(
                &catalogue,
                &dossier_moteurs,
                externe.as_deref(),
                &extracteurs,
                port_occupe,
            ),
            tache,
        }
    })
    .await
    .map_err(|e| format!("état des moteurs illisible : {e}"))
}

/// Lance l'installation d'un moteur en arrière-plan ; son avancement se lit avec [`lire_moteurs`].
#[tauri::command]
pub fn installer_moteur(moteurs: State<'_, Moteurs>, id: String) -> Result<(), String> {
    let nom = moteurs
        .catalogue
        .moteur(&id)
        .map(|m| m.nom.clone())
        .ok_or_else(|| format!("Moteur inconnu : {id}"))?;
    {
        let mut tache = moteurs
            .tache
            .lock()
            .map_err(|_| "suivi des installations indisponible".to_owned())?;
        if let Some(en_cours) = tache.as_ref().filter(|t| !t.fini) {
            return Err(format!("Une installation est déjà en cours ({})", en_cours.nom));
        }
        *tache = Some(Tache {
            id: id.clone(),
            nom,
            etape: "Préparation".to_owned(),
            ..Tache::default()
        });
    }
    moteurs.annuler.store(false, Ordering::Relaxed);
    let catalogue = moteurs.catalogue.clone();
    let dossier_moteurs = moteurs.dossier_moteurs.clone();
    let dossier = moteurs.dossier.clone();
    let orchestrateur = Arc::clone(&moteurs.orchestrateur);
    let tache = Arc::clone(&moteurs.tache);
    let annuler = Arc::clone(&moteurs.annuler);
    std::thread::spawn(move || {
        let resultat = installer(
            &catalogue,
            &id,
            &dossier_moteurs,
            &orchestrateur,
            &tache,
            &annuler,
        );
        match &resultat {
            Ok(fiche) => journal::noter(
                &dossier,
                "Moteur installé",
                &format!("{id} {}", fiche.version),
                journal::Resultat::Ok,
                None,
            ),
            Err(erreur) => journal::noter(
                &dossier,
                "Installation d'un moteur en échec",
                &id,
                journal::Resultat::Erreur,
                Some(erreur),
            ),
        }
        if let Ok(mut suivi) = tache.lock()
            && let Some(suivi) = suivi.as_mut()
        {
            suivi.fini = true;
            match resultat {
                Ok(_) => suivi.etape = "Installé ✓".to_owned(),
                Err(erreur) => suivi.erreur = Some(erreur),
            }
        }
    });
    Ok(())
}

fn installer(
    catalogue: &Catalogue,
    id: &str,
    dossier_moteurs: &Path,
    orchestrateur: &Mutex<Orchestrateur>,
    tache: &Mutex<Option<Tache>>,
    annuler: &AtomicBool,
) -> Result<Fiche, String> {
    let moteur = catalogue
        .moteur(id)
        .ok_or_else(|| format!("Moteur inconnu : {id}"))?;
    std::fs::create_dir_all(dossier_moteurs).map_err(|e| format!("{} : {e}", dossier_moteurs.display()))?;
    let libre = crate::disque::espace(dossier_moteurs).ok().map(|e| e.libre);
    let mut progression = |etape: &str, fait: u64, total: u64| {
        if let Ok(mut suivi) = tache.lock()
            && let Some(suivi) = suivi.as_mut()
        {
            etape.clone_into(&mut suivi.etape);
            suivi.fait = fait;
            suivi.total = total;
        }
        !annuler.load(Ordering::Relaxed)
    };
    let (extracteurs, lanceur) = outils();
    // Le moteur est arrêté par l'orchestrateur au moment de l'échange, pas avant : on télécharge pendant qu'il
    // sert encore.
    let prepare = moteurs::preparer(
        catalogue,
        id,
        dossier_moteurs,
        libre,
        false,
        &Github::new(),
        &extracteurs,
        &lanceur,
        &mut progression,
    )?;
    let _ = progression("Mise en place (attend la fin d'une génération en cours)", 0, 0);
    let mut orchestrateur = orchestrateur
        .lock()
        .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?;
    orchestrateur.sans_moteur(id);
    moteurs::mettre_en_place(prepare, port_occupe(&moteur.port))
}

/// Annule l'installation en cours, seulement pendant un téléchargement (le `.part` est gardé pour reprendre).
#[tauri::command]
pub fn annuler_installation_moteur(moteurs: State<'_, Moteurs>) {
    let telechargement = moteurs
        .tache
        .lock()
        .ok()
        .and_then(|t| {
            t.as_ref()
                .map(|t| !t.fini && t.etape.starts_with("Téléchargement"))
        })
        .unwrap_or(false);
    if telechargement {
        moteurs.annuler.store(true, Ordering::Relaxed);
    }
}

/// Revient à la version précédente d'un moteur ; rend la version qui revient (rien si elle avait été posée à la
/// main).
#[tauri::command]
pub async fn revenir_moteur(moteurs: State<'_, Moteurs>, id: String) -> Result<Option<String>, String> {
    let catalogue = moteurs.catalogue.clone();
    let dossier_moteurs = moteurs.dossier_moteurs.clone();
    let dossier = moteurs.dossier.clone();
    let orchestrateur = Arc::clone(&moteurs.orchestrateur);
    let tache = Arc::clone(&moteurs.tache);
    tauri::async_runtime::spawn_blocking(move || {
        let port = catalogue.moteur(&id).map(|m| m.port.clone()).unwrap_or_default();
        let fiche = {
            let mut orchestrateur = orchestrateur
                .lock()
                .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?;
            orchestrateur.sans_moteur(&id);
            moteurs::revenir(&catalogue, &id, &dossier_moteurs, port_occupe(&port))?
        };
        let version = fiche.map(|f| f.version);
        journal::noter(
            &dossier,
            "Moteur : retour à la version précédente",
            &format!("{id} {}", version.as_deref().unwrap_or("posée à la main")),
            journal::Resultat::Ok,
            None,
        );
        // Le message « Installé ✓ » de ce moteur ne vaut plus.
        if let Ok(mut suivi) = tache.lock()
            && suivi.as_ref().is_some_and(|t| t.fini && t.id == id)
        {
            *suivi = None;
        }
        Ok(version)
    })
    .await
    .map_err(|e| format!("retour impossible : {e}"))?
}

/// Rend la carte graphique : ComfyUI vide sa VRAM, llama-server s'arrête.
#[tauri::command]
pub async fn liberer_carte(moteurs: State<'_, Moteurs>) -> Result<(), String> {
    let orchestrateur = Arc::clone(&moteurs.orchestrateur);
    let dossier = moteurs.dossier.clone();
    tauri::async_runtime::spawn_blocking(move || {
        orchestrateur
            .lock()
            .map_err(|_| "orchestrateur des moteurs indisponible".to_owned())?
            .liberer()?;
        journal::noter(
            &dossier,
            "Carte graphique libérée",
            "",
            journal::Resultat::Ok,
            None,
        );
        Ok(())
    })
    .await
    .map_err(|e| format!("libération impossible : {e}"))?
}

/// Au lancement d'Aria, hors du fil de l'interface : reprend un ComfyUI resté seul.
pub fn reprendre(moteurs: &Moteurs) {
    let orchestrateur = Arc::clone(&moteurs.orchestrateur);
    std::thread::spawn(move || {
        if let Ok(mut orchestrateur) = orchestrateur.lock() {
            orchestrateur.reprendre();
        }
    });
}

/// À la fermeture d'Aria : les moteurs qu'elle a lancés s'arrêtent.
pub fn arreter(moteurs: &Moteurs) {
    if let Ok(mut orchestrateur) = moteurs.orchestrateur.lock() {
        orchestrateur.arreter();
    }
}
