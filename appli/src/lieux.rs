//! Réglages → Emplacements et clés, comme en v2 : les dossiers d'Aria (ouverts dans l'Explorateur), le dossier des
//! modèles (choisi au sélecteur natif, pris en compte au prochain lancement), et les clés Civitai et Hugging Face
//! (enregistrées chiffrées, testées auprès de leur source ; jamais renvoyées à l'interface).

use std::collections::BTreeMap;
use std::time::Duration;

use aria_coeur::Dossier;
use aria_coeur::cles::{self, EtatCle, Source};
use aria_coeur::emplacements::{Emplacements, EtatEmplacements};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::{FENETRE, disque, mise_a_jour};

/// Les dossiers, avec la place libre sur leur disque (en octets ; rien si le disque ne répond pas).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatLieux {
    #[serde(flatten)]
    pub emplacements: EtatEmplacements,
    pub libres: BTreeMap<String, Option<u64>>,
}

/// Le résultat d'un test de clé.
#[derive(Debug, Clone, Serialize)]
pub struct TestCle {
    pub ok: bool,
    pub message: String,
}

fn etat_lieux(lieux: &Emplacements) -> EtatLieux {
    let emplacements = lieux.etat();
    let libres = emplacements
        .dossiers
        .iter()
        .map(|d| {
            let libre = disque::espace(std::path::Path::new(&d.chemin))
                .ok()
                .map(|e| e.libre);
            (d.cle.clone(), libre)
        })
        .collect();
    EtatLieux { emplacements, libres }
}

async fn hors_interface<T: Send + 'static>(
    travail: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(travail)
        .await
        .map_err(|e| format!("Réglages indisponibles : {e}"))?
}

/// Les dossiers de ce lancement et le choix du dossier des modèles.
#[tauri::command]
pub async fn etat_emplacements(lieux: State<'_, Emplacements>) -> Result<EtatLieux, String> {
    let lieux = lieux.inner().clone();
    hors_interface(move || Ok(etat_lieux(&lieux))).await
}

/// Ouvre un dossier connu dans l'Explorateur de Windows.
#[tauri::command]
pub fn ouvrir_dossier(
    lieux: State<'_, Emplacements>,
    cle: String,
    sous: Option<String>,
) -> Result<(), String> {
    let dossier = lieux.a_ouvrir(&cle, sous.as_deref())?;
    std::process::Command::new("explorer")
        .arg(&dossier)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Explorateur indisponible : {e}"))
}

/// Le sélecteur de dossier natif ; rien n'est enregistré ici. Rien : l'utilisateur a renoncé.
#[tauri::command]
pub async fn parcourir_dossier(
    app: AppHandle,
    titre: String,
    depart: Option<String>,
) -> Result<Option<String>, String> {
    let fenetre = app.get_webview_window(FENETRE);
    hors_interface(move || Ok(choisir_dossier(fenetre, &titre, depart.as_deref()))).await
}

#[cfg(windows)]
fn choisir_dossier(
    fenetre: Option<tauri::WebviewWindow>,
    titre: &str,
    depart: Option<&str>,
) -> Option<String> {
    let mut dialogue = rfd::FileDialog::new().set_title(titre);
    if let Some(d) = depart.filter(|d| std::path::Path::new(d).is_dir()) {
        dialogue = dialogue.set_directory(d);
    }
    if let Some(f) = &fenetre {
        dialogue = dialogue.set_parent(f);
    }
    dialogue.pick_folder().map(|p| p.display().to_string())
}

/// Hors de Windows, pas de sélecteur : le chemin se tape.
#[cfg(not(windows))]
fn choisir_dossier(
    _fenetre: Option<tauri::WebviewWindow>,
    _titre: &str,
    _depart: Option<&str>,
) -> Option<String> {
    None
}

/// Enregistre le dossier des modèles (rien : revenir à celui par défaut), pris en compte au prochain lancement.
#[tauri::command]
pub async fn choisir_dossier_modeles(
    lieux: State<'_, Emplacements>,
    chemin: Option<String>,
) -> Result<EtatLieux, String> {
    let lieux = lieux.inner().clone();
    hors_interface(move || {
        lieux.choisir_modeles(chemin.as_deref())?;
        Ok(etat_lieux(&lieux))
    })
    .await
}

