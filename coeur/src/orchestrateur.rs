//! L'orchestrateur des moteurs, comme en v2 (`app/orchestrateur.py`) : une seule carte graphique, donc un seul
//! moteur qui la tient à la fois.
//!
//! - ComfyUI prend la main : llama-server est arrêté, la garde VRAM attend que la carte soit libre, ComfyUI démarre.
//! - Le LLM prend la main : ComfyUI vide sa VRAM sans s'arrêter (il repart plus vite), la garde attend, llama-server
//!   démarre avec le bon modèle.
//! - Installer ou revenir en arrière sur un moteur se fait moteur arrêté ([`Orchestrateur::sans_moteur`]).
//!
//! L'appelant garde l'orchestrateur dans un `Mutex` : chaque opération le tient le temps qu'elle dure.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use crate::carte;
use crate::comfy_serveur::{self, ServeurComfy};
use crate::garde::Garde;
use crate::llama::Lancement;
use crate::llama_serveur::{self, ServeurLlama};
use crate::modeles;

/// Le délai pour que ComfyUI vide sa VRAM.
const DELAI_LIBERATION: Duration = Duration::from_secs(30);

/// Le moteur qui tient la carte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actif {
    ComfyUI,
    Llama,
}

/// La pastille d'un moteur dans l'état en direct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pastille {
    /// Il tourne et tient la carte.
    Actif,
    /// Installé, arrêté ou en retrait.
    Veille,
    /// Pas installé.
    Absent,
}

/// La pastille d'un moteur, comme en v2 (`GET /api/etat`).
pub fn pastille(tourne: bool, tient_la_carte: bool, installe: bool) -> Pastille {
    match (tourne, tient_la_carte, installe) {
        (true, true, _) => Pastille::Actif,
        (true, false, _) | (false, _, true) => Pastille::Veille,
        (false, _, false) => Pastille::Absent,
    }
}

/// Les deux moteurs et celui qui tient la carte.
pub struct Orchestrateur {
    pub comfy: ServeurComfy,
    pub llama: ServeurLlama,
    actif: Option<Actif>,
    garde: Arc<Garde>,
    samplers: Option<(Vec<String>, Vec<String>)>,
    /// Ce que la garde lit de la carte (NVML ; une lecture fixe dans les tests).
    lire_vram: fn() -> Option<u64>,
    /// Le dossier des modèles : ComfyUI l'apprend par son fichier de chemins, réécrit avant chaque lancement.
    modeles: Option<std::path::PathBuf>,
}

impl Orchestrateur {
    pub fn new(comfy: ServeurComfy, llama: ServeurLlama, garde: Arc<Garde>) -> Self {
        Self {
            comfy,
            llama,
            actif: None,
            garde,
            samplers: None,
            lire_vram: carte::vram_utilisee,
            modeles: None,
        }
    }

    /// Le dossier des modèles que ComfyUI doit connaître.
    #[must_use]
    pub fn avec_modeles(self, modeles: std::path::PathBuf) -> Self {
        Self {
            modeles: Some(modeles),
            ..self
        }
    }

    /// Le moteur qui tient la carte.
    pub fn actif(&self) -> Option<Actif> {
        self.actif
    }

    /// Les samplers et ordonnanceurs de ComfyUI, lus à son premier démarrage.
    pub fn samplers(&self) -> Option<&(Vec<String>, Vec<String>)> {
        self.samplers.as_ref()
    }

    /// ComfyUI, prêt à travailler.
    ///
    /// # Errors
    /// L'attente de la carte est annulée, ou ComfyUI ne démarre pas.
    pub fn comfyui(&mut self) -> Result<&mut ServeurComfy, String> {
        if self.actif != Some(Actif::ComfyUI) {
            self.llama.arreter();
            self.garde.attendre("ComfyUI", self.lire_vram)?;
            // Comme en v2 : ComfyUI qui n'est pas lancé réapprend les dossiers des modèles avant de démarrer.
            if let Some(modeles) = &self.modeles
                && !self.comfy.vivant()
            {
                let cible = self.comfy.lancement().chemins_modeles.clone();
                modeles::ecrire_chemins_comfyui(modeles, &cible)
                    .map_err(|e| format!("chemins des modèles pour ComfyUI : {e}"))?;
            }
            self.comfy.demarrer(comfy_serveur::DELAI_DEMARRAGE)?;
            self.actif = Some(Actif::ComfyUI);
            if self.samplers.is_none() {
                // Sans eux, les réglages proposent les valeurs du workflow : pas une raison d'échouer.
                self.samplers = self.comfy.options_sampler().ok();
            }
        }
        Ok(&mut self.comfy)
    }

