//! Les Rushes du Plateau : dossiers, archives et corbeille, comme en v2 (`app/rushes.py`).
//!
//! Aucun fichier n'est déplacé : chaque rush garde son image et sa fiche dans `data\creations\`, et sa fiche note où
//! il est rangé. Ainsi, parents, variantes et références restent valides quoi qu'on range.
//!
//! - `dossier` : le nom de son dossier (un seul par rush) ; absent : « Non rangés » ;
//! - `archive` : vrai = sorti de « Tous » et de son dossier, visible dans « Archives » ;
//! - `corbeille` : la date de mise à la corbeille ; seul « Vider la corbeille » supprime pour de bon.
//!
//! Les dossiers sont propres à chaque genre : ceux des Rushes (images) et ceux des Bobines (clips) sont deux listes
//! distinctes, dans `data\dossiers.json` (`{"images": [...], "clips": [...]}`), qui garde aussi les dossiers vides et
//! leur ordre. L'ancien format (une liste commune) vaut pour les deux genres, et il est réécrit au nouveau format à
//! la première modification.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{Dossier, creations, journal};

/// Les vues des Rushes.
pub const VUES: [&str; 4] = ["tous", "non_ranges", "archives", "corbeille"];
/// Les genres : Rushes du Plateau, Bobines du Montage ; chacun ses dossiers.
pub const GENRES: [&str; 2] = ["images", "clips"];
/// Les actions sur une sélection.
pub const ACTIONS: [&str; 5] = ["corbeille", "restaurer", "archiver", "desarchiver", "ranger"];
/// Les rushes rendus par une vue, au plus.
pub const LIMITE: usize = 500;

/// Le fichier des dossiers, dans `data\`.
pub fn fichier_dossiers(data: &Path) -> PathBuf {
    data.join("dossiers.json")
}

fn genre_valide(genre: &str) -> Result<&str, String> {
    if GENRES.contains(&genre) {
        Ok(genre)
    } else {
        Err(format!("Genre inconnu : {genre}"))
    }
}

fn genre_de(f: &Value) -> &'static str {
    if f.get("type").and_then(Value::as_str) == Some("clip") {
        "clips"
    } else {
        "images"
    }
}

fn texte<'a>(f: &'a Value, cle: &str) -> Option<&'a str> {
    f.get(cle).and_then(Value::as_str).filter(|t| !t.is_empty())
}