/// La protection des secrets : DPAPI sous Windows ; ailleurs, aucune, et rien ne se range.
#[cfg(windows)]
type Protege = aria_coeur::secrets::Dpapi;
#[cfg(windows)]
const PROTECTION: Protege = aria_coeur::secrets::Dpapi;

#[cfg(not(windows))]
struct SansDpapi;

#[cfg(not(windows))]
impl aria_coeur::secrets::Protection for SansDpapi {
    fn proteger(&self, _: &[u8]) -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::other(
            "les clés se rangent avec DPAPI, sous Windows seulement",
        ))
    }
    fn deproteger(&self, _: &[u8]) -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::other(
            "les clés se rangent avec DPAPI, sous Windows seulement",
        ))
    }
}
#[cfg(not(windows))]
type Protege = SansDpapi;
#[cfg(not(windows))]
const PROTECTION: Protege = SansDpapi;

fn avec_coffre<T>(
    dossier: &Dossier,
    action: impl FnOnce(&aria_coeur::secrets::Coffre<'_, Protege>) -> T,
) -> T {
    let coffre = aria_coeur::secrets::Coffre::du_dossier(dossier, &PROTECTION);
    action(&coffre)
}

fn source(id: &str) -> Result<Source, String> {
    Source::depuis(id).ok_or_else(|| format!("Source inconnue : {id}"))
}

/// D'où vient chaque clé (Aria, variable d'environnement, ou aucune) : jamais la clé elle-même.
#[tauri::command]
pub fn etat_cles(etat: State<'_, crate::Etat>) -> Vec<EtatCle> {
    avec_coffre(&etat.dossier, cles::etat)
}

/// Enregistre une clé, chiffrée par DPAPI dans `secrets\`.
#[tauri::command]
pub fn enregistrer_cle(
    etat: State<'_, crate::Etat>,
    source_id: String,
    valeur: String,
) -> Result<Vec<EtatCle>, String> {
    let s = source(&source_id)?;
    avec_coffre(&etat.dossier, |c| {
        cles::enregistrer(c, s, &valeur)?;
        Ok(cles::etat(c))
    })
}

/// Retire une clé de `secrets\` (l'entrée de la v2 dans le Gestionnaire d'identification reste).
#[tauri::command]
pub fn supprimer_cle(etat: State<'_, crate::Etat>, source_id: String) -> Result<Vec<EtatCle>, String> {
    let s = source(&source_id)?;
    avec_coffre(&etat.dossier, |c| {
        cles::supprimer(c, s)?;
        Ok(cles::etat(c))
    })
}

/// Présente la clé à sa source, qui dit à quel compte elle appartient ou la refuse (comme en v2).
#[tauri::command]
pub async fn tester_cle(etat: State<'_, crate::Etat>, source_id: String) -> Result<TestCle, String> {
    let s = source(&source_id)?;
    let dossier = etat.dossier.clone();
    hors_interface(move || {
        let Some(valeur) = avec_coffre(&dossier, |c| cles::cle(c, s)) else {
            return Ok(TestCle {
                ok: false,
                message: "Aucune clé à tester".to_owned(),
            });
        };
        let (adresse, cle_nom) = match s {
            Source::HuggingFace => ("https://huggingface.co/api/whoami-v2", "name"),
            Source::Civitai => ("https://civitai.com/api/v1/me", "username"),
        };
        let appel = mise_a_jour::client(Duration::from_secs(20))
            .get(adresse)
            .header("Authorization", &format!("Bearer {valeur}"))
            .call();
        let mut reponse = match appel {
            Ok(r) => r,
            Err(ureq::Error::StatusCode(statut)) => {
                return Ok(TestCle {
                    ok: false,
                    message: format!("Clé refusée ({statut}) : vérifie-la, ou recrée-la"),
                });
            }
            Err(e) => {
                return Ok(TestCle {
                    ok: false,
                    message: format!("Source injoignable : {e}"),
                });
            }
        };
        let compte = reponse
            .body_mut()
            .read_to_string()
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.get(cle_nom)?.as_str().map(str::to_owned));
        Ok(TestCle {
            ok: true,
            message: match (s, compte) {
                (_, Some(nom)) => format!("Clé acceptée : compte {nom}"),
                (Source::Civitai, None) => "Clé acceptée par Civitai".to_owned(),
                (Source::HuggingFace, None) => "Clé acceptée : compte ?".to_owned(),
            },
        })
    })
    .await
}
