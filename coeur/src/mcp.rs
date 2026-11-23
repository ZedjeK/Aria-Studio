//! Le serveur MCP d'Aria Studio, comme en v2 (`app/mcp.py`) : Claude (Claude Code, Claude Desktop) pilote Aria par
//! cinq outils, l'état, écrire un prompt, lancer un rendu, le suivre, lire la fiche d'une image. Rien ne supprime, ne
//! déplace, ne change un réglage ni n'installe ; les images ne partent pas vers Claude, seulement leur chemin.
//!
//! Deux morceaux, comme en v2 :
//! - le **relais** : `Aria-Studio.exe --mcp`, que Claude lance lui-même et à qui il parle sur l'entrée et la sortie
//!   standard (transport stdio, un message JSON-RPC 2.0 par ligne ; spécification MCP 2025-11-25). Il n'ouvre aucun
//!   port et n'a aucun secret ;
//! - le **canal local** de l'appli lancée : un port sur 127.0.0.1 seulement, ouvert au démarrage, et un jeton neuf à
//!   chaque lancement, inscrits dans `cache\session.json` (la v2 y mettait le port et le jeton de son API). Le relais
//!   les y lit et transmet chaque appel d'outil ; l'appli l'exécute avec son Plateau et ses moteurs, donc avec les
//!   mêmes contrôles que l'interface, filtre AS-18 compris, et un seul moteur sur la carte.
//!
//! Sans Aria lancée, le relais répond quand même au protocole (poignée de main, liste des outils) et chaque outil dit
//! qu'Aria n'est pas lancée.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::{Dossier, identite};

/// La version du protocole d'Aria, et celles qu'elle accepte (même forme pour les outils).
pub const PROTOCOLE: &str = "2025-11-25";
pub const PROTOCOLES: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// Le nom du serveur, tel que Claude le déclare.
pub const NOM: &str = "aria-studio";
/// Écrire un prompt en réfléchissant prend une à deux minutes ; charger un modèle aussi.
pub const DELAI: Duration = Duration::from_secs(600);
/// Le message de chaque outil quand l'appli n'est pas lancée (ou que son canal ne répond pas).
pub const ARIA_ABSENTE: &str =
    "Aria Studio n'est pas lancée : ouvre-la (raccourci ou Aria-Studio.exe), puis réessaie.";
/// Les images d'un rendu demandé par Claude, au plus.
pub const NOMBRE_MAX: u64 = 4;
/// Une demande sur le canal, au plus (une idée de 2 000 caractères y tient largement).
const TAILLE_MAX: u64 = 64 * 1024;
/// Le fichier du canal, dans `cache\`.
const FICHIER_SESSION: &str = "session.json";

/// Exécute un outil : son nom et ses arguments (déjà contrôlés), ou le message d'erreur rendu à Claude.
pub type Executeur<'a> = dyn FnMut(&str, &Map<String, Value>) -> Result<Value, String> + 'a;
/// Ce que l'appli fait d'un appel d'outil reçu sur le canal.
pub type Gestionnaire = Arc<dyn Fn(&str, &Map<String, Value>) -> Result<Value, String> + Send + Sync>;

