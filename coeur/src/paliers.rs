//! Banc de palier, comme en v2 (`app/paliers.py`) : la VRAM qu'un modèle demande vraiment, mesurée en simulant des
//! cartes plus petites. Un outil du mainteneur, lancé par le binaire `paliers` (`cargo run -p aria-coeur --bin
//! paliers`), pas une fonction de l'appli.
//!
//! Pour simuler une carte de P Go sur la carte de mesure, un « lest » (un petit processus PyTorch, lancé avec le Python
//! de ComfyUI portable) occupe réellement la VRAM en trop : ComfyUI, Llama et le pilote ne voient plus que P Go, bureau
//! Windows compris. Sous Windows, le pilote déborde en mémoire partagée au lieu d'échouer : une petite carte se voit
//! donc surtout au temps.
//!
//! Pour chaque modèle, du plus grand palier au plus petit, avec la même génération (le premier prompt du Casting ; la
//! vidéo en Rapide : 5 s, 0,4 MP, 8 steps) :
//! - minimum = le plus petit palier où la génération aboutit dans le temps imparti ;
//! - confort = le plus petit palier où elle prend au plus [`CONFORT`] fois le temps du plus grand palier.
//!
//! Un palier qui échoue arrête les paliers plus petits. Chaque palier commence par une génération de chauffe (seed
//! [`SEED`]), puis mesure une autre génération (seed [`SEED`] + 1) : la même seed sortirait du cache de ComfyUI.
//! Un LLM est lancé comme Aria le lance, son chargement compté à part de la réécriture.
//!
//! `--ecrire` reporte un banc dans les fiches livrées (`workflows/<id>/fiche.yaml`, `catalogue/llm.yaml`) : les
//! commentaires et le reste restent tels quels, chaque fiche est revalidée.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::comfy_serveur::ServeurComfy;
use crate::llama_serveur::ServeurLlama;
use crate::llm::Lieux;
use crate::workflows::{self, Workflow};
use crate::{Dossier, carte, casting, fiches, llm, prompt, reglages, securite};

