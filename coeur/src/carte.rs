//! La carte graphique, lue par NVML, comme en v2 (`app/moteurs.py::carte`, `app/mesures.py`) : nom, pilote, VRAM,
//! et la RAM du PC. Les moteurs d'Aria demandent une carte NVIDIA avec un pilote assez récent pour CUDA 13.
//!
//! NVML est la bibliothèque du pilote NVIDIA (`nvml.dll`), chargée à l'exécution : sans carte NVIDIA, rien ne
//! plante, la carte est dite absente.

use std::sync::OnceLock;

use nvml_wrapper::Nvml;
use serde::Serialize;

const GIO: u64 = 1024 * 1024 * 1024;

/// Ce que l'onglet Moteurs dit de la carte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Carte {
    pub nvidia: bool,
    pub nom: Option<String>,
    pub pilote: Option<String>,
    /// VRAM totale, en Gio arrondis.
    pub vram_go: Option<u64>,
    /// RAM du PC, en Gio arrondis.
    pub ram_go: u64,
    /// Carte et pilote conviennent.
    pub ok: bool,
    pub message: Option<String>,
}

/// La carte d'après ce que NVML a lu ; `pilote_min` est la branche minimale du pilote (580 pour CUDA 13).
pub fn evaluer(lu: Option<(String, String, u64)>, ram: u64, pilote_min: u32) -> Carte {
    let ram_go = arrondi_go(ram);
    let Some((nom, pilote, vram)) = lu else {
        return Carte {
            nvidia: false,
            nom: None,
            pilote: None,
            vram_go: None,
            ram_go,
            ok: false,
            message: Some(
                "Aucune carte NVIDIA détectée : les moteurs d'Aria demandent une carte NVIDIA (CUDA 13)."
                    .to_owned(),
            ),
        };
    };
    let branche = pilote
        .split('.')
        .next()
        .and_then(|b| b.trim().parse::<u32>().ok());
    let ok = branche.is_some_and(|b| b >= pilote_min);
    let message = (!ok).then(|| {
        format!("Pilote {pilote} trop ancien : installe un pilote NVIDIA {pilote_min} ou plus récent.")
    });
    Carte {
        nvidia: true,
        nom: Some(nom),
        pilote: Some(pilote),
        vram_go: Some(arrondi_go(vram)),
        ram_go,
        ok,
        message,
    }
}

fn arrondi_go(octets: u64) -> u64 {
    octets.saturating_add(GIO / 2) / GIO
}

/// NVML, initialisée une fois ; rien sans pilote NVIDIA.
fn nvml() -> Option<&'static Nvml> {
    static NVML: OnceLock<Option<Nvml>> = OnceLock::new();
    NVML.get_or_init(|| Nvml::init().ok()).as_ref()
}

/// La carte de ce PC (la première carte NVIDIA).
pub fn lire(pilote_min: u32) -> Carte {
    let lu = nvml().and_then(|nvml| {
        let carte = nvml.device_by_index(0).ok()?;
        let nom = carte.name().ok()?;
        let vram = carte.memory_info().ok()?.total;
        let pilote = nvml.sys_driver_version().ok()?;
        Some((nom, pilote, vram))
    });
    evaluer(lu, ram_totale(), pilote_min)
}

/// La VRAM occupée sur la première carte, tous programmes compris ; rien sans NVML.
pub fn vram_utilisee() -> Option<u64> {
    let nvml = nvml()?;
    Some(nvml.device_by_index(0).ok()?.memory_info().ok()?.used)
}

/// La VRAM de la première carte à l'instant, en Go : de quoi remplir le fond des pastilles de l'état en direct.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vram {
    /// Ce que tous les programmes occupent, bureau compris.
    pub utilisee_go: f64,
    pub totale_go: f64,
}

/// La VRAM en Go (au centième) d'après ce que NVML a lu ; rien si la carte dit zéro octet, jamais de valeur
/// supposée. Une occupation au-delà du total est ramenée au total.
fn vram_en_go(utilisee: u64, totale: u64) -> Option<Vram> {
    if totale == 0 {
        return None;
    }
    Some(Vram {
        utilisee_go: en_go(utilisee.min(totale)),
        totale_go: en_go(totale),
    })
}

fn en_go(octets: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // des Go affichés au centième
    let go = octets as f64 / GIO as f64;
    (go * 100.0).round() / 100.0
}