/// Les outils, comme en v2.
pub fn outils() -> Value {
    json!([
        {"name": "etat",
         "description": "État d'Aria Studio : moteurs (ComfyUI, llama), VRAM, rendu en cours, modèles d'image prêts \
                         (identifiants à passer à generer_image et ecrire_prompt), modèle par défaut et formats.",
         "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}},
        {"name": "ecrire_prompt",
         "description": "✨ Écrire le prompt : le LLM local d'Aria réécrit une idée (souvent en français) en prompt \
                         anglais pour un modèle d'image, avec les règles de sa fiche. Peut prendre une à deux minutes.",
         "inputSchema": {"type": "object", "additionalProperties": false, "required": ["idee"], "properties": {
             "idee": {"type": "string",
                      "description": "L'idée de l'image, en langage courant (2 000 caractères au plus)."},
             "modele": {"type": "string",
                        "description": "Identifiant du modèle d'image (voir etat). Absent : celui par défaut."}}}},
        {"name": "generer_image",
         "description": "Lance un rendu d'image dans Aria (Plateau) avec ce prompt, tel quel. Rend l'identifiant de \
                         la commande, à suivre avec suivre_rendu. Les images rejoignent les Rushes d'Aria.",
         "inputSchema": {"type": "object", "additionalProperties": false, "required": ["prompt"], "properties": {
             "prompt": {"type": "string",
                        "description": "Le prompt, tel qu'il sera rendu (en anglais, idéalement écrit par \
                                        ecrire_prompt)."},
             "modele": {"type": "string",
                        "description": "Identifiant du modèle d'image (voir etat). Absent : celui par défaut."},
             "format": {"type": "string",
                        "description": "Format exact, tel que etat le liste (ex. « 1:1 (Square) »)."},
             "seed": {"type": "integer", "description": "Seed à rejouer. Absent : tirée au hasard."},
             "nombre": {"type": "integer", "minimum": 1, "maximum": NOMBRE_MAX,
                        "description": "Nombre d'images (1 à 4)."}}}},
        {"name": "suivre_rendu",
         "description": "Avancement d'un rendu lancé par generer_image ; une fois terminé, l'identifiant et le chemin \
                         de chaque image produite.",
         "inputSchema": {"type": "object", "additionalProperties": false, "required": ["commande"], "properties": {
             "commande": {"type": "string", "description": "Identifiant rendu par generer_image."}}}},
        {"name": "fiche_image",
         "description": "La fiche d'une image d'Aria (texte) : idée, prompt, modèle, format, seed, date.",
         "inputSchema": {"type": "object", "additionalProperties": false, "required": ["image"], "properties": {
             "image": {"type": "string", "description": "Identifiant de l'image (rendu par suivre_rendu)."}}}},
    ])
}

/// Le schéma d'un outil connu.
fn schema(nom: &str) -> Option<Value> {
    outils()
        .as_array()?
        .iter()
        .find(|o| o.get("name").and_then(Value::as_str) == Some(nom))
        .and_then(|o| o.get("inputSchema").cloned())
}

/// Le résultat d'un outil : un texte (JSON indenté), marqué en erreur s'il le faut.
fn resultat(contenu: &Value, erreur: bool) -> Value {
    let texte = match contenu {
        Value::String(s) => s.clone(),
        autre => serde_json::to_string_pretty(autre).unwrap_or_else(|_| autre.to_string()),
    };
    json!({"content": [{"type": "text", "text": texte}], "isError": erreur})
}

/// Une valeur « vide » au sens de la v2 (absente, nulle, chaîne vide, zéro, faux) : un paramètre requis manque.
fn vide(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_u64() == Some(0) || n.as_i64() == Some(0),
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Object(o)) => o.is_empty(),
    }
}

/// La réponse à un message JSON-RPC (rien pour une notification).
pub fn traiter(message: &Map<String, Value>, executer: &mut Executeur<'_>) -> Option<Value> {
    let ident = message.get("id").filter(|i| !i.is_null())?.clone();
    let ok = |r: Value| json!({"jsonrpc": "2.0", "id": ident, "result": r});
    let erreur = |code: i64, texte: String| json!({"jsonrpc": "2.0", "id": ident, "error": {"code": code, "message": texte}});
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let methode = message.get("method").and_then(Value::as_str).unwrap_or_default();
    Some(match methode {
        "initialize" => {
            let demande = params.get("protocolVersion").and_then(Value::as_str);
            let version = demande.filter(|d| PROTOCOLES.contains(d)).unwrap_or(PROTOCOLE);
            ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": NOM, "title": identite::NOM, "version": identite::VERSION},
                "instructions": "Aria Studio génère des images en local sur la carte graphique de l'utilisateur. \
                                 Commence par etat ; écris le prompt avec ecrire_prompt, lance generer_image, puis \
                                 suis le rendu avec suivre_rendu.",
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({"tools": outils()})),
        "tools/call" => {
            let nom = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let Some(schema) = schema(nom) else {
                return Some(erreur(-32602, format!("Outil inconnu : {nom}")));
            };
            let arguments = match params.get("arguments") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(a)) => a.clone(),
                Some(_) => return Some(erreur(-32602, "arguments : un objet JSON".to_owned())),
            };
            let requis: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let manque: Vec<&str> = requis.into_iter().filter(|c| vide(arguments.get(*c))).collect();
            if !manque.is_empty() {
                return Some(ok(resultat(
                    &json!(format!("Paramètre manquant : {}", manque.join(", "))),
                    true,
                )));
            }
            let connus = schema.get("properties").and_then(Value::as_object);
            let mut inconnus: Vec<&str> = arguments
                .keys()
                .map(String::as_str)
                .filter(|c| !connus.is_some_and(|p| p.contains_key(*c)))
                .collect();
            inconnus.sort_unstable();
            if !inconnus.is_empty() {
                return Some(ok(resultat(
                    &json!(format!("Paramètre inconnu : {}", inconnus.join(", "))),
                    true,
                )));
            }
            match executer(nom, &arguments) {
                Ok(v) => ok(resultat(&v, false)),
                Err(e) => ok(resultat(&json!(e), true)),
            }
        }
        autre => erreur(-32601, format!("Méthode inconnue : {autre}")),
    })
}

