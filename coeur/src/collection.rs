//! Modèles → Ma collection, comme en v2 (`app/collection.py`, `app/modeles.py`) : ce qui est installé, d'où ça vient
//! (téléchargé par Aria ou déposé à la main), à quoi ça sert ; les steps à valider d'un checkpoint ; les mots
//! déclencheurs et la force d'une LoRA ; la corbeille des modèles.
//!
//! La corbeille : un modèle supprimé n'est jamais effacé d'un coup. Il part dans `<dossier des modèles>\.corbeille\`, au
//! même chemin relatif (un renommage sur le même disque, instantané même pour 20 Go), et sa fiche quitte le registre
//! pour `data\corbeille_modeles.json` avec la date et le dossier des modèles d'alors ; elle y revient à la
//! restauration. Seul « Vider la corbeille » efface pour de bon. Seuls les modèles téléchargés par Aria se
//! suppriment : un fichier déposé à la main reste à l'utilisateur.
//!
//! « Retrouver les sources » est dans `retrouver`. Pas encore portés : les favoris des usages.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::Local;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::modeles::{self, DOSSIER_LLM};
use crate::telechargement::{modifier_registre, registre};
use crate::{Dossier, fiches, journal, llm, reglages, workflows};

/// Le dossier de la corbeille, dans le dossier des modèles.
pub const CORBEILLE: &str = ".corbeille";
/// Les mots déclencheurs d'une LoRA, au plus.
pub const LONGUEUR_DECLENCHEURS: usize = 300;
/// La force d'une LoRA, au plus.
pub const FORCE_MAX: f64 = 2.0;
const EXTENSIONS: [&str; 2] = ["safetensors", "gguf"];

/// Les groupes de Ma collection, dans l'ordre de la v2.
pub const GROUPES: [(&str, &str); 6] = [
    ("image", "Modèles image"),
    ("video", "Modèles vidéo"),
    ("lora", "LoRA"),
    ("llm", "LLM"),
    ("commun_image", "Fichiers de base · image (encodeurs, VAE…)"),
    ("commun_video", "Fichiers de base · vidéo (encodeurs, VAE…)"),
];

/// Les mots déclencheurs et la force d'une LoRA.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReglageLora {
    pub declencheurs: String,
    pub force: f64,
    /// Réglé par l'utilisateur (sinon, ce que dit la source).
    pub regle: bool,
    pub declencheurs_source: String,
    pub civitai: bool,
}

/// Ce que la fiche d'un workflow dit des steps de son checkpoint d'origine.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepsDits {
    pub valeur: Value,
    pub origine: String,
    pub source: Option<String>,
}

/// Un fichier de la collection.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub cle: String,
    pub nom: String,
    pub dossier: String,
    pub famille: Option<String>,
    pub taille: u64,
    /// `aria` (téléchargé par Aria), `retrouve` (source retrouvée), `main` (déposé à la main).
    pub origine: String,
    pub source: Option<String>,
    pub page: Option<String>,
    pub licence: Option<String>,
    pub declencheurs: Vec<String>,
    pub utilise_par: Vec<String>,
    pub supprimable: bool,
    /// Checkpoint de famille : ses steps validés…
    pub steps: Option<u64>,
    /// … ou ce qui aide à les valider (il n'est proposé nulle part avant).
    pub a_valider: Option<Vec<StepsDits>>,
    pub lora: Option<ReglageLora>,
}

/// Un groupe de Ma collection.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Groupe {
    pub id: String,
    pub nom: String,
    pub elements: Vec<Element>,
    pub taille: u64,
}

/// Ce que montre Ma collection.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Collection {
    pub groupes: Vec<Groupe>,
    /// Les fichiers déposés à la main.
    pub orphelins: usize,
}

/// Le groupe et la famille d'un dossier des modèles.
fn groupe_de(dossier: &str) -> (String, Option<String>) {
    if dossier == DOSSIER_LLM {
        return ("llm".to_owned(), None);
    }
    match dossier.split('/').collect::<Vec<_>>().as_slice() {
        [nature, modeles::BASE, _] => (format!("commun_{nature}"), None),
        [_, famille, "loras"] => ("lora".to_owned(), Some((*famille).to_owned())),
        [nature, famille, _] => ((*nature).to_owned(), Some((*famille).to_owned())),
        _ => ("image".to_owned(), None),
    }
}

