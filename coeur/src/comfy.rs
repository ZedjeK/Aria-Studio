//! Le pilote de ComfyUI, ce qui se teste sans le lancer, comme en v2 (`app/comfy.py`) : arguments de lancement,
//! dossiers de travail, préfixes uniques des workflows, lecture des événements et de l'historique, effacement des
//! sorties rapatriées, processus orphelins à adopter, stabilité de la VRAM. Le processus, HTTP et le WebSocket
//! sont dans l'appli.
//!
//! - ComfyUI écoute sur `127.0.0.1` seulement, sans `--enable-cors-header` : il garde son refus des autres sites
//!   du navigateur.
//! - Ses entrées, sorties et temporaires vont dans `cache\comfyui\` d'Aria : entrées et temporaires vidés avant
//!   chaque lancement, une sortie effacée dès qu'Aria l'a récupérée.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::installation::meme_dossier;

/// Le dossier de travail de ComfyUI, dans le cache d'Aria.
pub const DOSSIER_TRAVAIL: &str = "comfyui";
/// La VRAM est stable quand les trois dernières lectures ne s'écartent pas de plus de ceci.
pub const ECART_VRAM_STABLE: u64 = 64 * 1024 * 1024;

/// De quoi lancer un ComfyUI portable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lancement {
    /// Le dossier du ComfyUI portable (celui qui contient `python_embeded\` et `ComfyUI\`).
    pub dossier: PathBuf,
    pub port: u16,
    /// Le fichier des chemins des modèles (`--extra-model-paths-config`).
    pub chemins_modeles: PathBuf,
    /// `cache\comfyui\` d'Aria.
    pub travail: PathBuf,
}

impl Lancement {
    /// Le Python livré avec ComfyUI portable.
    pub fn python(&self) -> PathBuf {
        self.dossier.join("python_embeded").join("python.exe")
    }

    /// L'adresse de ComfyUI.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Les arguments du Python, à lancer depuis [`Lancement::dossier`].
    pub fn arguments(&self) -> Vec<OsString> {
        let chemin = |p: PathBuf| p.into_os_string();
        vec![
            "-s".into(),
            "ComfyUI/main.py".into(),
            "--listen".into(),
            "127.0.0.1".into(),
            "--port".into(),
            self.port.to_string().into(),
            "--disable-auto-launch".into(),
            "--extra-model-paths-config".into(),
            chemin(self.chemins_modeles.clone()),
            "--input-directory".into(),
            chemin(self.travail.join("input")),
            "--output-directory".into(),
            chemin(self.travail.join("output")),
            "--temp-directory".into(),
            chemin(self.travail.clone()),
        ]
    }

    /// Avant un lancement : entrées et temporaires vidés, les trois dossiers créés. Les sorties ne sont jamais
    /// vidées.
    ///
    /// # Errors
    /// Un dossier qui ne se crée pas.
    pub fn preparer_travail(&self) -> io::Result<()> {
        for vide in ["input", "temp"] {
            let dossier = self.travail.join(vide);
            if dossier.exists() {
                // Un fichier encore ouvert reste : le lancement ne doit pas échouer pour ça.
                let _ = fs::remove_dir_all(&dossier);
            }
        }
        for dossier in ["input", "output", "temp"] {
            fs::create_dir_all(self.travail.join(dossier))?;
        }
        Ok(())
    }
}

/// Un jeton de 8 caractères hexadécimaux, neuf à chaque envoi.
///
/// # Errors
/// Le système ne donne pas d'aléa.
pub fn nouveau_jeton() -> Result<String, String> {
    let mut octets = [0u8; 4];
    getrandom::fill(&mut octets).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    Ok(octets.iter().map(|o| format!("{o:02x}")).collect())
}

/// Une copie du workflow où chaque `filename_prefix` qui est du texte reçoit `_<jeton>` à la fin. Une seed rejouée
/// renverrait sinon le même workflow, que ComfyUI sert de son cache avec des fichiers déjà effacés.
pub fn avec_prefixes_uniques(workflow: &Value, jeton: &str) -> Value {
    let mut copie = workflow.clone();
    if let Some(noeuds) = copie.as_object_mut() {
        for noeud in noeuds.values_mut() {
            if let Some(Value::String(prefixe)) = noeud.pointer_mut("/inputs/filename_prefix") {
                prefixe.push('_');
                prefixe.push_str(jeton);
            }
        }
    }
    copie
}