/// Vrai pour une valeur que Python tient pour vraie (une archive notée `true`, une date de corbeille).
fn vrai(f: &Value, cle: &str) -> bool {
    match f.get(cle) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Toutes les fiches, la plus récente en premier ; une fiche illisible est sautée.
fn fiches(data: &Path) -> Vec<Value> {
    creations::liste(data, usize::MAX)
}

/// Les listes de dossiers du fichier, par genre.
fn lire_dossiers(data: &Path) -> Map<String, Value> {
    let lu = fs::read_to_string(fichier_dossiers(data))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let noms = |v: Option<&Value>| -> Value {
        Value::Array(
            v.and_then(Value::as_array)
                .map(|l| l.iter().filter(|n| n.is_string()).cloned().collect())
                .unwrap_or_default(),
        )
    };
    GENRES
        .iter()
        .map(|g| {
            let liste = match &lu {
                // Ancien format : la liste commune vaut pour les deux genres.
                Some(Value::Array(_)) => noms(lu.as_ref()),
                Some(objet) => noms(objet.get(*g)),
                None => Value::Array(Vec::new()),
            };
            ((*g).to_owned(), liste)
        })
        .collect()
}

fn noms_du_fichier(data: &Path, genre: &str) -> Vec<String> {
    lire_dossiers(data)
        .get(genre)
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

fn ecrire_dossiers(data: &Path, genre: &str, noms: &[String]) -> Result<(), String> {
    let mut toutes = lire_dossiers(data);
    toutes.insert(genre.to_owned(), json!(noms));
    let mut octets = Vec::new();
    let formateur = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serialiseur = serde_json::Serializer::with_formatter(&mut octets, formateur);
    Value::Object(toutes)
        .serialize(&mut serialiseur)
        .map_err(|e| format!("dossiers.json : {e}"))?;
    fs::create_dir_all(data).map_err(|e| format!("dossiers.json : {e}"))?;
    let chemin = fichier_dossiers(data);
    let temporaire = data.join("dossiers.json.tmp");
    fs::write(&temporaire, octets)
        .and_then(|()| fs::rename(&temporaire, &chemin))
        .map_err(|e| {
            let _ = fs::remove_file(&temporaire);
            format!("dossiers.json : {e}")
        })
}

fn dossiers_de(data: &Path, toutes: &[Value], genre: &str) -> Vec<String> {
    let mut noms = noms_du_fichier(data, genre);
    // Un dossier noté dans une fiche mais absent de la liste (fichier effacé à la main) réapparaît.
    let mut en_plus: Vec<String> = toutes
        .iter()
        .filter(|f| genre_de(f) == genre)
        .filter_map(|f| texte(f, "dossier"))
        .filter(|d| !noms.iter().any(|n| n == d))
        .map(str::to_owned)
        .collect();
    en_plus.sort();
    en_plus.dedup();
    noms.extend(en_plus);
    noms
}

/// Les dossiers d'un genre, dans l'ordre de l'utilisateur.
///
/// # Errors
/// Genre inconnu.
pub fn dossiers(data: &Path, genre: &str) -> Result<Vec<String>, String> {
    let genre = genre_valide(genre)?;
    Ok(dossiers_de(data, &fiches(data), genre))
}

fn visible(f: &Value, vue: &str, dossier: Option<&str>) -> bool {
    if vue == "corbeille" {
        return vrai(f, "corbeille");
    }
    if vrai(f, "corbeille") {
        return false;
    }
    if vue == "archives" {
        return vrai(f, "archive");
    }
    if vrai(f, "archive") {
        return false;
    }
    if let Some(d) = dossier {
        return f.get("dossier").and_then(Value::as_str) == Some(d);
    }
    vue == "tous" || texte(f, "dossier").is_none()
}

/// Les rushes d'une vue (`tous`, `non_ranges`, `archives`, `corbeille`) ou d'un dossier, les plus récents d'abord.
///
/// # Errors
/// Vue ou genre inconnus.
pub fn liste(data: &Path, vue: &str, dossier: Option<&str>, genre: &str) -> Result<Vec<Value>, String> {
    if !VUES.contains(&vue) {
        return Err(format!("Vue inconnue : {vue}"));
    }
    let genre = genre_valide(genre)?;
    Ok(fiches(data)
        .into_iter()
        .filter(|f| genre_de(f) == genre && visible(f, vue, dossier))
        .take(LIMITE)
        .collect())
}

/// Un dossier et son nombre de rushes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompteDossier {
    pub nom: String,
    pub nombre: usize,
}

/// Les compteurs des pastilles de rangement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatRushes {
    pub tous: usize,
    pub non_ranges: usize,
    pub archives: usize,
    pub corbeille: usize,
    pub dossiers: Vec<CompteDossier>,
}

/// Les compteurs d'un genre.
///
/// # Errors
/// Genre inconnu.
pub fn etat(data: &Path, genre: &str) -> Result<EtatRushes, String> {
    let genre = genre_valide(genre)?;
    let toutes = fiches(data);
    let du_genre: Vec<&Value> = toutes.iter().filter(|f| genre_de(f) == genre).collect();
    let compte =
        |vue: &str, dossier: Option<&str>| du_genre.iter().filter(|f| visible(f, vue, dossier)).count();
    Ok(EtatRushes {
        tous: compte("tous", None),
        non_ranges: compte("non_ranges", None),
        archives: compte("archives", None),
        corbeille: compte("corbeille", None),
        dossiers: dossiers_de(data, &toutes, genre)
            .into_iter()
            .map(|nom| CompteDossier {
                nombre: compte("tous", Some(&nom)),
                nom,
            })
            .collect(),
    })
}

