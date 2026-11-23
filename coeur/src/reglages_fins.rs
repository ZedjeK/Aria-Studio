//! Les réglages fins des LLM et des workflows (onglet Réglages), comme en v2 (`app/reglages.py`) : rangés dans les
//! sections `llm` et `workflows` de `reglages.json`, ils ne contiennent que les écarts de l'utilisateur ; une clé
//! absente revient à la valeur d'origine. Chaque valeur est contrôlée avant d'être enregistrée.
//!
//! ```json
//! "llm": {"<fichier.gguf>": {"temperature_reecriture": 0.5, "contexte": 8192, "consignes": {"reecriture": "…"}}}
//! "workflows": {"<id>": {"cfg": 1.0, "sampler": "euler", "modele": "<checkpoint>",
//!                        "variantes": {"<checkpoint>": {"lora_workflow": false, "steps": 6}}}}
//! ```
//!
//! Pas encore portés : « Recommander » et « Adapter à ma machine », les résolutions du Montage, les consignes des
//! usages qui viendront avec leurs pages (précisions, vidéo, découpage).

use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::llm::{self, AUTO, MOTEUR_OFFICIEL, MOTEUR_PRISM, SANS_VISION};
use crate::workflows::{self, FORMATS, Workflow};
use crate::{Dossier, prompt, reglages};

/// Une consigne réécrite fait au plus 4 000 caractères.
pub const LONGUEUR_CONSIGNE: usize = 4000;
/// Les usages dont la consigne se réécrit déjà dans cette version.
pub const CONSIGNES: [&str; 5] = ["reecriture", "retouche", "references", "video", "decoupage"];
/// Samplers et ordonnanceurs de secours, tant que ComfyUI n'a jamais annoncé les siens.
pub const SAMPLERS_SECOURS: [&str; 14] = [
    "euler",
    "euler_ancestral",
    "heun",
    "dpm_2",
    "dpm_2_ancestral",
    "lms",
    "dpmpp_2m",
    "dpmpp_2m_sde",
    "dpmpp_3m_sde",
    "dpmpp_sde",
    "ddim",
    "uni_pc",
    "res_multistep",
    "lcm",
];
pub const SCHEDULERS_SECOURS: [&str; 9] = [
    "simple",
    "normal",
    "karras",
    "exponential",
    "sgm_uniform",
    "beta",
    "ddim_uniform",
    "linear_quadratic",
    "kl_optimal",
];

/// Un réglage proposé : son type, sa valeur d'origine, ses bornes ou ses choix.
#[derive(Debug, Clone, PartialEq)]
pub struct Def {
    pub cle: String,
    pub libelle: String,
    /// `entier`, `reel`, `booleen` ou `choix`.
    pub genre: &'static str,
    pub defaut: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub pas: Option<f64>,
    pub choix: Vec<Value>,
    pub aide: String,
    /// Les libellés des choix qui ne parlent pas d'eux-mêmes.
    pub libelles: Vec<(String, String)>,
}

/// Un réglage tel que l'interface le montre.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub cle: String,
    pub libelle: String,
    #[serde(rename = "type")]
    pub genre: &'static str,
    pub defaut: Value,
    pub valeur: Value,
    pub modifie: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub pas: Option<f64>,
    pub choix: Vec<Value>,
    pub aide: String,
    pub libelles: Map<String, Value>,
}

impl Def {
    fn nombre(cle: &str, libelle: &str, genre: &'static str, defaut: Value, bornes: (f64, f64, f64)) -> Self {
        Self {
            cle: cle.to_owned(),
            libelle: libelle.to_owned(),
            genre,
            defaut,
            min: Some(bornes.0),
            max: Some(bornes.1),
            pas: Some(bornes.2),
            choix: Vec::new(),
            aide: String::new(),
            libelles: Vec::new(),
        }
    }

    fn reel(cle: &str, libelle: &str, defaut: f64, bornes: (f64, f64, f64)) -> Self {
        Self::nombre(cle, libelle, "reel", json!(defaut), bornes)
    }

    fn entier(cle: &str, libelle: &str, defaut: i64, bornes: (f64, f64, f64)) -> Self {
        Self::nombre(cle, libelle, "entier", json!(defaut), bornes)
    }

    fn booleen(cle: &str, libelle: &str, defaut: bool) -> Self {
        Self {
            cle: cle.to_owned(),
            libelle: libelle.to_owned(),
            genre: "booleen",
            defaut: json!(defaut),
            min: None,
            max: None,
            pas: None,
            choix: Vec::new(),
            aide: String::new(),
            libelles: Vec::new(),
        }
    }

    fn choix(cle: &str, libelle: &str, defaut: &str, choix: &[String]) -> Self {
        let mut uniques: Vec<Value> = Vec::new();
        for c in std::iter::once(defaut.to_owned()).chain(choix.iter().cloned()) {
            if !uniques.contains(&json!(c)) {
                uniques.push(json!(c));
            }
        }
        Self {
            cle: cle.to_owned(),
            libelle: libelle.to_owned(),
            genre: "choix",
            defaut: json!(defaut),
            min: None,
            max: None,
            pas: None,
            choix: uniques,
            aide: String::new(),
            libelles: Vec::new(),
        }
    }

    fn aide(mut self, aide: &str) -> Self {
        aide.clone_into(&mut self.aide);
        self
    }

    fn libelles(mut self, libelles: &[(&str, &str)]) -> Self {
        self.libelles = libelles
            .iter()
            .map(|(v, l)| ((*v).to_owned(), (*l).to_owned()))
            .collect();
        self
    }