/// Ce qu'un message du WebSocket dit de notre exécution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evenement {
    /// Avancement d'un nœud.
    Progression { valeur: u64, max: u64 },
    /// L'exécution a échoué ou a été interrompue.
    Erreur(String),
    /// Notre exécution est finie.
    Fin,
    /// Rien qui nous concerne.
    Autre,
}

/// Lit un message texte du WebSocket de ComfyUI pour l'exécution `prompt_id`.
pub fn lire_evenement(texte: &str, prompt_id: &str) -> Evenement {
    let Ok(evenement) = serde_json::from_str::<Value>(texte) else {
        return Evenement::Autre;
    };
    let donnees = evenement.get("data").cloned().unwrap_or(Value::Null);
    let sien = donnees.get("prompt_id").and_then(Value::as_str);
    if sien.is_some_and(|id| id != prompt_id) {
        return Evenement::Autre;
    }
    let texte_de = |cle: &str| {
        donnees
            .get(cle)
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
            .unwrap_or_else(|| "None".to_owned())
    };
    match evenement.get("type").and_then(Value::as_str) {
        Some("progress") => Evenement::Progression {
            valeur: donnees.get("value").and_then(Value::as_u64).unwrap_or(0),
            max: donnees.get("max").and_then(Value::as_u64).unwrap_or(0),
        },
        Some("execution_error") => Evenement::Erreur(format!(
            "{} : {}",
            texte_de("node_type"),
            texte_de("exception_message")
        )),
        Some("execution_interrupted") => Evenement::Erreur("Exécution interrompue".to_owned()),
        Some("executing") if donnees.get("node").is_none_or(Value::is_null) && sien == Some(prompt_id) => {
            Evenement::Fin
        }
        _ => Evenement::Autre,
    }
}

/// Les fichiers de sortie annoncés par `/history/<prompt_id>` : chaque objet avec un `filename` et de type
/// `output` (par défaut), jamais les aperçus temporaires. Chacun se demande à `/view` avec ses champs en paramètres.
///
/// # Errors
/// L'historique ne contient pas cette exécution.
pub fn sorties_a_recuperer(historique: &Value, prompt_id: &str) -> Result<Vec<Map<String, Value>>, String> {
    let sorties = historique
        .get(prompt_id)
        .and_then(|e| e.get("outputs"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("ComfyUI n'a pas d'historique pour l'exécution {prompt_id}"))?;
    let mut fichiers = Vec::new();
    for sortie in sorties.values().filter_map(Value::as_object) {
        for valeur in sortie.values().filter_map(Value::as_array) {
            for fichier in valeur.iter().filter_map(Value::as_object) {
                let de_sortie = fichier.get("type").and_then(Value::as_str).unwrap_or("output") == "output";
                if fichier.contains_key("filename") && de_sortie {
                    fichiers.push(fichier.clone());
                }
            }
        }
    }
    Ok(fichiers)
}

/// Les paramètres de `/view` pour un fichier annoncé : chaque champ texte ou nombre, tel quel.
pub fn parametres_vue(fichier: &Map<String, Value>) -> Vec<(String, String)> {
    fichier
        .iter()
        .filter_map(|(cle, valeur)| {
            let texte = match valeur {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => return None,
            };
            Some((cle.clone(), texte))
        })
        .collect()
}

/// Efface de `output` un fichier récupéré, s'il est strictement dedans. Rien d'autre n'est jamais effacé ; un
/// échec est sans conséquence (le fichier reste dans `output`).
pub fn effacer_sortie(output: &Path, fichier: &Map<String, Value>) {
    let Some(nom) = fichier.get("filename").and_then(Value::as_str) else {
        return;
    };
    let sous_dossier = fichier
        .get("subfolder")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let candidat = output.join(sous_dossier).join(nom);
    let (Ok(base), Ok(cible)) = (fs::canonicalize(output), fs::canonicalize(&candidat)) else {
        return;
    };
    if cible != base && cible.starts_with(&base) && cible.is_file() {
        let _ = fs::remove_file(&cible);
    }
}

/// Le type MIME d'une sortie, d'après son extension : une image PNG par défaut.
pub fn type_mime(nom: &str) -> &'static str {
    let extension = nom
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "mp4" => "video/mp4",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => "image/png",
    }
}

