//! Réglages recommandés (onglet Réglages), comme en v2 (`app/optimiseur.py`) : « ⚡ Adapter à ma machine » pour les
//! LLM, « Recommander » pour un modèle d'image ou de vidéo.
//!
//! Rien n'est appliqué ici : on rend ce qui changerait (« actuel → proposé »), avec la raison et la source de chaque
//! ligne ; c'est l'utilisateur qui garde ou non, et l'interface passe ensuite par les commandes de réglages.
//!
//! Sources des recommandations, relevées sur les pages officielles des modèles :
//! - workflows : bloc `recommandations` de `workflows/<id>/fiche.yaml` ;
//! - LLM : bloc `recommandations` des fiches de `catalogue/llm.yaml`, complété par l'ancien fichier local
//!   `modeles/llm/recommandations.yaml` (clé = nom du GGUF) pour les GGUF hors catalogue.

use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::llm::{self, Lieux};
use crate::reglages_fins::{self, Def};
use crate::workflows::{self, Workflow};
use crate::{Dossier, carte, fiches, modeles};

const GO: f64 = 1_073_741_824.0;
/// Contexte 4096 et tampons de calcul, au-delà du poids du GGUF (comme Découvrir).
pub const SURCOUT_LLM_GO: f64 = 1.5;
/// Le contexte que [`SURCOUT_LLM_GO`] compte.
pub const CONTEXTE_BASE: u64 = 4096;
/// Cache du contexte (KV) par tranche de 1024 tokens au-delà de la base, en part du poids du GGUF : estimation (14B
/// dense ≈ 0,17 Go, 27B ≈ 0,3 Go) ; prudente pour les modèles à attention locale ou MoE, qui en demandent moins.
pub const KV_PAR_1024_PART_DU_POIDS: f64 = 0.02;
/// L'ancien fichier local des recommandations, dans `modeles/llm`.
const FICHIER_LLM: &str = "recommandations.yaml";

/// La VRAM au-delà du poids du GGUF : tampons, et cache du contexte qui grandit avec lui.
pub fn surcout_llm_go(poids_go: f64, contexte: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // quelques centaines de milliers de tokens au plus
    let au_dela = contexte.saturating_sub(CONTEXTE_BASE) as f64;
    SURCOUT_LLM_GO + au_dela / 1024.0 * poids_go * KV_PAR_1024_PART_DU_POIDS
}

/// La machine, telle que l'onglet la résume.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Materiel {
    pub gpu: String,
    pub vram_totale_go: f64,
    /// La part des autres applis (bureau, navigateur…), plafonnée pour ne pas compter un moteur d'Aria déjà chargé.
    pub vram_autres_go: f64,
    pub vram_libre_go: f64,
    pub ram_go: f64,
    pub coeurs: Option<usize>,
    pub threads: Option<usize>,
}

/// Un réglage qui changerait.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposition {
    /// `llm` ou `workflow`.
    #[serde(rename = "type")]
    pub genre: String,
    /// Le GGUF ou le workflow.
    pub cible: String,
    pub nom: String,
    pub cle: String,
    pub libelle: String,
    pub actuel: Value,
    pub propose: Value,
    pub raison: String,
    pub source: Option<String>,
}

/// « Recommander » pour un modèle d'image ou de vidéo.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommandations {
    pub workflow: String,
    pub nom: String,
    pub propositions: Vec<Proposition>,
    pub alertes: Vec<String>,
    pub sans_recommandation: bool,
}

/// « ⚡ Adapter à ma machine » pour les LLM.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adaptation {
    pub materiel: Materiel,
    pub propositions: Vec<Proposition>,
    pub alertes: Vec<String>,
    /// Les LLM sans recommandation officielle rangée : seul le matériel compte pour eux.
    pub sans_recommandation: Vec<String>,
}

fn go(octets: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // des Go, au dixième
    let g = octets as f64 / GO;
    (g * 10.0).round() / 10.0
}

/// Un nombre comme la v2 l'écrivait (`{:g}`) : sans zéros inutiles.
fn g(x: f64) -> String {
    let arrondi = (x * 100.0).round() / 100.0;
    format!("{arrondi}")
}