    /// llama-server, lancé avec ce modèle et ces réglages.
    ///
    /// # Errors
    /// ComfyUI ne libère pas la carte, l'attente est annulée, ou llama-server ne démarre pas.
    pub fn llm(&mut self, lancement: Lancement) -> Result<&mut ServeurLlama, String> {
        if self.actif == Some(Actif::ComfyUI) && self.comfy.est_pret() {
            self.comfy.liberer_vram(DELAI_LIBERATION)?;
        }
        if self.actif != Some(Actif::Llama) {
            self.garde.attendre("Llama", self.lire_vram)?;
        }
        self.llama.demarrer(lancement, llama_serveur::DELAI_DEMARRAGE)?;
        self.actif = Some(Actif::Llama);
        Ok(&mut self.llama)
    }

    /// Rend la carte : ComfyUI vide sa VRAM, llama-server s'arrête.
    ///
    /// # Errors
    /// ComfyUI ne répond pas à la demande de libération.
    pub fn liberer(&mut self) -> Result<(), String> {
        let resultat = match self.actif {
            Some(Actif::ComfyUI) => self.comfy.liberer_vram(DELAI_LIBERATION).map(|_| ()),
            Some(Actif::Llama) => {
                self.llama.arreter();
                Ok(())
            }
            None => Ok(()),
        };
        self.actif = None;
        resultat
    }

    /// Arrête les deux moteurs (Aria qui se ferme).
    pub fn arreter(&mut self) {
        self.llama.arreter();
        self.comfy.arreter();
        self.actif = None;
    }

    /// Arrête le moteur qu'on va installer ou échanger (`comfyui`, ou un llama : `llama` et `llama-prism` ont le
    /// même pilote). L'appelant garde l'orchestrateur verrouillé pendant l'échange.
    pub fn sans_moteur(&mut self, id: &str) {
        if id == "comfyui" {
            self.comfy.arreter();
            if self.actif == Some(Actif::ComfyUI) {
                self.actif = None;
            }
        } else {
            self.llama.arreter();
            if self.actif == Some(Actif::Llama) {
                self.actif = None;
            }
        }
    }

    /// Au lancement d'Aria : reprend un ComfyUI resté seul ; s'il répond, il tient la carte.
    pub fn reprendre(&mut self) {
        if self.comfy.adopter() && self.comfy.est_pret() {
            self.actif = Some(Actif::ComfyUI);
        }
    }

    /// Les pastilles de ComfyUI et du LLM ; `installe` dit si chaque moteur est sur le disque.
    pub fn pastilles(&mut self, comfy_installe: bool, llama_installe: bool) -> (Pastille, Pastille) {
        let comfy = pastille(
            self.comfy.vivant(),
            self.actif == Some(Actif::ComfyUI),
            comfy_installe,
        );
        let llama = pastille(
            self.llama.tourne(),
            self.actif == Some(Actif::Llama),
            llama_installe,
        );
        (comfy, llama)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comfy;

    #[test]
    fn les_pastilles_sont_celles_de_la_v2() {
        // Le LLM tient la carte, ComfyUI encore lancé : (veille, actif).
        assert_eq!(
            (pastille(true, false, true), pastille(true, true, true)),
            (Pastille::Veille, Pastille::Actif)
        );
        // ComfyUI tient la carte.
        assert_eq!(pastille(true, true, true), Pastille::Actif);
        // Carte libérée : les deux en veille.
        assert_eq!(pastille(false, false, true), Pastille::Veille);
        // Moteur introuvable.
        assert_eq!(pastille(false, false, false), Pastille::Absent);
    }

    fn orchestrateur(dossier: &std::path::Path) -> Orchestrateur {
        let port_libre = || {
            std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        };
        let comfy = ServeurComfy::new(
            comfy::Lancement {
                dossier: dossier.join("comfyui"),
                port: port_libre(),
                chemins_modeles: dossier.join("chemins.yaml"),
                travail: dossier.join("travail"),
            },
            dossier.to_path_buf(),
        );
        let llama = ServeurLlama::new(port_libre(), dossier.to_path_buf());
        Orchestrateur {
            lire_vram: || None,
            ..Orchestrateur::new(comfy, llama, Arc::new(Garde::default()))
        }
    }

    #[test]
    fn rien_ne_tourne_au_depart() {
        let temp = tempfile::tempdir().unwrap();
        let mut o = orchestrateur(temp.path());
        assert_eq!(o.actif(), None);
        assert_eq!(o.pastilles(false, true), (Pastille::Absent, Pastille::Veille));
        assert!(o.liberer().is_ok());
        o.sans_moteur("comfyui");
        o.sans_moteur("llama-prism");
        o.arreter();
        assert_eq!(o.actif(), None);
    }

    #[test]
    fn un_comfyui_absent_est_dit_et_ne_prend_pas_la_carte() {
        let temp = tempfile::tempdir().unwrap();
        let mut o = orchestrateur(temp.path());
        let erreur = o.comfyui().err().unwrap();
        assert!(erreur.starts_with("ComfyUI introuvable"), "{erreur}");
        assert_eq!(o.actif(), None);
    }
}