    /// La valeur contrôlée comme en v2 : type, bornes, choix ; un réel arrondi à quatre décimales.
    ///
    /// # Errors
    /// Le message de la v2.
    pub fn valider(&self, valeur: &Value) -> Result<Value, String> {
        match self.genre {
            "booleen" => valeur
                .as_bool()
                .map(Value::Bool)
                .ok_or_else(|| format!("{} : oui ou non attendu", self.libelle)),
            "choix" => {
                if self.choix.contains(valeur) {
                    Ok(valeur.clone())
                } else {
                    let affiche = valeur.as_str().map_or_else(|| valeur.to_string(), str::to_owned);
                    Err(format!("{} : « {affiche} » n'est pas proposé", self.libelle))
                }
            }
            _ => {
                let nombre = valeur
                    .as_f64()
                    .filter(|_| !valeur.is_boolean())
                    .ok_or_else(|| format!("{} : nombre attendu", self.libelle))?;
                let propre = if self.genre == "entier" {
                    if nombre.fract() != 0.0 {
                        return Err(format!("{} : nombre entier attendu", self.libelle));
                    }
                    nombre
                } else {
                    (nombre * 10_000.0).round() / 10_000.0
                };
                let (min, max) = (self.min.unwrap_or(f64::MIN), self.max.unwrap_or(f64::MAX));
                if !(min..=max).contains(&propre) {
                    return Err(format!("{} : entre {min} et {max}", self.libelle));
                }
                if self.genre == "entier" {
                    #[allow(clippy::cast_possible_truncation)] // un entier entre ses bornes
                    let entier = propre as i64;
                    Ok(json!(entier))
                } else {
                    Ok(json!(propre))
                }
            }
        }
    }

    /// La valeur en vigueur, ou celle d'origine si la sienne n'est plus valable (choix disparu, type changé).
    fn en_vigueur(&self, propre: Option<&Value>) -> Value {
        propre
            .and_then(|v| self.valider(v).ok())
            .unwrap_or_else(|| self.defaut.clone())
    }

    fn public(&self, valeur: Value) -> Param {
        Param {
            cle: self.cle.clone(),
            libelle: self.libelle.clone(),
            genre: self.genre,
            modifie: valeur != self.defaut,
            defaut: self.defaut.clone(),
            valeur,
            min: self.min,
            max: self.max,
            pas: self.pas,
            choix: self.choix.clone(),
            aide: self.aide.clone(),
            libelles: self
                .libelles
                .iter()
                .map(|(v, l)| (v.clone(), Value::String(l.clone())))
                .collect(),
        }
    }
}

/// Range les changements dans une section : valeur contrôlée, ou retirée si elle est nulle ou d'origine.
fn appliquer(
    section: &mut Map<String, Value>,
    defs: &[Def],
    changements: &Map<String, Value>,
) -> Result<(), String> {
    for (cle, valeur) in changements {
        let def = defs
            .iter()
            .find(|d| d.cle == *cle)
            .ok_or_else(|| format!("Réglage inconnu : {cle}"))?;
        if valeur.is_null() {
            section.remove(cle);
            continue;
        }
        let valeur = def.valider(valeur)?;
        if valeur == def.defaut {
            section.remove(cle);
        } else {
            section.insert(cle.clone(), valeur);
        }
    }
    Ok(())
}

fn temperature(cle: &str, libelle: &str, defaut: f64) -> Def {
    Def::reel(cle, libelle, defaut, (0.0, 2.0, 0.05))
}

