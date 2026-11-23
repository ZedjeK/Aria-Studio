//! Le serveur MCP pour Claude, côté appli (`aria_coeur::mcp`) : le relais `Aria-Studio.exe --mcp`, le canal local ouvert
//! au lancement, et les cinq outils exécutés avec le Plateau et les moteurs de ce lancement, comme l'interface les
//! exécute (mêmes contrôles, filtre AS-18 compris). Réglages → Claude (MCP) donne la commande qui le déclare à Claude.

use std::io::{BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use aria_coeur::mcp::{self, Gestionnaire, NOMBRE_MAX, Session};
use aria_coeur::orchestrateur::{Actif, Orchestrateur};
use aria_coeur::plateau::{Demande, EcritureAnnulable, EtatCommande, Plateau};
use aria_coeur::scenario::{self, DemandeEcriture};
use aria_coeur::{Dossier, carte, creations, journal};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::moteurs::Moteurs;

/// L'argument qui lance le relais.
pub const ARGUMENT: &str = "--mcp";
/// L'idée d'un prompt, au plus (comme le dit l'outil).
const LONGUEUR_IDEE: usize = 2000;

/// Le relais : le protocole MCP sur l'entrée et la sortie standard, jusqu'à ce que Claude ferme l'entrée. Rien ne va
/// sur la sortie standard en dehors des messages ; une entrée ou une sortie coupée (Claude fermé) arrête le relais en
/// silence, sur la sortie d'erreur, sans boîte de dialogue.
pub fn relayer(dossier: &Dossier) {
    let entree = BufReader::new(std::io::stdin().lock());
    let mut sortie = std::io::stdout().lock();
    let resultat = mcp::relayer(dossier, entree, &mut sortie);
    let _ = sortie.flush();
    if let Err(e) = resultat {
        let _ = writeln!(std::io::stderr(), "serveur MCP d'Aria : {e}");
    }
}

/// Ce que les outils utilisent : le Plateau, l'orchestrateur (pour l'écriture du prompt) et son drapeau d'annulation.
#[derive(Clone)]
struct Outils {
    plateau: Arc<Plateau>,
    orchestrateur: Arc<Mutex<Orchestrateur>>,
    annule: Arc<AtomicBool>,
}

/// Ouvre le canal local de ce lancement ; un échec est noté au journal, l'appli continue sans MCP.
pub fn ouvrir(dossier: &Dossier, plateau: &Arc<Plateau>, moteurs: &Moteurs) -> Option<Session> {
    let outils = Outils {
        plateau: Arc::clone(plateau),
        orchestrateur: Arc::clone(&moteurs.orchestrateur),
        annule: Arc::clone(&moteurs.ecriture_annulee),
    };
    let gestionnaire: Gestionnaire =
        Arc::new(move |outil: &str, arguments: &Map<String, Value>| outils.executer(outil, arguments));
    match mcp::ouvrir_canal(dossier, gestionnaire) {
        Ok(session) => Some(session),
        Err(e) => {
            journal::noter(
                dossier,
                "Serveur MCP indisponible",
                "",
                journal::Resultat::Erreur,
                Some(e.as_str()),
            );
            None
        }
    }
}

fn texte(arguments: &Map<String, Value>, cle: &str) -> Option<String> {
    arguments
        .get(cle)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

/// Des octets en Gio, au dixième (quelques dizaines de Gio : la précision de f64 suffit largement).
fn go(octets: Option<u64>) -> Option<f64> {
    octets.map(|o| (o as f64 / f64::from(1u32 << 30) * 10.0).round() / 10.0)
}

impl Outils {
    fn executer(&self, outil: &str, a: &Map<String, Value>) -> Result<Value, String> {
        match outil {
            "etat" => Ok(self.etat()),
            "ecrire_prompt" => self.ecrire_prompt(a),
            "generer_image" => self.generer_image(a),
            "suivre_rendu" => self.suivre_rendu(a),
            "fiche_image" => {
                let id = mcp::identifiant(a.get("image"))?;
                creations::fiche(&self.plateau.contexte().data, &id)
            }
            autre => Err(format!("Outil inconnu : {autre}")),
        }
    }

    fn etat(&self) -> Value {
        let options = self.plateau.options();
        // Le verrou est tenu pendant qu'un moteur travaille : « occupé » plutôt que d'attendre.
        let actif = match self.orchestrateur.try_lock() {
            Ok(o) => match o.actif() {
                Some(Actif::ComfyUI) => "ComfyUI",
                Some(Actif::Llama) => "llama",
                None => "aucun",
            },
            Err(_) => "occupé",
        };
        let en_cours = self
            .plateau
            .commandes()
            .into_iter()
            .find(|c| c.etat == EtatCommande::EnCours)
            .map(|c| json!({"commande": c.id, "type": c.genre, "etape": c.etape}));
        json!({
            "moteur_sur_la_carte": actif,
            "vram_go": {"utilisee": go(carte::vram_utilisee()), "totale": go(carte::vram_totale())},
            "rendu_en_cours": en_cours,
            "modeles_image": options.workflows.iter().map(|w| json!({"id": w.id, "nom": w.nom})).collect::<Vec<_>>(),
            "modele_par_defaut": options.defaut,
            "formats": options.formats,
            "nombre_max": NOMBRE_MAX,
        })
    }

    fn ecrire_prompt(&self, a: &Map<String, Value>) -> Result<Value, String> {
        let idee = texte(a, "idee").ok_or_else(|| "Paramètre manquant : idee".to_owned())?;
        if idee.chars().count() > LONGUEUR_IDEE {
            return Err(format!("idee : {LONGUEUR_IDEE} caractères au plus"));
        }
        let demande = DemandeEcriture {
            atelier: "image".to_owned(),
            idee,
            workflow: texte(a, "modele"),
            ..DemandeEcriture::default()
        };
        // Une nouvelle écriture repart sans annulation, comme au Scénario (✕ Annuler l'arrête aussi).
        self.annule.store(false, Ordering::Relaxed);
        let mut ecriture = EcritureAnnulable {
            orchestrateur: Arc::clone(&self.orchestrateur),
            annule: Arc::clone(&self.annule),
        };
        let ecrit = scenario::ecrire_prompt(self.plateau.contexte(), &mut ecriture, &demande)?;
        Ok(json!({"prompt": ecrit.prompt, "mots": ecrit.mots, "llm": ecrit.llm, "duree_s": ecrit.duree_s}))
    }

    fn generer_image(&self, a: &Map<String, Value>) -> Result<Value, String> {
        let options = self.plateau.options();
        let workflow = texte(a, "modele").unwrap_or_else(|| options.defaut.clone());
        let format = texte(a, "format")
            .or_else(|| {
                options
                    .workflows
                    .iter()
                    .find(|w| w.id == workflow)
                    .and_then(|w| w.format.clone())
            })
            .or_else(|| options.formats.first().cloned())
            .unwrap_or_default();
        let nombre = match a.get("nombre") {
            None | Some(Value::Null) => 1,
            Some(n) => n
                .as_u64()
                .filter(|n| (1..=NOMBRE_MAX).contains(n))
                .ok_or_else(|| format!("nombre : de 1 à {NOMBRE_MAX} images"))?,
        };
        let seed = match a.get("seed") {
            None | Some(Value::Null) => None,
            Some(s) => Some(s.as_u64().ok_or_else(|| "seed : un entier positif".to_owned())?),
        };
        let demande: Demande = serde_json::from_value(json!({
            "prompt": texte(a, "prompt").unwrap_or_default(),
            "workflow": workflow,
            "format": format,
            "seed": seed,
            "nombre": nombre,
            "modele": null,
        }))
        .map_err(|e| format!("Demande illisible : {e}"))?;
        // Les contrôles du Plateau, filtre AS-18 compris.
        let c = self.plateau.commander(&demande)?;
        Ok(
            json!({"commande": c.id, "etat": c.etat, "modele": c.workflow, "format": c.format, "nombre": c.nombre}),
        )
    }

    fn suivre_rendu(&self, a: &Map<String, Value>) -> Result<Value, String> {
        let id = mcp::identifiant(a.get("commande"))?;
        let c = self
            .plateau
            .commandes()
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| {
                format!("Commande inconnue : {id} (les commandes sont celles de ce lancement d'Aria)")
            })?;
        let data = &self.plateau.contexte().data;
        let mut sortie = json!({
            "commande": c.id,
            "etat": c.etat,
            "etape": c.etape,
            "progression": (c.progression * 100.0).round() / 100.0,
        });
        if let Some(objet) = sortie.as_object_mut() {
            if let Some(e) = &c.erreur {
                objet.insert("erreur".to_owned(), json!(e));
            }
            if !c.images.is_empty() {
                let images: Vec<Value> = c
                    .images
                    .iter()
                    .map(|i| {
                        let chemin = creations::chemin_image(data, i)
                            .map(|p| p.display().to_string())
                            .ok();
                        json!({"id": i, "chemin": chemin})
                    })
                    .collect();
                objet.insert("images".to_owned(), Value::Array(images));
            }
        }
        Ok(sortie)
    }
}

/// Ce que Réglages → Claude (MCP) montre : la commande `claude mcp add` et le bloc « mcpServers », avec cet exe.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationMcp {
    pub nom: String,
    pub exe: String,
    pub commande: String,
    pub serveur: Value,
}

/// La déclaration du serveur MCP d'Aria pour Claude Code.
#[tauri::command]
pub fn commande_mcp() -> Result<DeclarationMcp, String> {
    let exe = std::env::current_exe().map_err(|e| format!("chemin de l'exe illisible : {e}"))?;
    let exe = exe.display().to_string();
    Ok(DeclarationMcp {
        nom: mcp::NOM.to_owned(),
        commande: format!(
            "claude mcp add --transport stdio --scope user {} -- \"{exe}\" {ARGUMENT}",
            mcp::NOM
        ),
        serveur: json!({"type": "stdio", "command": exe, "args": [ARGUMENT]}),
        exe,
    })
}
