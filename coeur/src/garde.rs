//! La garde VRAM, comme en v2 (`app/garde.py`) : avant de lancer un moteur, attendre que la carte soit assez libre
//! (un jeu ou une autre appli peut l'occuper). L'utilisateur peut forcer le lancement, ou l'annuler.
//!
//! Sans NVML, la garde ne bloque rien : on ne sait pas lire la carte.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Au-delà de cette VRAM occupée par d'autres, on attend (3,5 Gio).
pub const SEUIL: u64 = 3584 * 1024 * 1024;
/// La carte est relue à cet intervalle.
pub const INTERVALLE: Duration = Duration::from_secs(5);
/// Le message d'une attente annulée.
pub const ANNULEE: &str = "Annulé : la carte graphique était occupée par une autre appli";

/// Ce que l'interface montre pendant une attente.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attente {
    pub moteur: String,
    pub vram_go: f64,
    pub seuil_go: f64,
    pub depuis_s: u64,
}

/// La garde : une attente à la fois, forçable et annulable depuis l'interface.
#[derive(Debug, Default)]
pub struct Garde {
    forcer: AtomicBool,
    annuler: AtomicBool,
    attente: Mutex<Option<Attente>>,
}

impl Garde {
    /// L'attente en cours, s'il y en a une.
    pub fn attente(&self) -> Option<Attente> {
        self.attente.lock().ok().and_then(|a| a.clone())
    }

    /// Lancer quand même le moteur qui attend.
    pub fn forcer(&self) {
        self.forcer.store(true, Ordering::Relaxed);
    }

    /// Renoncer au lancement qui attend.
    pub fn annuler(&self) {
        self.annuler.store(true, Ordering::Relaxed);
    }

    /// Attend que la VRAM occupée (lue par `lire_vram`) passe sous [`SEUIL`], que l'utilisateur force, ou qu'il
    /// annule. `lire_vram` rend rien sans NVML : la garde rend alors la main tout de suite.
    ///
    /// # Errors
    /// L'utilisateur a annulé ([`ANNULEE`]).
    pub fn attendre(&self, moteur: &str, lire_vram: impl Fn() -> Option<u64>) -> Result<(), String> {
        self.attendre_avec(moteur, lire_vram, SEUIL, INTERVALLE)
    }

    fn attendre_avec(
        &self,
        moteur: &str,
        lire_vram: impl Fn() -> Option<u64>,
        seuil: u64,
        intervalle: Duration,
    ) -> Result<(), String> {
        self.forcer.store(false, Ordering::Relaxed);
        self.annuler.store(false, Ordering::Relaxed);
        let debut = Instant::now();
        let resultat = loop {
            let Some(vram) = lire_vram() else {
                break Ok(());
            };
            if vram <= seuil || self.forcer.load(Ordering::Relaxed) {
                break Ok(());
            }
            if self.annuler.load(Ordering::Relaxed) {
                break Err(ANNULEE.to_owned());
            }
            if let Ok(mut attente) = self.attente.lock() {
                *attente = Some(Attente {
                    moteur: moteur.to_owned(),
                    vram_go: go(vram),
                    seuil_go: go(seuil),
                    depuis_s: debut.elapsed().as_secs(),
                });
            }
            // Relire la carte à l'intervalle, mais réagir vite à « forcer » et « annuler ».
            let prochaine = Instant::now() + intervalle;
            while Instant::now() < prochaine {
                if self.forcer.load(Ordering::Relaxed) || self.annuler.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200).min(intervalle));
            }
            if self.forcer.load(Ordering::Relaxed) {
                break Ok(());
            }
            if self.annuler.load(Ordering::Relaxed) {
                break Err(ANNULEE.to_owned());
            }
        };
        if let Ok(mut attente) = self.attente.lock() {
            *attente = None;
        }
        resultat
    }
}

/// Des octets en Gio, au dixième.
fn go(octets: u64) -> f64 {
    let dixiemes = octets.saturating_mul(10) / (1024 * 1024 * 1024);
    dixiemes as f64 / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Arc;

    const GIO: u64 = 1024 * 1024 * 1024;

    #[test]
    fn sans_nvml_rien_n_attend() {
        let garde = Garde::default();
        assert!(garde.attendre("ComfyUI", || None).is_ok());
        assert_eq!(garde.attente(), None);
    }

    #[test]
    fn une_carte_libre_rend_la_main() {
        let garde = Garde::default();
        assert!(garde.attendre("ComfyUI", || Some(GIO)).is_ok());
    }

    #[test]
    fn on_attend_que_la_carte_se_libere() {
        let garde = Garde::default();
        let lectures = Cell::new(0);
        let lire = || {
            lectures.set(lectures.get() + 1);
            Some(if lectures.get() < 3 { 8 * GIO } else { GIO })
        };
        assert!(
            garde
                .attendre_avec("Llama", lire, SEUIL, Duration::from_millis(10))
                .is_ok()
        );
        assert_eq!(lectures.get(), 3);
        assert_eq!(garde.attente(), None, "l'attente est effacée à la fin");
    }

    #[test]
    fn forcer_ou_annuler_depuis_un_autre_fil() {
        for forcer in [true, false] {
            let garde = Arc::new(Garde::default());
            let autre = Arc::clone(&garde);
            let fil = std::thread::spawn(move || {
                while autre.attente().is_none() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let attente = autre.attente().unwrap();
                assert_eq!(attente.moteur, "ComfyUI");
                assert!((attente.vram_go - 8.0).abs() < 1e-9);
                if forcer { autre.forcer() } else { autre.annuler() }
            });
            let resultat = garde.attendre_avec("ComfyUI", || Some(8 * GIO), SEUIL, Duration::from_secs(60));
            fil.join().unwrap();
            if forcer {
                assert!(resultat.is_ok());
            } else {
                assert_eq!(resultat.unwrap_err(), ANNULEE);
            }
        }
    }
}