/// La carte, la RAM et le processeur.
pub fn materiel() -> Materiel {
    let totale = carte::vram_totale().unwrap_or(0);
    let autres = carte::vram_utilisee().unwrap_or(0).min(carte::AUTRES_MAX);
    Materiel {
        gpu: carte::lire(0).nom.unwrap_or_else(|| "carte inconnue".to_owned()),
        vram_totale_go: go(totale),
        vram_autres_go: go(autres),
        vram_libre_go: go(totale.saturating_sub(autres)),
        ram_go: go(carte::ram_totale()),
        coeurs: sysinfo::System::physical_core_count().ok(),
        threads: std::thread::available_parallelism()
            .ok()
            .map(std::num::NonZero::get),
    }
}

/// Mégapixels que la VRAM restante encaisse sans déborder (estimation prudente) ; jamais au-delà du natif.
pub fn plafond_megapixels(marge_go: f64, natif: f64) -> f64 {
    if marge_go >= 12.0 {
        natif
    } else if marge_go >= 6.0 {
        natif.min(2.0)
    } else {
        natif.min(1.0)
    }
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    v.get(cle).and_then(Value::as_str).map(str::to_owned)
}

/// Ajoute une proposition si ce réglage accepte la valeur et qu'elle change quelque chose.
#[allow(clippy::too_many_arguments)] // les champs d'une proposition, comme en v2
fn proposer(
    sortie: &mut Vec<Proposition>,
    genre: &str,
    cible: &str,
    nom: &str,
    def: &Def,
    actuel: &Value,
    propose: &Value,
    raison: String,
    source: Option<String>,
) {
    let Ok(propose) = def.valider(propose) else {
        return; // valeur que ce réglage n'accepte pas ici (sampler absent de ComfyUI…) : on ne propose rien
    };
    if &propose != actuel {
        sortie.push(Proposition {
            genre: genre.to_owned(),
            cible: cible.to_owned(),
            nom: nom.to_owned(),
            cle: def.cle.clone(),
            libelle: def.libelle.clone(),
            actuel: actuel.clone(),
            propose,
            raison,
            source,
        });
    }
}

/// Ce que la taille du modèle demande à la carte graphique : des notes, jamais des réglages imposés.
fn alertes_workflow(wf: &Workflow, defs: &[Def], actuelles: &Map<String, Value>, libre: f64) -> Vec<String> {
    let mut alertes = Vec::new();
    let reco = wf.fiche.get("recommandations");
    let nom = texte(&wf.fiche, "nom").unwrap_or_else(|| wf.id.clone());
    let estimee = wf
        .fiche
        .pointer("/materiel/vram_estimee_go")
        .and_then(Value::as_f64);
    if let Some(e) = estimee
        && e > libre
    {
        alertes.push(format!(
            "{nom} demande environ {} Go de VRAM pour {} Go libres : ComfyUI en décharge une partie en RAM, ça \
             fonctionne mais plus lentement.",
            g(e),
            g(libre)
        ));
    }
    let Some(natif) = reco
        .and_then(|r| r.get("resolution_native_mp"))
        .and_then(Value::as_f64)
    else {
        return alertes;
    };
    if wf.tache().ends_with("video") {
        // Vidéo : la taille coûte des minutes par clip ; c'est un choix de résolution (Montage), pas un réglage à
        // corriger. On informe sans proposer.
        let mp = actuelles
            .get("megapixels")
            .and_then(Value::as_f64)
            .map_or_else(|| "?".to_owned(), g);
        alertes.push(format!(
            "{nom} est entraîné vers {} MP. Dans le Montage, c'est la résolution choisie qui décide (1 MP ≈ natif) ; \
             ici, {mp} MP pour le Casting. Plus haut = plus fin mais nettement plus lent (voir les notes de sa fiche).",
            g(natif)
        ));
    } else if defs.iter().any(|d| d.cle == "megapixels") {
        let cible = plafond_megapixels(libre - estimee.unwrap_or(0.0), natif);
        if cible < natif {
            alertes.push(format!(
                "{nom} est entraîné jusqu'à {} MP ; avec ta VRAM, je conseille de rester à {} MP (au-delà, risque de \
                 débordement et lenteur).",
                g(natif),
                g(cible)
            ));
        }
    }
    alertes
}