/// Clé d'un fichier → les workflows qui le chargent (fichiers des fiches, checkpoints et LoRA de la famille).
fn utilisations(modeles_dir: &Path) -> BTreeMap<String, Vec<String>> {
    let mut par_cle: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for wf in workflows::catalogue().0 {
        let nom = wf
            .fiche
            .get("nom")
            .and_then(Value::as_str)
            .unwrap_or(&wf.id)
            .to_owned();
        let mut ajouter = |cle: String| {
            let noms = par_cle.entry(cle).or_default();
            if !noms.contains(&nom) {
                noms.push(nom.clone());
            }
        };
        for f in wf
            .fiche
            .get("fichiers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(d), Some(n)) = (
                f.get("dossier").and_then(Value::as_str),
                f.get("nom").and_then(Value::as_str),
            ) {
                ajouter(format!("{d}/{n}"));
            }
        }
        for dossier in [wf.dossier_modeles(), wf.dossier_loras()] {
            for n in modeles::fichiers(modeles_dir, &dossier, &modeles::EXTENSIONS_IMAGE) {
                ajouter(format!("{dossier}/{n}"));
            }
        }
    }
    par_cle
}

fn steps_dits_par_la_fiche(wf: &workflows::Workflow) -> Vec<StepsDits> {
    let mut dits = Vec::new();
    if let Some(defaut) = wf.fiche.pointer("/defauts/steps") {
        dits.push(StepsDits {
            valeur: defaut.clone(),
            origine: "défaut du template du workflow".to_owned(),
            source: None,
        });
    }
    for r in wf
        .fiche
        .pointer("/recommandations/valeurs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| r.get("cle").and_then(Value::as_str) == Some("steps"))
    {
        dits.push(StepsDits {
            valeur: r.get("valeur").cloned().unwrap_or(Value::Null),
            origine: r
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or("recommandé")
                .to_owned(),
            source: r.get("source").and_then(Value::as_str).map(str::to_owned),
        });
    }
    dits
}

/// Clé d'un checkpoint de famille → ses steps validés, ou ce que dit la fiche pour les valider.
fn steps_des_checkpoints(
    dossier: &Dossier,
    modeles_dir: &Path,
) -> BTreeMap<String, Result<u64, Vec<StepsDits>>> {
    let section = reglages::section(dossier, "workflows").unwrap_or_default();
    let mut sortie = BTreeMap::new();
    for wf in workflows::catalogue().0 {
        if wf.entree("steps").is_none() {
            continue;
        }
        let reglage = section.get(&wf.id).and_then(Value::as_object);
        for nom in wf.presents(modeles_dir) {
            let cle = format!("{}/{nom}", wf.dossier_modeles());
            if sortie.contains_key(&cle) {
                continue;
            }
            let etat = if wf.a_valider(reglage, &nom) {
                Err(if wf.modele_origine().as_deref() == Some(nom.as_str()) {
                    steps_dits_par_la_fiche(&wf)
                } else {
                    Vec::new()
                })
            } else {
                Ok(wf.steps_de(reglage, &nom))
            };
            sortie.insert(cle, etat);
        }
    }
    sortie
}

fn memoire_loras(dossier: &Dossier) -> Map<String, Value> {
    reglages::section(dossier, "loras").unwrap_or_default()
}

fn reglage_lora(cle: &str, registre: &Map<String, Value>, memoire: &Map<String, Value>) -> ReglageLora {
    let retenu = memoire.get(cle).and_then(Value::as_object);
    let infos = registre.get(cle);
    let source: Vec<String> = infos
        .and_then(|i| i.get("mots_declencheurs"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str().map(str::to_owned))
        .collect();
    let source = source.join(", ");
    ReglageLora {
        declencheurs: retenu
            .and_then(|r| r.get("declencheurs"))
            .and_then(Value::as_str)
            .map_or_else(|| source.clone(), str::to_owned),
        force: retenu
            .and_then(|r| r.get("force"))
            .and_then(Value::as_f64)
            .or_else(|| infos.and_then(|i| i.get("force_auteur")).and_then(Value::as_f64))
            .unwrap_or(1.0),
        regle: retenu.is_some(),
        declencheurs_source: source,
        civitai: infos.and_then(|i| i.get("source")).and_then(Value::as_str) == Some("civitai"),
    }
}

/// Ce que montre Ma collection.
pub fn collection(dossier: &Dossier, data: &Path, modeles_dir: &Path) -> Collection {
    let registre = registre(data);
    let memoire = memoire_loras(dossier);
    let utilise = utilisations(modeles_dir);
    let steps = steps_des_checkpoints(dossier, modeles_dir);
    let defaut = llm::defaut(modeles_dir);
    let mut groupes: BTreeMap<String, Vec<Element>> = BTreeMap::new();
    for dossier_fichier in modeles::dossiers() {
        for nom in modeles::fichiers(modeles_dir, &dossier_fichier, &EXTENSIONS) {
            let cle = format!("{dossier_fichier}/{nom}");
            let taille = fs::metadata(modeles_dir.join(&cle)).map(|m| m.len()).unwrap_or(0);
            let (groupe, famille) = groupe_de(&dossier_fichier);
            let mut infos = registre
                .get(&cle)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let suivi = registre.contains_key(&cle);
            let origine = if !suivi {
                "main"
            } else if infos.get("retrouve").and_then(Value::as_bool) == Some(true) {
                "retrouve"
            } else {
                "aria"
            };
            let mut utilise_par = utilise.get(&cle).cloned().unwrap_or_default();
            if groupe == "llm" {
                utilise_par = if nom == defaut {
                    vec!["Réécriture des prompts (par défaut)".to_owned()]
                } else {
                    Vec::new()
                };
                if !suivi && let Some(fiche) = fiches::fiche_llm(&nom) {
                    infos.insert(
                        "page".to_owned(),
                        fiche.get("page").cloned().unwrap_or(Value::Null),
                    );
                    infos.insert(
                        "licence".to_owned(),
                        fiche.pointer("/licence/nom").cloned().unwrap_or(Value::Null),
                    );
                }
            }
            let texte = |k: &str| infos.get(k).and_then(Value::as_str).map(str::to_owned);
            let etat_steps = steps.get(&cle);
            groupes.entry(groupe.clone()).or_default().push(Element {
                declencheurs: infos
                    .get("mots_declencheurs")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m.as_str().map(str::to_owned))
                    .collect(),
                source: texte("source"),
                page: texte("page"),
                licence: texte("licence"),
                steps: etat_steps.and_then(|e| e.as_ref().ok().copied()),
                a_valider: etat_steps.and_then(|e| e.as_ref().err().cloned()),
                lora: (groupe == "lora").then(|| reglage_lora(&cle, &registre, &memoire)),
                supprimable: suivi,
                origine: origine.to_owned(),
                utilise_par,
                famille,
                dossier: dossier_fichier.clone(),
                nom,
                taille,
                cle,
            });
        }
    }
    let orphelins = groupes.values().flatten().filter(|e| e.origine == "main").count();
    Collection {
        groupes: GROUPES
            .iter()
            .map(|(id, nom)| {
                let elements = groupes.remove(*id).unwrap_or_default();
                Groupe {
                    id: (*id).to_owned(),
                    nom: (*nom).to_owned(),
                    taille: elements.iter().map(|e| e.taille).sum(),
                    elements,
                }
            })
            .collect(),
        orphelins,
    }
}

/// Valide les steps d'un checkpoint de famille par sa clé : ils deviennent les siens dans tous les workflows de sa
/// famille ; sa suggestion attendue au registre est oubliée.
///
/// # Errors
/// Pas un checkpoint de famille, steps hors bornes, ou `reglages.json` qui ne s'écrit pas.
pub fn valider_checkpoint(
    dossier: &Dossier,
    data: &Path,
    modeles_dir: &Path,
    cle: &str,
    steps: u64,
) -> Result<(), String> {
    let (dossier_fichier, nom) = cle
        .rsplit_once('/')
        .ok_or_else(|| format!("Pas un checkpoint de famille : {cle}"))?;
    if !modeles::est_checkpoint(dossier_fichier) {
        return Err(format!("Pas un checkpoint de famille : {cle}"));
    }
    let wf = workflows::catalogue()
        .0
        .into_iter()
        .find(|w| w.dossier_modeles() == dossier_fichier && w.entree("steps").is_some())
        .ok_or_else(|| format!("Checkpoint absent, ou sans workflow qui règle ses steps : {cle}"))?;
    workflows::valider_checkpoint(dossier, modeles_dir, &wf.id, nom, steps)?;
    if registre(data)
        .get(cle)
        .is_some_and(|e| e.get("a_valider").is_some())
    {
        modifier_registre(data, |r| {
            if let Some(entree) = r.get_mut(cle).and_then(Value::as_object_mut) {
                entree.remove("a_valider");
            }
        })
        .map_err(|e| format!("registre des modèles : {e}"))?;
    }
    Ok(())
}

/// Règle les mots déclencheurs et la force d'une LoRA de famille (`reglages.json`, section `loras`, comme en v2).
///
/// # Errors
/// LoRA inconnue, mots trop longs, force hors de 0 à 2, ou `reglages.json` qui ne s'écrit pas.
pub fn regler_lora(
    dossier: &Dossier,
    data: &Path,
    modeles_dir: &Path,
    cle: &str,
    declencheurs: &str,
    force: f64,
) -> Result<ReglageLora, String> {
    exiger_lora(modeles_dir, cle)?;
    let declencheurs = declencheurs.split_whitespace().collect::<Vec<_>>().join(" ");
    if declencheurs.chars().count() > LONGUEUR_DECLENCHEURS {
        return Err(format!(
            "Mots déclencheurs : {LONGUEUR_DECLENCHEURS} caractères au plus"
        ));
    }
    if !(0.0..=FORCE_MAX).contains(&force) {
        return Err("Force de LoRA entre 0 et 2".to_owned());
    }
    let force = (force * 100.0).round() / 100.0;
    reglages::modifier_section(dossier, "loras", |s| {
        s.insert(
            cle.to_owned(),
            json!({"declencheurs": declencheurs, "force": force}),
        );
    })
    .map_err(|e| e.to_string())?;
    Ok(reglage_lora(cle, &registre(data), &memoire_loras(dossier)))
}

/// Revient aux mots et à la force de la source.
///
/// # Errors
/// LoRA inconnue, ou `reglages.json` qui ne s'écrit pas.
pub fn oublier_reglage_lora(
    dossier: &Dossier,
    data: &Path,
    modeles_dir: &Path,
    cle: &str,
) -> Result<ReglageLora, String> {
    exiger_lora(modeles_dir, cle)?;
    reglages::modifier_section(dossier, "loras", |s| {
        s.remove(cle);
    })
    .map_err(|e| e.to_string())?;
    Ok(reglage_lora(cle, &registre(data), &memoire_loras(dossier)))
}

fn exiger_lora(modeles_dir: &Path, cle: &str) -> Result<(), String> {
    let inconnue = || format!("LoRA inconnue : {cle}");
    let (dossier, fichier) = cle.rsplit_once('/').ok_or_else(inconnue)?;
    let (groupe, famille) = groupe_de(dossier);
    let bonne = groupe == "lora"
        && famille.is_some_and(|f| modeles::dossier_famille(&f, "loras").is_ok_and(|d| d == dossier))
        && modeles::fichiers(modeles_dir, dossier, &modeles::EXTENSIONS_IMAGE)
            .iter()
            .any(|f| f == fichier);
    if bonne { Ok(()) } else { Err(inconnue()) }
}

// --- corbeille -------------------------------------------------------------------------------------------------

/// Un modèle à la corbeille.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ALaCorbeille {
    pub cle: String,
    pub nom: String,
    pub taille: u64,
    pub present: bool,
    pub date: Option<String>,
    pub dossier_modeles: String,
    /// Dans la corbeille d'un ancien dossier des modèles.
    pub ailleurs: bool,
}