/// Les réglages d'un LLM, comme en v2, avec les projecteurs d'images présents pour la vision.
pub fn defs_llm(modeles: &Path) -> Vec<Def> {
    let projecteurs = llm::projecteurs(modeles);
    let mut visions = vec![AUTO.to_owned(), SANS_VISION.to_owned()];
    visions.extend(projecteurs);
    vec![
        temperature("temperature_reecriture", "Température · écriture sans réflexion", 0.7)
            .aide("Idée → prompt quand la Réflexion est coupée, et les tours de la conversation"),
        temperature("temperature_retouche", "Température · consigne de retouche", 0.2),
        temperature("temperature_traduction", "Température · traduction des précisions", 0.2),
        Def::reel("top_p", "Top-p · sans réflexion", 0.95, (0.0, 1.0, 0.05))
            .aide("Garde les mots les plus probables jusqu'à ce total"),
        Def::entier("top_k", "Top-k · sans réflexion", 40, (0.0, 200.0, 1.0))
            .aide("Nombre de mots candidats (0 = sans limite)"),
        Def::reel("min_p", "Min-p · sans réflexion", 0.05, (0.0, 1.0, 0.01))
            .aide("Écarte les mots trop improbables"),
        Def::reel("presence_penalty", "Pénalité de présence · sans réflexion", 0.0, (-2.0, 2.0, 0.1))
            .aide("Positif = moins de répétitions"),
        temperature("temperature_reflexion", "Température · écriture avec réflexion", 0.7)
            .aide("Idée → prompt et découpage, quand la Réflexion est active"),
        Def::reel("top_p_reflexion", "Top-p · avec réflexion", 0.95, (0.0, 1.0, 0.05))
            .aide("Garde les mots les plus probables jusqu'à ce total"),
        Def::entier("top_k_reflexion", "Top-k · avec réflexion", 40, (0.0, 200.0, 1.0))
            .aide("Nombre de mots candidats (0 = sans limite)"),
        Def::reel("min_p_reflexion", "Min-p · avec réflexion", 0.05, (0.0, 1.0, 0.01))
            .aide("Écarte les mots trop improbables"),
        Def::reel(
            "presence_penalty_reflexion",
            "Pénalité de présence · avec réflexion",
            0.0,
            (-2.0, 2.0, 0.1),
        )
        .aide("Positif = moins de répétitions"),
        Def::entier("max_tokens", "Longueur maximale de réponse (tokens)", 512, (64.0, 4096.0, 64.0)),
        Def::entier(
            "contexte",
            "Contexte (tokens)",
            i64::from(llm::CONTEXTE_DEFAUT),
            (1024.0, 262_144.0, 1024.0),
        )
        .aide(
            "Plus grand = plus de VRAM ; changer redémarre llama-server. Avec la Réflexion ou la vision, Aria le \
             relève d'office à 16 384 au minimum",
        ),
        Def::booleen("reflexion", "Réflexion (thinking)", llm::REFLEXION_DEFAUT).aide(
            "Pour les modèles qui la proposent ; plus lent. Sert à la réécriture et au découpage, jamais aux courtes \
             traductions (retouche, précisions)",
        ),
        Def::choix(
            "moteur",
            "Moteur",
            AUTO,
            &[MOTEUR_OFFICIEL.to_owned(), MOTEUR_PRISM.to_owned()],
        )
        .libelles(&[
            (AUTO, "Automatique"),
            (MOTEUR_OFFICIEL, "llama.cpp officiel"),
            (MOTEUR_PRISM, "PrismML (ternaire)"),
        ])
        .aide(
            "Automatique : d'après le format du GGUF. PrismML lit les formats ternaires de Bonsai 2 et ses dérivés ; \
             changer redémarre llama-server",
        ),
        Def::choix("vision", "Vision (projecteur d'images)", AUTO, &visions)
            .libelles(&[(AUTO, "Automatique"), (SANS_VISION, "Aucune (aveugle)")])
            .aide(
                "Le LLM voit l'image à animer, à retoucher ou prise en référence. Automatique : le projecteur \
                 (mmproj) de la même famille, dans modeles/llm. Environ 0,6 Go de VRAM en plus",
            ),
    ]
}