const GO: u64 = 1 << 30;
/// Les cartes simulées, en Go.
pub const PALIERS: [u32; 3] = [16, 12, 8];
/// Le confort : au plus ce facteur du temps du plus grand palier.
pub const CONFORT: f64 = 1.5;
pub const SEED: u64 = 42;
/// Une génération est abandonnée au-delà de max(`LIMITE_MIN_S`, 4 × le temps du plus grand palier).
pub const LIMITE_MIN_S: f64 = 600.0;
/// Comme le garde-VRAM : au-delà, une autre appli tient la carte et les mesures seraient faussées.
pub const OCCUPATION_MAX_GO: f64 = 3.5;
/// Rapide : ce qu'un utilisateur d'une petite carte lancerait.
const VIDEO: [(&str, f64); 3] = [("duree", 5.0), ("megapixels", 0.4), ("steps", 8.0)];
/// Le lest : « + » ajoute 128 Mo, écrits (Windows ne réserve la mémoire qu'à l'usage) ; il s'arrête à la fin de son
/// entrée.
const LEST: &str = "
import sys, torch
morceaux = []
for ligne in sys.stdin:
    morceaux.append(torch.ones(128 << 20, dtype=torch.uint8, device=\"cuda\"))
    torch.cuda.synchronize()
    print(\"ok\", flush=True)
";

/// La mesure d'un palier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mesure {
    /// `ok`, `memoire`, `erreur` ou `trop_lent`.
    pub etat: String,
    pub temps_s: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub essais: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chargement_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jetons_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Mesure {
    fn echec(etat: &str, temps_s: f64, detail: &str) -> Self {
        Self {
            etat: etat.to_owned(),
            temps_s,
            essais: Vec::new(),
            chargement_s: None,
            jetons_s: None,
            detail: Some(detail.chars().take(300).collect()),
        }
    }
}

/// Les mesures d'un modèle, par palier.
pub type Paliers = BTreeMap<u32, Mesure>;

/// Un banc : la carte, la date, et les mesures de chaque workflow et de chaque LLM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resultats {
    pub date: String,
    pub carte: String,
    pub paliers: Vec<u32>,
    pub confort: f64,
    pub workflows: BTreeMap<String, Paliers>,
    pub llm: BTreeMap<String, Paliers>,
}

/// La VRAM libre que laisserait une carte de `palier` Go faisant tourner le même bureau : la carte de mesure
/// (`totale`) ramenée à palier/16 de sa taille, moins ce que le bureau occupe déjà.
pub fn libre_vise(palier: u32, totale: u64, occupee: u64) -> i64 {
    let ramenee = u128::from(totale) * u128::from(palier) / 16;
    i64::try_from(ramenee).unwrap_or(i64::MAX) - i64::try_from(occupee).unwrap_or(i64::MAX)
}

/// L'état d'une génération en erreur : un manque de mémoire, ou une autre erreur.
pub fn etat_erreur(message: &str) -> &'static str {
    let m = message.to_lowercase();
    let memoire = [
        "out of memory",
        "outofmemory",
        "cuda error: out",
        "allocation on device",
        "not enough memory",
    ];
    if memoire.iter().any(|motif| m.contains(motif)) {
        "memoire"
    } else {
        "erreur"
    }
}

/// (minimum, confort) d'après les mesures ; rien si le plus grand palier échoue.
pub fn conclure(paliers: &Paliers) -> (Option<u32>, Option<u32>) {
    let reussis: BTreeMap<u32, f64> = paliers
        .iter()
        .filter(|(_, m)| m.etat == "ok")
        .map(|(p, m)| (*p, m.temps_s))
        .collect();
    let Some(plus_grand) = paliers.keys().max() else {
        return (None, None);
    };
    let Some(reference) = reussis.get(plus_grand) else {
        return (None, None);
    };
    let minimum = reussis.keys().min().copied();
    let confort = reussis
        .iter()
        .filter(|(_, t)| **t <= CONFORT * reference)
        .map(|(p, _)| *p)
        .min();
    (minimum, confort)
}

fn temps(t: f64) -> String {
    let arrondi = (t * 10.0).round() / 10.0;
    if arrondi.fract() == 0.0 {
        format!("{arrondi:.1}")
    } else {
        arrondi.to_string()
    }
}

/// Le bloc `mesure` d'une fiche, en YAML, à l'indentation du bloc `materiel` (sans l'indentation de sa première ligne).
pub fn bloc_mesure(paliers: &Paliers, carte: &str, jour: &str, indentation: &str, fin: &str) -> String {
    let detail: Vec<String> = paliers
        .iter()
        .rev()
        .map(|(p, m)| {
            if m.etat == "ok" {
                format!("{p} Go : {} s", temps(m.temps_s))
            } else {
                format!("{p} Go : {}", m.etat)
            }
        })
        .collect();
    let liste: Vec<String> = PALIERS.iter().map(u32::to_string).collect();
    let methode = format!(
        "banc de palier (coeur/src/paliers.rs) : VRAM réellement occupée par un lest pour simuler {} Go ; confort = \
         au plus {CONFORT} × le temps du plus grand palier",
        liste.join(", ")
    );
    [
        "mesure:".to_owned(),
        format!("    carte: {carte}"),
        format!("    date: {jour}"),
        format!("    methode: \"{methode}\""),
        format!("    paliers: \"{}\"", detail.join(", ")),
    ]
    .join(&format!("{fin}{indentation}"))
}

/// Remplace `vram_min_go`, `vram_confort_go` et `mesure: null` dans le bloc `materiel` qui suit `ancre` dans le texte
/// d'une fiche ; les commentaires et le reste restent tels quels, fins de ligne comprises.
///
/// # Errors
/// Ancre ou bloc introuvable (fiche déjà mesurée, ou d'une autre forme).
pub fn reporter(
    texte: &str,
    ancre: &str,
    paliers: &Paliers,
    carte: &str,
    jour: &str,
) -> Result<String, String> {
    let debut = texte
        .find(ancre)
        .ok_or_else(|| format!("Ancre introuvable : « {ancre} »"))?;
    let fin = if texte.contains("\r\n") { "\r\n" } else { "\n" };
    // Les lignes après l'ancre, avec leur position dans le texte.
    let mut lignes: Vec<(usize, &str)> = Vec::new();
    let mut position = debut;
    for ligne in texte[debut..].split_inclusive('\n') {
        lignes.push((position, ligne));
        position += ligne.len();
    }
    let introuvable = || format!("Bloc materiel introuvable après « {ancre} »");
    let i = lignes
        .iter()
        .position(|(_, l)| l.trim_start().starts_with("vram_min_go: "))
        .ok_or_else(introuvable)?;
    let indentation: String = lignes[i].1.chars().take_while(|c| *c == ' ').collect();
    let suite = |j: usize, cle: &str| {
        lignes.get(j).is_some_and(|(_, l)| {
            l.strip_prefix(indentation.as_str())
                .is_some_and(|r| r.starts_with(cle))
        })
    };
    if !(suite(i + 1, "vram_confort_go: ") && suite(i + 2, "mesure: null")) {
        return Err(introuvable());
    }
    let (depart, _) = lignes[i];
    let (derniere, l) = lignes[i + 2];
    let arrivee = derniere + l.trim_end_matches(['\r', '\n']).len();
    let (minimum, confort) = conclure(paliers);
    let nombre = |v: Option<u32>| v.map_or_else(|| "null".to_owned(), |p| p.to_string());
    let nouveau = format!(
        "{indentation}vram_min_go: {}  # mesurée (banc de palier){fin}{indentation}vram_confort_go: {}{fin}{indentation}{}",
        nombre(minimum),
        nombre(confort),
        bloc_mesure(paliers, carte, jour, &indentation, fin)
    );
    Ok(format!("{}{nouveau}{}", &texte[..depart], &texte[arrivee..]))
}

/// Reporte un banc dans les fiches de workflow et le catalogue des LLM du dépôt `racine` ; chaque fiche est
/// revalidée avant d'être écrite.
///
/// # Errors
/// Fiche introuvable, bloc `materiel` absent ou déjà mesuré, ou fiche invalide après le report.
pub fn ecrire(racine: &Path, resultats: &Resultats) -> Result<(), String> {
    for (id, paliers) in &resultats.workflows {
        let f = racine.join("workflows").join(id).join("fiche.yaml");
        let texte = fs::read_to_string(&f).map_err(|e| format!("{} : {e}", f.display()))?;
        let texte = reporter(&texte, "materiel:", paliers, &resultats.carte, &resultats.date)?;
        let p = fiches::problemes_workflow(&fiches::lire(&texte)?);
        if !p.is_empty() {
            return Err(format!("{id} : {}", p.join(" ; ")));
        }
        fs::write(&f, texte).map_err(|e| format!("{} : {e}", f.display()))?;
    }
    if !resultats.llm.is_empty() {
        let f = racine.join("catalogue").join("llm.yaml");
        let mut texte = fs::read_to_string(&f).map_err(|e| format!("{} : {e}", f.display()))?;
        for (fichier, paliers) in &resultats.llm {
            texte = reporter(
                &texte,
                &format!("id: {fichier}"),
                paliers,
                &resultats.carte,
                &resultats.date,
            )?;
        }
        let donnees = fiches::lire(&texte)?;
        for fiche in donnees.get("llm").and_then(Value::as_array).into_iter().flatten() {
            let p = fiches::problemes_llm(fiche);
            if !p.is_empty() {
                let id = fiche.get("id").and_then(Value::as_str).unwrap_or("?");
                return Err(format!("llm.yaml, {id} : {}", p.join(" ; ")));
            }
        }
        fs::write(&f, texte).map_err(|e| format!("{} : {e}", f.display()))?;
    }
    Ok(())
}

// --- le lest ------------------------------------------------------------------------------------------------------

/// Occupe la VRAM pour simuler une carte de `palier` Go (rien à faire au palier de la carte de mesure) ; relâchée quand
/// elle disparaît.
struct Lest {
    process: Option<(Child, ChildStdin, BufReader<ChildStdout>)>,
}

impl Lest {
    fn poser(palier: u32, python: &Path) -> Result<Self, String> {
        let totale = carte::vram_totale().ok_or("VRAM illisible : il faut une carte NVIDIA")?;
        let occupee = carte::vram_utilisee().unwrap_or(0);
        if u128::from(palier) * u128::from(GO) * 100 >= u128::from(totale) * 98 {
            return Ok(Self { process: None }); // la carte de mesure elle-même
        }
        // Le lest ajoute 128 Mo à la demande jusqu'à ce que NVML voie la VRAM libre visée : PyTorch et NVML ne
        // comptent pas pareil sous Windows.
        let vise = libre_vise(palier, totale, occupee);
        let mut commande = Command::new(python);
        commande
            .args(["-c", LEST])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            commande.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut enfant = commande
            .spawn()
            .map_err(|e| format!("lest impossible à lancer ({}) : {e}", python.display()))?;
        let (Some(entree), Some(sortie)) = (enfant.stdin.take(), enfant.stdout.take()) else {
            let _ = enfant.kill();
            return Err("lest sans entrée ni sortie".to_owned());
        };
        let mut lest = Self {
            process: Some((enfant, entree, BufReader::new(sortie))),
        };
        loop {
            let libre =
                i64::try_from(totale.saturating_sub(carte::vram_utilisee().unwrap_or(0))).unwrap_or(i64::MAX);
            if libre <= vise + (64 << 20) {
                return Ok(lest);
            }
            let Some((_, entree, sortie)) = lest.process.as_mut() else {
                return Ok(lest);
            };
            let mut ligne = String::new();
            let ecrit = entree
                .write_all(b"+\n")
                .and_then(|()| entree.flush())
                .and_then(|()| sortie.read_line(&mut ligne));
            if ecrit.is_err() || !ligne.starts_with("ok") {
                return Err(format!(
                    "Le lest n'a pas pu occuper la VRAM pour simuler {palier} Go"
                ));
            }
        }
    }
}

impl Drop for Lest {
    fn drop(&mut self) {
        if let Some((mut enfant, entree, _)) = self.process.take() {
            drop(entree); // le lest attend la fin de son entrée pour s'arrêter
            let debut = Instant::now();
            while debut.elapsed() < Duration::from_secs(15) {
                if matches!(enfant.try_wait(), Ok(Some(_))) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            let _ = enfant.kill();
            let _ = enfant.wait();
        }
    }
}

// --- les mesures --------------------------------------------------------------------------------------------------

fn arrondi(secondes: f64) -> f64 {
    (secondes * 10.0).round() / 10.0
}

/// Ce qu'il faut pour mesurer : les dossiers d'Aria et le journal du banc.
pub struct Banc<'a> {
    pub dossier: &'a Dossier,
    pub data: &'a Path,
    pub modeles: &'a Path,
    pub journal: &'a mut dyn FnMut(&str),
}

/// La génération de référence : le premier prompt du Casting pour la tâche, Rapide pour la vidéo.
fn generer(comfy: &ServeurComfy, banc: &Banc<'_>, wf: &Workflow, limite: f64, seed: u64) -> Mesure {
    let debut = Instant::now();
    let echec =
        |etat: &str, detail: &str| Mesure::echec(etat, arrondi(debut.elapsed().as_secs_f64()), detail);
    let preparer = || -> Result<Value, String> {
        let tache = wf.tache();
        let essai = casting::jeu_de_prompts(tache)?
            .into_iter()
            .next()
            .ok_or_else(|| format!("Aucun prompt du Casting pour {tache}"))?;
        let mut valeurs = Map::new();
        valeurs.insert("prompt".to_owned(), json!(essai.prompt));
        valeurs.insert("seed".to_owned(), json!(seed));
        valeurs.insert("prefixe".to_owned(), json!("aria_paliers"));
        let video = tache.ends_with("video");
        if video {
            for (cle, valeur) in VIDEO {
                if wf.entree(cle).is_some() {
                    valeurs.insert(cle.to_owned(), json!(valeur));
                }
            }
        }
        if let Some(nom) = &essai.source {
            let octets =
                casting::source(banc.data, nom).ok_or_else(|| format!("Image de départ absente : {nom}"))?;
            valeurs.insert("image".to_owned(), json!(comfy.televerser(nom, &octets)?));
        }
        let reglage = reglages::section(banc.dossier, "workflows")
            .and_then(|s| s.get(&wf.id).and_then(Value::as_object).cloned());
        let filtre = securite::filtre(banc.dossier).map_err(|e| e.to_string())?;
        let mut api = wf.preparer(&valeurs, banc.modeles, reglage.as_ref(), &filtre)?;
        if video {
            wf.ajouter_affiche(&mut api, "aria_paliers")?;
        }
        Ok(api)
    };
    let api = match preparer() {
        Ok(api) => api,
        Err(e) => return echec("erreur", &e),
    };
    let annule = AtomicBool::new(false);
    let fini = AtomicBool::new(false);
    let resultat = thread::scope(|s| {
        s.spawn(|| {
            while !fini.load(Ordering::Relaxed) {
                if debut.elapsed().as_secs_f64() >= limite {
                    annule.store(true, Ordering::Relaxed); // ComfyUI retire l'exécution et l'interrompt
                    return;
                }
                thread::sleep(Duration::from_millis(200));
            }
        });
        let r = comfy.executer(&api, &mut |_, _| {}, &annule);
        fini.store(true, Ordering::Relaxed);
        r
    });
    match resultat {
        _ if annule.load(Ordering::Relaxed) => echec("trop_lent", "au-delà du temps imparti"),
        Ok(sorties) if sorties.is_empty() => echec("erreur", "aucune sortie"),
        Ok(_) => Mesure {
            etat: "ok".to_owned(),
            temps_s: arrondi(debut.elapsed().as_secs_f64()),
            essais: Vec::new(),
            chargement_s: None,
            jetons_s: None,
            detail: None,
        },
        Err(e) => echec(etat_erreur(&e), &e),
    }
}

/// La même génération à chaque palier, ComfyUI relancé sous le lest : une chauffe, puis la mesure avec une autre seed.
pub fn mesurer_workflow(comfy: &mut ServeurComfy, banc: &mut Banc<'_>, wf: &Workflow) -> Paliers {
    let python = comfy.lancement().python();
    let mut resultats = Paliers::new();
    let mut reference: Option<f64> = None;
    for palier in PALIERS {
        let limite = reference.map_or(3.0 * LIMITE_MIN_S, |r| (4.0 * r).max(LIMITE_MIN_S));
        let mesure = match Lest::poser(palier, &python) {
            Err(e) => Mesure::echec("erreur", 0.0, &e),
            Ok(_lest) => {
                let mesure = match comfy.demarrer(Duration::from_secs(600)) {
                    Err(e) => Mesure::echec("erreur", 0.0, &e),
                    Ok(()) => {
                        let mut essais = vec![generer(comfy, banc, wf, limite, SEED)];
                        if essais[0].etat == "ok" {
                            essais.push(generer(comfy, banc, wf, limite, SEED + 1));
                        }
                        let temps: Vec<f64> = essais.iter().map(|e| e.temps_s).collect();
                        let mut derniere = essais
                            .pop()
                            .unwrap_or_else(|| Mesure::echec("erreur", 0.0, "aucun essai"));
                        derniere.essais = temps;
                        derniere
                    }
                };
                comfy.arreter();
                mesure
            }
        };
        (banc.journal)(&format!(
            "{} · {palier} Go : {} en {} s",
            wf.id, mesure.etat, mesure.temps_s
        ));
        let ok = mesure.etat == "ok";
        let temps_s = mesure.temps_s;
        resultats.insert(palier, mesure);
        if !ok {
            break;
        }
        reference = reference.or(Some(temps_s));
    }
    resultats
}

/// Une réécriture (la première idée du Casting LLM) à chaque palier, le LLM lancé comme Aria le lance ; `temps_s` : la
/// réécriture seule, le chargement compté à part.
pub fn mesurer_llm(
    llama: &mut ServeurLlama,
    lieux: &Lieux,
    python: &Path,
    fichier: &str,
    banc: &mut Banc<'_>,
) -> Paliers {
    let mut resultats = Paliers::new();
    for palier in PALIERS {
        let debut = Instant::now();
        let echec = |e: &str| Mesure::echec(etat_erreur(e), arrondi(debut.elapsed().as_secs_f64()), e);
        let mesure = match Lest::poser(palier, python) {
            Err(e) => Mesure::echec("erreur", 0.0, &e),
            Ok(_lest) => {
                let mesurer = |llama: &mut ServeurLlama| -> Result<Mesure, String> {
                    let idee = casting::jeu_de_prompts("reecriture")?
                        .into_iter()
                        .next()
                        .ok_or("Aucune idée du Casting LLM")?;
                    let images = prompt::Images {
                        references: 0,
                        vues: &[],
                    };
                    let mut ecriture =
                        prompt::preparer_reecriture(lieux, &idee.prompt, Some(fichier), None, None, &images)?;
                    ecriture.demande.seed = Some(SEED);
                    ecriture.demande.temperature = 0.0;
                    let chargement = Instant::now();
                    llama.demarrer(ecriture.lancement.clone(), Duration::from_secs(600))?;
                    let chargement_s = arrondi(chargement.elapsed().as_secs_f64());
                    let ecrit = Instant::now();
                    let reponse = llama.generer(&ecriture.demande)?;
                    Ok(Mesure {
                        etat: "ok".to_owned(),
                        temps_s: arrondi(ecrit.elapsed().as_secs_f64()),
                        essais: Vec::new(),
                        chargement_s: Some(chargement_s),
                        jetons_s: Some(arrondi(reponse.ecriture_tps)),
                        detail: None,
                    })
                };
                let mesure = mesurer(llama).unwrap_or_else(|e| echec(&e));
                llama.arreter();
                mesure
            }
        };
        (banc.journal)(&format!(
            "{fichier} · {palier} Go : {} en {} s",
            mesure.etat, mesure.temps_s
        ));
        let ok = mesure.etat == "ok";
        resultats.insert(palier, mesure);
        if !ok {
            break;
        }
    }
    resultats
}

/// Les workflows et les LLM à mesurer : ceux demandés (tout le catalogue sinon), les images d'abord (plus courtes).
pub fn cibles(modeles: &Path, ids: &[String]) -> (Vec<Workflow>, Vec<String>) {
    let voulu = |id: &str| ids.is_empty() || ids.iter().any(|i| i == id);
    let mut wfs: Vec<Workflow> = workflows::catalogue()
        .0
        .into_iter()
        .filter(|wf| voulu(&wf.id))
        .collect();
    wfs.sort_by_key(|wf| wf.tache().ends_with("video"));
    let installes = llm::disponibles(modeles);
    let gguf = fiches::catalogue_llm()
        .unwrap_or_default()
        .iter()
        .filter_map(|f| f.get("id").and_then(Value::as_str).map(str::to_owned))
        .filter(|id| voulu(id) && installes.contains(id))
        .collect();
    (wfs, gguf)
}

/// Le dossier d'un banc : `data\bancs\paliers\<date>`.
pub fn dossier_bancs(data: &Path) -> PathBuf {
    data.join("bancs").join("paliers")
}

/// Mesure les workflows et les LLM demandés (tout le catalogue par défaut) ; écrit `resultats.json` au fil de l'eau.
///
/// # Errors
/// La carte déjà occupée, un moteur d'Aria déjà lancé, ou le dossier du banc qui ne s'écrit pas.
pub fn banc(
    comfy: &mut ServeurComfy,
    llama: &mut ServeurLlama,
    lieux: &Lieux,
    data: &Path,
    ids: &[String],
    journal: &mut dyn FnMut(&str),
) -> Result<Resultats, String> {
    let occupee = carte::vram_utilisee().ok_or("VRAM illisible : il faut une carte NVIDIA")?;
    #[allow(clippy::cast_precision_loss)] // des Go de VRAM
    let occupee_go = occupee as f64 / GO as f64;
    if occupee_go > OCCUPATION_MAX_GO {
        return Err(format!(
            "La carte est déjà occupée ({occupee_go:.1} Go) : ferme les autres applis, les mesures seraient faussées"
        ));
    }
    if comfy.est_pret() {
        return Err("Un ComfyUI tourne déjà sur le port d'Aria : quitte Aria pendant le banc".to_owned());
    }
    if llama.est_pret() {
        return Err(
            "Un llama-server tourne déjà sur le port d'Aria : quitte Aria pendant le banc".to_owned(),
        );
    }
    let (wfs, gguf) = cibles(&lieux.modeles, ids);
    let dossier = dossier_bancs(data).join(Local::now().format("%Y-%m-%d_%H%M%S").to_string());
    fs::create_dir_all(&dossier).map_err(|e| format!("{} : {e}", dossier.display()))?;
    let nom = carte::lire(0).nom.unwrap_or_else(|| "carte inconnue".to_owned());
    let vram = carte::vram_totale().unwrap_or(0) / GO;
    let ram = carte::ram_totale() / GO;
    let mut resultats = Resultats {
        date: Local::now().format("%Y-%m-%d").to_string(),
        carte: format!("{nom} ({vram} Go), {ram} Go de RAM"),
        paliers: PALIERS.to_vec(),
        confort: CONFORT,
        workflows: BTreeMap::new(),
        llm: BTreeMap::new(),
    };
    let sauver = |r: &Resultats| {
        if let Ok(t) = serde_json::to_string_pretty(r) {
            let _ = fs::write(dossier.join("resultats.json"), t);
        }
    };
    let python = comfy.lancement().python();
    let mut banc = Banc {
        dossier: &lieux.dossier,
        data,
        modeles: &lieux.modeles,
        journal,
    };
    for wf in &wfs {
        if !wf.manquants(&lieux.modeles).is_empty() {
            (banc.journal)(&format!("{} : fichiers manquants, pas mesuré", wf.id));
            continue;
        }
        let paliers = mesurer_workflow(comfy, &mut banc, wf);
        (banc.journal)(&format!(
            "{} : minimum, confort = {:?}",
            wf.id,
            conclure(&paliers)
        ));
        resultats.workflows.insert(wf.id.clone(), paliers);
        sauver(&resultats);
    }
    for fichier in &gguf {
        let paliers = mesurer_llm(llama, lieux, &python, fichier, &mut banc);
        (banc.journal)(&format!(
            "{fichier} : minimum, confort = {:?}",
            conclure(&paliers)
        ));
        resultats.llm.insert(fichier.clone(), paliers);
        sauver(&resultats);
    }
    Ok(resultats)
}

/// Le dernier banc écrit (le nom de dossier le plus grand : sa date).
pub fn dernier_banc(data: &Path) -> Option<PathBuf> {
    fs::read_dir(dossier_bancs(data))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("resultats.json").is_file())
        .max()
}