fn nom_valide(nom: &str) -> Result<String, String> {
    let nom = nom.trim();
    let bon = Regex::new(r"^[\w][\w .'’&()-]{0,39}\z").is_ok_and(|m| m.is_match(nom));
    if bon {
        Ok(nom.to_owned())
    } else {
        Err("Nom de dossier invalide (lettres, chiffres, espaces, 40 caractères au plus)".to_owned())
    }
}

/// Applique une action à une sélection de rushes ; rend le nombre de rushes modifiés. Tout est vérifié avant de
/// modifier quoi que ce soit. `dossier` sert à « ranger » ; rien : « Non rangés ».
///
/// # Errors
/// Action inconnue, rush inconnu, dossier inconnu pour le genre d'un rush, ou fiche qui ne s'écrit pas.
pub fn agir(data: &Path, ids: &[String], action: &str, dossier: Option<&str>) -> Result<usize, String> {
    if !ACTIONS.contains(&action) {
        return Err(format!("Action inconnue : {action}"));
    }
    let mut uniques: Vec<&String> = Vec::new();
    for id in ids {
        if !uniques.contains(&id) {
            uniques.push(id);
        }
    }
    let mut lues = uniques
        .iter()
        .map(|id| creations::fiche(data, id))
        .collect::<Result<Vec<Value>, String>>()?;
    if action == "ranger"
        && let Some(d) = dossier
    {
        let toutes = fiches(data);
        for f in &lues {
            // Un rush ne se range que dans un dossier de son genre.
            if !dossiers_de(data, &toutes, genre_de(f)).iter().any(|n| n == d) {
                return Err(format!("Dossier inconnu : {d}"));
            }
        }
    }
    let maintenant = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    for f in &mut lues {
        let Some(objet) = f.as_object_mut() else {
            continue;
        };
        match (action, dossier) {
            ("corbeille", _) => {
                objet.insert("corbeille".to_owned(), json!(maintenant));
            }
            ("restaurer", _) => {
                objet.remove("corbeille");
            }
            ("archiver", _) => {
                objet.insert("archive".to_owned(), json!(true));
            }
            ("desarchiver", _) => {
                objet.remove("archive");
            }
            (_, None) => {
                objet.remove("dossier");
            }
            (_, Some(d)) => {
                objet.insert("dossier".to_owned(), json!(d));
            }
        }
        creations::ecrire_fiche(data, f)?;
    }
    Ok(lues.len())
}

/// La seule suppression définitive : image, vidéo et fiche des rushes du genre mis à la corbeille. Rend leur nombre.
///
/// # Errors
/// Genre inconnu, ou un fichier qui ne s'efface pas (ceux déjà effacés le restent).
pub fn vider_corbeille(dossier: &Dossier, data: &Path, genre: &str) -> Result<usize, String> {
    let genre = genre_valide(genre)?;
    let a_supprimer: Vec<String> = fiches(data)
        .iter()
        .filter(|f| vrai(f, "corbeille") && genre_de(f) == genre)
        .filter_map(|f| texte(f, "id").map(str::to_owned))
        .collect();
    let mut effaces = 0;
    let mut erreur = None;
    for id in &a_supprimer {
        match creations::supprimer(data, id) {
            Ok(()) => effaces += 1,
            Err(e) => erreur = Some(e),
        }
    }
    let objet = if genre == "clips" { "clips" } else { "rushes" };
    let detail = format!("{effaces} effacé(s)");
    match erreur {
        None => {
            journal::noter(
                dossier,
                "Corbeille vidée",
                objet,
                journal::Resultat::Ok,
                Some(&detail),
            );
            Ok(effaces)
        }
        Some(e) => {
            let detail = format!("{detail} ; {e}");
            journal::noter(
                dossier,
                "Corbeille vidée",
                objet,
                journal::Resultat::Partiel,
                Some(&detail),
            );
            Err(e)
        }
    }
}

