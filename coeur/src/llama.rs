//! Le pilote de llama-server, ce qui se teste sans le lancer, comme en v2 (`app/llm.py`) : arguments de lancement,
//! clé tirée à chaque lancement, corps des demandes, lecture de la réponse en flux et messages d'erreur. Le processus
//! et HTTP sont dans l'appli.
//!
//! - llama-server écoute sur `127.0.0.1` seulement, et exige une clé neuve à chaque lancement. Elle passe par la
//!   variable d'environnement `LLAMA_API_KEY`, jamais sur la ligne de commande ni dans un fichier.
//! - Ni `--n-gpu-layers`, ni `--api-key`, ni `--api-key-file`.
//! - Avec la réflexion, la longueur maximale de la réponse reçoit en plus le budget de réflexion ; sans elle,
//!   `--reasoning-budget 0` au lancement et `enable_thinking: false` à chaque demande.

use std::ffi::OsString;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Les tokens qu'une réponse peut passer à réfléchir, en plus de sa longueur maximale.
pub const BUDGET_REFLEXION: u32 = 8192;
/// Le contexte au moins, quand la réflexion ou la vision est active.
pub const CONTEXTE_REFLEXION: u32 = 16384;
/// La variable d'environnement qui porte la clé.
pub const VARIABLE_CLE: &str = "LLAMA_API_KEY";

/// De quoi lancer llama-server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lancement {
    pub serveur: PathBuf,
    pub modele: PathBuf,
    pub port: u16,
    pub contexte: u32,
    pub reflexion: bool,
    /// Le projecteur d'images (`--mmproj`), pour un LLM qui voit.
    pub vision: Option<PathBuf>,
}

