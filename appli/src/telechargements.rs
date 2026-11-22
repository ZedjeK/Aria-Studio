//! La page Modèles, première partie : les workflows livrés et les LLM du catalogue, ce qui leur manque, et la file
//! de téléchargement (Hugging Face et Civitai, avec les clés de Réglages). Le travail est dans le cœur
//! (`aria_coeur::telechargement`, `aria_coeur::installations`) ; ici, le HTTPS, les clés et les commandes.

use std::io::{self, Read};
use std::sync::Arc;
use std::time::Duration;

use aria_coeur::cles::{self, Source};
use aria_coeur::emplacements::Emplacements;
use aria_coeur::installations::{self, EtatLlm, EtatWorkflow};
use aria_coeur::telechargement::{Contexte, Reseau, Tache, Telechargeur};
use aria_coeur::{Dossier, identite};
use serde::Serialize;
use tauri::State;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::disque;

/// Hugging Face et Civitai en HTTPS, avec les certificats et le proxy de Windows. Les redirections vers leurs serveurs
/// de fichiers sont suivies (sans la clé, que ureq ne renvoie pas à un autre hôte) ; l'empreinte SHA-256 fait foi.
struct Https(Agent);

impl Https {
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

impl Reseau for Https {
    fn ouvrir(
        &self,
        adresse: &str,
        entetes: &[(String, String)],
        depuis: u64,
    ) -> io::Result<(u16, Box<dyn Read>)> {
        let mut requete = self.0.get(adresse);
        for (nom, valeur) in entetes {
            requete = requete.header(nom.as_str(), valeur.as_str());
        }
        if depuis > 0 {
            requete = requete.header("Range", format!("bytes={depuis}-"));
        }
        let reponse = requete.call().map_err(io::Error::other)?;
        let statut = reponse.status().as_u16();
        Ok((statut, Box::new(reponse.into_body().into_reader())))
    }
}

/// La clé d'une source, lue à chaque téléchargement (une clé collée entre-temps sert aussitôt).
#[cfg(windows)]
fn cle_de(dossier: &Dossier, source: Source) -> Option<String> {
    let protection = aria_coeur::secrets::Dpapi;
    let coffre = aria_coeur::secrets::Coffre::du_dossier(dossier, &protection);
    cles::cle(&coffre, source)
}

/// Hors de Windows, pas de coffre : la variable d'environnement seule.
#[cfg(not(windows))]
fn cle_de(_dossier: &Dossier, source: Source) -> Option<String> {
    std::env::var(source.variable())
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// La file de ce lancement.
pub fn demarrer(emplacements: &Emplacements) -> Arc<Telechargeur> {
    let dossier = emplacements.dossier.clone();
    let pour_les_cles = dossier.clone();
    Arc::new(Telechargeur::demarrer(
        Contexte {
            dossier,
            data: emplacements.data.clone(),
            modeles: emplacements.modeles.clone(),
            entetes: Box::new(move |source| {
                Source::depuis(source)
                    .and_then(|s| cle_de(&pour_les_cles, s))
                    .map(|cle| vec![("Authorization".to_owned(), format!("Bearer {cle}"))])
                    .unwrap_or_default()
            }),
            espace_libre: Box::new(|chemin| disque::espace(chemin).ok().map(|e| e.libre)),
        },
        Https::new(),
    ))
}

/// Ce que montre la page Modèles.
#[derive(Debug, Clone, Serialize)]
pub struct EtatModeles {
    pub workflows: Vec<EtatWorkflow>,
    pub llm: Vec<EtatLlm>,
    pub taches: Vec<Tache>,
}

async fn hors_interface<T: Send + 'static>(
    file: &State<'_, Arc<Telechargeur>>,
    travail: impl FnOnce(&Telechargeur) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let file = Arc::clone(file);
    tauri::async_runtime::spawn_blocking(move || travail(&file))
        .await
        .map_err(|e| format!("Modèles indisponibles : {e}"))?
}

/// Les workflows, les LLM du catalogue et la file.
#[tauri::command]
pub async fn etat_modeles(file: State<'_, Arc<Telechargeur>>) -> Result<EtatModeles, String> {
    hors_interface(&file, |f| {
        let modeles = &f.contexte().modeles;
        Ok(EtatModeles {
            workflows: installations::workflows(modeles),
            llm: installations::llm(modeles),
            taches: f.taches(),
        })
    })
    .await
}

/// La file de téléchargement.
#[tauri::command]
pub fn lire_telechargements(file: State<'_, Arc<Telechargeur>>) -> Vec<Tache> {
    file.taches()
}

/// Met en file ce qui manque, comme la v2 : un fichier déjà dans la file n'est pas une erreur.
fn mettre_en_file(
    file: &Telechargeur,
    a_faire: Vec<(aria_coeur::telechargement::Fiche, String)>,
) -> Result<usize, String> {
    let mut ajoutes = 0;
    for (fiche, dossier) in a_faire {
        let nom = fiche.nom.clone();
        match file.ajouter(fiche, &dossier) {
            Ok(_) => ajoutes += 1,
            Err(e) if e == "Ce fichier est déjà dans la file" => {}
            Err(e) => return Err(format!("{nom} : {e}")),
        }
    }
    Ok(ajoutes)
}

/// Met en file les fichiers manquants d'un workflow (le clic, après lecture de sa licence, vaut accord).
#[tauri::command]
pub async fn telecharger_workflow(file: State<'_, Arc<Telechargeur>>, id: String) -> Result<usize, String> {
    hors_interface(&file, move |f| {
        let a_faire = installations::manquants(&f.contexte().modeles, &id)?;
        mettre_en_file(f, a_faire)
    })
    .await
}

/// Met en file un LLM du catalogue, avec la consigne et la vision d'un spécialiste.
#[tauri::command]
pub async fn installer_llm(file: State<'_, Arc<Telechargeur>>, fichier: String) -> Result<usize, String> {
    hors_interface(&file, move |f| {
        let a_faire = installations::llm_manquant(&f.contexte().modeles, &fichier)?;
        mettre_en_file(f, a_faire)
    })
    .await
}

/// Annule un téléchargement.
#[tauri::command]
pub fn annuler_telechargement(file: State<'_, Arc<Telechargeur>>, id: String) -> Result<Tache, String> {
    file.annuler(&id)
}