/// La corbeille des modèles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Corbeille {
    pub modeles: Vec<ALaCorbeille>,
    pub taille: u64,
}

fn fichier_corbeille(data: &Path) -> PathBuf {
    data.join("corbeille_modeles.json")
}

fn lire_json(chemin: &Path) -> Map<String, Value> {
    fs::read_to_string(chemin)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn ecrire_json(chemin: &Path, valeur: &Value, indent: usize) -> Result<(), String> {
    let mut octets = Vec::new();
    let espaces = vec![b' '; indent];
    let formateur = serde_json::ser::PrettyFormatter::with_indent(&espaces);
    let mut serialiseur = serde_json::Serializer::with_formatter(&mut octets, formateur);
    valeur
        .serialize(&mut serialiseur)
        .map_err(|e| format!("{} : {e}", chemin.display()))?;
    if let Some(parent) = chemin.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{} : {e}", parent.display()))?;
    }
    let temporaire = chemin.with_extension("json.tmp");
    fs::write(&temporaire, octets)
        .and_then(|()| fs::rename(&temporaire, chemin))
        .map_err(|e| format!("{} : {e}", chemin.display()))
}

/// Un chemin relatif qui ne sort pas de sa base.
fn dans(base: &Path, cle: &str) -> Result<PathBuf, String> {
    let relatif = Path::new(cle);
    let sain = !cle.is_empty() && relatif.components().all(|c| matches!(c, Component::Normal(_)));
    if sain {
        Ok(base.join(relatif))
    } else {
        Err("Chemin hors du dossier des modèles".to_owned())
    }
}