/// La vidéo (premier `.mp4`) et son affiche (premier `.png`) parmi des noms de sorties.
pub fn video_et_affiche(noms: &[String]) -> (Option<&String>, Option<&String>) {
    let finit = |n: &String, ext: &str| n.to_lowercase().ends_with(ext);
    (
        noms.iter().find(|n| finit(n, ".mp4")),
        noms.iter().find(|n| finit(n, ".png")),
    )
}

/// Après `/free`, la VRAM est-elle redescendue et stable ? Au moins quatre lectures, et les trois dernières à moins
/// de [`ECART_VRAM_STABLE`] l'une de l'autre.
pub fn vram_stable(lectures: &[u64]) -> bool {
    let [.., a, b, c] = lectures else {
        return false;
    };
    let haut = (*a).max(*b).max(*c);
    let bas = (*a).min(*b).min(*c);
    lectures.len() >= 4 && haut - bas < ECART_VRAM_STABLE
}

/// Ce qu'on sait d'un processus Python, pour décider d'adopter un ComfyUI resté seul.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processus {
    pub pid: u32,
    pub nom: String,
    pub ligne_de_commande: Vec<String>,
    /// Son dossier courant, s'il se lit.
    pub dossier: Option<PathBuf>,
    /// Son parent tourne-t-il encore ? Faux aussi quand le numéro du parent a été repris par un processus plus
    /// jeune que lui.
    pub parent_vivant: bool,
}

/// Un ComfyUI lancé par une Aria qui n'est plus là : un Python qui fait tourner `ComfyUI/main.py` sur notre port,
/// depuis notre dossier de ComfyUI, dont le parent est mort, et qui n'est pas nous.
pub fn est_orphelin(processus: &Processus, port: u16, dossier: &Path, moi: u32) -> bool {
    let ligne = &processus.ligne_de_commande;
    let port = port.to_string();
    processus.pid != moi
        && processus.nom.to_lowercase().starts_with("python")
        && ligne.iter().any(|a| a == "ComfyUI/main.py")
        && ligne.iter().any(|a| a == "--port")
        && ligne.iter().any(|a| *a == port)
        && processus
            .dossier
            .as_deref()
            .is_some_and(|d| meme_dossier(d, dossier))
        && !processus.parent_vivant
}

