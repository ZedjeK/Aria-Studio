//! ComfyUI lancé (ou retrouvé) par Aria, comme en v2 (`app/comfy.py`, classe `ComfyUI`) : démarrage, attente,
//! arrêt, exécution d'un workflow suivie par WebSocket, rapatriement des sorties, envoi d'une image, libération de la
//! VRAM.
//!
//! - Un ComfyUI qui répond déjà sur le port est utilisé tel quel : rien n'est vidé ni lancé.
//! - Un port pris sans réponse (ComfyUI occupé, ou autre programme) : on attend, sans jamais lancer un second
//!   processus.
//! - L'annulation d'une exécution la retire de la file de ComfyUI et l'interrompt.

use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::Message;
use tungstenite::protocol::WebSocketConfig;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::comfy::{
    Evenement, Lancement, Processus, avec_prefixes_uniques, effacer_sortie, est_orphelin, lire_evenement,
    nouveau_jeton, parametres_vue, sorties_a_recuperer, type_mime, vram_stable, workflow_refuse,
};
use crate::local;

/// Le délai de démarrage par défaut.
pub const DELAI_DEMARRAGE: Duration = Duration::from_secs(180);
/// Le message d'une exécution annulée.
pub const ANNULEE: &str = "Annulé";

/// Une sortie rapatriée : son nom et ses octets.
pub type Fichier = (String, Vec<u8>);

/// ComfyUI, lancé ou non.
pub struct ServeurComfy {
    lancement: Lancement,
    /// `data\` d'Aria : le journal va dans `data\logs\comfyui.log`.
    data: std::path::PathBuf,
    processus: Option<Child>,
    /// Un ComfyUI lancé par une Aria qui n'est plus là, repris par celle-ci : son numéro de processus.
    adopte: Option<u32>,
}

impl ServeurComfy {
    pub fn new(lancement: Lancement, data: std::path::PathBuf) -> Self {
        Self {
            lancement,
            data,
            processus: None,
            adopte: None,
        }
    }

    /// ComfyUI tourne-t-il pour Aria (lancé ou adopté) ?
    pub fn vivant(&mut self) -> bool {
        if self.adopte.is_some_and(|pid| !processus_existe(pid)) {
            self.adopte = None;
        }
        self.tourne() || self.adopte.is_some()
    }

    /// Reprend un ComfyUI resté seul après une Aria arrêtée (notre port, notre dossier, parent mort), comme en v2.
    /// Rend vrai s'il en a trouvé un.
    pub fn adopter(&mut self) -> bool {
        if self.vivant() {
            return false;
        }
        let moi = std::process::id();
        let trouve = pythons()
            .into_iter()
            .find(|p| est_orphelin(p, self.lancement.port, &self.lancement.dossier, moi));
        self.adopte = trouve.map(|p| p.pid);
        self.adopte.is_some()
    }

    pub fn lancement(&self) -> &Lancement {
        &self.lancement
    }

    /// Change de ComfyUI (celui d'Aria ou celui de l'utilisateur). À faire ComfyUI arrêté.
    pub fn changer_de_dossier(&mut self, dossier: std::path::PathBuf) {
        self.arreter();
        self.lancement.dossier = dossier;
    }

    pub fn url(&self) -> String {
        self.lancement.url()
    }

    /// Le processus lancé par Aria tourne-t-il ?
    pub fn tourne(&mut self) -> bool {
        self.processus.as_mut().is_some_and(local::tourne)
    }

    /// `/system_stats` répond-il 200 ?
    pub fn est_pret(&self) -> bool {
        local::client(Duration::from_secs(2))
            .get(format!("{}/system_stats", self.url()))
            .call()
            .is_ok_and(|r| r.status().as_u16() == 200)
    }

    fn port_pris(&self) -> bool {
        local::port_pris(self.lancement.port, Duration::from_secs(1))
    }