/// Les samplers et ordonnanceurs annoncés par ComfyUI (`cache\comfyui_samplers.json`, écrit par la v2 comme par la
/// v3), sinon ceux de secours ; et s'ils viennent de ComfyUI.
pub fn options_sampler(dossier: &Dossier) -> (Vec<String>, Vec<String>, bool) {
    let lu = std::fs::read_to_string(dossier.cache().join("comfyui_samplers.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let liste = |v: Option<&Value>| -> Option<Vec<String>> {
        let l: Vec<String> = v?
            .as_array()?
            .iter()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect();
        (!l.is_empty()).then_some(l)
    };
    match lu
        .as_ref()
        .and_then(|v| Some((liste(v.get("samplers"))?, liste(v.get("schedulers"))?)))
    {
        Some((samplers, schedulers)) => (samplers, schedulers, true),
        None => (
            SAMPLERS_SECOURS.iter().map(|s| (*s).to_owned()).collect(),
            SCHEDULERS_SECOURS.iter().map(|s| (*s).to_owned()).collect(),
            false,
        ),
    }
}

/// Retient les samplers et ordonnanceurs que ComfyUI annonce, pour quand il est arrêté (comme la v2).
///
/// # Errors
/// Le cache ne s'écrit pas.
pub fn memoriser_samplers(
    dossier: &Dossier,
    samplers: &[String],
    schedulers: &[String],
) -> Result<(), String> {
    memoriser_samplers_dans(&dossier.cache(), samplers, schedulers)
}

/// [`memoriser_samplers`] dans ce dossier de cache.
///
/// # Errors
/// Le cache ne s'écrit pas.
pub fn memoriser_samplers_dans(
    cache: &Path,
    samplers: &[String],
    schedulers: &[String],
) -> Result<(), String> {
    if samplers.is_empty() || schedulers.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    std::fs::write(
        cache.join("comfyui_samplers.json"),
        json!({"samplers": samplers, "schedulers": schedulers}).to_string(),
    )
    .map_err(|e| e.to_string())
}

fn texte_noeud(wf: &Workflow, noeud: &str, entree: &str) -> Option<String> {
    wf.api
        .get(noeud)?
        .get("inputs")?
        .get(entree)?
        .as_str()
        .map(str::to_owned)
}

/// Les réglages des boutons de résolution du Montage : `resolution_<clé>`.
const PREFIXE_RESOLUTION: &str = "resolution_";

/// Un workflow vidéo : sa fiche a une section `video` (fps, durée, son), comme le teste la v2.
fn est_video(wf: &Workflow) -> bool {
    wf.fiche
        .get("video")
        .is_some_and(|v| !v.is_null() && v.as_bool() != Some(false))
}

fn reel_noeud(wf: &Workflow, noeud: &str, entree: &str) -> Option<f64> {
    wf.api.get(noeud)?.get("inputs")?.get(entree)?.as_f64()
}

/// Les réglages proposés pour ce workflow, selon ce que son graphe contient vraiment (comme en v2).
pub fn defs_workflow(
    wf: &Workflow,
    modeles: &Path,
    dossier: &Dossier,
    samplers: &(Vec<String>, Vec<String>),
) -> Vec<Def> {
    let defauts = wf.fiche.get("defauts");
    let defaut_f = |cle: &str, repli: f64| {
        defauts
            .and_then(|d| d.get(cle))
            .and_then(Value::as_f64)
            .unwrap_or(repli)
    };
    let mut defs = Vec::new();
    // Les steps se règlent par checkpoint quand le workflow a un checkpoint.
    if wf.entree("steps").is_some() && wf.entree("modele").is_none() {
        #[allow(clippy::cast_possible_truncation)] // des steps
        let steps = defaut_f("steps", 20.0) as i64;
        defs.push(
            Def::entier("steps", "Steps", steps, (1.0, 150.0, 1.0))
                .aide("Plus = plus lent ; les modèles Turbo / Lightning sont faits pour peu de steps"),
        );
    }
    let (liste_samplers, liste_schedulers) = samplers;
    if let Some(ks) = workflows::noeud_unique(&wf.api, "KSampler") {
        defs.push(
            Def::reel(
                "cfg",
                "CFG",
                reel_noeud(wf, &ks, "cfg").unwrap_or(1.0),
                (0.0, 30.0, 0.1),
            )
            .aide("Modèles distillés (Turbo, Lightning) : 1. Au-dessus, environ deux fois plus lent"),
        );
        if let Some(s) = texte_noeud(wf, &ks, "sampler_name") {
            defs.push(Def::choix("sampler", "Sampler", &s, liste_samplers));
        }
        if let Some(s) = texte_noeud(wf, &ks, "scheduler") {
            defs.push(Def::choix("scheduler", "Scheduler", &s, liste_schedulers));
        }
    } else {
        if let Some(s) = workflows::noeud_unique(&wf.api, "KSamplerSelect")
            .and_then(|n| texte_noeud(wf, &n, "sampler_name"))
        {
            defs.push(Def::choix("sampler", "Sampler", &s, liste_samplers));
        }
        if let Some(s) =
            workflows::noeud_unique(&wf.api, "BasicScheduler").and_then(|n| texte_noeud(wf, &n, "scheduler"))
        {
            defs.push(Def::choix("scheduler", "Scheduler", &s, liste_schedulers));
        }
    }
    if let Some(shift) =
        workflows::noeud_unique(&wf.api, "ModelSamplingAuraFlow").and_then(|n| reel_noeud(wf, &n, "shift"))
    {
        defs.push(Def::reel("shift", "Shift", shift, (0.0, 20.0, 0.1)));
    }
    if wf.entree("megapixels").is_some() {
        let montage = if est_video(wf) {
            " · Montage : la résolution choisie l'emporte"
        } else {
            ""
        };
        defs.push(
            Def::reel(
                "megapixels",
                "Mégapixels",
                defaut_f("megapixels", 1.0),
                (0.25, 6.0, 0.05),
            )
            .aide(&format!(
                "Taille de l'image : plus = plus de VRAM et plus lent{montage}"
            )),
        );
    }
    if est_video(wf) && wf.entree("megapixels").is_some() {
        // Les trois boutons de résolution du Montage : un nombre de mégapixels chacun, le même pour les workflows
        // vidéo de la famille (modifier_workflow les tient d'accord).
        for (rang, (cle, nom, _, megapixels, _, _)) in crate::montage::QUALITES.iter().enumerate() {
            defs.push(
                Def::reel(
                    &format!("{PREFIXE_RESOLUTION}{cle}"),
                    &format!("Résolution {} · d'origine {nom}", rang + 1),
                    *megapixels,
                    (0.25, 2.0, 0.05),
                )
                .aide(
                    "Mégapixels de ce bouton dans le Montage et les séquences. Plus = plus fin, plus de VRAM et \
                     nettement plus lent (le temps affiché au Montage suit, à titre indicatif).",
                ),
            );
        }
    }
    if wf.entree("ratio").is_some() {
        let ratio = defauts
            .and_then(|d| d.get("ratio"))
            .and_then(Value::as_str)
            .unwrap_or(FORMATS[0]);
        let formats: Vec<String> = FORMATS.iter().map(|f| (*f).to_owned()).collect();
        defs.push(Def::choix("ratio", "Format par défaut", ratio, &formats));
    }
    if let Some(mots) = wf
        .fiche
        .pointer("/profil_prompt/mots_max")
        .and_then(Value::as_i64)
    {
        defs.push(Def::entier(
            "mots_max",
            "Prompt : mots conseillés",
            mots,
            (20.0, 1000.0, 10.0),
        ));
    }
    if wf.entree("modele").is_some() {
        let reglages = reglages_workflow(dossier, &wf.id);
        let presents = wf.variantes(modeles, reglages.as_ref());
        let origine = wf.modele_origine().unwrap_or_default();
        let premier = presents.first().cloned().unwrap_or_else(|| origine.clone());
        let mut def = Def::choix("modele", "Checkpoint par défaut", &premier, &presents).aide(&format!(
            "Checkpoint utilisé partout (Plateau, Montage, séquences) quand rien d'autre n'est choisi. Dépose les tiens \
             dans modeles/{}",
            wf.dossier_modeles()
        ));
        if presents.is_empty() {
            let libelle = format!("{origine} (pas installé)");
            def = def.libelles(&[(origine.as_str(), libelle.as_str())]);
        }
        defs.push(def);
    }
    defs
}

/// Les réglages d'un checkpoint de la famille : LoRA du workflow gardée ou non, steps imposés.
pub fn defs_variante(wf: &Workflow) -> Vec<Def> {
    let mut defs = Vec::new();
    let loras: Vec<String> = wf
        .api
        .as_object()
        .into_iter()
        .flat_map(|g| g.values())
        .filter(|n| n.get("class_type").and_then(Value::as_str) == Some("LoraLoaderModelOnly"))
        .filter_map(|n| {
            n.pointer("/inputs/lora_name")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    if !loras.is_empty() {
        defs.push(
            Def::booleen("lora_workflow", "LoRA du workflow", true).aide(&format!(
                "{} : décoche-la si ce checkpoint l'intègre déjà (Turbo, Lightning…)",
                loras.join(", ")
            )),
        );
    }
    if wf.entree("steps").is_some() {
        defs.push(Def::entier("steps", "Steps", 0, (0.0, 150.0, 1.0)).aide(
            "Les steps de ce checkpoint, partout (Plateau, Montage, séquences). 0 : ceux de sa fiche si elle est \
             claire, sinon il repasse « À valider »",
        ));
    }
    defs
}

fn sous_section(section: Option<Map<String, Value>>, cle: &str) -> Option<Map<String, Value>> {
    match section?.remove(cle) {
        Some(Value::Object(m)) => Some(m),
        _ => None,
    }
}

fn reglages_workflow(dossier: &Dossier, id: &str) -> Option<Map<String, Value>> {
    sous_section(reglages::section(dossier, "workflows"), id)
}

/// Un LLM de l'onglet Réglages : ses réglages et ses consignes réécrites.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatLlm {
    pub fichier: String,
    pub nom: String,
    pub params: Vec<Param>,
    pub consignes: Vec<ConsigneLlm>,
}

/// Une consigne d'usage : celle d'origine, celle en vigueur, et si elle est réécrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsigneLlm {
    pub cle: String,
    pub defaut: String,
    pub valeur: String,
    pub modifie: bool,
}

/// Un checkpoint de la famille et ses réglages.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatVariante {
    pub fichier: String,
    pub origine: bool,
    pub params: Vec<Param>,
}