/// Met un modèle téléchargé par Aria à la corbeille.
///
/// # Errors
/// Fichier déposé à la main, déjà à la corbeille, ou que Windows refuse de déplacer (ouvert par ComfyUI).
pub fn mettre_a_la_corbeille(
    dossier: &Dossier,
    data: &Path,
    modeles_dir: &Path,
    cle: &str,
) -> Result<(), String> {
    let Some(entree) = registre(data).get(cle).cloned() else {
        return Err("Seuls les modèles téléchargés par l'appli peuvent être supprimés".to_owned());
    };
    let fichier = dans(modeles_dir, cle)?;
    let mut corbeille = lire_json(&fichier_corbeille(data));
    if fichier.is_file() {
        let nom = fichier
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let cible = dans(&modeles_dir.join(CORBEILLE), cle)?;
        if corbeille.contains_key(cle) || cible.exists() {
            return Err(format!(
                "{nom} est déjà dans la corbeille : restaure-le ou vide la corbeille d'abord"
            ));
        }
        if let Some(parent) = cible.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::rename(&fichier, &cible).map_err(|e| {
            format!("{nom} n'a pas pu aller à la corbeille ({e}) : il est peut-être ouvert par ComfyUI. Arrête-le, puis réessaie")
        })?;
        let mut garde = entree.as_object().cloned().unwrap_or_default();
        garde.insert(
            "corbeille".to_owned(),
            json!(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
        );
        garde.insert(
            "dossier_modeles".to_owned(),
            json!(modeles_dir.display().to_string()),
        );
        corbeille.insert(cle.to_owned(), Value::Object(garde));
        ecrire_json(&fichier_corbeille(data), &Value::Object(corbeille), 2)?;
    }
    // Sans fichier (déjà effacé à la main), seule sa fiche s'en va.
    modifier_registre(data, |r| {
        r.remove(cle);
    })
    .map_err(|e| format!("registre des modèles : {e}"))?;
    journal::noter(
        dossier,
        "Modèle mis à la corbeille",
        cle,
        journal::Resultat::Ok,
        None,
    );
    Ok(())
}

/// La corbeille des modèles, la plus récente d'abord.
pub fn corbeille(data: &Path, modeles_dir: &Path) -> Corbeille {
    let mut entrees: Vec<(String, Value)> = lire_json(&fichier_corbeille(data)).into_iter().collect();
    entrees.sort_by(|a, b| {
        let date = |e: &Value| {
            e.get("corbeille")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        date(&b.1).cmp(&date(&a.1))
    });
    let mut modeles = Vec::new();
    for (cle, entree) in entrees {
        let base = entree
            .get("dossier_modeles")
            .and_then(Value::as_str)
            .map_or_else(|| modeles_dir.to_path_buf(), PathBuf::from);
        let Ok(chemin) = dans(&base.join(CORBEILLE), &cle) else {
            continue;
        };
        let taille = fs::metadata(&chemin).map(|m| m.len()).ok();
        modeles.push(ALaCorbeille {
            nom: cle.rsplit('/').next().unwrap_or(&cle).to_owned(),
            taille: taille.unwrap_or(0),
            present: taille.is_some(),
            date: entree.get("corbeille").and_then(Value::as_str).map(str::to_owned),
            ailleurs: base != modeles_dir,
            dossier_modeles: base.display().to_string(),
            cle,
        });
    }
    Corbeille {
        taille: modeles.iter().map(|m| m.taille).sum(),
        modeles,
    }
}

fn supprimer_dossiers_vides(dossier: &Path) {
    let Ok(lecture) = fs::read_dir(dossier) else {
        return;
    };
    for e in lecture.filter_map(Result::ok) {
        if e.path().is_dir() {
            supprimer_dossiers_vides(&e.path());
        }
    }
    let _ = fs::remove_dir(dossier);
}

/// Remet un modèle à sa place et sa fiche au registre.
///
/// # Errors
/// Pas à la corbeille, dans la corbeille d'un ancien dossier des modèles, déjà présent (retéléchargé), effacé à la
/// main, ou que Windows refuse de déplacer.
pub fn restaurer(dossier: &Dossier, data: &Path, modeles_dir: &Path, cle: &str) -> Result<(), String> {
    let mut corbeille = lire_json(&fichier_corbeille(data));
    let entree = corbeille
        .get(cle)
        .cloned()
        .ok_or_else(|| "Ce modèle n'est pas dans la corbeille".to_owned())?;
    let base = entree
        .get("dossier_modeles")
        .and_then(Value::as_str)
        .map_or_else(|| modeles_dir.to_path_buf(), PathBuf::from);
    if base != modeles_dir {
        return Err(format!(
            "Ce modèle est dans la corbeille de l'ancien dossier des modèles ({}) : reprends ce dossier dans Réglages \
             pour le restaurer, ou vide la corbeille",
            base.display()
        ));
    }
    let source = dans(&modeles_dir.join(CORBEILLE), cle)?;
    let cible = dans(modeles_dir, cle)?;
    let nom = cible
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if cible.exists() || registre(data).contains_key(cle) {
        return Err(format!(
            "{nom} est déjà présent (retéléchargé ?) : la copie de la corbeille n'est pas restaurée, vide la corbeille \
             pour libérer la place"
        ));
    }
    if !source.is_file() {
        corbeille.remove(cle);
        ecrire_json(&fichier_corbeille(data), &Value::Object(corbeille), 2)?;
        return Err(format!(
            "{nom} n'est plus dans la corbeille (effacé à la main ?) : rien à restaurer"
        ));
    }
    if let Some(parent) = cible.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(&source, &cible).map_err(|e| format!("{nom} n'a pas pu être restauré ({e})"))?;
    let mut remise = entree.as_object().cloned().unwrap_or_default();
    remise.remove("corbeille");
    remise.remove("dossier_modeles");
    modifier_registre(data, |r| {
        r.insert(cle.to_owned(), Value::Object(remise));
    })
    .map_err(|e| format!("registre des modèles : {e}"))?;
    corbeille.remove(cle);
    ecrire_json(&fichier_corbeille(data), &Value::Object(corbeille), 2)?;
    supprimer_dossiers_vides(&modeles_dir.join(CORBEILLE));
    journal::noter(dossier, "Modèle restauré", cle, journal::Resultat::Ok, None);
    Ok(())
}

/// La seule suppression définitive : chaque fichier de la corbeille (anciens dossiers des modèles compris). Rend le
/// nombre de modèles effacés et les octets libérés.
///
/// # Errors
/// Le fichier de la corbeille ne s'écrit pas.
pub fn vider_corbeille(dossier: &Dossier, data: &Path, modeles_dir: &Path) -> Result<(usize, u64), String> {
    let corbeille = lire_json(&fichier_corbeille(data));
    let mut effaces = 0;
    let mut liberes = 0;
    let mut restes = Map::new();
    let mut bases = Vec::new();
    for (cle, entree) in corbeille {
        let base = entree
            .get("dossier_modeles")
            .and_then(Value::as_str)
            .map_or_else(|| modeles_dir.to_path_buf(), PathBuf::from);
        let efface = dans(&base.join(CORBEILLE), &cle).and_then(|chemin| {
            let taille = fs::metadata(&chemin).map(|m| m.len()).unwrap_or(0);
            match fs::remove_file(&chemin) {
                Ok(()) => Ok(taille),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
                Err(e) => Err(e.to_string()),
            }
        });
        match efface {
            Ok(taille) => {
                effaces += 1;
                liberes += taille;
            }
            Err(_) => {
                restes.insert(cle, entree);
            }
        }
        if !bases.contains(&base) {
            bases.push(base);
        }
    }
    for base in bases {
        supprimer_dossiers_vides(&base.join(CORBEILLE));
    }
    let partiel = !restes.is_empty();
    ecrire_json(&fichier_corbeille(data), &Value::Object(restes.clone()), 2)?;
    #[allow(clippy::cast_precision_loss)] // des gigaoctets affichés
    let go = liberes as f64 / f64::from(1u32 << 30);
    let mut detail = format!("{go:.1} Go libérés");
    if partiel {
        detail.push_str(&format!(", {} fichier(s) non effacé(s)", restes.len()));
    }
    journal::noter(
        dossier,
        "Corbeille des modèles vidée",
        &format!("{effaces} modèle(s)"),
        if partiel {
            journal::Resultat::Partiel
        } else {
            journal::Resultat::Ok
        },
        Some(&detail),
    );
    Ok((effaces, liberes))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Banc {
        _temp: tempfile::TempDir,
        dossier: Dossier,
        data: PathBuf,
        modeles: PathBuf,
    }

    fn banc() -> Banc {
        let temp = tempfile::tempdir().unwrap();
        Banc {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles: temp.path().join("modeles"),
            _temp: temp,
        }
    }

    fn poser(b: &Banc, cle: &str, octets: &[u8], au_registre: bool) {
        let chemin = b.modeles.join(cle);
        fs::create_dir_all(chemin.parent().unwrap()).unwrap();
        fs::write(&chemin, octets).unwrap();
        if au_registre {
            let mut r = registre(&b.data);
            r.insert(
                cle.to_owned(),
                json!({"source": "huggingface", "page": "p", "url": "u", "licence": "MIT",
                "mots_declencheurs": ["neon"], "sha256": "ab", "taille": octets.len()}),
            );
            ecrire_json(&b.data.join("modeles.json"), &Value::Object(r), 4).unwrap();
        }
    }

    #[test]
    fn collection_rangee_par_groupe() {
        let b = banc();
        let wf = workflows::obtenir("krea2_turbo").unwrap();
        let origine = wf.modele_origine().unwrap();
        poser(&b, &format!("{}/{origine}", wf.dossier_modeles()), b"x", true);
        poser(
            &b,
            &format!("{}/style.safetensors", wf.dossier_loras()),
            b"xx",
            false,
        );
        poser(&b, "llm/mistral.gguf", b"xxx", false);
        poser(&b, "image/_base/vae/notes.txt", b"rien", false);
        let c = collection(&b.dossier, &b.data, &b.modeles);
        let groupe = |id: &str| c.groupes.iter().find(|g| g.id == id).unwrap();
        let image = &groupe("image").elements[0];
        assert_eq!((image.origine.as_str(), image.supprimable), ("aria", true));
        assert_eq!(image.steps, Some(8));
        assert!(image.utilise_par.contains(&"Krea 2".to_owned()));
        let lora = &groupe("lora").elements[0];
        assert_eq!(lora.origine, "main");
        assert_eq!(lora.lora.as_ref().unwrap().force, 1.0);
        assert_eq!(
            groupe("llm").elements[0].utilise_par,
            vec!["Réécriture des prompts (par défaut)".to_owned()]
        );
        assert!(groupe("commun_image").elements.is_empty());
        assert_eq!(c.orphelins, 2);
    }

    #[test]
    fn steps_a_valider_puis_valides() {
        let b = banc();
        let wf = workflows::obtenir("qwen_image_2_1").unwrap();
        let origine = wf.modele_origine().unwrap();
        let cle = format!("{}/{origine}", wf.dossier_modeles());
        poser(&b, &cle, b"x", true);
        let mut r = registre(&b.data);
        r[&cle]["a_valider"] = json!({"steps": {"statut": "aucun"}});
        ecrire_json(&b.data.join("modeles.json"), &Value::Object(r), 4).unwrap();
        let element = |b: &Banc| {
            collection(&b.dossier, &b.data, &b.modeles).groupes[0]
                .elements
                .iter()
                .find(|e| e.cle == cle)
                .cloned()
                .unwrap()
        };
        let avant = element(&b);
        assert!(avant.steps.is_none());
        assert!(avant.a_valider.unwrap().iter().any(|d| d.valeur == json!(25)));
        valider_checkpoint(&b.dossier, &b.data, &b.modeles, &cle, 25).unwrap();
        assert_eq!(element(&b).steps, Some(25));
        assert!(registre(&b.data)[&cle].get("a_valider").is_none());
        assert!(valider_checkpoint(&b.dossier, &b.data, &b.modeles, "llm/x.gguf", 25).is_err());
    }

    #[test]
    fn reglage_d_une_lora() {
        let b = banc();
        let wf = workflows::obtenir("krea2_turbo").unwrap();
        let cle = format!("{}/style.safetensors", wf.dossier_loras());
        poser(&b, &cle, b"x", true);
        let r = regler_lora(&b.dossier, &b.data, &b.modeles, &cle, "  neon   glow ", 0.756).unwrap();
        assert_eq!(
            (r.declencheurs.as_str(), r.force, r.regle),
            ("neon glow", 0.76, true)
        );
        assert_eq!(r.declencheurs_source, "neon");
        assert!(regler_lora(&b.dossier, &b.data, &b.modeles, &cle, "x", 3.0).is_err());
        assert!(regler_lora(&b.dossier, &b.data, &b.modeles, &cle, &"a".repeat(301), 1.0).is_err());
        assert!(regler_lora(&b.dossier, &b.data, &b.modeles, "llm/x.gguf", "x", 1.0).is_err());
        let r = oublier_reglage_lora(&b.dossier, &b.data, &b.modeles, &cle).unwrap();
        assert_eq!((r.declencheurs.as_str(), r.regle), ("neon", false));
    }

    #[test]
    fn corbeille_aller_retour() {
        let b = banc();
        poser(&b, "llm/mistral.gguf", b"poids", true);
        poser(&b, "llm/main.gguf", b"poids", false);
        assert!(
            mettre_a_la_corbeille(&b.dossier, &b.data, &b.modeles, "llm/main.gguf")
                .unwrap_err()
                .starts_with("Seuls les modèles téléchargés")
        );
        assert!(mettre_a_la_corbeille(&b.dossier, &b.data, &b.modeles, "../x").is_err());
        mettre_a_la_corbeille(&b.dossier, &b.data, &b.modeles, "llm/mistral.gguf").unwrap();
        assert!(!b.modeles.join("llm/mistral.gguf").exists());
        assert!(b.modeles.join(".corbeille/llm/mistral.gguf").is_file());
        assert!(!registre(&b.data).contains_key("llm/mistral.gguf"));
        let c = corbeille(&b.data, &b.modeles);
        assert_eq!(
            (
                c.modeles.len(),
                c.taille,
                c.modeles[0].present,
                c.modeles[0].ailleurs
            ),
            (1, 5, true, false)
        );
        restaurer(&b.dossier, &b.data, &b.modeles, "llm/mistral.gguf").unwrap();
        assert!(b.modeles.join("llm/mistral.gguf").is_file());
        assert!(registre(&b.data)["llm/mistral.gguf"].get("corbeille").is_none());
        assert!(!b.modeles.join(CORBEILLE).exists());
        assert!(restaurer(&b.dossier, &b.data, &b.modeles, "llm/mistral.gguf").is_err());
        mettre_a_la_corbeille(&b.dossier, &b.data, &b.modeles, "llm/mistral.gguf").unwrap();
        assert_eq!(vider_corbeille(&b.dossier, &b.data, &b.modeles).unwrap(), (1, 5));
        assert!(corbeille(&b.data, &b.modeles).modeles.is_empty());
        assert_eq!(journal::lire(&b.dossier)[0].action, "Corbeille des modèles vidée");
    }
}