/// Crée un dossier, à la fin de la liste.
///
/// # Errors
/// Nom invalide ou déjà pris, genre inconnu, ou fichier qui ne s'écrit pas.
pub fn creer_dossier(data: &Path, nom: &str, genre: &str) -> Result<String, String> {
    let nom = nom_valide(nom)?;
    let mut noms = dossiers(data, genre)?;
    if noms.contains(&nom) {
        return Err(format!("Le dossier « {nom} » existe déjà"));
    }
    noms.push(nom.clone());
    ecrire_dossiers(data, genre, &noms)?;
    Ok(nom)
}

/// Renomme un dossier, et ses rushes avec lui.
///
/// # Errors
/// Nom invalide ou déjà pris, dossier inconnu, ou fichier qui ne s'écrit pas.
pub fn renommer_dossier(data: &Path, ancien: &str, nouveau: &str, genre: &str) -> Result<String, String> {
    let nouveau = nom_valide(nouveau)?;
    let noms = dossiers(data, genre)?;
    if !noms.iter().any(|n| n == ancien) {
        return Err(format!("Dossier inconnu : {ancien}"));
    }
    if nouveau != ancien && noms.contains(&nouveau) {
        return Err(format!("Le dossier « {nouveau} » existe déjà"));
    }
    for mut f in fiches(data) {
        if f.get("dossier").and_then(Value::as_str) == Some(ancien) && genre_de(&f) == genre {
            f["dossier"] = json!(nouveau);
            creations::ecrire_fiche(data, &f)?;
        }
    }
    let renommes: Vec<String> = noms
        .into_iter()
        .map(|n| if n == ancien { nouveau.clone() } else { n })
        .collect();
    ecrire_dossiers(data, genre, &renommes)?;
    Ok(nouveau)
}

/// Supprime un dossier, pas ses images : elles retournent dans « Non rangés ». Rend leur nombre.
///
/// # Errors
/// Dossier inconnu, ou fichier qui ne s'écrit pas.
pub fn supprimer_dossier(data: &Path, nom: &str, genre: &str) -> Result<usize, String> {
    let noms = dossiers(data, genre)?;
    if !noms.iter().any(|n| n == nom) {
        return Err(format!("Dossier inconnu : {nom}"));
    }
    let mut liberes = 0;
    for mut f in fiches(data) {
        if f.get("dossier").and_then(Value::as_str) == Some(nom)
            && genre_de(&f) == genre
            && let Some(objet) = f.as_object_mut()
        {
            objet.remove("dossier");
            creations::ecrire_fiche(data, &f)?;
            liberes += 1;
        }
    }
    let restants: Vec<String> = noms.into_iter().filter(|n| n != nom).collect();
    ecrire_dossiers(data, genre, &restants)?;
    Ok(liberes)
}