/// Lit un message par ligne jusqu'à la fin de l'entrée ; écrit une réponse par ligne. Rien d'autre ne va sur la sortie
/// standard : les erreurs de l'entrée et de la sortie arrêtent le relais.
///
/// # Errors
/// L'entrée ou la sortie standard ne se lit ou ne s'écrit plus.
pub fn servir(
    entree: impl BufRead,
    sortie: &mut impl Write,
    executer: &mut Executeur<'_>,
) -> std::io::Result<()> {
    for ligne in entree.lines() {
        let ligne = ligne?;
        if ligne.trim().is_empty() {
            continue;
        }
        let reponse = match serde_json::from_str::<Value>(&ligne) {
            Err(_) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "JSON illisible"}}),
            ),
            Ok(Value::Object(message)) => traiter(&message, executer),
            Ok(_) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "Requête invalide"}}),
            ),
        };
        if let Some(r) = reponse {
            writeln!(sortie, "{r}")?;
            sortie.flush()?;
        }
    }
    Ok(())
}

/// Un identifiant d'Aria (lettres et chiffres ASCII, tirets, soulignés) : rien d'autre ne passe.
///
/// # Errors
/// Identifiant vide ou avec un autre caractère.
pub fn identifiant(valeur: Option<&Value>) -> Result<String, String> {
    let texte = match valeur {
        Some(Value::String(s)) => s.clone(),
        Some(autre) => autre.to_string(),
        None => String::new(),
    };
    if !texte.is_empty()
        && texte
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(texte)
    } else {
        Err(format!("Identifiant invalide : {texte:?}"))
    }
}

// --- le canal local de l'appli ---

/// Le port et le jeton du canal de ce lancement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub port: u16,
    pub jeton: String,
}

/// `cache\session.json`.
pub fn fichier_session(dossier: &Dossier) -> PathBuf {
    dossier.cache().join(FICHIER_SESSION)
}

/// La session inscrite par l'appli lancée.
///
/// # Errors
/// [`ARIA_ABSENTE`] : pas de fichier, ou illisible.
pub fn lire_session(dossier: &Dossier) -> Result<Session, String> {
    let texte = std::fs::read_to_string(fichier_session(dossier)).map_err(|_| ARIA_ABSENTE.to_owned())?;
    let v: Value = serde_json::from_str(&texte).map_err(|_| ARIA_ABSENTE.to_owned())?;
    let port = v
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0);
    let jeton = v.get("jeton").and_then(Value::as_str).filter(|j| !j.is_empty());
    match (port, jeton) {
        (Some(port), Some(jeton)) => Ok(Session {
            port,
            jeton: jeton.to_owned(),
        }),
        _ => Err(ARIA_ABSENTE.to_owned()),
    }
}

fn ecrire_session(dossier: &Dossier, session: &Session) -> Result<(), String> {
    let fichier = fichier_session(dossier);
    if let Some(parent) = fichier.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{} : {e}", parent.display()))?;
    }
    let temporaire = fichier.with_extension("tmp");
    let texte = json!({"port": session.port, "jeton": session.jeton}).to_string();
    std::fs::write(&temporaire, texte)
        .and_then(|()| std::fs::rename(&temporaire, &fichier))
        .map_err(|e| format!("{} : {e}", fichier.display()))
}