impl Lancement {
    /// Les arguments de llama-server, après avoir vérifié que les fichiers existent.
    ///
    /// # Errors
    /// Le message à montrer si le modèle, le serveur ou le projecteur manque.
    pub fn arguments(&self) -> Result<Vec<OsString>, String> {
        if !self.modele.is_file() {
            return Err(format!("Modèle introuvable : {}", self.modele.display()));
        }
        if !self.serveur.is_file() {
            return Err(format!("Serveur introuvable : {}", self.serveur.display()));
        }
        let mut arguments: Vec<OsString> = vec![
            "-m".into(),
            self.modele.clone().into_os_string(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            self.port.to_string().into(),
            "--ctx-size".into(),
            self.contexte.to_string().into(),
        ];
        if !self.reflexion {
            arguments.extend(["--reasoning-budget".into(), "0".into()]);
        }
        if let Some(vision) = &self.vision {
            if !vision.is_file() {
                return Err(format!("Projecteur d'images introuvable : {}", vision.display()));
            }
            arguments.extend(["--mmproj".into(), vision.clone().into_os_string()]);
        }
        Ok(arguments)
    }

    /// Le dossier d'où lancer llama-server : le sien (ses DLL CUDA sont à côté de lui).
    pub fn dossier(&self) -> Option<&Path> {
        self.serveur.parent()
    }
}

/// Une clé neuve : 32 octets aléatoires du système, en base64 « URL » sans remplissage (43 caractères).
///
/// # Errors
/// Le système ne donne pas d'aléa.
pub fn nouvelle_cle() -> Result<String, String> {
    let mut octets = [0u8; 32];
    getrandom::fill(&mut octets).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    Ok(base64_url(&octets))
}

fn base64_url(octets: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let caractere = |index: u32| {
        char::from(
            ALPHABET
                .get(usize::try_from(index & 63).unwrap_or(0))
                .copied()
                .unwrap_or(b'A'),
        )
    };
    let mut sortie = String::new();
    for morceau in octets.chunks(3) {
        let mut bloc = [0u8; 3];
        for (case, octet) in bloc.iter_mut().zip(morceau) {
            *case = *octet;
        }
        let n = (u32::from(bloc[0]) << 16) | (u32::from(bloc[1]) << 8) | u32::from(bloc[2]);
        let utiles = morceau.len() + 1;
        for decalage in [18u32, 12, 6, 0].iter().take(utiles) {
            sortie.push(caractere(n >> decalage));
        }
    }
    sortie
}

/// Une demande de génération.
#[derive(Debug, Clone, PartialEq)]
pub struct Demande {
    /// Les messages, au format de l'API OpenAI de llama-server.
    pub messages: Value,
    pub temperature: f64,
    /// La longueur maximale de la réponse, sans la réflexion.
    pub max_tokens: u32,
    pub seed: Option<u64>,
    /// Ce que la réflexion peut prendre en plus (0 pour un spécialiste qui le compte déjà).
    pub budget_reflexion: u32,
    /// Laisser réfléchir le modèle, s'il a été lancé avec la réflexion.
    pub reflechir: bool,
    /// Le reste de l'échantillonnage : `top_p`, `top_k`, `min_p`, `presence_penalty`…
    pub echantillonnage: Map<String, Value>,
}

impl Default for Demande {
    fn default() -> Self {
        Self {
            messages: Value::Array(Vec::new()),
            temperature: 0.7,
            max_tokens: 512,
            seed: None,
            budget_reflexion: BUDGET_REFLEXION,
            reflechir: true,
            echantillonnage: Map::new(),
        }
    }
}

/// Ce qu'il faut envoyer à `/v1/chat/completions`.
#[derive(Debug, Clone, PartialEq)]
pub struct Envoi {
    pub corps: Value,
    /// La longueur maximale demandée, budget de réflexion compris.
    pub max_tokens: u32,
    /// Le modèle réfléchit-il pour cette demande ?
    pub reflexion: bool,
}

impl Demande {
    /// Le corps de la demande, en flux, avec l'usage à la fin. `reflexion_active` : le modèle a été lancé avec la
    /// réflexion. Son budget s'ajoute alors à la longueur maximale, même si cette demande coupe la réflexion.
    pub fn envoi(&self, reflexion_active: bool) -> Envoi {
        let reflexion = reflexion_active && self.reflechir;
        let max_tokens = if reflexion_active {
            self.max_tokens.saturating_add(self.budget_reflexion)
        } else {
            self.max_tokens
        };
        let mut corps = Map::new();
        corps.insert("messages".to_owned(), self.messages.clone());
        corps.insert("temperature".to_owned(), json!(self.temperature));
        corps.insert("max_tokens".to_owned(), json!(max_tokens));
        for (cle, valeur) in &self.echantillonnage {
            corps.insert(cle.clone(), valeur.clone());
        }
        corps.insert("stream".to_owned(), Value::Bool(true));
        corps.insert("stream_options".to_owned(), json!({ "include_usage": true }));
        if let Some(seed) = self.seed {
            corps.insert("seed".to_owned(), json!(seed));
        }
        if !reflexion {
            corps.insert(
                "chat_template_kwargs".to_owned(),
                json!({ "enable_thinking": false }),
            );
        }
        Envoi {
            corps: Value::Object(corps),
            max_tokens,
            reflexion,
        }
    }
}

/// Le message d'une réponse HTTP autre que 200. `contexte` : celui avec lequel llama-server a été lancé.
pub fn erreur_http(statut: u16, corps: &str, contexte: u32) -> String {
    let bas = corps.to_lowercase();
    if statut == 400 && (bas.contains("exceed") || bas.contains("too large") || bas.contains("context")) {
        return format!(
            "Le contexte du LLM ({contexte} tokens) est trop petit pour cette demande (consigne, images, \
             conversation et réflexion comptent) : augmente-le dans Réglages, ou coupe la Réflexion"
        );
    }
    format!("Génération refusée ({statut}) : {}", debut(corps, 300))
}

/// Ce que le LLM a répondu.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reponse {
    pub texte: String,
    pub tokens_entree: u64,
    pub tokens_sortie: u64,
    /// Tokens lus par seconde.
    pub lecture_tps: f64,
    /// Tokens écrits par seconde.
    pub ecriture_tps: f64,
}

/// Lit la réponse en flux (événements `data: {…}`) jusqu'à `data: [DONE]` ou la fin. Le raisonnement
/// (`reasoning_content`) est ignoré ; une réponse vide n'est jamais rendue en silence.
///
/// # Errors
/// Une ligne `error:`, une lecture qui échoue, ou une réponse vide (avec ce qu'il faut faire).
pub fn lire_flux(flux: impl BufRead, envoi: &Envoi) -> Result<Reponse, String> {
    let mut texte = String::new();
    let mut fin: Option<String> = None;
    let mut usage: Option<Value> = None;
    let mut timings: Option<Value> = None;
    for ligne in flux.lines() {
        let ligne = ligne.map_err(|e| format!("Génération interrompue : {e}"))?;
        let ligne = ligne.trim();
        if let Some(reste) = ligne.strip_prefix("error:") {
            return Err(format!("Génération interrompue : {}", debut(reste.trim(), 300)));
        }
        let Some(donnee) = ligne.strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if donnee == "[DONE]" {
            break;
        }
        let Ok(morceau) = serde_json::from_str::<Value>(donnee) else {
            continue;
        };
        for choix in morceau
            .get("choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(contenu) = choix.pointer("/delta/content").and_then(Value::as_str) {
                texte.push_str(contenu);
            }
            if let Some(raison) = choix.get("finish_reason").and_then(Value::as_str) {
                fin = Some(raison.to_owned());
            }
        }
        if morceau.get("usage").is_some_and(|u| !u.is_null()) {
            usage = morceau.get("usage").cloned();
            timings = morceau.get("timings").cloned();
        }
    }
    let texte = texte.trim().to_owned();
    if texte.is_empty() {
        return Err(if fin.as_deref() != Some("length") {
            "Le LLM a rendu une réponse vide".to_owned()
        } else if envoi.reflexion {
            format!(
                "Le LLM a passé ses {} tokens à réfléchir sans répondre : coupe sa Réflexion ou augmente sa \
                 longueur maximale dans Réglages",
                envoi.max_tokens
            )
        } else {
            format!(
                "Le LLM a atteint sa limite de {} tokens sans répondre : augmente sa longueur maximale dans \
                 Réglages",
                envoi.max_tokens
            )
        });
    }
    let nombre =
        |source: &Option<Value>, cle: &str| source.as_ref().and_then(|v| v.get(cle)).and_then(Value::as_u64);
    let vitesse = |cle: &str| {
        timings
            .as_ref()
            .and_then(|t| t.get(cle))
            .and_then(Value::as_f64)
            .map_or(0.0, |v| (v * 10.0).round() / 10.0)
    };
    Ok(Reponse {
        texte,
        tokens_entree: nombre(&usage, "prompt_tokens").unwrap_or(0),
        tokens_sortie: nombre(&usage, "completion_tokens").unwrap_or(0),
        lecture_tps: vitesse("prompt_per_second"),
        ecriture_tps: vitesse("predicted_per_second"),
    })
}

