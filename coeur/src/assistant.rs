//! L'assistant de premier lancement, comme en v2 (`app/assistant.py`) : packs à cocher (`catalogue/packs.yaml`),
//! licences, téléchargement, favoris.
//!
//! Les fichiers d'un pack sont ceux des fiches (`workflows/*/fiche.yaml`, `catalogue/llm.yaml`) : adresse, taille et
//! SHA-256 y sont figés, aucune adresse ne vient de l'interface. Ils partent dans la file du téléchargeur habituel
//! (reprise, empreinte vérifiée, source et licence au registre), seulement une fois la licence de chaque pack
//! acceptée. Ce qui est retenu va dans `reglages.json`, section `assistant` : les licences acceptées (date), les packs
//! et la date de fin.

use std::fmt;
use std::marker::PhantomData;
use std::path::Path;

use chrono::Local;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};

use crate::installations::page_de;
use crate::moteurs::{EtatMoteur, Statut};
use crate::plateau::{Demande, LIBERTE_DEFAUT};
use crate::telechargement::Fiche;
use crate::{Dossier, fiches, llm, modeles, reglages, workflows};

/// Le catalogue des packs, livré avec l'appli.
pub const CATALOGUE: &str = include_str!("../../catalogue/packs.yaml");
/// La place laissée libre après le téléchargement (Windows, rushes, fichiers temporaires).
pub const MARGE: u64 = 10 * GIO;
const GIO: u64 = 1 << 30;
const SECTION: &str = "assistant";

/// Une table YAML lue dans son ordre (l'ordre des packs décide des favoris : un pack plus bas l'emporte).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ordonne<V>(pub Vec<(String, V)>);

impl<V> Default for Ordonne<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordonne<V> {
    fn deserialize<D: Deserializer<'de>>(lecteur: D) -> Result<Self, D::Error> {
        struct Lecture<V>(PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for Lecture<V> {
            type Value = Ordonne<V>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("une table")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut table: A) -> Result<Self::Value, A::Error> {
                let mut sortie = Vec::new();
                while let Some((cle, valeur)) = table.next_entry::<String, V>()? {
                    sortie.push((cle, valeur));
                }
                Ok(Ordonne(sortie))
            }
        }

        lecteur.deserialize_map(Lecture(PhantomData))
    }
}

/// Un pack, tel que `catalogue/packs.yaml` le décrit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackCatalogue {
    pub nom: String,
    pub usage: String,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub llm: Vec<String>,
    #[serde(default)]
    pub moteurs: Vec<String>,
    #[serde(default)]
    pub conseille: bool,
    #[serde(default)]
    pub facultatif: bool,
    #[serde(default)]
    pub testee: Option<String>,
    /// `section → clé → valeur` : réglés à la fin, si ce qu'ils désignent est installé.
    #[serde(default)]
    pub favoris: Ordonne<Ordonne<String>>,
}

#[derive(Deserialize)]
struct FichierPacks {
    packs: Ordonne<PackCatalogue>,
}

/// Les packs, dans l'ordre du catalogue.
///
/// # Errors
/// Le catalogue livré ne se lit pas.
pub fn catalogue() -> Result<Vec<(String, PackCatalogue)>, String> {
    serde_saphyr::from_str::<FichierPacks>(CATALOGUE)
        .map(|f| f.packs.0)
        .map_err(|e| format!("catalogue des packs illisible : {e}"))
}

/// Un fichier d'un pack, tel que sa fiche le décrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FichierPack {
    pub dossier: String,
    pub nom: String,
    pub url: String,
    pub taille: u64,
    pub sha256: Option<String>,
    pub licence: String,
}

/// Une licence d'un pack, avec ce qu'il faut savoir avant de l'accepter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicencePack {
    pub nom: String,
    pub url: String,
    pub resume: String,
    pub commercial: Option<bool>,
    pub territoires_exclus: Vec<String>,
    /// « fichier : licence » des composants qui ont la leur.
    pub composants: Vec<String>,
}

/// Un pack, tel que l'écran « Ce que tu veux faire » le montre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub id: String,
    pub nom: String,
    pub usage: String,
    pub conseille: bool,
    pub facultatif: bool,
    pub testee: Option<String>,
    /// Octets de tous ses fichiers, et de ceux qui manquent.
    pub taille: u64,
    pub a_telecharger: u64,
    pub installe: bool,
    pub licences: Vec<LicencePack>,
    /// La date où sa licence a été acceptée.
    pub licence_acceptee: Option<String>,
    pub moteurs_absents: Vec<String>,
}