/// Un workflow de l'onglet Réglages.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatWorkflow {
    pub id: String,
    pub nom: String,
    pub tache: String,
    pub famille: String,
    pub dossier_modeles: String,
    pub params: Vec<Param>,
    pub variantes: Vec<EtatVariante>,
}

/// Ce que montre l'onglet Réglages → LLM et workflows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatReglages {
    pub llm: Vec<EtatLlm>,
    pub defaut_llm: String,
    /// Le LLM par défaut de la v2 a disparu : lequel le remplace.
    pub note_defaut: Option<String>,
    pub clause_mineurs: String,
    pub workflows: Vec<EtatWorkflow>,
    pub samplers_de_comfyui: bool,
}

fn etat_llm(dossier: &Dossier, modeles: &Path, fichier: &str) -> EtatLlm {
    let section = sous_section(reglages::section(dossier, "llm"), fichier).unwrap_or_default();
    let (moteur, raison) = llm::moteur(dossier, modeles, fichier);
    let (projecteur, pourquoi) = llm::projecteur(dossier, modeles, fichier);
    let params = defs_llm(modeles)
        .into_iter()
        .map(|d| {
            let valeur = d.en_vigueur(section.get(&d.cle));
            let mut p = d.public(valeur);
            if p.cle == "moteur" {
                let nom = if moteur == MOTEUR_PRISM {
                    "PrismML (ternaire)"
                } else {
                    "llama.cpp officiel"
                };
                p.aide = format!("En service : {nom}, {raison}. {}", p.aide);
            } else if p.cle == "vision" {
                p.aide = match &projecteur {
                    Some(proj) => format!("En service : {proj}, {pourquoi}. {}", p.aide),
                    None => format!("Aveugle : {pourquoi}. {}", p.aide),
                };
            }
            p
        })
        .collect();
    let consignes = CONSIGNES
        .iter()
        .filter_map(|usage| {
            let defaut = prompt::consigne_d_origine(usage)?;
            let propre = llm::consigne(dossier, fichier, usage);
            Some(ConsigneLlm {
                cle: (*usage).to_owned(),
                modifie: propre.is_some(),
                valeur: propre.unwrap_or_else(|| defaut.clone()),
                defaut,
            })
        })
        .collect();
    let nom = llm::FAVORIS.iter().find(|(f, _)| *f == fichier).map_or_else(
        || fichier.trim_end_matches(".gguf").to_owned(),
        |(_, n)| (*n).to_owned(),
    );
    EtatLlm {
        fichier: fichier.to_owned(),
        nom,
        params,
        consignes,
    }
}