/// La VRAM utilisée et totale de la première carte, lues d'un seul appel à NVML ; rien sans carte NVIDIA.
pub fn vram_directe() -> Option<Vram> {
    let memoire = nvml()?.device_by_index(0).ok()?.memory_info().ok()?;
    vram_en_go(memoire.used, memoire.total)
}

/// La VRAM totale de la première carte, en octets ; rien sans NVML.
pub fn vram_totale() -> Option<u64> {
    let nvml = nvml()?;
    Some(nvml.device_by_index(0).ok()?.memory_info().ok()?.total)
}

/// Ce que les autres programmes (bureau, navigateur…) peuvent occuper au plus dans le calcul de la VRAM libre : au-delà,
/// c'est sans doute un moteur d'Aria déjà chargé, qui se videra (comme en v2 : `casting.vram_max_occupee_go`).
pub const AUTRES_MAX: u64 = 5 << 29;

/// La VRAM libre pour Aria, en octets, comme en v2 (`optimiseur.profil_materiel`) : la totale moins ce qu'occupent
/// les autres programmes, plafonné à [`AUTRES_MAX`] ; rien sans NVML.
pub fn vram_libre() -> Option<u64> {
    let totale = vram_totale()?;
    Some(libre(totale, vram_utilisee().unwrap_or(0)))
}

fn libre(totale: u64, utilisee: u64) -> u64 {
    totale.saturating_sub(utilisee.min(AUTRES_MAX))
}

/// La RAM du PC, en octets (0 si elle ne se lit pas).
pub fn ram_totale() -> u64 {
    let Ok(mut systeme) = sysinfo::System::new() else {
        return 0;
    };
    systeme.refresh_memory();
    systeme.total_memory()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vram_libre_plafonnee() {
        // Le bureau compte ; un moteur d'Aria déjà chargé (au-delà de 2,5 Gio) ne compte pas.
        assert_eq!(libre(16 * GIO, GIO), 15 * GIO);
        assert_eq!(libre(16 * GIO, 12 * GIO), 16 * GIO - AUTRES_MAX);
        assert_eq!(libre(GIO, 0), GIO);
    }

    #[test]
    fn la_vram_se_rend_en_go_au_centieme() {
        let v = vram_en_go(98 * GIO / 10, 16 * GIO).unwrap();
        assert!((v.utilisee_go - 9.8).abs() < 1e-9, "{v:?}");
        assert!((v.totale_go - 16.0).abs() < 1e-9, "{v:?}");
    }

    #[test]
    fn une_vram_sans_total_ne_se_devine_pas() {
        assert_eq!(vram_en_go(GIO, 0), None);
    }

    #[test]
    fn une_occupation_au_dela_du_total_est_ramenee_au_total() {
        let v = vram_en_go(20 * GIO, 16 * GIO).unwrap();
        assert!((v.utilisee_go - v.totale_go).abs() < 1e-9, "{v:?}");
    }

    #[test]
    fn la_vram_directe_ne_plante_jamais_meme_sans_carte() {
        // Sur la CI, il n'y a pas de carte NVIDIA : rien à rendre, sans erreur.
        if let Some(v) = vram_directe() {
            assert!(v.utilisee_go <= v.totale_go);
        }
    }

    #[test]
    fn sans_nvidia_la_carte_est_dite_absente() {
        let c = evaluer(None, 32 * GIO, 580);
        assert!(!c.nvidia && !c.ok);
        assert_eq!(c.ram_go, 32);
        assert!(c.message.unwrap().contains("Aucune carte NVIDIA"));
    }

    #[test]
    fn un_pilote_trop_ancien_cite_le_minimum() {
        let c = evaluer(
            Some(("RTX 4080".to_owned(), "566.36".to_owned(), 16 * GIO)),
            64 * GIO,
            580,
        );
        assert!(c.nvidia && !c.ok);
        assert_eq!(c.vram_go, Some(16));
        let message = c.message.unwrap();
        assert!(message.contains("566.36") && message.contains("580"), "{message}");
    }

    #[test]
    fn un_pilote_recent_convient() {
        let c = evaluer(
            Some(("RTX 5090".to_owned(), "581.15".to_owned(), 32 * GIO)),
            64 * GIO,
            580,
        );
        assert!(c.ok);
        assert_eq!(c.message, None);
    }

    #[test]
    fn lire_ne_plante_jamais_meme_sans_carte() {
        // Sur la CI, il n'y a pas de carte NVIDIA : la carte est dite absente, sans erreur.
        let c = lire(580);
        if !c.nvidia {
            assert!(!c.ok);
        }
    }
}