/// Un fichier à télécharger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FichierPlan {
    pub dossier: String,
    pub nom: String,
    pub taille: u64,
}

/// Ce que coûte une sélection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Les fichiers manquants, comptés une fois.
    pub fichiers: Vec<FichierPlan>,
    pub a_telecharger: u64,
    /// La place libre sur le disque des modèles ; rien : inconnue.
    pub libre: Option<u64>,
    pub place_ok: bool,
    /// Les packs dont la licence attend d'être acceptée.
    pub licences_a_accepter: Vec<String>,
}

/// L'assistant au lancement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EtatAssistant {
    /// S'ouvrir tout seul : pas encore terminé, et il manque de quoi générer.
    pub ouvrir: bool,
    pub termine: Option<String>,
    pub packs: Vec<Pack>,
}

/// Un usage du récapitulatif de fin et son modèle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recap {
    pub usage: String,
    pub nom: String,
}

/// La fin de l'assistant : les favoris réglés, ceux pas encore là, et l'essai possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fin {
    /// `section.clé → valeur`.
    pub favoris: Map<String, Value>,
    pub regles: Vec<Recap>,
    pub manquants: Vec<Recap>,
    pub essai: bool,
}

/// Les favoris de `packs.yaml`, tels que l'écran de fin les nomme.
const USAGES: [(&str, &str, &str); 6] = [
    ("generation", "texte_vers_image", "Créer une image"),
    ("generation", "references", "Créer depuis des références"),
    ("generation", "retouche", "Retoucher"),
    ("generation", "image_vers_video", "Animer une image"),
    ("generation", "texte_vers_video", "Clip depuis un texte"),
    ("llama", "modele", "Réécrire tes idées"),
];

/// L'essai de l'écran de fin : peu de steps et une petite image, juste pour voir toute la chaîne tourner.
pub const IDEE_ESSAI: &str = "Un chat roux endormi sur un rebord de fenêtre, la nuit, sous la pluie, avec les néons \
                              de la ville derrière la vitre";
pub const PROMPT_ESSAI: &str = "A ginger cat asleep on a windowsill at night, rain streaking down the glass, blurred \
                                neon city lights glowing behind the window, cozy warm interior light, photographic";
const FORMAT_ESSAI: &str = "1:1 (Square)";