/// Pour ranger une nouvelle création : rien (non rangée) ou un dossier existant du genre.
///
/// # Errors
/// Dossier inconnu.
pub fn valider_dossier(data: &Path, nom: Option<&str>, genre: &str) -> Result<Option<String>, String> {
    match nom.filter(|n| !n.is_empty()) {
        None => Ok(None),
        Some(n) if dossiers(data, genre)?.iter().any(|d| d == n) => Ok(Some(n.to_owned())),
        Some(n) => Err(format!("Dossier inconnu : {n}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut o = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
        o.extend_from_slice(&13u32.to_be_bytes());
        o.extend_from_slice(b"IHDR");
        o.extend_from_slice(&8u32.to_be_bytes());
        o.extend_from_slice(&8u32.to_be_bytes());
        o
    }

    fn creer(data: &Path, infos: Value) -> String {
        creations::enregistrer(data, &png(), infos.as_object().unwrap()).unwrap()
    }

    fn ids(v: &[Value]) -> Vec<String> {
        let mut l: Vec<String> = v.iter().map(|f| f["id"].as_str().unwrap().to_owned()).collect();
        l.sort();
        l
    }

    fn tries(mut l: Vec<String>) -> Vec<String> {
        l.sort();
        l
    }

    #[test]
    fn vues_et_compteurs() {
        let temp = tempfile::tempdir().unwrap();
        let d = temp.path();
        let a = creer(d, json!({"type": "creation"}));
        let b = creer(d, json!({"type": "creation", "dossier": "Renards"}));
        let c = creer(d, json!({"type": "creation", "archive": true}));
        let e = creer(d, json!({"type": "creation", "corbeille": "2026-11-20T10:00:00"}));
        let clip = creer(d, json!({"type": "clip"}));
        assert_eq!(
            ids(&liste(d, "tous", None, "images").unwrap()),
            tries(vec![a.clone(), b.clone()])
        );
        assert_eq!(
            ids(&liste(d, "non_ranges", None, "images").unwrap()),
            vec![a.clone()]
        );
        assert_eq!(ids(&liste(d, "archives", None, "images").unwrap()), vec![c]);
        assert_eq!(ids(&liste(d, "corbeille", None, "images").unwrap()), vec![e]);
        assert_eq!(
            ids(&liste(d, "tous", Some("Renards"), "images").unwrap()),
            vec![b]
        );
        assert_eq!(ids(&liste(d, "tous", None, "clips").unwrap()), vec![clip]);
        assert!(liste(d, "autre", None, "images").is_err());
        assert!(liste(d, "tous", None, "sons").is_err());

        let compteurs = etat(d, "images").unwrap();
        assert_eq!(
            (
                compteurs.tous,
                compteurs.non_ranges,
                compteurs.archives,
                compteurs.corbeille
            ),
            (2, 1, 1, 1)
        );
        // Le dossier noté dans une fiche réapparaît même absent de dossiers.json.
        assert_eq!(
            compteurs.dossiers,
            vec![CompteDossier {
                nom: "Renards".to_owned(),
                nombre: 1
            }]
        );
        assert!(etat(d, "clips").unwrap().dossiers.is_empty());
    }

    #[test]
    fn actions_sur_une_selection() {
        let temp = tempfile::tempdir().unwrap();
        let d = temp.path();
        let a = creer(d, json!({"prompt": "a fox"}));
        let b = creer(d, json!({"prompt": "a cat"}));
        let deux = vec![a.clone(), b.clone(), a.clone()];
        assert_eq!(agir(d, &deux, "archiver", None).unwrap(), 2);
        assert_eq!(liste(d, "archives", None, "images").unwrap().len(), 2);
        agir(d, &deux, "desarchiver", None).unwrap();
        assert!(liste(d, "archives", None, "images").unwrap().is_empty());

        agir(d, std::slice::from_ref(&a), "corbeille", None).unwrap();
        let f = creations::fiche(d, &a).unwrap();
        assert_eq!(f["corbeille"].as_str().unwrap().len(), 19);
        assert_eq!(f["prompt"], json!("a fox"));
        agir(d, std::slice::from_ref(&a), "restaurer", None).unwrap();
        assert!(creations::fiche(d, &a).unwrap().get("corbeille").is_none());

        // Ranger : dans un dossier connu seulement ; sans dossier, « Non rangés ».
        assert_eq!(
            agir(d, std::slice::from_ref(&a), "ranger", Some("Renards")).unwrap_err(),
            "Dossier inconnu : Renards"
        );
        creer_dossier(d, "Renards", "images").unwrap();
        agir(d, std::slice::from_ref(&a), "ranger", Some("Renards")).unwrap();
        assert_eq!(creations::fiche(d, &a).unwrap()["dossier"], json!("Renards"));
        agir(d, std::slice::from_ref(&a), "ranger", None).unwrap();
        assert!(creations::fiche(d, &a).unwrap().get("dossier").is_none());

        // Un rush inconnu : rien n'est modifié.
        let avec_inconnu = vec![b.clone(), "20200101-000000-000000".to_owned()];
        assert!(agir(d, &avec_inconnu, "archiver", None).is_err());
        assert!(creations::fiche(d, &b).unwrap().get("archive").is_none());
        assert!(agir(d, &[b], "effacer", None).is_err());
    }

    #[test]
    fn dossiers_crees_renommes_supprimes() {
        let temp = tempfile::tempdir().unwrap();
        let d = temp.path();
        assert_eq!(creer_dossier(d, "  Portraits ", "images").unwrap(), "Portraits");
        creer_dossier(d, "Été 2026", "images").unwrap();
        creer_dossier(d, "Clips du chat", "clips").unwrap();
        assert!(creer_dossier(d, "Portraits", "images").is_err());
        assert!(creer_dossier(d, "../x", "images").is_err());
        assert!(creer_dossier(d, &"a".repeat(41), "images").is_err());
        assert_eq!(dossiers(d, "images").unwrap(), vec!["Portraits", "Été 2026"]);
        assert_eq!(dossiers(d, "clips").unwrap(), vec!["Clips du chat"]);
        let texte = fs::read_to_string(fichier_dossiers(d)).unwrap();
        assert!(texte.contains("\n  \"images\": [\n    \"Portraits\""), "{texte}");

        let a = creer(d, json!({}));
        let b = creer(d, json!({}));
        agir(d, &[a.clone(), b.clone()], "ranger", Some("Portraits")).unwrap();
        assert_eq!(
            renommer_dossier(d, "Portraits", "Visages", "images").unwrap(),
            "Visages"
        );
        assert_eq!(dossiers(d, "images").unwrap(), vec!["Visages", "Été 2026"]);
        assert_eq!(creations::fiche(d, &a).unwrap()["dossier"], json!("Visages"));
        assert!(renommer_dossier(d, "Visages", "Été 2026", "images").is_err());
        assert!(renommer_dossier(d, "Absent", "X", "images").is_err());

        assert_eq!(supprimer_dossier(d, "Visages", "images").unwrap(), 2);
        assert_eq!(dossiers(d, "images").unwrap(), vec!["Été 2026"]);
        assert!(creations::fiche(d, &b).unwrap().get("dossier").is_none());
        assert_eq!(etat(d, "images").unwrap().non_ranges, 2);

        assert_eq!(valider_dossier(d, None, "images").unwrap(), None);
        assert_eq!(valider_dossier(d, Some(""), "images").unwrap(), None);
        assert_eq!(
            valider_dossier(d, Some("Été 2026"), "images").unwrap().as_deref(),
            Some("Été 2026")
        );
        assert!(valider_dossier(d, Some("Clips du chat"), "images").is_err());
    }

    #[test]
    fn ancien_format_des_dossiers() {
        let temp = tempfile::tempdir().unwrap();
        let d = temp.path();
        fs::write(fichier_dossiers(d), r#"["Paysages", "Portraits"]"#).unwrap();
        assert_eq!(dossiers(d, "images").unwrap(), vec!["Paysages", "Portraits"]);
        assert_eq!(dossiers(d, "clips").unwrap(), vec!["Paysages", "Portraits"]);
        creer_dossier(d, "Nuit", "images").unwrap();
        let relu: Value = serde_json::from_str(&fs::read_to_string(fichier_dossiers(d)).unwrap()).unwrap();
        assert_eq!(
            relu,
            json!({"images": ["Paysages", "Portraits", "Nuit"], "clips": ["Paysages", "Portraits"]})
        );
    }

    #[test]
    fn vider_la_corbeille_efface_pour_de_bon() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let d = &temp.path().join("data");
        let garde = creer(d, json!({}));
        let jetee = creer(d, json!({}));
        let clip_jete = creer(d, json!({"type": "clip"}));
        agir(d, &[jetee.clone(), clip_jete.clone()], "corbeille", None).unwrap();
        assert_eq!(vider_corbeille(&dossier, d, "images").unwrap(), 1);
        assert!(creations::fiche(d, &jetee).is_err());
        assert!(!creations::dossier(d).join(format!("{jetee}.png")).exists());
        assert!(creations::fiche(d, &garde).is_ok());
        assert!(creations::fiche(d, &clip_jete).is_ok());
        let entree = &journal::lire(&dossier)[0];
        assert_eq!(
            (entree.action.as_str(), entree.objet.as_str()),
            ("Corbeille vidée", "rushes")
        );
        assert_eq!(vider_corbeille(&dossier, d, "images").unwrap(), 0);
    }
}