/// Les `n` premiers caractères d'un texte.
fn debut(texte: &str, n: usize) -> String {
    texte.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    fn lancement(dossier: &Path) -> Lancement {
        let serveur = dossier.join("llama").join("llama-server.exe");
        let modele = dossier.join("modeles").join("llm").join("bonsai.gguf");
        for f in [&serveur, &modele] {
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, "x").unwrap();
        }
        Lancement {
            serveur,
            modele,
            port: 8095,
            contexte: 4096,
            reflexion: true,
            vision: None,
        }
    }

    fn textes(arguments: &[OsString]) -> Vec<String> {
        arguments
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn les_arguments_sont_ceux_de_la_v2_sans_cle_ni_gpu_layers() {
        let temp = tempfile::tempdir().unwrap();
        let l = lancement(temp.path());
        let a = textes(&l.arguments().unwrap());
        assert_eq!(
            a[1..],
            [
                l.modele.to_string_lossy().into_owned(),
                "--host".into(),
                "127.0.0.1".into(),
                "--port".into(),
                "8095".into(),
                "--ctx-size".into(),
                "4096".into()
            ]
        );
        assert_eq!(a[0], "-m");
        for interdit in [
            "--api-key",
            "--api-key-file",
            "--n-gpu-layers",
            "--reasoning-budget",
        ] {
            assert!(!a.iter().any(|x| x == interdit), "{interdit}");
        }
        assert_eq!(l.dossier(), l.serveur.parent());
    }

    #[test]
    fn sans_reflexion_le_budget_de_raisonnement_est_nul() {
        let temp = tempfile::tempdir().unwrap();
        let l = Lancement {
            reflexion: false,
            ..lancement(temp.path())
        };
        let a = textes(&l.arguments().unwrap());
        assert!(a.windows(2).any(|w| w == ["--reasoning-budget", "0"]));
    }

    #[test]
    fn la_vision_ajoute_le_projecteur_s_il_existe() {
        let temp = tempfile::tempdir().unwrap();
        let mut l = lancement(temp.path());
        let projecteur = temp.path().join("mmproj.gguf");
        l.vision = Some(projecteur.clone());
        assert!(
            l.arguments()
                .unwrap_err()
                .contains("Projecteur d'images introuvable")
        );
        fs::write(&projecteur, "x").unwrap();
        let a = textes(&l.arguments().unwrap());
        assert!(
            a.windows(2)
                .any(|w| w[0] == "--mmproj" && w[1] == projecteur.to_string_lossy())
        );
    }

    #[test]
    fn un_fichier_absent_est_dit() {
        let temp = tempfile::tempdir().unwrap();
        let mut l = lancement(temp.path());
        l.modele = temp.path().join("absent.gguf");
        assert!(l.arguments().unwrap_err().starts_with("Modèle introuvable"));
        let mut l = lancement(temp.path());
        l.serveur = temp.path().join("absent.exe");
        assert!(l.arguments().unwrap_err().starts_with("Serveur introuvable"));
    }

    #[test]
    fn une_cle_neuve_a_chaque_fois_de_43_caracteres_url() {
        let a = nouvelle_cle().unwrap();
        let b = nouvelle_cle().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{a}"
        );
    }

    #[test]
    fn le_base64_url_est_le_bon() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(b"foob"), "Zm9vYg");
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn sans_reflexion_la_limite_reste_celle_de_la_reponse() {
        let envoi = Demande::default().envoi(false);
        assert_eq!(envoi.max_tokens, 512);
        assert!(!envoi.reflexion);
        assert_eq!(envoi.corps["max_tokens"], 512);
        assert_eq!(envoi.corps["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(envoi.corps["stream"], true);
        assert_eq!(envoi.corps["stream_options"]["include_usage"], true);
    }

    #[test]
    fn la_reflexion_a_son_propre_budget() {
        let envoi = Demande::default().envoi(true);
        assert_eq!(envoi.max_tokens, 512 + BUDGET_REFLEXION);
        assert!(envoi.reflexion);
        assert!(envoi.corps.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn reflexion_coupee_pour_une_demande_garde_le_budget() {
        let demande = Demande {
            reflechir: false,
            ..Demande::default()
        };
        let envoi = demande.envoi(true);
        assert_eq!(envoi.max_tokens, 512 + BUDGET_REFLEXION);
        assert_eq!(envoi.corps["chat_template_kwargs"]["enable_thinking"], false);
    }

    #[test]
    fn un_specialiste_compte_deja_sa_reflexion() {
        let demande = Demande {
            budget_reflexion: 0,
            ..Demande::default()
        };
        assert_eq!(demande.envoi(true).max_tokens, 512);
    }

    #[test]
    fn le_contexte_tient_la_reflexion() {
        const { assert!(CONTEXTE_REFLEXION >= BUDGET_REFLEXION + 4096) };
    }

    #[test]
    fn l_echantillonnage_et_la_graine_passent() {
        let mut echantillonnage = Map::new();
        echantillonnage.insert("top_k".to_owned(), json!(20));
        let demande = Demande {
            seed: Some(42),
            echantillonnage,
            ..Demande::default()
        };
        let corps = demande.envoi(false).corps;
        assert_eq!(corps["top_k"], 20);
        assert_eq!(corps["seed"], 42);
    }

    #[test]
    fn un_contexte_trop_petit_dit_quoi_faire() {
        let m = erreur_http(
            400,
            r#"{"error": "the request exceeds the available context size"}"#,
            4096,
        );
        assert!(
            m.contains("contexte du LLM (4096 tokens)") && m.contains("Réglages"),
            "{m}"
        );
        let m = erreur_http(500, &"x".repeat(400), 4096);
        assert!(m.starts_with("Génération refusée (500) : "));
        assert_eq!(m.chars().filter(|c| *c == 'x').count(), 300);
    }

    fn flux(lignes: &[&str]) -> Cursor<Vec<u8>> {
        Cursor::new(lignes.join("\n").into_bytes())
    }

    #[test]
    fn les_morceaux_sont_recolles_sans_le_raisonnement() {
        let envoi = Demande::default().envoi(false);
        let r = lire_flux(
            flux(&[
                r#"data: {"choices": [{"delta": {"reasoning_content": "je pense"}}]}"#,
                "",
                r#"data: {"choices": [{"delta": {"content": "  Bon"}}]}"#,
                r#"data: {"choices": [{"delta": {"content": "jour  "}, "finish_reason": "stop"}]}"#,
                r#"data: {"choices": [], "usage": {"prompt_tokens": 700, "completion_tokens": 512}, "timings": {"prompt_per_second": 61.04, "predicted_per_second": 12.36}}"#,
                "data: [DONE]",
            ]),
            &envoi,
        )
        .unwrap();
        assert_eq!(r.texte, "Bonjour");
        assert_eq!((r.tokens_entree, r.tokens_sortie), (700, 512));
        assert!((r.lecture_tps - 61.0).abs() < 1e-9, "{}", r.lecture_tps);
        assert!((r.ecriture_tps - 12.4).abs() < 1e-9, "{}", r.ecriture_tps);
    }

    #[test]
    fn une_reponse_vide_n_est_jamais_rendue_en_silence() {
        let fin_longueur = [r#"data: {"choices": [{"delta": {"content": ""}, "finish_reason": "length"}]}"#];
        let avec = Demande::default().envoi(true);
        let e = lire_flux(flux(&fin_longueur), &avec).unwrap_err();
        assert!(
            e.contains("à réfléchir") && e.contains(&avec.max_tokens.to_string()),
            "{e}"
        );
        let sans = Demande::default().envoi(false);
        let e = lire_flux(flux(&fin_longueur), &sans).unwrap_err();
        assert!(e.contains("limite de 512 tokens"), "{e}");
        let e = lire_flux(flux(&["data: [DONE]"]), &sans).unwrap_err();
        assert_eq!(e, "Le LLM a rendu une réponse vide");
    }

    #[test]
    fn une_ligne_d_erreur_interrompt() {
        let envoi = Demande::default().envoi(false);
        let e = lire_flux(flux(&["error: plus de mémoire"]), &envoi).unwrap_err();
        assert_eq!(e, "Génération interrompue : plus de mémoire");
    }
}