fn texte(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn memoire(dossier: &Dossier) -> Map<String, Value> {
    reglages::section(dossier, SECTION).unwrap_or_default()
}

fn acceptees(dossier: &Dossier) -> Map<String, Value> {
    memoire(dossier)
        .get("licences")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn retenir(dossier: &Dossier, valeurs: Map<String, Value>) -> Result<(), String> {
    reglages::modifier_section(dossier, SECTION, |s| {
        for (cle, valeur) in valeurs {
            s.insert(cle, valeur);
        }
    })
    .map_err(|e| e.to_string())
}

fn maintenant() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Les packs choisis, dans l'ordre du catalogue (les favoris en dépendent).
fn choisis(ids: &[String]) -> Result<Vec<(String, PackCatalogue)>, String> {
    let packs = catalogue()?;
    if let Some(inconnu) = ids.iter().find(|i| !packs.iter().any(|(id, _)| id == *i)) {
        return Err(format!("Pack inconnu : {inconnu}"));
    }
    if ids.is_empty() {
        return Err("Aucun pack choisi".to_owned());
    }
    Ok(packs.into_iter().filter(|(id, _)| ids.contains(id)).collect())
}

fn fichier_de(f: &Value, dossier: &str, licence: &str) -> FichierPack {
    FichierPack {
        dossier: dossier.to_owned(),
        nom: texte(f.get("nom")),
        url: texte(f.get("url")),
        taille: f.get("taille").and_then(Value::as_u64).unwrap_or(0),
        sha256: f.get("sha256").and_then(Value::as_str).map(str::to_owned),
        licence: licence.to_owned(),
    }
}

/// Les fichiers d'un pack, tels que ses fiches les décrivent ; un LLM vient avec son projecteur d'images.
///
/// # Errors
/// Un workflow ou un LLM du pack que l'appli ne connaît pas.
pub fn fichiers(pack: &PackCatalogue) -> Result<Vec<FichierPack>, String> {
    let mut sortie = Vec::new();
    for id in &pack.workflows {
        let wf = workflows::obtenir(id)?;
        let licence = texte(wf.fiche.pointer("/licence/nom"));
        for f in wf
            .fiche
            .get("fichiers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            sortie.push(fichier_de(f, &texte(f.get("dossier")), &licence));
        }
    }
    for g in &pack.llm {
        let fiche = fiches::fiche_llm(g).ok_or_else(|| format!("LLM inconnu du catalogue : {g}"))?;
        let licence = texte(fiche.pointer("/licence/nom"));
        let mut gguf = fichier_de(&fiche, modeles::DOSSIER_LLM, &licence);
        gguf.nom = texte(fiche.get("id"));
        sortie.push(gguf);
        if let Some(vision) = fiche.get("vision").filter(|v| v.is_object()) {
            sortie.push(fichier_de(vision, modeles::DOSSIER_LLM, &licence));
        }
    }
    Ok(sortie)
}

fn present(modeles_dir: &Path, f: &FichierPack) -> bool {
    modeles::destination(modeles_dir, &f.dossier, &f.nom).is_ok_and(|p| p.is_file())
}

/// Chaque fichier une fois (un VAE partagé par deux workflows).
fn uniques(fichiers: Vec<FichierPack>) -> Vec<FichierPack> {
    let mut sortie: Vec<FichierPack> = Vec::new();
    for f in fichiers {
        if !sortie.iter().any(|g| g.dossier == f.dossier && g.nom == f.nom) {
            sortie.push(f);
        }
    }
    sortie
}

fn licence_de(v: &Value) -> LicencePack {
    let liste = |cle: &str| -> Vec<String> {
        v.get(cle)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    LicencePack {
        nom: texte(v.get("nom")),
        url: texte(v.get("url")),
        resume: texte(v.get("resume")),
        commercial: v.get("commercial").and_then(Value::as_bool),
        territoires_exclus: liste("territoires_exclus"),
        composants: v
            .get("composants")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|c| format!("{} : {}", texte(c.get("fichier")), texte(c.get("licence"))))
            .collect(),
    }
}

/// Les licences distinctes d'un pack (workflows et LLM).
fn licences(pack: &PackCatalogue) -> Vec<LicencePack> {
    let mut vues: Vec<(String, Value)> = Vec::new();
    let blocs = pack
        .workflows
        .iter()
        .filter_map(|w| workflows::obtenir(w).ok()?.fiche.get("licence").cloned())
        .chain(
            pack.llm
                .iter()
                .filter_map(|g| fiches::fiche_llm(g)?.get("licence").cloned()),
        );
    for bloc in blocs {
        let id = texte(bloc.get("id"));
        if !vues.iter().any(|(i, _)| *i == id) {
            vues.push((id, bloc));
        }
    }
    vues.iter().map(|(_, l)| licence_de(l)).collect()
}

/// Les packs tels que l'écran « Ce que tu veux faire » les montre.
///
/// # Errors
/// Le catalogue des packs, ou une fiche qu'il désigne, ne se lit pas.
pub fn packs(dossier: &Dossier, modeles_dir: &Path, moteurs: &[EtatMoteur]) -> Result<Vec<Pack>, String> {
    let acceptees = acceptees(dossier);
    catalogue()?
        .into_iter()
        .map(|(id, p)| {
            let tous = uniques(fichiers(&p)?);
            let a_telecharger: u64 = tous
                .iter()
                .filter(|f| !present(modeles_dir, f))
                .map(|f| f.taille)
                .sum();
            Ok(Pack {
                taille: tous.iter().map(|f| f.taille).sum(),
                installe: tous.iter().all(|f| present(modeles_dir, f)),
                a_telecharger,
                licences: licences(&p),
                licence_acceptee: acceptees.get(&id).and_then(Value::as_str).map(str::to_owned),
                moteurs_absents: p
                    .moteurs
                    .iter()
                    .filter_map(|m| moteurs.iter().find(|e| e.id == *m))
                    .filter(|e| e.statut == Statut::Absent)
                    .map(|e| e.nom.clone())
                    .collect(),
                id,
                nom: p.nom,
                usage: p.usage,
                conseille: p.conseille,
                facultatif: p.facultatif,
                testee: p.testee,
            })
        })
        .collect()
}

fn manquants(modeles_dir: &Path, choisis: &[(String, PackCatalogue)]) -> Result<Vec<FichierPack>, String> {
    let mut tous = Vec::new();
    for (_, p) in choisis {
        tous.extend(fichiers(p)?);
    }
    Ok(uniques(tous)
        .into_iter()
        .filter(|f| !present(modeles_dir, f))
        .collect())
}

/// Ce que coûte une sélection : fichiers manquants (comptés une fois), place, licences encore à accepter.
///
/// # Errors
/// Pack inconnu, aucun pack, ou une fiche qui ne se lit pas.
pub fn plan(
    dossier: &Dossier,
    modeles_dir: &Path,
    ids: &[String],
    libre: Option<u64>,
) -> Result<Plan, String> {
    let choisis = choisis(ids)?;
    let manquants = manquants(modeles_dir, &choisis)?;
    let a_telecharger = manquants.iter().map(|f| f.taille).sum();
    let acceptees = acceptees(dossier);
    Ok(Plan {
        fichiers: manquants
            .into_iter()
            .map(|f| FichierPlan {
                dossier: f.dossier,
                nom: f.nom,
                taille: f.taille,
            })
            .collect(),
        a_telecharger,
        libre,
        place_ok: libre.is_none_or(|l| l >= a_telecharger.saturating_add(MARGE)),
        licences_a_accepter: choisis
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| !acceptees.contains_key(id))
            .collect(),
    })
}

/// « J'ai lu et j'accepte » : daté, pack par pack.
///
/// # Errors
/// Pack inconnu, aucun pack, ou `reglages.json` qui ne s'écrit pas.
pub fn accepter(dossier: &Dossier, ids: &[String]) -> Result<(), String> {
    choisis(ids)?;
    let mut licences = acceptees(dossier);
    let date = maintenant();
    for id in ids {
        licences.insert(id.clone(), json!(date));
    }
    let mut valeurs = Map::new();
    valeurs.insert("licences".to_owned(), Value::Object(licences));
    retenir(dossier, valeurs)
}

/// Ce qu'il faut mettre dans la file du téléchargeur ; refusé tant qu'une licence n'est pas acceptée ou que la place
/// manque.
///
/// # Errors
/// Le message de la v2.
pub fn a_telecharger(
    dossier: &Dossier,
    modeles_dir: &Path,
    ids: &[String],
    libre: Option<u64>,
) -> Result<Vec<(Fiche, String)>, String> {
    let p = plan(dossier, modeles_dir, ids, libre)?;
    if !p.licences_a_accepter.is_empty() {
        let noms: Vec<String> = catalogue()?
            .into_iter()
            .filter(|(id, _)| p.licences_a_accepter.contains(id))
            .map(|(_, pk)| pk.nom)
            .collect();
        return Err(format!("Accepte d'abord la licence de : {}", noms.join(", ")));
    }
    if !p.place_ok {
        let go = |o: u64| o.div_ceil(GIO);
        return Err(format!(
            "Place insuffisante : {} Go à télécharger, {} Go libres (il en faut {} de marge)",
            go(p.a_telecharger),
            go(p.libre.unwrap_or(0)),
            MARGE / GIO
        ));
    }
    Ok(manquants(modeles_dir, &choisis(ids)?)?
        .into_iter()
        .map(|f| {
            let fiche = Fiche {
                source: "huggingface".to_owned(),
                nom: f.nom,
                taille: f.taille,
                sha256: f.sha256,
                page: page_de(&f.url),
                url: f.url,
                licence: Some(f.licence),
                ..Fiche::default()
            };
            (fiche, f.dossier)
        })
        .collect())
}

fn usage(section: &str, cle: &str) -> String {
    USAGES
        .iter()
        .find(|(s, c, _)| *s == section && *c == cle)
        .map_or_else(|| format!("{section}.{cle}"), |(_, _, u)| (*u).to_owned())
}

fn nom_de(section: &str, valeur: &str) -> String {
    if section == "generation" {
        return workflows::obtenir(valeur)
            .map_or_else(|_| valeur.to_owned(), |wf| texte(wf.fiche.get("nom")));
    }
    fiches::fiche_llm(valeur)
        .and_then(|f| f.get("nom").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| valeur.to_owned())
}

fn pret(modeles_dir: &Path, section: &str, valeur: &str) -> bool {
    if section == "generation" {
        workflows::obtenir(valeur).is_ok_and(|wf| wf.manquants(modeles_dir).is_empty())
    } else {
        modeles::destination(modeles_dir, modeles::DOSSIER_LLM, valeur).is_ok_and(|p| p.is_file())
    }
}

/// Une clé de favori dans une liste ordonnée : remplacée sur place si elle y est déjà, ajoutée sinon.
fn poser(liste: &mut Vec<(String, String, String)>, section: &str, cle: &str, valeur: &str) {
    match liste.iter_mut().find(|(s, c, _)| s == section && c == cle) {
        Some(deja) => valeur.clone_into(&mut deja.2),
        None => liste.push((section.to_owned(), cle.to_owned(), valeur.to_owned())),
    }
}

/// Règle les favoris sur ce qui est installé (packs dans l'ordre du catalogue, un pack plus bas l'emporte) et note
/// la fin de l'assistant. Rend le récapitulatif : chaque usage avec son modèle, réglé ou pas encore là.
///
/// # Errors
/// Pack inconnu, aucun pack, ou `reglages.json` qui ne s'écrit pas.
pub fn terminer(dossier: &Dossier, modeles_dir: &Path, ids: &[String]) -> Result<Fin, String> {
    let choisis = choisis(ids)?;
    let (mut regles, mut manquants) = (Vec::new(), Vec::new());
    for (_, p) in &choisis {
        for (section, valeurs) in &p.favoris.0 {
            for (cle, valeur) in &valeurs.0 {
                let cible = if pret(modeles_dir, section, valeur) {
                    &mut regles
                } else {
                    &mut manquants
                };
                poser(cible, section, cle, valeur);
            }
        }
    }
    // Ce qui est réglé n'est plus « pas encore là » (un pack plus haut l'a fourni).
    manquants.retain(|(s, c, _)| !regles.iter().any(|(rs, rc, _)| rs == s && rc == c));
    reglages::modifier_section(dossier, "favoris", |favoris| {
        for (section, cle, valeur) in &regles {
            let s = favoris.entry(section.clone()).or_insert_with(|| json!({}));
            if !s.is_object() {
                *s = json!({});
            }
            if let Some(s) = s.as_object_mut() {
                s.insert(cle.clone(), json!(valeur));
            }
        }
    })
    .map_err(|e| e.to_string())?;
    let mut fin = Map::new();
    fin.insert("termine".to_owned(), json!(maintenant()));
    fin.insert(
        "packs".to_owned(),
        json!(choisis.iter().map(|(id, _)| id).collect::<Vec<_>>()),
    );
    retenir(dossier, fin)?;
    let recap = |liste: &[(String, String, String)]| -> Vec<Recap> {
        liste
            .iter()
            .map(|(s, c, v)| Recap {
                usage: usage(s, c),
                nom: nom_de(s, v),
            })
            .collect()
    };
    Ok(Fin {
        favoris: regles
            .iter()
            .map(|(s, c, v)| (format!("{s}.{c}"), json!(v)))
            .collect(),
        essai: regles
            .iter()
            .any(|(s, c, _)| s == "generation" && c == "texte_vers_image"),
        regles: recap(&regles),
        manquants: recap(&manquants),
    })
}

/// La demande de l'image d'essai : le favori « Créer une image », réécrit par le LLM par défaut s'il est installé,
/// sinon le prompt anglais tel quel ; et les valeurs imposées (peu de steps, petite image).
///
/// # Errors
/// Aucun modèle d'image installé.
pub fn demande_essai(dossier: &Dossier, modeles_dir: &Path) -> Result<(Demande, Map<String, Value>), String> {
    let workflow = reglages::section(dossier, "favoris")
        .and_then(|f| {
            f.get("generation")?
                .get("texte_vers_image")?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|w| pret(modeles_dir, "generation", w))
        .ok_or_else(|| "Aucun modèle d'image installé pour l'essai".to_owned())?;
    let avec_llm = pret(modeles_dir, "llama", &llm::defaut(dossier, modeles_dir));
    let demande = Demande {
        prompt: (if avec_llm { IDEE_ESSAI } else { PROMPT_ESSAI }).to_owned(),
        reecrire: avec_llm,
        llm: None,
        workflow: Some(workflow),
        format: FORMAT_ESSAI.to_owned(),
        seed: None,
        nombre: 1,
        modele: None,
        references: Vec::new(),
        mode_reference: None,
        liberte: LIBERTE_DEFAUT,
        loras: Vec::new(),
        dossier: None,
        verifier: false,
    };
    let mut imposees = Map::new();
    imposees.insert("steps".to_owned(), json!(10));
    imposees.insert("megapixels".to_owned(), json!(0.5));
    Ok((demande, imposees))
}

/// S'ouvrir au lancement ? Seulement tant qu'il n'a pas été terminé, et s'il manque de quoi générer.
///
/// # Errors
/// Le catalogue des packs ne se lit pas.
pub fn etat(dossier: &Dossier, modeles_dir: &Path, moteurs: &[EtatMoteur]) -> Result<EtatAssistant, String> {
    let termine = memoire(dossier)
        .get("termine")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let moteur_absent = moteurs
        .iter()
        .any(|m| m.statut == Statut::Absent && !m.facultatif);
    let aucun_workflow = !workflows::catalogue()
        .0
        .iter()
        .any(|wf| wf.manquants(modeles_dir).is_empty());
    Ok(EtatAssistant {
        ouvrir: termine.is_none() && (moteur_absent || aucun_workflow),
        termine,
        packs: packs(dossier, modeles_dir, moteurs)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn lieux(temp: &tempfile::TempDir) -> (Dossier, std::path::PathBuf) {
        let modeles = temp.path().join("modeles");
        fs::create_dir_all(modeles.join("llm")).unwrap();
        (Dossier::a(temp.path()), modeles)
    }

    fn ids(liste: &[&str]) -> Vec<String> {
        liste.iter().map(|i| (*i).to_owned()).collect()
    }

    fn moteur(id: &str, statut: Statut, facultatif: bool) -> EtatMoteur {
        EtatMoteur {
            id: id.to_owned(),
            nom: id.to_uppercase(),
            role: String::new(),
            facultatif,
            dossier: id.to_owned(),
            version: "1".to_owned(),
            version_installee: None,
            statut,
            chemin: String::new(),
            externe: false,
            precedente: false,
            a_telecharger: 0,
            occupe: false,
            prerequis: None,
        }
    }

    #[test]
    fn catalogue_lu_dans_son_ordre() {
        let packs = catalogue().unwrap();
        let ordre: Vec<&str> = packs.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ordre, ["images", "video", "llm"]);
        let images = &packs[0].1;
        assert!(images.conseille && images.workflows.len() == 2);
        let favoris: Vec<&str> = images.favoris.0[0].1.0.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(favoris, ["texte_vers_image", "references", "retouche"]);
        // Le LLM vient avec son projecteur d'images, dans modeles\llm.
        let llm = fichiers(&packs[2].1).unwrap();
        assert_eq!(llm.len(), 2);
        assert!(
            llm.iter()
                .all(|f| f.dossier == "llm" && f.taille > 0 && f.sha256.is_some())
        );
        assert_eq!(llm[0].nom, "Ternary-Bonsai-2-27B-PTQ1_0.gguf");
    }

    #[test]
    fn plan_licences_et_telechargement() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, modeles) = lieux(&temp);
        let choix = ids(&["images", "video"]);
        let p = plan(&dossier, &modeles, &choix, Some(u64::MAX)).unwrap();
        assert_eq!(p.licences_a_accepter, choix);
        assert!(p.place_ok && p.a_telecharger > 0);
        // Les fichiers communs (VAE, encodeur) ne comptent qu'une fois.
        let mut noms: Vec<(&str, &str)> = p
            .fichiers
            .iter()
            .map(|f| (f.dossier.as_str(), f.nom.as_str()))
            .collect();
        let total = noms.len();
        noms.sort_unstable();
        noms.dedup();
        assert_eq!(noms.len(), total);
        assert!(
            !plan(&dossier, &modeles, &choix, Some(p.a_telecharger))
                .unwrap()
                .place_ok
        );
        assert!(plan(&dossier, &modeles, &ids(&["rien"]), None).is_err());
        assert!(plan(&dossier, &modeles, &[], None).is_err());
        // Refusé tant que les licences ne sont pas acceptées, puis tant que la place manque.
        let refus = a_telecharger(&dossier, &modeles, &choix, None).unwrap_err();
        assert!(
            refus.starts_with("Accepte d'abord la licence de : Images, Vidéo"),
            "{refus}"
        );
        accepter(&dossier, &choix).unwrap();
        assert!(
            plan(&dossier, &modeles, &choix, None)
                .unwrap()
                .licences_a_accepter
                .is_empty()
        );
        assert!(
            a_telecharger(&dossier, &modeles, &choix, Some(GIO))
                .unwrap_err()
                .starts_with("Place insuffisante")
        );
        let a_faire = a_telecharger(&dossier, &modeles, &choix, None).unwrap();
        assert_eq!(a_faire.len(), p.fichiers.len());
        assert!(
            a_faire
                .iter()
                .all(|(f, _)| f.licence.is_some() && !f.page.contains("/resolve/"))
        );
        // Un fichier posé n'est plus à télécharger.
        let (premier, dossier_premier) = &a_faire[0];
        let chemin = modeles::destination(&modeles, dossier_premier, &premier.nom).unwrap();
        fs::create_dir_all(chemin.parent().unwrap()).unwrap();
        fs::write(&chemin, b"x").unwrap();
        assert_eq!(
            plan(&dossier, &modeles, &choix, None).unwrap().fichiers.len(),
            p.fichiers.len() - 1
        );
    }

    #[test]
    fn packs_et_ouverture() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, modeles) = lieux(&temp);
        let moteurs = [
            moteur("comfyui", Statut::Absent, false),
            moteur("llama", Statut::AJour, false),
        ];
        let e = etat(&dossier, &modeles, &moteurs).unwrap();
        assert!(e.ouvrir && e.termine.is_none());
        let images = e.packs.iter().find(|p| p.id == "images").unwrap();
        assert!(!images.installe && images.a_telecharger == images.taille);
        assert_eq!(images.moteurs_absents, ["COMFYUI"]);
        assert!(
            images
                .licences
                .iter()
                .all(|l| !l.nom.is_empty() && !l.url.is_empty())
        );
        // Terminé sans rien d'installé : aucun favori réglé, tout est « pas encore là », pas d'essai.
        let fin = terminer(&dossier, &modeles, &ids(&["images", "llm"])).unwrap();
        assert!(fin.regles.is_empty() && fin.favoris.is_empty() && !fin.essai);
        let usages: Vec<&str> = fin.manquants.iter().map(|r| r.usage.as_str()).collect();
        assert_eq!(
            usages,
            [
                "Créer une image",
                "Créer depuis des références",
                "Retoucher",
                "Réécrire tes idées"
            ]
        );
        let e = etat(&dossier, &modeles, &moteurs).unwrap();
        assert!(!e.ouvrir && e.termine.is_some());
        assert!(demande_essai(&dossier, &modeles).is_err());
    }

    #[test]
    fn favoris_regles_et_essai() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, modeles) = lieux(&temp);
        // Le pack Images entier posé sur le disque.
        let images = &catalogue().unwrap()[0].1;
        for f in fichiers(images).unwrap() {
            let chemin = modeles::destination(&modeles, &f.dossier, &f.nom).unwrap();
            fs::create_dir_all(chemin.parent().unwrap()).unwrap();
            fs::write(&chemin, b"x").unwrap();
        }
        let fin = terminer(&dossier, &modeles, &ids(&["images"])).unwrap();
        assert!(fin.essai && fin.manquants.is_empty());
        assert_eq!(
            fin.favoris.get("generation.texte_vers_image"),
            Some(&json!("qwen_image_2_1"))
        );
        assert_eq!(fin.regles[0].usage, "Créer une image");
        let favoris = reglages::section(&dossier, "favoris").unwrap();
        assert_eq!(favoris["generation"]["retouche"], "qwen_image_2_1_edit");
        // Sans LLM installé, l'essai part du prompt anglais, avec ses valeurs imposées.
        let (demande, imposees) = demande_essai(&dossier, &modeles).unwrap();
        assert!(!demande.reecrire && demande.prompt == PROMPT_ESSAI);
        assert_eq!(demande.workflow.as_deref(), Some("qwen_image_2_1"));
        assert_eq!(imposees["steps"], 10);
    }
}
