//! llama-server lancé par Aria, comme en v2 (`app/llm.py`, classe `Llama`) : lancement avec une clé neuve, attente
//! qu'il réponde, arrêt, et génération en flux. Une seule instance à la fois ; relancer avec les mêmes réglages ne
//! fait rien.

use std::io::BufReader;
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};

use crate::llama::{Demande, Lancement, Reponse, VARIABLE_CLE, erreur_http, lire_flux, nouvelle_cle};
use crate::local;

/// Le délai de démarrage par défaut.
pub const DELAI_DEMARRAGE: Duration = Duration::from_secs(120);
/// Un silence plus long que ceci pendant une génération l'interrompt.
pub const SILENCE_MAX: Duration = Duration::from_secs(300);

/// llama-server, lancé ou non.
pub struct ServeurLlama {
    port: u16,
    /// `data\` d'Aria : le journal va dans `data\logs\llama.log`.
    data: PathBuf,
    processus: Option<Child>,
    /// La clé de ce lancement, jamais écrite nulle part.
    cle: Option<String>,
    actif: Option<Lancement>,
}

impl ServeurLlama {
    pub fn new(port: u16, data: PathBuf) -> Self {
        Self {
            port,
            data,
            processus: None,
            cle: None,
            actif: None,
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Les réglages du lancement en cours.
    pub fn actif(&self) -> Option<&Lancement> {
        self.actif.as_ref()
    }

    /// Le processus lancé par Aria tourne-t-il ?
    pub fn tourne(&mut self) -> bool {
        self.processus.as_mut().is_some_and(local::tourne)
    }

    fn entete(&self) -> Option<String> {
        self.cle.as_ref().map(|cle| format!("Bearer {cle}"))
    }

    /// `/health` répond-il 200, avec la clé ?
    pub fn est_pret(&self) -> bool {
        let mut requete = local::client(Duration::from_secs(2)).get(format!("{}/health", self.url()));
        if let Some(entete) = self.entete() {
            requete = requete.header("Authorization", entete);
        }
        requete.call().is_ok_and(|r| r.status().as_u16() == 200)
    }

    /// Lance llama-server avec ces réglages et attend qu'il réponde. Déjà lancé avec les mêmes : rien à faire ;
    /// avec d'autres : il est arrêté puis relancé.
    ///
    /// # Errors
    /// Un fichier manque, le programme ne se lance pas, s'arrête au démarrage ou ne répond pas à temps (il est
    /// alors arrêté).
    pub fn demarrer(&mut self, lancement: Lancement, delai: Duration) -> Result<(), String> {
        if self.actif.as_ref() == Some(&lancement) && self.tourne() {
            return Ok(());
        }
        self.arreter();
        let arguments = lancement.arguments()?;
        let (_, journal) = local::journal(&self.data, "llama")
            .map_err(|e| format!("journal de llama-server impossible : {e}"))?;
        let cle = nouvelle_cle()?;
        let processus = local::lancer(
            &lancement.serveur,
            &arguments,
            lancement.dossier(),
            &journal,
            &[(VARIABLE_CLE, cle.as_str())],
        )
        .map_err(|e| format!("llama-server ne se lance pas : {e}"))?;
        self.processus = Some(processus);
        self.cle = Some(cle);
        self.actif = Some(lancement);
        let fin = Instant::now() + delai;
        loop {
            let arrete = self.processus.as_mut().and_then(|p| {
                if local::tourne(p) {
                    None
                } else {
                    Some(local::code(p))
                }
            });
            if let Some(code) = arrete {
                self.arreter();
                return Err(format!(
                    "llama-server s'est arrêté au démarrage (code {code}), voir data/logs/llama.log"
                ));
            }
            if self.est_pret() {
                return Ok(());
            }
            if Instant::now() >= fin {
                self.arreter();
                return Err(format!("llama-server n'a pas répondu en {} s", delai.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Arrête llama-server s'il tourne ; la clé est oubliée.
    pub fn arreter(&mut self) {
        if let Some(mut processus) = self.processus.take() {
            local::arreter(&mut processus);
        }
        self.cle = None;
        self.actif = None;
    }

    /// Génère une réponse, en flux.
    ///
    /// # Errors
    /// llama-server n'est pas lancé ou ne répond pas, refuse la demande, ou rend une réponse vide : le message dit
    /// quoi faire.
    pub fn generer(&self, demande: &Demande) -> Result<Reponse, String> {
        let actif = self
            .actif
            .as_ref()
            .ok_or_else(|| "llama-server n'est pas lancé".to_owned())?;
        let envoi = demande.envoi(actif.reflexion);
        let corps = serde_json::to_string(&envoi.corps).map_err(|e| e.to_string())?;
        let mut requete = local::client_flux(SILENCE_MAX)
            .post(format!("{}/v1/chat/completions", self.url()))
            .header("Content-Type", "application/json");
        if let Some(entete) = self.entete() {
            requete = requete.header("Authorization", entete);
        }
        let reponse = requete
            .send(corps.as_str())
            .map_err(|e| format!("llama-server ne répond pas : {e}"))?;
        let statut = reponse.status().as_u16();
        let mut corps = reponse.into_body();
        if statut != 200 {
            let texte = corps.read_to_string().unwrap_or_default();
            return Err(erreur_http(statut, &texte, actif.contexte));
        }
        lire_flux(BufReader::new(corps.into_reader()), &envoi)
    }
}

impl Drop for ServeurLlama {
    /// Aria qui s'arrête n'abandonne pas un llama-server derrière elle.
    fn drop(&mut self) {
        self.arreter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// Un faux llama-server : répond à une requête, et rend ce qu'il a reçu.
    fn faux_serveur(reponse: &'static str) -> (u16, mpsc::Receiver<String>) {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ecoute.local_addr().unwrap().port();
        let (envoi, reception) = mpsc::channel();
        std::thread::spawn(move || {
            let (flux, _) = ecoute.accept().unwrap();
            let mut lecteur = std::io::BufReader::new(flux.try_clone().unwrap());
            let mut requete = String::new();
            let mut longueur = 0usize;
            loop {
                let mut ligne = String::new();
                lecteur.read_line(&mut ligne).unwrap();
                if let Some(v) = ligne.to_lowercase().strip_prefix("content-length:") {
                    longueur = v.trim().parse().unwrap();
                }
                requete.push_str(&ligne);
                if ligne == "\r\n" || ligne.is_empty() {
                    break;
                }
            }
            let mut corps = vec![0u8; longueur];
            lecteur.read_exact(&mut corps).unwrap();
            requete.push_str(&String::from_utf8(corps).unwrap());
            let mut flux = flux;
            flux.write_all(reponse.as_bytes()).unwrap();
            flux.flush().unwrap();
            envoi.send(requete).unwrap();
        });
        (port, reception)
    }

    fn serveur_actif(port: u16, cle: &str) -> ServeurLlama {
        let temp = std::env::temp_dir();
        let mut s = ServeurLlama::new(port, temp.clone());
        s.cle = Some(cle.to_owned());
        s.actif = Some(Lancement {
            serveur: temp.join("llama-server.exe"),
            modele: temp.join("m.gguf"),
            port,
            contexte: 4096,
            reflexion: false,
            vision: None,
        });
        s
    }

    #[test]
    fn une_generation_en_flux_presente_la_cle() {
        let (port, recu) = faux_serveur(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n\
             data: {\"choices\": [{\"delta\": {\"content\": \"Bonjour\"}, \"finish_reason\": \"stop\"}]}\n\n\
             data: [DONE]\n\n",
        );
        let s = serveur_actif(port, "cle-essai");
        let reponse = s.generer(&Demande::default()).unwrap();
        assert_eq!(reponse.texte, "Bonjour");
        let requete = recu.recv().unwrap();
        assert!(requete.starts_with("POST /v1/chat/completions"), "{requete}");
        assert!(requete.contains("Bearer cle-essai"), "{requete}");
        assert!(requete.contains("\"stream\":true"), "{requete}");
    }

    #[test]
    fn un_contexte_trop_petit_est_dit() {
        let (port, _recu) = faux_serveur(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n\
             {\"error\": \"the request exceeds context size\"}",
        );
        let s = serveur_actif(port, "cle");
        let erreur = s.generer(&Demande::default()).unwrap_err();
        assert!(erreur.contains("contexte du LLM (4096 tokens)"), "{erreur}");
    }

    #[test]
    fn generer_sans_lancement_est_refuse() {
        let s = ServeurLlama::new(1, std::env::temp_dir());
        assert_eq!(
            s.generer(&Demande::default()).unwrap_err(),
            "llama-server n'est pas lancé"
        );
    }

    #[test]
    fn un_modele_absent_est_dit_avant_tout_lancement() {
        let temp = tempfile::tempdir().unwrap();
        let mut s = ServeurLlama::new(1, temp.path().to_path_buf());
        let erreur = s
            .demarrer(
                Lancement {
                    serveur: temp.path().join("llama-server.exe"),
                    modele: temp.path().join("absent.gguf"),
                    port: 1,
                    contexte: 4096,
                    reflexion: true,
                    vision: None,
                },
                Duration::from_secs(1),
            )
            .unwrap_err();
        assert!(erreur.starts_with("Modèle introuvable"), "{erreur}");
        assert!(!s.tourne());
        assert!(!temp.path().join("logs").exists(), "rien n'est lancé ni écrit");
    }

    #[test]
    fn la_sante_demande_la_cle() {
        let (port, recu) =
            faux_serveur("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        let s = serveur_actif(port, "cle-sante");
        assert!(s.est_pret());
        let requete = recu.recv().unwrap();
        assert!(requete.starts_with("GET /health"), "{requete}");
        assert!(requete.contains("Bearer cle-sante"), "{requete}");
    }
}