/// Le message d'un workflow refusé par `/prompt`.
pub fn workflow_refuse(corps: &str) -> String {
    format!("Workflow refusé : {corps}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lancement(dossier: &Path) -> Lancement {
        Lancement {
            dossier: dossier.join("comfyui"),
            port: 8195,
            chemins_modeles: dossier.join("cache").join("comfyui_chemins.yaml"),
            travail: dossier.join("cache").join(DOSSIER_TRAVAIL),
        }
    }

    #[test]
    fn les_arguments_gardent_comfyui_sur_la_boucle_locale_sans_cors() {
        let temp = tempfile::tempdir().unwrap();
        let l = lancement(temp.path());
        let a: Vec<String> = l
            .arguments()
            .iter()
            .map(|x| x.to_string_lossy().into_owned())
            .collect();
        assert!(a.windows(2).any(|w| w[0] == "--listen" && w[1] == "127.0.0.1"));
        assert!(a.windows(2).any(|w| w[0] == "--port" && w[1] == "8195"));
        assert!(!a.iter().any(|x| x == "--enable-cors-header"));
        assert_eq!(a[..2], ["-s", "ComfyUI/main.py"]);
        assert!(
            l.python().ends_with("python_embeded/python.exe")
                || l.python().ends_with("python_embeded\\python.exe")
        );
        assert_eq!(l.url(), "http://127.0.0.1:8195");
    }

    #[test]
    fn les_trois_dossiers_de_travail_sont_dans_le_cache() {
        let temp = tempfile::tempdir().unwrap();
        let l = lancement(temp.path());
        let a = l.arguments();
        let apres = |option: &str| {
            let i = a.iter().position(|x| x == option).unwrap();
            PathBuf::from(&a[i + 1])
        };
        assert_eq!(apres("--input-directory"), l.travail.join("input"));
        assert_eq!(apres("--output-directory"), l.travail.join("output"));
        assert_eq!(apres("--temp-directory"), l.travail);
        l.preparer_travail().unwrap();
        for d in ["input", "output", "temp"] {
            assert!(l.travail.join(d).is_dir(), "{d}");
        }
    }

    #[test]
    fn entrees_et_temporaires_vides_sorties_gardees() {
        let temp = tempfile::tempdir().unwrap();
        let l = lancement(temp.path());
        fs::create_dir_all(l.travail.join("input").join("sous")).unwrap();
        fs::write(l.travail.join("input").join("sous").join("a.png"), "a").unwrap();
        fs::create_dir_all(l.travail.join("temp")).unwrap();
        fs::write(l.travail.join("temp").join("b.png"), "b").unwrap();
        fs::create_dir_all(l.travail.join("output")).unwrap();
        fs::write(l.travail.join("output").join("c.png"), "c").unwrap();
        l.preparer_travail().unwrap();
        assert_eq!(fs::read_dir(l.travail.join("input")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(l.travail.join("temp")).unwrap().count(), 0);
        assert!(l.travail.join("output").join("c.png").is_file());
    }

    #[test]
    fn seuls_les_prefixes_de_texte_recoivent_le_jeton() {
        let workflow = json!({
            "1": { "class_type": "SaveImage", "inputs": { "filename_prefix": "a" } },
            "2": { "class_type": "SaveImage", "inputs": { "filename_prefix": ["5", 0] } },
            "3": { "class_type": "KSampler", "inputs": { "seed": 42 } },
            "4": "pas un nœud"
        });
        let copie = avec_prefixes_uniques(&workflow, "0a1b2c3d");
        assert_eq!(copie["1"]["inputs"]["filename_prefix"], "a_0a1b2c3d");
        assert_eq!(copie["2"]["inputs"]["filename_prefix"], json!(["5", 0]));
        assert_eq!(copie["3"], workflow["3"]);
        assert_eq!(
            workflow["1"]["inputs"]["filename_prefix"], "a",
            "l'original ne change pas"
        );
    }

    #[test]
    fn un_prefixe_avec_sous_dossier_garde_son_sous_dossier() {
        let workflow = json!({ "9": { "inputs": { "filename_prefix": "aria_banc/b1/krea2" } } });
        let copie = avec_prefixes_uniques(&workflow, "deadbeef");
        assert_eq!(
            copie["9"]["inputs"]["filename_prefix"],
            "aria_banc/b1/krea2_deadbeef"
        );
    }

    #[test]
    fn un_jeton_neuf_a_chaque_envoi() {
        let a = nouveau_jeton().unwrap();
        let b = nouveau_jeton().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 8);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "{a}"
        );
    }

    #[test]
    fn les_evenements_de_notre_execution_seulement() {
        let id = "p1";
        assert_eq!(
            lire_evenement(
                r#"{"type": "progress", "data": {"value": 3, "max": 8, "prompt_id": "p1"}}"#,
                id
            ),
            Evenement::Progression { valeur: 3, max: 8 }
        );
        assert_eq!(
            lire_evenement(
                r#"{"type": "progress", "data": {"value": 3, "max": 8, "prompt_id": "autre"}}"#,
                id
            ),
            Evenement::Autre
        );
        assert_eq!(
            lire_evenement(
                r#"{"type": "execution_error", "data": {"prompt_id": "p1", "node_type": "KSampler", "exception_message": "plus de mémoire"}}"#,
                id
            ),
            Evenement::Erreur("KSampler : plus de mémoire".to_owned())
        );
        assert_eq!(
            lire_evenement(
                r#"{"type": "execution_interrupted", "data": {"prompt_id": "p1"}}"#,
                id
            ),
            Evenement::Erreur("Exécution interrompue".to_owned())
        );
        assert_eq!(
            lire_evenement(
                r#"{"type": "executing", "data": {"node": null, "prompt_id": "p1"}}"#,
                id
            ),
            Evenement::Fin
        );
        assert_eq!(
            lire_evenement(
                r#"{"type": "executing", "data": {"node": "7", "prompt_id": "p1"}}"#,
                id
            ),
            Evenement::Autre
        );
        assert_eq!(
            lire_evenement(r#"{"type": "executing", "data": {"node": null}}"#, id),
            Evenement::Autre
        );
        assert_eq!(lire_evenement("pas du json", id), Evenement::Autre);
    }

    #[test]
    fn les_sorties_sans_les_apercus_temporaires() {
        let historique = json!({ "p1": { "outputs": {
            "9": { "images": [
                { "filename": "a_1.png", "subfolder": "", "type": "output" },
                { "filename": "apercu.png", "subfolder": "", "type": "temp" }
            ] },
            "12": { "gifs": [ { "filename": "clip.mp4", "subfolder": "videos" } ], "animated": [true] }
        } } });
        let fichiers = sorties_a_recuperer(&historique, "p1").unwrap();
        let noms: Vec<&str> = fichiers.iter().map(|f| f["filename"].as_str().unwrap()).collect();
        assert!(
            noms.contains(&"a_1.png") && noms.contains(&"clip.mp4"),
            "{noms:?}"
        );
        assert!(!noms.contains(&"apercu.png"));
        assert!(sorties_a_recuperer(&historique, "absent").is_err());
        let video = fichiers.iter().find(|f| f["filename"] == "clip.mp4").unwrap();
        let params = parametres_vue(video);
        assert!(params.contains(&("subfolder".to_owned(), "videos".to_owned())));
    }

    #[test]
    fn une_sortie_recuperee_est_effacee_sous_dossier_compris() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        fs::create_dir_all(output.join("videos")).unwrap();
        fs::write(output.join("videos").join("clip.mp4"), "v").unwrap();
        fs::write(output.join("a.png"), "a").unwrap();
        let fichier = |nom: &str, sous: &str| {
            json!({ "filename": nom, "subfolder": sous })
                .as_object()
                .unwrap()
                .clone()
        };
        effacer_sortie(&output, &fichier("clip.mp4", "videos"));
        effacer_sortie(&output, &fichier("a.png", ""));
        assert!(!output.join("videos").join("clip.mp4").exists());
        assert!(!output.join("a.png").exists());
    }

    #[test]
    fn rien_n_est_efface_hors_des_sorties() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        fs::create_dir_all(&output).unwrap();
        fs::write(temp.path().join("garde.txt"), "là").unwrap();
        let fichier = json!({ "filename": "garde.txt", "subfolder": "../" })
            .as_object()
            .unwrap()
            .clone();
        effacer_sortie(&output, &fichier);
        assert!(temp.path().join("garde.txt").is_file());
    }

    #[test]
    fn les_types_et_la_video_avec_son_affiche() {
        assert_eq!(type_mime("clip.MP4"), "video/mp4");
        assert_eq!(type_mime("voix.flac"), "audio/flac");
        assert_eq!(type_mime("image"), "image/png");
        let noms = vec!["a.PNG".to_owned(), "b.mp4".to_owned(), "c.png".to_owned()];
        let (video, affiche) = video_et_affiche(&noms);
        assert_eq!(video.map(String::as_str), Some("b.mp4"));
        assert_eq!(affiche.map(String::as_str), Some("a.PNG"));
    }

    #[test]
    fn la_vram_est_stable_apres_quatre_lectures_proches() {
        let mo = 1024 * 1024;
        assert!(!vram_stable(&[]));
        assert!(!vram_stable(&[900 * mo, 900 * mo, 900 * mo]));
        assert!(vram_stable(&[4000 * mo, 900 * mo, 910 * mo, 905 * mo]));
        assert!(!vram_stable(&[4000 * mo, 2000 * mo, 900 * mo, 905 * mo]));
    }

    #[test]
    fn seul_un_comfyui_orphelin_de_notre_dossier_est_adopte() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = temp.path().join("comfyui");
        fs::create_dir_all(&dossier).unwrap();
        let orphelin = Processus {
            pid: 40,
            nom: "python.exe".to_owned(),
            ligne_de_commande: ["python.exe", "-s", "ComfyUI/main.py", "--port", "8195"]
                .map(str::to_owned)
                .to_vec(),
            dossier: Some(dossier.clone()),
            parent_vivant: false,
        };
        assert!(est_orphelin(&orphelin, 8195, &dossier, 1));
        assert!(!est_orphelin(
            &Processus {
                parent_vivant: true,
                ..orphelin.clone()
            },
            8195,
            &dossier,
            1
        ));
        assert!(!est_orphelin(&orphelin, 8196, &dossier, 1));
        assert!(!est_orphelin(&orphelin, 8195, &temp.path().join("ailleurs"), 1));
        assert!(!est_orphelin(&orphelin, 8195, &dossier, 40));
        assert!(!est_orphelin(
            &Processus {
                nom: "node.exe".to_owned(),
                ..orphelin.clone()
            },
            8195,
            &dossier,
            1
        ));
        assert!(!est_orphelin(
            &Processus {
                dossier: None,
                ..orphelin
            },
            8195,
            &dossier,
            1
        ));
    }
}