    /// Démarre ComfyUI, ou attend celui qui tourne déjà sur le port.
    ///
    /// # Errors
    /// ComfyUI ne se lance pas, s'arrête au démarrage, ou ne répond pas à temps.
    pub fn demarrer(&mut self, delai: Duration) -> Result<(), String> {
        if self.est_pret() {
            self.adopter();
            return Ok(());
        }
        if self.tourne() || self.port_pris() {
            self.adopter();
            return self.attendre(delai, false);
        }
        let python = self.lancement.python();
        if !python.is_file() {
            return Err(format!("ComfyUI introuvable : {}", python.display()));
        }
        self.lancement
            .preparer_travail()
            .map_err(|e| format!("dossiers de travail de ComfyUI : {e}"))?;
        let (_, journal) = local::journal(&self.data, "comfyui")
            .map_err(|e| format!("journal de ComfyUI impossible : {e}"))?;
        let processus = local::lancer(
            &python,
            &self.lancement.arguments(),
            Some(&self.lancement.dossier),
            &journal,
            &[],
        )
        .map_err(|e| format!("ComfyUI ne se lance pas : {e}"))?;
        self.processus = Some(processus);
        self.attendre(delai, true)
    }

    fn attendre(&mut self, delai: Duration, lance: bool) -> Result<(), String> {
        let fin = Instant::now() + delai;
        loop {
            if lance {
                let arrete = self.processus.as_mut().and_then(|p| {
                    if local::tourne(p) {
                        None
                    } else {
                        Some(local::code(p))
                    }
                });
                if let Some(code) = arrete {
                    self.processus = None;
                    return Err(format!(
                        "ComfyUI s'est arrêté au démarrage (code {code}), voir data/logs/comfyui.log"
                    ));
                }
            }
            if self.est_pret() {
                return Ok(());
            }
            if Instant::now() >= fin {
                if lance {
                    self.arreter();
                    return Err(format!("ComfyUI n'a pas répondu en {} s", delai.as_secs()));
                }
                return Err(format!(
                    "ComfyUI tourne mais ne répond plus depuis {} s (occupé ou bloqué)",
                    delai.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    /// Arrête le ComfyUI lancé ou adopté par Aria (jamais un ComfyUI lancé par quelqu'un d'autre).
    pub fn arreter(&mut self) {
        if let Some(mut processus) = self.processus.take() {
            local::arreter(&mut processus);
        }
        if let Some(pid) = self.adopte.take() {
            tuer(pid);
        }
    }

    /// La VRAM occupée d'après ComfyUI (première carte).
    ///
    /// # Errors
    /// ComfyUI ne répond pas ou ne dit rien de la carte.
    pub fn vram_utilisee(&self) -> Result<u64, String> {
        let mut reponse = local::client(Duration::from_secs(5))
            .get(format!("{}/system_stats", self.url()))
            .call()
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        let stats: Value =
            serde_json::from_str(&reponse.body_mut().read_to_string().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let carte = stats
            .pointer("/devices/0")
            .ok_or_else(|| "ComfyUI ne voit pas de carte graphique".to_owned())?;
        let total = carte.get("vram_total").and_then(Value::as_u64).unwrap_or(0);
        let libre = carte.get("vram_free").and_then(Value::as_u64).unwrap_or(0);
        Ok(total.saturating_sub(libre))
    }

    /// Demande à ComfyUI de décharger ses modèles, puis attend que la VRAM se stabilise (30 s au plus). Rend la
    /// dernière lecture.
    ///
    /// # Errors
    /// ComfyUI refuse ou ne répond pas.
    pub fn liberer_vram(&self, delai: Duration) -> Result<u64, String> {
        let reponse = local::client(Duration::from_secs(5))
            .post(format!("{}/free", self.url()))
            .header("Content-Type", "application/json")
            .send(r#"{"unload_models": true, "free_memory": true}"#)
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        if reponse.status().as_u16() != 200 {
            return Err(format!(
                "ComfyUI a refusé de libérer la VRAM ({})",
                reponse.status().as_u16()
            ));
        }
        let fin = Instant::now() + delai;
        let mut lectures = Vec::new();
        loop {
            std::thread::sleep(Duration::from_millis(500));
            lectures.push(self.vram_utilisee()?);
            if vram_stable(&lectures) || Instant::now() >= fin {
                return lectures
                    .last()
                    .copied()
                    .ok_or_else(|| "aucune lecture de la VRAM".to_owned());
            }
        }
    }

    /// Envoie une image dans les entrées de ComfyUI et rend le nom sous lequel la citer dans un workflow.
    ///
    /// # Errors
    /// Un nom refusé, ou ComfyUI qui refuse.
    pub fn televerser(&self, nom: &str, octets: &[u8]) -> Result<String, String> {
        if nom.is_empty() || nom.chars().any(|c| matches!(c, '"' | '\r' | '\n' | '/' | '\\')) {
            return Err(format!("nom d'image refusé : {nom}"));
        }
        let limite = format!("aria{}{}", nouveau_jeton()?, nouveau_jeton()?);
        let corps = corps_multipart(&limite, nom, type_mime(nom), octets);
        let mut reponse = local::client(Duration::from_secs(60))
            .post(format!("{}/upload/image", self.url()))
            .header("Content-Type", format!("multipart/form-data; boundary={limite}"))
            .send(corps.as_slice())
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        let texte = reponse.body_mut().read_to_string().map_err(|e| e.to_string())?;
        let statut = reponse.status().as_u16();
        if statut != 200 {
            return Err(format!("ComfyUI a refusé l'image ({statut}) : {texte}"));
        }
        let rendu: Value = serde_json::from_str(&texte).map_err(|e| e.to_string())?;
        let nom = rendu.get("name").and_then(Value::as_str).unwrap_or(nom);
        Ok(match rendu.get("subfolder").and_then(Value::as_str) {
            Some(sous) if !sous.is_empty() => format!("{sous}/{nom}"),
            _ => nom.to_owned(),
        })
    }

    /// Exécute un workflow et rend ses fichiers de sortie, effacés ensuite de `output\`. `progression(valeur, max)`
    /// suit chaque nœud ; `annule` passé à vrai retire l'exécution de la file de ComfyUI et l'interrompt.
    ///
    /// # Errors
    /// ComfyUI refuse le workflow, une erreur de nœud, une interruption, une annulation ([`ANNULEE`]), ou une
    /// coupure.
    pub fn executer(
        &self,
        workflow: &Value,
        progression: &mut impl FnMut(u64, u64),
        annule: &AtomicBool,
    ) -> Result<Vec<Fichier>, String> {
        let workflow = avec_prefixes_uniques(workflow, &nouveau_jeton()?);
        let client_id = format!(
            "{}{}{}{}",
            nouveau_jeton()?,
            nouveau_jeton()?,
            nouveau_jeton()?,
            nouveau_jeton()?
        );
        let mut socket = self.ouvrir_websocket(&client_id)?;
        let corps = json!({ "prompt": workflow, "client_id": client_id }).to_string();
        let mut reponse = local::client(Duration::from_secs(30))
            .post(format!("{}/prompt", self.url()))
            .header("Content-Type", "application/json")
            .send(corps.as_str())
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        let texte = reponse.body_mut().read_to_string().map_err(|e| e.to_string())?;
        if reponse.status().as_u16() != 200 {
            return Err(workflow_refuse(&texte));
        }
        let prompt_id = serde_json::from_str::<Value>(&texte)
            .ok()
            .and_then(|v| v.get("prompt_id").and_then(Value::as_str).map(str::to_owned))
            .ok_or_else(|| format!("ComfyUI n'a pas rendu d'identifiant d'exécution : {texte}"))?;
        loop {
            if annule.load(Ordering::Relaxed) {
                self.interrompre(&prompt_id);
                return Err(ANNULEE.to_owned());
            }
            match socket.read() {
                Ok(Message::Text(message)) => match lire_evenement(message.as_str(), &prompt_id) {
                    Evenement::Progression { valeur, max } => progression(valeur, max),
                    Evenement::Erreur(erreur) => return Err(erreur),
                    Evenement::Fin => break,
                    Evenement::Autre => {}
                },
                Ok(_) => {}
                // Le délai de lecture sert à revoir l'annulation chaque seconde.
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => return Err(format!("ComfyUI a coupé la liaison pendant l'exécution : {e}")),
            }
        }
        let _ = socket.close(None);
        self.rapatrier(&prompt_id)
    }

    fn ouvrir_websocket(&self, client_id: &str) -> Result<tungstenite::WebSocket<TcpStream>, String> {
        let adresse = SocketAddr::from(([127, 0, 0, 1], self.lancement.port));
        let flux = TcpStream::connect_timeout(&adresse, Duration::from_secs(10))
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        flux.set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        let url = format!("ws://127.0.0.1:{}/ws?clientId={client_id}", self.lancement.port);
        // Les aperçus binaires peuvent être gros : pas de limite de taille de message.
        let reglages = WebSocketConfig::default()
            .max_message_size(None)
            .max_frame_size(None);
        let (socket, _) = tungstenite::client::client_with_config(url.as_str(), flux, Some(reglages))
            .map_err(|e| format!("liaison avec ComfyUI impossible : {e}"))?;
        Ok(socket)
    }

    /// Retire l'exécution de la file et l'interrompt ; les erreurs sont sans conséquence (elle est peut-être finie).
    fn interrompre(&self, prompt_id: &str) {
        let client = local::client(Duration::from_secs(5));
        let _ = client
            .post(format!("{}/queue", self.url()))
            .header("Content-Type", "application/json")
            .send(json!({ "delete": [prompt_id] }).to_string().as_str());
        let _ = client
            .post(format!("{}/interrupt", self.url()))
            .header("Content-Type", "application/json")
            .send(json!({ "prompt_id": prompt_id }).to_string().as_str());
    }

    fn rapatrier(&self, prompt_id: &str) -> Result<Vec<Fichier>, String> {
        let mut reponse = local::client(Duration::from_secs(30))
            .get(format!("{}/history/{prompt_id}", self.url()))
            .call()
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        let historique: Value =
            serde_json::from_str(&reponse.body_mut().read_to_string().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let annonces = sorties_a_recuperer(&historique, prompt_id)?;
        let client = local::client(Duration::from_secs(300));
        let mut fichiers = Vec::new();
        for annonce in &annonces {
            let mut requete = client.get(format!("{}/view", self.url()));
            for (cle, valeur) in parametres_vue(annonce) {
                requete = requete.query(cle, valeur);
            }
            let mut vue = requete
                .call()
                .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
            if vue.status().as_u16() != 200 {
                return Err(format!(
                    "ComfyUI n'a pas rendu une sortie ({})",
                    vue.status().as_u16()
                ));
            }
            // Une vidéo dépasse la limite par défaut de 10 Mo.
            let octets = vue
                .body_mut()
                .with_config()
                .limit(u64::MAX)
                .read_to_vec()
                .map_err(|e| format!("sortie de ComfyUI incomplète : {e}"))?;
            let nom = annonce
                .get("filename")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            fichiers.push((nom, octets));
        }
        let output = self.lancement.travail.join("output");
        for annonce in &annonces {
            effacer_sortie(&output, annonce);
        }
        Ok(fichiers)
    }

    /// Les samplers et ordonnanceurs que connaît ce ComfyUI.
    ///
    /// # Errors
    /// ComfyUI ne répond pas ou ne les donne pas.
    pub fn options_sampler(&self) -> Result<(Vec<String>, Vec<String>), String> {
        let mut reponse = local::client(Duration::from_secs(10))
            .get(format!("{}/object_info/KSampler", self.url()))
            .call()
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        let info: Value =
            serde_json::from_str(&reponse.body_mut().read_to_string().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let liste = |champ: &str| -> Vec<String> {
            info.pointer(&format!("/KSampler/input/required/{champ}/0"))
                .and_then(Value::as_array)
                .map(|v| v.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default()
        };
        Ok((liste("sampler_name"), liste("scheduler")))
    }

    /// Les types de nœuds que connaît ce ComfyUI.
    ///
    /// # Errors
    /// ComfyUI ne répond pas.
    pub fn noeuds(&self) -> Result<Vec<String>, String> {
        let mut reponse = local::client(Duration::from_secs(60))
            .get(format!("{}/object_info", self.url()))
            .call()
            .map_err(|e| format!("ComfyUI ne répond pas : {e}"))?;
        let texte = reponse
            .body_mut()
            .with_config()
            .limit(u64::MAX)
            .read_to_string()
            .map_err(|e| e.to_string())?;
        let info: Value = serde_json::from_str(&texte).map_err(|e| e.to_string())?;
        Ok(info
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default())
    }
}

impl Drop for ServeurComfy {
    /// Aria qui s'arrête n'abandonne pas le ComfyUI qu'elle a lancé.
    fn drop(&mut self) {
        self.arreter();
    }
}

/// Les processus Python qui tournent, avec leur ligne de commande et leur dossier courant.
fn pythons() -> Vec<Processus> {
    let Ok(mut systeme) = System::new() else {
        return Vec::new();
    };
    let quoi = ProcessRefreshKind::nothing()
        .with_cmd(UpdateKind::Always)
        .with_cwd(UpdateKind::Always);
    systeme.refresh_processes_specifics(ProcessesToUpdate::All, true, quoi);
    let tous = systeme.processes();
    tous.values()
        .filter(|p| p.name().to_string_lossy().to_lowercase().starts_with("python"))
        .map(|p| {
            // Un parent plus jeune que l'enfant est un autre processus qui a repris son numéro.
            let parent_vivant = p
                .parent()
                .and_then(|parent| tous.get(&parent))
                .is_some_and(|parent| parent.start_time() <= p.start_time());
            Processus {
                pid: p.pid().as_u32(),
                nom: p.name().to_string_lossy().into_owned(),
                ligne_de_commande: p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect(),
                dossier: p.cwd().map(std::path::Path::to_path_buf),
                parent_vivant,
            }
        })
        .collect()
}

fn processus_existe(pid: u32) -> bool {
    let Ok(mut systeme) = System::new() else {
        return false;
    };
    let pid = Pid::from_u32(pid);
    systeme.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    systeme.process(pid).is_some()
}

/// Arrête un ComfyUI adopté.
fn tuer(pid: u32) {
    let Ok(mut systeme) = System::new() else {
        return;
    };
    let pid = Pid::from_u32(pid);
    systeme.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    if let Some(processus) = systeme.process(pid) {
        // Déjà arrêté entre-temps : sans conséquence.
        let _ = processus.kill();
    }
}

/// Le corps `multipart/form-data` d'un envoi d'image : le fichier, puis `overwrite=true`.
fn corps_multipart(limite: &str, nom: &str, mime: &str, octets: &[u8]) -> Vec<u8> {
    let mut corps = Vec::with_capacity(octets.len() + 512);
    corps.extend_from_slice(
        format!(
            "--{limite}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{nom}\"\r\n\
             Content-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    corps.extend_from_slice(octets);
    corps.extend_from_slice(
        format!("\r\n--{limite}\r\nContent-Disposition: form-data; name=\"overwrite\"\r\n\r\ntrue\r\n--{limite}--\r\n")
            .as_bytes(),
    );
    corps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_corps_multipart_porte_l_image_et_overwrite() {
        let corps = corps_multipart("LIM", "a.png", "image/png", b"PNG");
        let texte = String::from_utf8(corps).unwrap();
        assert!(
            texte.starts_with(
                "--LIM\r\nContent-Disposition: form-data; name=\"image\"; filename=\"a.png\"\r\n"
            )
        );
        assert!(texte.contains("Content-Type: image/png\r\n\r\nPNG\r\n--LIM\r\n"));
        assert!(texte.contains("name=\"overwrite\"\r\n\r\ntrue\r\n--LIM--\r\n"));
    }

    #[test]
    fn un_nom_d_image_qui_casserait_l_envoi_est_refuse() {
        let temp = tempfile::tempdir().unwrap();
        let serveur = ServeurComfy::new(
            Lancement {
                dossier: temp.path().to_path_buf(),
                port: 1,
                chemins_modeles: temp.path().join("chemins.yaml"),
                travail: temp.path().join("travail"),
            },
            temp.path().to_path_buf(),
        );
        for nom in ["", "a\".png", "a\r\nb.png", "../a.png", "sous\\a.png"] {
            assert!(
                serveur.televerser(nom, b"x").unwrap_err().contains("refusé"),
                "{nom:?}"
            );
        }
    }

    #[test]
    fn un_comfyui_absent_est_dit_sans_rien_lancer() {
        let temp = tempfile::tempdir().unwrap();
        // Un port libre : rien n'y répond.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut serveur = ServeurComfy::new(
            Lancement {
                dossier: temp.path().join("comfyui"),
                port,
                chemins_modeles: temp.path().join("chemins.yaml"),
                travail: temp.path().join("travail"),
            },
            temp.path().to_path_buf(),
        );
        let erreur = serveur.demarrer(Duration::from_secs(1)).unwrap_err();
        assert!(erreur.starts_with("ComfyUI introuvable"), "{erreur}");
        assert!(!temp.path().join("travail").exists(), "rien n'est préparé");
        assert!(!serveur.tourne());
    }
}