/// Les résultats d'un banc écrit.
///
/// # Errors
/// Fichier absent ou illisible.
pub fn lire_banc(dossier: &Path) -> Result<Resultats, String> {
    let f = dossier.join("resultats.json");
    let t = fs::read_to_string(&f).map_err(|e| format!("{} : {e}", f.display()))?;
    serde_json::from_str(&t).map_err(|e| format!("{} : {e}", f.display()))
}

/// Une ligne du journal du banc, horodatée.
pub fn horodater(ligne: &str) -> String {
    format!("{} {ligne}", Local::now().format("%H:%M:%S"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesure(etat: &str, temps_s: f64) -> Mesure {
        Mesure {
            etat: etat.to_owned(),
            temps_s,
            essais: Vec::new(),
            chargement_s: None,
            jetons_s: None,
            detail: None,
        }
    }

    fn paliers(liste: &[(u32, &str, f64)]) -> Paliers {
        liste.iter().map(|(p, e, t)| (*p, mesure(e, *t))).collect()
    }

    #[test]
    fn minimum_et_confort() {
        // Comme en v2 : le confort tient au plus 1,5 fois le temps du plus grand palier.
        let p = paliers(&[(16, "ok", 100.0), (12, "ok", 140.0), (8, "ok", 400.0)]);
        assert_eq!(conclure(&p), (Some(8), Some(12)));
        let p = paliers(&[(16, "ok", 100.0), (12, "memoire", 30.0)]);
        assert_eq!(conclure(&p), (Some(16), Some(16)));
        assert_eq!(conclure(&paliers(&[(16, "trop_lent", 1800.0)])), (None, None));
        assert_eq!(conclure(&Paliers::new()), (None, None));
    }

    #[test]
    fn vram_visee_et_erreurs() {
        let g = GO;
        // Une 4080 (16 Go) dont le bureau occupe 1 Go : une carte de 8 Go laisserait 7 Go libres.
        assert_eq!(libre_vise(8, 16 * g, g), i64::try_from(7 * g).unwrap());
        assert_eq!(libre_vise(16, 16 * g, g), i64::try_from(15 * g).unwrap());
        assert_eq!(etat_erreur("CUDA error: out of memory"), "memoire");
        assert_eq!(etat_erreur("Allocation on device 0 would exceed"), "memoire");
        assert_eq!(etat_erreur("nœud inconnu"), "erreur");
        assert_eq!(etat_erreur("torch.OutOfMemoryError"), "memoire");
    }

    const FICHE: &str = "materiel:\n    vram_estimee_go: 19.5  # poids chargés\n    vram_min_go: null  # pas encore fait\n    vram_confort_go: null\n    mesure: null\n    cartes_verifiees: [RTX 4080 16 Go]\nprofil_prompt:\n    style: prose\n";

    #[test]
    fn report_dans_une_fiche() {
        let p = paliers(&[(16, "ok", 100.0), (12, "ok", 120.5), (8, "memoire", 10.0)]);
        let r = reporter(
            FICHE,
            "materiel:",
            &p,
            "RTX 4080 (16 Go), 64 Go de RAM",
            "2026-11-23",
        )
        .unwrap();
        assert!(r.contains(
            "    vram_min_go: 12  # mesurée (banc de palier)\n    vram_confort_go: 12\n    mesure:\n"
        ));
        assert!(r.contains("        carte: RTX 4080 (16 Go), 64 Go de RAM\n        date: 2026-11-23\n"));
        assert!(r.contains("        paliers: \"16 Go : 100.0 s, 12 Go : 120.5 s, 8 Go : memoire\""));
        // Le reste de la fiche ne change pas, et le résultat se relit.
        assert!(r.starts_with("materiel:\n    vram_estimee_go: 19.5  # poids chargés\n"));
        assert!(r.ends_with("    cartes_verifiees: [RTX 4080 16 Go]\nprofil_prompt:\n    style: prose\n"));
        let lu = fiches::lire(&r).unwrap();
        assert_eq!(lu["materiel"]["vram_min_go"], json!(12));
        assert_eq!(lu["materiel"]["mesure"]["date"], json!("2026-11-23"));
        // Une fiche déjà mesurée, ou une ancre absente : refusé.
        assert!(reporter(&r, "materiel:", &p, "x", "y").is_err());
        assert!(reporter(FICHE, "id: absent", &p, "x", "y").is_err());
        // Les fins de ligne Windows restent.
        let crlf = FICHE.replace('\n', "\r\n");
        let r = reporter(&crlf, "materiel:", &p, "x", "y").unwrap();
        assert!(!r.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn report_d_un_llm_dans_le_catalogue() {
        let catalogue = "llm:\n-   id: a.gguf\n    materiel:\n        vram_min_go: null\n        vram_confort_go: null\n        mesure: null\n-   id: b.gguf\n    materiel:\n        vram_min_go: null\n        vram_confort_go: null\n        mesure: null\n";
        let p = paliers(&[(16, "ok", 10.0)]);
        let r = reporter(catalogue, "id: b.gguf", &p, "x", "y").unwrap();
        let lu = fiches::lire(&r).unwrap();
        assert!(lu["llm"][0]["materiel"]["vram_min_go"].is_null());
        assert_eq!(lu["llm"][1]["materiel"]["vram_min_go"], json!(16));
    }

    #[test]
    fn banc_relu() {
        let temp = tempfile::tempdir().unwrap();
        let d = dossier_bancs(temp.path());
        for nom in ["2026-11-01_100000", "2026-11-20_090000"] {
            fs::create_dir_all(d.join(nom)).unwrap();
            let r = Resultats {
                date: nom[..10].to_owned(),
                carte: "x".to_owned(),
                paliers: PALIERS.to_vec(),
                confort: CONFORT,
                workflows: BTreeMap::from([("krea2_turbo".to_owned(), paliers(&[(16, "ok", 9.0)]))]),
                llm: BTreeMap::new(),
            };
            fs::write(
                d.join(nom).join("resultats.json"),
                serde_json::to_string(&r).unwrap(),
            )
            .unwrap();
        }
        let dernier = dernier_banc(temp.path()).unwrap();
        assert!(dernier.ends_with("2026-11-20_090000"));
        let r = lire_banc(&dernier).unwrap();
        assert_eq!(r.workflows["krea2_turbo"][&16].temps_s, 9.0);
    }
}