/// « Recommander » : toutes les valeurs que la fiche du modèle recommande (celles déjà en place comprises : l'interface
/// les compare à ce que le formulaire contient, pas à ce qui est enregistré), et en notes ce que sa taille demande à
/// la carte graphique. Ne propose jamais la résolution : elle dépend de la carte, c'est une note.
///
/// # Errors
/// Workflow inconnu.
pub fn recommandations_workflow(
    dossier: &Dossier,
    modeles: &Path,
    id: &str,
) -> Result<Recommandations, String> {
    let wf = workflows::obtenir(id)?;
    let (samplers, schedulers, _) = reglages_fins::options_sampler(dossier);
    let defs = reglages_fins::defs_workflow(&wf, modeles, dossier, &(samplers, schedulers));
    let actuelles = reglages_fins::valeurs_workflow(dossier, modeles, &wf);
    let nom = texte(&wf.fiche, "nom").unwrap_or_else(|| wf.id.clone());
    let valeurs = wf
        .fiche
        .pointer("/recommandations/valeurs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut propositions = Vec::new();
    for r in &valeurs {
        let cle = texte(r, "cle").unwrap_or_default();
        // Les steps se valident par checkpoint : ils ne sont pas dans ces réglages.
        let Some(def) = defs.iter().find(|d| d.cle == cle) else {
            continue;
        };
        let Ok(valeur) = def.valider(r.get("valeur").unwrap_or(&Value::Null)) else {
            continue;
        };
        propositions.push(Proposition {
            genre: "workflow".to_owned(),
            cible: wf.id.clone(),
            nom: nom.clone(),
            cle: def.cle.clone(),
            libelle: def.libelle.clone(),
            actuel: actuelles.get(&def.cle).cloned().unwrap_or(Value::Null),
            propose: valeur,
            raison: texte(r, "note").unwrap_or_else(|| "Recommandation officielle".to_owned()),
            source: texte(r, "source"),
        });
    }
    Ok(Recommandations {
        workflow: wf.id.clone(),
        alertes: alertes_workflow(&wf, &defs, &actuelles, materiel().vram_libre_go),
        sans_recommandation: valeurs.is_empty(),
        nom,
        propositions,
    })
}

/// Celles des fiches du catalogue (livré avec Aria), complétées par l'ancien fichier local pour les GGUF ajoutés à la
/// main ; le catalogue l'emporte.
fn recommandations_llm(modeles: &Path) -> Map<String, Value> {
    let mut toutes = std::fs::read_to_string(modeles.join(modeles::DOSSIER_LLM).join(FICHIER_LLM))
        .ok()
        .and_then(|t| fiches::lire(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    for f in fiches::catalogue_llm().unwrap_or_default() {
        if let (Some(id), Some(reco)) = (texte(&f, "id"), f.get("recommandations")) {
            toutes.insert(id, reco.clone());
        }
    }
    toutes
}

/// (recommandations, modèle d'origine) : celles du GGUF, ou à défaut celles du modèle du catalogue dont il dérive,
/// reconnu au début de son nom : celui du projecteur qu'ils partagent (comme la vision).
fn recommandations_de(fichier: &str, recos: &Map<String, Value>) -> (Value, Option<String>) {
    if let Some(r) = recos.get(fichier).filter(|r| !r.is_null()) {
        return (r.clone(), None);
    }
    for f in fiches::catalogue_llm().unwrap_or_default() {
        let (Some(id), Some(vision)) = (texte(&f, "id"), f.pointer("/vision/nom").and_then(Value::as_str))
        else {
            continue;
        };
        if let Some(r) = recos.get(&id)
            && fichier
                .to_lowercase()
                .starts_with(&llm::famille(vision).to_lowercase())
        {
            return (r.clone(), texte(&f, "nom"));
        }
    }
    (Value::Null, None)
}

fn taille_go(chemin: &Path) -> f64 {
    std::fs::metadata(chemin).map_or(0.0, |m| go(m.len()))
}

/// « ⚡ Adapter à ma machine » : les LLM seulement (les modèles d'image et de vidéo ont leur « Recommander »).
pub fn adapter(lieux: &Lieux) -> Adaptation {
    let materiel = materiel();
    let libre = materiel.vram_libre_go;
    let (mut propositions, mut alertes, mut sans_recommandation) = (Vec::new(), Vec::new(), Vec::new());
    let recos = recommandations_llm(&lieux.modeles);
    let defs = reglages_fins::defs_llm(&lieux.modeles);
    let def = |cle: &str| defs.iter().find(|d| d.cle == cle);
    for fichier in llm::disponibles(&lieux.modeles) {
        let nom = reglages_fins::nom_llm(&fichier);
        let actuelles = reglages_fins::valeurs_llm(&lieux.dossier, &lieux.modeles, &fichier);
        let actuel = |cle: &str| actuelles.get(cle).cloned().unwrap_or(Value::Null);
        let (reco, origine) = recommandations_de(&fichier, &recos);
        let valeurs = reco
            .get("valeurs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if valeurs.is_empty() {
            sans_recommandation.push(nom.clone());
        }
        for r in &valeurs {
            let Some(d) = texte(r, "cle").and_then(|c| def(&c)) else {
                continue;
            };
            let mut raison = texte(r, "note").unwrap_or_else(|| "Recommandation officielle".to_owned());
            if let Some(o) = &origine {
                raison = format!("{raison} ; reprise de {o}, son modèle d'origine");
            }
            let source = texte(r, "source").or_else(|| texte(&reco, "source"));
            proposer(
                &mut propositions,
                "llm",
                &fichier,
                &nom,
                d,
                &actuel(&d.cle),
                r.get("valeur").unwrap_or(&Value::Null),
                raison,
                source,
            );
        }
        // Le contexte ne se réduit pas : llama.cpp place lui-même ce qui ne tient pas. On ne propose que de le
        // remonter au minimum que la Réflexion ou la vision exige ; le débordement, lui, est signalé.
        let Ok(lancement) = llm::lancement(lieux, Some(&fichier)) else {
            continue;
        };
        let mut taille = taille_go(&lancement.modele);
        let vision = lancement.vision.filter(|v| v.is_file());
        if let Some(v) = &vision {
            taille += taille_go(v);
        }
        let contexte = u64::from(lancement.contexte);
        let regle = actuel("contexte").as_u64().unwrap_or(contexte);
        let reflexion = actuel("reflexion").as_bool().unwrap_or(false);
        if let Some(d) = def("contexte")
            && regle < contexte
        {
            let cause = match (reflexion, vision.is_some()) {
                (true, true) => "La Réflexion et la vision",
                (false, true) => "La vision",
                (true, false) => "La Réflexion",
                (false, false) => "Le LLM",
            };
            proposer(
                &mut propositions,
                "llm",
                &fichier,
                &nom,
                d,
                &actuel("contexte"),
                &Value::from(contexte),
                format!(
                    "{cause} demande {contexte} tokens au minimum (consigne, images, réflexion, réponse) : Aria les \
                     lance déjà ainsi ; ce réglage le montre au lieu de {regle}"
                ),
                None,
            );
        }
        if taille + surcout_llm_go(taille, contexte) > libre {
            let avec = if vision.is_some() { ", vision comprise" } else { "" };
            let conseil = if reflexion
                && vision.is_none()
                && contexte > regle
                && taille + surcout_llm_go(taille, regle) <= libre
            {
                format!(
                    " Sans la Réflexion, le contexte retomberait à {regle} tokens et le modèle tiendrait."
                )
            } else {
                String::new()
            };
            alertes.push(format!(
                "{nom} ({taille:.1} Go{avec}, contexte {contexte}) dépasse ta VRAM libre ({} Go, estimation) : \
                 llama.cpp en place une partie sur le processeur. Peu d'effet pour un modèle MoE, nettement plus lent \
                 pour un modèle dense.{conseil}",
                g(libre)
            ));
        }
    }
    Adaptation {
        materiel,
        propositions,
        alertes,
        sans_recommandation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn surcout_et_plafond_comme_la_v2() {
        assert!((surcout_llm_go(10.0, 4096) - 1.5).abs() < 1e-9);
        // 16 384 tokens : 12 tranches de plus, 2 % du poids chacune.
        assert!((surcout_llm_go(10.0, 16_384) - (1.5 + 12.0 * 10.0 * 0.02)).abs() < 1e-9);
        assert!((plafond_megapixels(14.0, 4.2) - 4.2).abs() < 1e-9);
        assert!((plafond_megapixels(8.0, 4.2) - 2.0).abs() < 1e-9);
        assert!((plafond_megapixels(3.0, 4.2) - 1.0).abs() < 1e-9);
        assert!((plafond_megapixels(3.0, 0.98) - 0.98).abs() < 1e-9);
        assert_eq!(g(16.0), "16");
        assert_eq!(g(4.2), "4.2");
    }

    #[test]
    fn recommandations_d_un_derive() {
        let catalogue = fiches::catalogue_llm().unwrap();
        // Un LLM du catalogue qui a une vision et des recommandations : un GGUF dérivé, qui partage le début de nom de
        // son projecteur, reprend ses recommandations.
        let Some(f) = catalogue
            .iter()
            .find(|f| f.pointer("/vision/nom").is_some() && f.get("recommandations").is_some())
        else {
            return;
        };
        let id = texte(f, "id").unwrap();
        let famille = llm::famille(f.pointer("/vision/nom").and_then(Value::as_str).unwrap()).to_owned();
        let mut recos = Map::new();
        recos.insert(id.clone(), json!({"valeurs": [{"cle": "top_k", "valeur": 20}]}));
        let (r, origine) = recommandations_de(&format!("{famille}-Heretic-Q4.gguf"), &recos);
        assert_eq!(r["valeurs"][0]["cle"], "top_k");
        assert_eq!(origine, texte(f, "nom"));
        // Le sien d'abord ; un inconnu n'en a pas.
        recos.insert("propre.gguf".to_owned(), json!({"valeurs": []}));
        assert_eq!(recommandations_de("propre.gguf", &recos).1, None);
        assert!(recommandations_de("zzz.gguf", &recos).0.is_null());
    }

    #[test]
    fn recommander_un_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let modeles = temp.path().join("modeles");
        std::fs::create_dir_all(&modeles).unwrap();
        let r = recommandations_workflow(&dossier, &modeles, "krea2_turbo").unwrap();
        assert!(!r.sans_recommandation);
        // Les steps se valident par checkpoint : jamais proposés ici ; le CFG l'est, avec sa source.
        assert!(r.propositions.iter().all(|p| p.cle != "steps"));
        let cfg = r.propositions.iter().find(|p| p.cle == "cfg").unwrap();
        assert_eq!(
            (cfg.genre.as_str(), cfg.propose.clone()),
            ("workflow", json!(1.0))
        );
        assert!(cfg.source.is_some());
        assert!(recommandations_workflow(&dossier, &modeles, "inconnu").is_err());
    }

    #[test]
    fn alertes_d_un_gros_modele() {
        let wf = workflows::obtenir("krea2_turbo").unwrap();
        let defs = vec![];
        let peu = alertes_workflow(&wf, &defs, &Map::new(), 2.0);
        // Trop gros pour 2 Go libres, s'il a une estimation dans sa fiche.
        if wf
            .fiche
            .pointer("/materiel/vram_estimee_go")
            .and_then(Value::as_f64)
            .is_some()
        {
            assert!(peu.iter().any(|a| a.contains("décharge")));
        }
        let video = workflows::obtenir("minimax_h3_i2v").unwrap();
        let mut actuelles = Map::new();
        actuelles.insert("megapixels".to_owned(), json!(0.4));
        let notes = alertes_workflow(&video, &defs, &actuelles, 64.0);
        if video
            .fiche
            .pointer("/recommandations/resolution_native_mp")
            .is_some()
        {
            assert!(notes.iter().any(|a| a.contains("0.4 MP pour le Casting")));
        }
    }
}