fn etat_workflow(
    dossier: &Dossier,
    modeles: &Path,
    wf: &Workflow,
    samplers: &(Vec<String>, Vec<String>),
) -> EtatWorkflow {
    let section = reglages_workflow(dossier, &wf.id).unwrap_or_default();
    let params = defs_workflow(wf, modeles, dossier, samplers)
        .into_iter()
        .map(|d| {
            let valeur = d.en_vigueur(section.get(&d.cle));
            d.public(valeur)
        })
        .collect();
    let variantes = if wf.entree("modele").is_some() {
        let origine = wf.modele_origine();
        let toutes = sous_section(Some(section.clone()), "variantes").unwrap_or_default();
        wf.presents(modeles)
            .into_iter()
            .map(|fichier| {
                let propre = sous_section(Some(toutes.clone()), &fichier).unwrap_or_default();
                EtatVariante {
                    origine: origine.as_deref() == Some(fichier.as_str()),
                    params: defs_variante(wf)
                        .into_iter()
                        .map(|d| {
                            let valeur = d.en_vigueur(propre.get(&d.cle));
                            d.public(valeur)
                        })
                        .collect(),
                    fichier,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    EtatWorkflow {
        id: wf.id.clone(),
        nom: wf
            .fiche
            .get("nom")
            .and_then(Value::as_str)
            .unwrap_or(&wf.id)
            .to_owned(),
        tache: wf.tache().to_owned(),
        famille: wf.famille().to_owned(),
        dossier_modeles: wf.dossier_modeles(),
        params,
        variantes,
    }
}

/// L'état de l'onglet.
pub fn etat(dossier: &Dossier, modeles: &Path) -> EtatReglages {
    let (samplers, schedulers, de_comfyui) = options_sampler(dossier);
    let samplers = (samplers, schedulers);
    let defaut = llm::defaut(dossier, modeles);
    EtatReglages {
        llm: llm::disponibles(modeles)
            .iter()
            .map(|f| etat_llm(dossier, modeles, f))
            .collect(),
        note_defaut: (defaut != llm::LLM_ORIGINE).then(|| {
            format!(
                "{} n'est pas dans modeles/llm : {defaut} le remplace comme LLM par défaut (workflows sans LLM choisi, \
                 traductions).",
                llm::LLM_ORIGINE
            )
        }),
        defaut_llm: defaut,
        clause_mineurs: prompt::CLAUSE_MINEURS.to_owned(),
        workflows: workflows::catalogue()
            .0
            .iter()
            .map(|wf| etat_workflow(dossier, modeles, wf, &samplers))
            .collect(),
        samplers_de_comfyui: de_comfyui,
    }
}

/// Modifie les réglages et les consignes d'un LLM installé. Une consigne vide ou nulle revient à celle d'origine.
///
/// # Errors
/// GGUF absent, réglage ou consigne inconnus, valeur refusée, consigne trop longue, `reglages.json` qui ne s'écrit
/// pas.
pub fn modifier_llm(
    dossier: &Dossier,
    modeles: &Path,
    fichier: &str,
    changements: &Map<String, Value>,
    consignes: &Map<String, Value>,
) -> Result<EtatLlm, String> {
    if !llm::disponibles(modeles).iter().any(|f| f == fichier) {
        return Err(format!("GGUF absent de modeles/llm : {fichier}"));
    }
    let defs = defs_llm(modeles);
    let mut section = sous_section(reglages::section(dossier, "llm"), fichier).unwrap_or_default();
    appliquer(&mut section, &defs, changements)?;
    let mut textes = sous_section(Some(section.clone()), "consignes").unwrap_or_default();
    for (nom, texte) in consignes {
        if !CONSIGNES.contains(&nom.as_str()) {
            return Err(format!("Consigne inconnue : {nom}"));
        }
        match texte.as_str().map(str::trim).filter(|t| !t.is_empty()) {
            None => {
                textes.remove(nom);
            }
            Some(t) if t.chars().count() > LONGUEUR_CONSIGNE => {
                return Err(format!(
                    "Consigne trop longue (plus de {LONGUEUR_CONSIGNE} caractères)"
                ));
            }
            Some(t) => {
                textes.insert(nom.clone(), json!(t));
            }
        }
    }
    if textes.is_empty() {
        section.remove("consignes");
    } else {
        section.insert("consignes".to_owned(), Value::Object(textes));
    }
    reglages::modifier_section(dossier, "llm", |llms| {
        if section.is_empty() {
            llms.remove(fichier);
        } else {
            llms.insert(fichier.to_owned(), Value::Object(section));
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(etat_llm(dossier, modeles, fichier))
}

fn ecrire_workflow(dossier: &Dossier, id: &str, section: Map<String, Value>) -> Result<(), String> {
    reglages::modifier_section(dossier, "workflows", |wfs| {
        if section.is_empty() {
            wfs.remove(id);
        } else {
            wfs.insert(id.to_owned(), Value::Object(section));
        }
    })
    .map_err(|e| e.to_string())
}

/// Modifie les réglages d'un workflow.
///
/// # Errors
/// Workflow inconnu, réglage inconnu, valeur refusée, `reglages.json` qui ne s'écrit pas.
pub fn modifier_workflow(
    dossier: &Dossier,
    modeles: &Path,
    id: &str,
    changements: &Map<String, Value>,
) -> Result<EtatWorkflow, String> {
    let wf = workflows::obtenir(id)?;
    let (samplers, schedulers, _) = options_sampler(dossier);
    let samplers = (samplers, schedulers);
    let mut section = reglages_workflow(dossier, id).unwrap_or_default();
    appliquer(
        &mut section,
        &defs_workflow(&wf, modeles, dossier, &samplers),
        changements,
    )?;
    ecrire_workflow(dossier, id, section)?;
    // Les résolutions vidéo sont celles de la famille : image vers vidéo et texte vers vidéo restent d'accord.
    let resolutions: Map<String, Value> = changements
        .iter()
        .filter(|(cle, _)| cle.starts_with(PREFIXE_RESOLUTION))
        .map(|(cle, valeur)| (cle.clone(), valeur.clone()))
        .collect();
    if !resolutions.is_empty() && est_video(&wf) {
        for autre in workflows::catalogue().0 {
            if autre.id != wf.id
                && autre.famille() == wf.famille()
                && est_video(&autre)
                && autre.entree("megapixels").is_some()
            {
                let mut section = reglages_workflow(dossier, &autre.id).unwrap_or_default();
                appliquer(
                    &mut section,
                    &defs_workflow(&autre, modeles, dossier, &samplers),
                    &resolutions,
                )?;
                ecrire_workflow(dossier, &autre.id, section)?;
            }
        }
    }
    Ok(etat_workflow(dossier, modeles, &wf, &samplers))
}

/// Modifie les réglages d'un checkpoint de la famille d'un workflow.
///
/// # Errors
/// Workflow inconnu, checkpoint absent, réglage inconnu, valeur refusée, `reglages.json` qui ne s'écrit pas.
pub fn modifier_variante(
    dossier: &Dossier,
    modeles: &Path,
    id: &str,
    fichier: &str,
    changements: &Map<String, Value>,
) -> Result<EtatWorkflow, String> {
    let wf = workflows::obtenir(id)?;
    if !wf.presents(modeles).iter().any(|p| p == fichier) {
        return Err(format!(
            "Checkpoint absent de modeles/{} : {fichier}",
            wf.dossier_modeles()
        ));
    }
    let mut section = reglages_workflow(dossier, id).unwrap_or_default();
    let mut variantes = sous_section(Some(section.clone()), "variantes").unwrap_or_default();
    let mut propre = sous_section(Some(variantes.clone()), fichier).unwrap_or_default();
    appliquer(&mut propre, &defs_variante(&wf), changements)?;
    if propre.is_empty() {
        variantes.remove(fichier);
    } else {
        variantes.insert(fichier.to_owned(), Value::Object(propre));
    }
    if variantes.is_empty() {
        section.remove("variantes");
    } else {
        section.insert("variantes".to_owned(), Value::Object(variantes));
    }
    ecrire_workflow(dossier, id, section)?;
    let (samplers, schedulers, _) = options_sampler(dossier);
    Ok(etat_workflow(dossier, modeles, &wf, &(samplers, schedulers)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gguf::essai;
    use std::fs;

    fn lieux(temp: &tempfile::TempDir) -> (Dossier, std::path::PathBuf) {
        let modeles = temp.path().join("modeles");
        fs::create_dir_all(modeles.join("llm")).unwrap();
        (Dossier::a(temp.path()), modeles)
    }

    fn changements(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn valeurs_controlees_comme_la_v2() {
        let t = temperature("temperature_reecriture", "Température", 0.7);
        assert_eq!(t.valider(&json!(0.123_456)).unwrap(), json!(0.1235));
        assert_eq!(t.valider(&json!(3)).unwrap_err(), "Température : entre 0 et 2");
        assert_eq!(
            t.valider(&json!(true)).unwrap_err(),
            "Température : nombre attendu"
        );
        let k = Def::entier("top_k", "Top-k", 40, (0.0, 200.0, 1.0));
        assert_eq!(k.valider(&json!(20.0)).unwrap(), json!(20));
        assert_eq!(
            k.valider(&json!(2.5)).unwrap_err(),
            "Top-k : nombre entier attendu"
        );
        let b = Def::booleen("reflexion", "Réflexion", true);
        assert_eq!(
            b.valider(&json!("oui")).unwrap_err(),
            "Réflexion : oui ou non attendu"
        );
        let c = Def::choix("moteur", "Moteur", "auto", &["llama".to_owned()]);
        assert_eq!(c.valider(&json!("llama")).unwrap(), json!("llama"));
        assert_eq!(
            c.valider(&json!("ollama")).unwrap_err(),
            "Moteur : « ollama » n'est pas proposé"
        );
    }

    #[test]
    fn reglages_d_un_llm() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, modeles) = lieux(&temp);
        fs::write(modeles.join("llm").join("mistral.gguf"), essai::gguf(&[12])).unwrap();
        fs::write(dossier.reglages(), r#"{"fond": {"actif": true}}"#).unwrap();
        assert!(modifier_llm(&dossier, &modeles, "absent.gguf", &Map::new(), &Map::new()).is_err());
        let e = modifier_llm(
            &dossier,
            &modeles,
            "mistral.gguf",
            &changements(json!({"temperature_reecriture": 0.5, "top_k": 40, "reflexion": false})),
            &changements(json!({"reecriture": "  Ma consigne  "})),
        )
        .unwrap();
        let p = |cle: &str| e.params.iter().find(|p| p.cle == cle).unwrap().clone();
        assert_eq!(
            (
                p("temperature_reecriture").valeur,
                p("temperature_reecriture").modifie
            ),
            (json!(0.5), true)
        );
        assert!(!p("top_k").modifie);
        assert!(
            p("moteur")
                .aide
                .starts_with("En service : llama.cpp officiel, format standard.")
        );
        let reecriture = e.consignes.iter().find(|c| c.cle == "reecriture").unwrap();
        assert_eq!(
            (reecriture.valeur.as_str(), reecriture.modifie),
            ("Ma consigne", true)
        );
        // Au format de la v2, seulement les écarts ; le reste du fichier gardé.
        let relu: Value = serde_json::from_str(&fs::read_to_string(dossier.reglages()).unwrap()).unwrap();
        assert_eq!(
            relu["llm"]["mistral.gguf"],
            json!({"temperature_reecriture": 0.5, "reflexion": false, "consignes": {"reecriture": "Ma consigne"}})
        );
        assert_eq!(relu["fond"], json!({"actif": true}));
        // La consigne vide revient à l'origine, une valeur d'origine retire la clé.
        modifier_llm(
            &dossier,
            &modeles,
            "mistral.gguf",
            &changements(json!({"temperature_reecriture": 0.7, "reflexion": null})),
            &changements(json!({"reecriture": ""})),
        )
        .unwrap();
        let relu: Value = serde_json::from_str(&fs::read_to_string(dossier.reglages()).unwrap()).unwrap();
        assert!(relu.get("llm").is_none());
        assert_eq!(
            modifier_llm(
                &dossier,
                &modeles,
                "mistral.gguf",
                &changements(json!({"inconnu": 1})),
                &Map::new()
            )
            .unwrap_err(),
            "Réglage inconnu : inconnu"
        );
        assert!(
            modifier_llm(
                &dossier,
                &modeles,
                "mistral.gguf",
                &Map::new(),
                &changements(json!({"libre": "x"}))
            )
            .is_err()
        );
        let longue = "x".repeat(LONGUEUR_CONSIGNE + 1);
        assert!(
            modifier_llm(
                &dossier,
                &modeles,
                "mistral.gguf",
                &Map::new(),
                &changements(json!({"retouche": longue}))
            )
            .unwrap_err()
            .starts_with("Consigne trop longue")
        );
    }

    #[test]
    fn resolutions_video_de_la_famille() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, modeles) = lieux(&temp);
        // Réglées sur l'image vers vidéo, les résolutions valent aussi pour le texte vers vidéo de la famille.
        let e = modifier_workflow(
            &dossier,
            &modeles,
            "minimax_h3_i2v",
            &changements(json!({"resolution_compromis": 0.8})),
        )
        .unwrap();
        let r = e.params.iter().find(|p| p.cle == "resolution_compromis").unwrap();
        assert_eq!((r.valeur.clone(), r.modifie), (json!(0.8), true));
        assert!(e.params.iter().any(|p| p.cle == "resolution_rapide"));
        let t2v = reglages_workflow(&dossier, "minimax_h3_t2v").unwrap();
        assert_eq!(t2v["resolution_compromis"], json!(0.8));
        assert!(
            modifier_workflow(
                &dossier,
                &modeles,
                "minimax_h3_i2v",
                &changements(json!({"resolution_rapide": 3.0}))
            )
            .is_err()
        );
        // Une image n'a pas de boutons de résolution.
        let krea = workflows::obtenir("krea2_turbo").unwrap();
        let samplers = (Vec::new(), Vec::new());
        assert!(
            !defs_workflow(&krea, &modeles, &dossier, &samplers)
                .iter()
                .any(|d| d.cle.starts_with(PREFIXE_RESOLUTION))
        );
    }

    #[test]
    fn reglages_d_un_workflow_et_de_ses_checkpoints() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, modeles) = lieux(&temp);
        let e = etat(&dossier, &modeles);
        assert!(!e.samplers_de_comfyui);
        let krea = e.workflows.iter().find(|w| w.id == "krea2_turbo").unwrap();
        let cles: Vec<&str> = krea.params.iter().map(|p| p.cle.as_str()).collect();
        assert!(
            cles.contains(&"cfg")
                && cles.contains(&"sampler")
                && cles.contains(&"ratio")
                && cles.contains(&"modele")
        );
        assert!(!cles.contains(&"steps"));
        let modifie = modifier_workflow(
            &dossier,
            &modeles,
            "krea2_turbo",
            &changements(json!({"cfg": 2.5, "ratio": "16:9 (Widescreen)"})),
        )
        .unwrap();
        let cfg = modifie.params.iter().find(|p| p.cle == "cfg").unwrap();
        assert_eq!((cfg.valeur.clone(), cfg.modifie), (json!(2.5), true));
        assert!(
            modifier_workflow(
                &dossier,
                &modeles,
                "krea2_turbo",
                &changements(json!({"ratio": "5:4"}))
            )
            .is_err()
        );
        // Un checkpoint posé : ses réglages propres.
        let wf = workflows::obtenir("krea2_turbo").unwrap();
        let origine = wf.modele_origine().unwrap();
        let d = modeles.join(wf.dossier_modeles());
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(&origine), b"x").unwrap();
        fs::write(d.join("autre.safetensors"), b"x").unwrap();
        let e = modifier_variante(
            &dossier,
            &modeles,
            "krea2_turbo",
            "autre.safetensors",
            &changements(json!({"steps": 6})),
        )
        .unwrap();
        assert_eq!(e.variantes.len(), 2);
        let autre = e
            .variantes
            .iter()
            .find(|v| v.fichier == "autre.safetensors")
            .unwrap();
        assert_eq!(
            autre.params.iter().find(|p| p.cle == "steps").unwrap().valeur,
            json!(6)
        );
        assert!(e.variantes.iter().any(|v| v.origine));
        assert!(
            modifier_variante(
                &dossier,
                &modeles,
                "krea2_turbo",
                "absent.safetensors",
                &Map::new()
            )
            .is_err()
        );
        let section = reglages::section(&dossier, "workflows").unwrap();
        assert_eq!(
            section["krea2_turbo"]["variantes"]["autre.safetensors"],
            json!({"steps": 6})
        );
        assert_eq!(section["krea2_turbo"]["cfg"], json!(2.5));
        // Le préparateur de graphe lit les mêmes réglages.
        assert_eq!(
            wf.steps_de(section["krea2_turbo"].as_object(), "autre.safetensors"),
            6
        );
    }

    #[test]
    fn samplers_annonces_par_comfyui() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, _) = lieux(&temp);
        assert_eq!(options_sampler(&dossier).0.len(), SAMPLERS_SECOURS.len());
        memoriser_samplers(
            &dossier,
            &["euler".to_owned(), "nouveau".to_owned()],
            &["simple".to_owned()],
        )
        .unwrap();
        let (samplers, schedulers, de_comfyui) = options_sampler(&dossier);
        assert_eq!((samplers.len(), schedulers.len(), de_comfyui), (2, 1, true));
    }
}