/// Un jeton neuf : 32 octets de l'aléa du système, en hexadécimal.
///
/// # Errors
/// Aléa du système indisponible.
pub fn nouveau_jeton() -> Result<String, String> {
    let mut octets = [0u8; 32];
    getrandom::fill(&mut octets).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    Ok(octets.iter().map(|o| format!("{o:02x}")).collect())
}

/// Deux jetons comparés en temps constant (pour leur longueur).
fn memes(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Une ligne lue sur le canal, bornée à [`TAILLE_MAX`].
fn lire_ligne(flux: &TcpStream) -> std::io::Result<String> {
    let mut ligne = String::new();
    BufReader::new(flux.take(TAILLE_MAX)).read_line(&mut ligne)?;
    Ok(ligne)
}

/// Répond à une demande du canal : jeton contrôlé, puis l'outil exécuté par l'appli.
fn repondre(demande: &str, jeton: &str, gestionnaire: &Gestionnaire) -> Value {
    let Ok(Value::Object(d)) = serde_json::from_str::<Value>(demande) else {
        return json!({"erreur": "Demande illisible"});
    };
    if !memes(d.get("jeton").and_then(Value::as_str).unwrap_or_default(), jeton) {
        return json!({"erreur": "Jeton refusé : relance Claude après avoir relancé Aria"});
    }
    let outil = d.get("outil").and_then(Value::as_str).unwrap_or_default();
    let arguments = d
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    match gestionnaire(outil, &arguments) {
        Ok(v) => json!({"resultat": v}),
        Err(e) => json!({"erreur": e}),
    }
}

fn servir_connexion(flux: &mut TcpStream, jeton: &str, gestionnaire: &Gestionnaire) {
    let _ = flux.set_read_timeout(Some(Duration::from_secs(30)));
    let Ok(ligne) = lire_ligne(flux) else { return };
    let reponse = repondre(&ligne, jeton, gestionnaire);
    let _ = writeln!(flux, "{reponse}");
    let _ = flux.flush();
}

/// Ouvre le canal de ce lancement : un port de 127.0.0.1 choisi par le système, un jeton neuf, inscrits dans
/// `cache\session.json` ; chaque connexion est servie à part (un prompt qui s'écrit n'empêche pas de suivre un rendu).
///
/// # Errors
/// Le port ne s'ouvre pas, l'aléa manque ou `session.json` ne s'écrit pas.
pub fn ouvrir_canal(dossier: &Dossier, gestionnaire: Gestionnaire) -> Result<Session, String> {
    let ecoute = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .map_err(|e| format!("canal MCP : {e}"))?;
    let port = ecoute
        .local_addr()
        .map_err(|e| format!("canal MCP : {e}"))?
        .port();
    let session = Session {
        port,
        jeton: nouveau_jeton()?,
    };
    ecrire_session(dossier, &session)?;
    let jeton = session.jeton.clone();
    std::thread::Builder::new()
        .name("canal-mcp".to_owned())
        .spawn(move || {
            for flux in ecoute.incoming().flatten() {
                let jeton = jeton.clone();
                let gestionnaire = Arc::clone(&gestionnaire);
                let _ = std::thread::Builder::new()
                    .name("appel-mcp".to_owned())
                    .spawn(move || {
                        let mut flux = flux;
                        servir_connexion(&mut flux, &jeton, &gestionnaire);
                    });
            }
        })
        .map_err(|e| format!("canal MCP : {e}"))?;
    Ok(session)
}

/// Ferme le canal à la sortie de l'appli : `session.json` s'efface s'il est encore celui de ce lancement.
pub fn fermer_canal(dossier: &Dossier, session: &Session) {
    if lire_session(dossier).is_ok_and(|s| s == *session) {
        let _ = std::fs::remove_file(fichier_session(dossier));
    }
}

/// Transmet un appel d'outil à l'appli lancée, et rend sa réponse.
///
/// # Errors
/// [`ARIA_ABSENTE`] si l'appli ne répond pas ; sinon le message de l'appli, tel quel.
pub fn appeler(session: &Session, outil: &str, arguments: &Map<String, Value>) -> Result<Value, String> {
    let adresse = SocketAddr::from((Ipv4Addr::LOCALHOST, session.port));
    let mut flux =
        TcpStream::connect_timeout(&adresse, Duration::from_secs(3)).map_err(|_| ARIA_ABSENTE.to_owned())?;
    flux.set_read_timeout(Some(DELAI))
        .map_err(|_| ARIA_ABSENTE.to_owned())?;
    let demande = json!({"jeton": session.jeton, "outil": outil, "arguments": arguments});
    writeln!(flux, "{demande}").map_err(|_| ARIA_ABSENTE.to_owned())?;
    flux.flush().map_err(|_| ARIA_ABSENTE.to_owned())?;
    let mut ligne = String::new();
    BufReader::new(&flux)
        .read_line(&mut ligne)
        .map_err(|_| ARIA_ABSENTE.to_owned())?;
    match serde_json::from_str::<Value>(&ligne) {
        Ok(Value::Object(mut r)) => match (r.remove("resultat"), r.remove("erreur")) {
            (_, Some(e)) => Err(e.as_str().map_or_else(|| e.to_string(), str::to_owned)),
            (Some(v), None) => Ok(v),
            (None, None) => Err(ARIA_ABSENTE.to_owned()),
        },
        _ => Err(ARIA_ABSENTE.to_owned()),
    }
}

/// Le relais `Aria-Studio.exe --mcp` : le protocole sur l'entrée et la sortie standard, chaque outil transmis à
/// l'appli lancée (la session est relue à chaque appel : Aria a pu être relancée entre-temps).
///
/// # Errors
/// L'entrée ou la sortie standard ne se lit ou ne s'écrit plus.
pub fn relayer(dossier: &Dossier, entree: impl BufRead, sortie: &mut impl Write) -> std::io::Result<()> {
    let mut executer = |outil: &str, arguments: &Map<String, Value>| {
        let session = lire_session(dossier)?;
        appeler(&session, outil, arguments)
    };
    servir(entree, sortie, &mut executer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echanger(lignes: &[&str], executer: &mut Executeur<'_>) -> Vec<Value> {
        let entree = lignes.join("\n");
        let mut sortie = Vec::new();
        servir(entree.as_bytes(), &mut sortie, executer).unwrap();
        String::from_utf8(sortie)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn rien(_: &str, _: &Map<String, Value>) -> Result<Value, String> {
        Err("rien".to_owned())
    }

    #[test]
    fn poignee_de_main() {
        let r = echanger(
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
            ],
            &mut rien,
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            r[0]["result"]["capabilities"],
            json!({"tools": {"listChanged": false}})
        );
        assert_eq!(r[0]["result"]["serverInfo"]["name"], NOM);
        assert_eq!(r[1], json!({"jsonrpc": "2.0", "id": 2, "result": {}}));
        let inconnue = echanger(
            &[r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01"}}"#],
            &mut rien,
        );
        assert_eq!(inconnue[0]["result"]["protocolVersion"], PROTOCOLE);
    }

    #[test]
    fn outils_listes() {
        let r = echanger(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#], &mut rien);
        let noms: Vec<&str> = r[0]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            noms,
            [
                "etat",
                "ecrire_prompt",
                "generer_image",
                "suivre_rendu",
                "fiche_image"
            ]
        );
        for o in r[0]["result"]["tools"].as_array().unwrap() {
            assert_eq!(o["inputSchema"]["type"], "object");
            assert!(!o["description"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn erreurs_du_protocole() {
        let r = echanger(
            &[
                "pas du json",
                "[1, 2]",
                r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#,
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"supprimer"}}"#,
                r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"etat","arguments":[1]}}"#,
            ],
            &mut rien,
        );
        let codes: Vec<i64> = r.iter().map(|x| x["error"]["code"].as_i64().unwrap()).collect();
        assert_eq!(codes, [-32700, -32600, -32601, -32602, -32602]);
        assert!(
            r[3]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Outil inconnu : supprimer")
        );
    }

    #[test]
    fn la_sortie_n_a_que_des_messages() {
        let mut sortie = Vec::new();
        let entree = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n";
        servir(entree.as_bytes(), &mut sortie, &mut rien).unwrap();
        // Une réponse par ligne, chacune un message JSON-RPC complet, et rien d'autre.
        let texte = String::from_utf8(sortie).unwrap();
        assert!(texte.ends_with('\n'));
        let lignes: Vec<Value> = texte.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(
            lignes,
            [
                json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
                json!({"jsonrpc": "2.0", "id": 2, "result": {}})
            ]
        );
    }

    #[test]
    fn appel_d_outil_controle() {
        let mut appels = Vec::new();
        let mut executer = |nom: &str, a: &Map<String, Value>| {
            appels.push((nom.to_owned(), a.clone()));
            if nom == "fiche_image" {
                Err("Création inconnue".to_owned())
            } else {
                Ok(json!({"ok": true}))
            }
        };
        let r = echanger(
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ecrire_prompt","arguments":{}}}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"etat","arguments":{"x":1}}}"#,
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"etat"}}"#,
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fiche_image","arguments":{"image":"a"}}}"#,
            ],
            &mut executer,
        );
        assert_eq!(r[0]["result"]["isError"], json!(true));
        assert_eq!(r[0]["result"]["content"][0]["text"], "Paramètre manquant : idee");
        assert_eq!(r[1]["result"]["content"][0]["text"], "Paramètre inconnu : x");
        assert_eq!(r[2]["result"]["isError"], json!(false));
        assert!(
            r[2]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("\"ok\": true")
        );
        assert_eq!(r[3]["result"]["isError"], json!(true));
        assert_eq!(r[3]["result"]["content"][0]["text"], "Création inconnue");
        // Seuls les appels bien formés atteignent l'appli.
        assert_eq!(appels.len(), 2);
    }

    #[test]
    fn identifiants() {
        assert_eq!(
            identifiant(Some(&json!("20261122-101500-abcdef"))).unwrap(),
            "20261122-101500-abcdef"
        );
        assert!(identifiant(Some(&json!("../x"))).is_err());
        assert!(identifiant(Some(&json!(""))).is_err());
        assert!(identifiant(None).is_err());
        assert!(identifiant(Some(&json!("a b"))).is_err());
    }

    #[test]
    fn canal_local_avec_jeton() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        // Sans Aria : chaque outil le dit.
        assert_eq!(lire_session(&dossier).unwrap_err(), ARIA_ABSENTE);
        let gestionnaire: Gestionnaire =
            Arc::new(|outil: &str, arguments: &Map<String, Value>| match outil {
                "etat" => Ok(json!({"outil": outil, "arguments": arguments.len()})),
                _ => Err(format!("refusé : {outil}")),
            });
        let session = ouvrir_canal(&dossier, gestionnaire).unwrap();
        assert_eq!(lire_session(&dossier).unwrap(), session);
        assert_eq!(session.jeton.len(), 64);
        assert_eq!(
            appeler(&session, "etat", &Map::new()).unwrap(),
            json!({"outil": "etat", "arguments": 0})
        );
        assert_eq!(
            appeler(&session, "autre", &Map::new()).unwrap_err(),
            "refusé : autre"
        );
        // Un autre jeton est refusé.
        let faux = Session {
            port: session.port,
            jeton: "0".repeat(64),
        };
        assert!(
            appeler(&faux, "etat", &Map::new())
                .unwrap_err()
                .starts_with("Jeton refusé")
        );
        // Le relais de bout en bout.
        let mut sortie = Vec::new();
        relayer(
            &dossier,
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"etat"}}"#.as_bytes(),
            &mut sortie,
        )
        .unwrap();
        let r: Value = serde_json::from_slice(&sortie).unwrap();
        assert_eq!(r["result"]["isError"], json!(false));
        // Fermé : le fichier s'efface, l'outil dit qu'Aria n'est pas lancée.
        fermer_canal(&dossier, &session);
        assert!(!fichier_session(&dossier).exists());
        let mut sortie = Vec::new();
        relayer(
            &dossier,
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"etat"}}"#.as_bytes(),
            &mut sortie,
        )
        .unwrap();
        let r: Value = serde_json::from_slice(&sortie).unwrap();
        assert_eq!(r["result"]["content"][0]["text"], ARIA_ABSENTE);
        assert!(memes("abc", "abc") && !memes("abc", "abd") && !memes("abc", "ab"));
    }
}
