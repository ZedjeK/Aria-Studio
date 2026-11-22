//! Les personnages enregistrés du Scénario, comme en v2 (`app/personnages.py`, `data\personnages.json`).
//!
//! On garde les choix (valeurs françaises du bloc « personnage ») et l'âge précis, pas le texte anglais : un
//! personnage se rappelle dans n'importe quelle scène et suit les listes si elles évoluent. Les valeurs d'avant les
//! simplifications des listes de la v2 (origines, silhouettes, poses, actions) sont converties à la lecture.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::Dossier;
use crate::listes::{self, DemandeAssemblage};

const FICHIER: &str = "personnages.json";

/// Les correspondances des anciennes valeurs (sous-liste → ancienne → nouvelle ; rien : retirée).
/// Ancienne valeur → nouvelle (rien : retirée).
type Correspondances = &'static [(&'static str, Option<&'static str>)];

const ANCIENNES: [(&str, Correspondances); 4] = [
    (
        "Origine",
        &[
            ("Française aux racines bretonnes", Some("Européenne de l'Ouest")),
            ("Française", Some("Européenne de l'Ouest")),
            ("Britannique", Some("Européenne de l'Ouest")),
            ("Irlandaise", Some("Européenne de l'Ouest")),
            ("Allemande", Some("Européenne de l'Ouest")),
            ("Nord-américaine", Some("Européenne de l'Ouest")),
            ("Scandinave", Some("Nordique")),
            (
                "Méditerranéenne (italienne, espagnole, grecque)",
                Some("Méditerranéenne"),
            ),
            ("Slave (russe, polonaise, ukrainienne)", Some("Slave")),
            ("Mongole", Some("Chinoise")),
            ("Vietnamienne", Some("Asiatique du Sud-Est")),
            ("Thaïlandaise", Some("Asiatique du Sud-Est")),
            ("Philippine", Some("Asiatique du Sud-Est")),
            ("Indonésienne", Some("Asiatique du Sud-Est")),
            ("Eurasienne franco-japonaise", Some("Eurasienne")),
            ("Eurasienne franco-coréenne", Some("Eurasienne")),
            ("Eurasienne franco-vietnamienne", Some("Eurasienne")),
            ("Eurasienne franco-chinoise", Some("Eurasienne")),
            ("Indienne", Some("Sud-asiatique")),
            ("Maghrébine", Some("Moyen-orientale")),
            ("Libanaise", Some("Moyen-orientale")),
            ("Persane", Some("Moyen-orientale")),
            ("Turque", Some("Moyen-orientale")),
            ("Ouest-africaine", Some("Africaine")),
            ("Est-africaine", Some("Africaine")),
            ("Afro-américaine", Some("Afro-descendante")),
            ("Antillaise", Some("Afro-descendante")),
            ("Brésilienne", Some("Latino-américaine")),
            ("Mexicaine", Some("Latino-américaine")),
        ],
    ),
    (
        "Silhouette",
        &[
            ("Longiligne", Some("Fine et élancée")),
            ("Sportive et tonique", Some("Athlétique")),
            ("Musclée de danseuse", Some("Athlétique")),
            ("Sec et nerveux", Some("Athlétique")),
            ("Musclé", Some("Musclé(e)")),
            ("Carrure large", Some("Musclé(e)")),
            ("Voluptueuse", Some("Pulpeuse")),
            ("Trapu", Some("Trapu(e)")),
            ("Robuste", Some("Trapu(e)")),
            ("Généreuse", Some("Ronde")),
        ],
    ),
    (
        "Pose",
        &[
            ("Assise", Some("Assis(e)")),
            ("Assise en tailleur", Some("Assis(e) en tailleur")),
            ("Accroupie", Some("Accroupi(e)")),
            ("Agenouillée", Some("Agenouillé(e)")),
            ("Allongée dans l'herbe", Some("Allongé(e) dans l'herbe")),
            ("Appuyée contre un mur", Some("Appuyé(e) contre un mur")),
            ("Recroquevillée", Some("Recroquevillé(e)")),
            ("Allongée sur un lit", Some("Allongé(e) sur un lit")),
        ],
    ),
    (
        "Action",
        &[
            ("Écoute de la musique", Some("Écoute de la musique au casque")),
            ("Aucune", None),
        ],
    ),
];

/// Un personnage enregistré.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Personnage {
    pub nom: String,
    /// {sous-liste: valeur} du bloc « personnage ».
    pub choix: BTreeMap<String, String>,
    pub age: Option<u32>,
}

static NOM: OnceLock<Option<Regex>> = OnceLock::new();

/// Un nom de personnage : lettres, chiffres, espaces, 40 caractères au plus.
fn nom_valide(nom: &str) -> bool {
    NOM.get_or_init(|| Regex::new(r"^\w[\w .'’-]{0,39}$").ok())
        .as_ref()
        .is_some_and(|r| r.is_match(nom))
}

fn chemin(data: &Path) -> std::path::PathBuf {
    data.join(FICHIER)
}

fn ecrire(data: &Path, personnages: &Map<String, Value>) -> Result<(), String> {
    fs::create_dir_all(data).map_err(|e| format!("{} : {e}", data.display()))?;
    let texte =
        serde_json::to_string_pretty(&Value::Object(personnages.clone())).map_err(|e| e.to_string())?;
    let temporaire = data.join("personnages.tmp");
    fs::write(&temporaire, texte)
        .and_then(|()| fs::rename(&temporaire, chemin(data)))
        .map_err(|e| format!("{} : {e}", chemin(data).display()))
}

/// Le fichier, ses anciennes valeurs converties (et le fichier réécrit s'il y en avait).
fn lire(data: &Path) -> Result<Map<String, Value>, String> {
    let Ok(texte) = fs::read_to_string(chemin(data)) else {
        return Ok(Map::new());
    };
    let mut personnages = match serde_json::from_str::<Value>(&texte) {
        Ok(Value::Object(m)) => m,
        _ => return Err(format!("{} illisible", chemin(data).display())),
    };
    let mut converti = false;
    for p in personnages.values_mut() {
        let Some(choix) = p.get_mut("choix").and_then(Value::as_object_mut) else {
            continue;
        };
        for (sous_liste, correspondances) in ANCIENNES {
            let Some(ancienne) = choix.get(sous_liste).and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            if let Some((_, nouvelle)) = correspondances.iter().find(|(a, _)| *a == ancienne) {
                match nouvelle {
                    Some(n) => {
                        choix.insert(sous_liste.to_owned(), json!(n));
                    }
                    None => {
                        choix.remove(sous_liste);
                    }
                }
                converti = true;
            }
        }
    }
    if converti {
        ecrire(data, &personnages)?;
    }
    Ok(personnages)
}

fn personnage(nom: &str, p: &Value) -> Personnage {
    Personnage {
        nom: nom.to_owned(),
        choix: p
            .get("choix")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(sl, v)| v.as_str().map(|v| (sl.clone(), v.to_owned())))
            .collect(),
        age: p
            .get("age")
            .and_then(Value::as_u64)
            .and_then(|a| u32::try_from(a).ok()),
    }
}

/// Les personnages, par nom (sans tenir compte de la casse).
///
/// # Errors
/// Fichier illisible.
pub fn liste(data: &Path) -> Result<Vec<Personnage>, String> {
    let mut sortie: Vec<Personnage> = lire(data)?.iter().map(|(nom, p)| personnage(nom, p)).collect();
    sortie.sort_by_key(|p| p.nom.to_lowercase());
    Ok(sortie)
}

/// Enregistre un personnage, après les mêmes contrôles qu'à l'assemblage (genre, incompatibilités, valeurs adultes
/// et mineurs, filtre AS-18).
///
/// # Errors
/// Nom invalide, rien à enregistrer, règle des listes ou filtre AS-18, fichier qui ne s'écrit pas.
pub fn enregistrer(
    dossier: &Dossier,
    data: &Path,
    nom: &str,
    choix: &BTreeMap<String, String>,
    age: Option<u32>,
) -> Result<Personnage, String> {
    let nom = nom.trim();
    if !nom_valide(nom) {
        return Err(
            "Nom de personnage invalide (lettres, chiffres, espaces, 40 caractères au plus)".to_owned(),
        );
    }
    let choix: BTreeMap<String, String> = choix
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if choix.is_empty() && age.is_none() {
        return Err("Rien à enregistrer : choisis au moins une valeur du personnage".to_owned());
    }
    let mut bloc = Map::new();
    for (sl, v) in &choix {
        bloc.insert(sl.clone(), json!(v));
    }
    let mut choix_listes = Map::new();
    choix_listes.insert("personnage".to_owned(), Value::Object(bloc));
    listes::assembler(
        dossier,
        &DemandeAssemblage {
            sujet: "personnage".to_owned(),
            choix: choix_listes,
            age,
            ..DemandeAssemblage::default()
        },
    )?;
    let mut personnages = lire(data)?;
    personnages.insert(nom.to_owned(), json!({"choix": choix, "age": age}));
    ecrire(data, &personnages)?;
    Ok(Personnage {
        nom: nom.to_owned(),
        choix,
        age,
    })
}

/// Supprime un personnage.
///
/// # Errors
/// Personnage inconnu, ou fichier qui ne s'écrit pas.
pub fn supprimer(data: &Path, nom: &str) -> Result<(), String> {
    let mut personnages = lire(data)?;
    if personnages.remove(nom).is_none() {
        return Err(format!("Personnage inconnu : {nom}"));
    }
    ecrire(data, &personnages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enregistrer_relire_supprimer() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let data = temp.path().join("data");
        let choix: BTreeMap<String, String> = [
            ("Genre".to_owned(), "Femme".to_owned()),
            ("Haut".to_owned(), String::new()),
        ]
        .into_iter()
        .collect();
        let p = enregistrer(&dossier, &data, "  Léa  ", &choix, Some(30)).unwrap();
        assert_eq!((p.nom.as_str(), p.choix.len(), p.age), ("Léa", 1, Some(30)));
        assert!(enregistrer(&dossier, &data, "", &choix, None).is_err());
        assert!(enregistrer(&dossier, &data, &"a".repeat(41), &choix, None).is_err());
        assert!(enregistrer(&dossier, &data, "Vide", &BTreeMap::new(), None).is_err());
        let robot: BTreeMap<String, String> =
            [("Genre".to_owned(), "Robot".to_owned())].into_iter().collect();
        assert!(enregistrer(&dossier, &data, "Robot", &robot, None).is_err());
        enregistrer(&dossier, &data, "anna", &choix, None).unwrap();
        let noms: Vec<String> = liste(&data).unwrap().into_iter().map(|p| p.nom).collect();
        assert_eq!(noms, ["anna", "Léa"]);
        supprimer(&data, "anna").unwrap();
        assert!(supprimer(&data, "anna").is_err());
        assert_eq!(liste(&data).unwrap().len(), 1);
    }

    #[test]
    fn anciennes_valeurs_converties() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path();
        fs::write(
            data.join(FICHIER),
            r#"{"Yuki": {"choix": {"Origine": "Mongole", "Action": "Aucune", "Genre": "Femme"}, "age": null}}"#,
        )
        .unwrap();
        let p = &liste(data).unwrap()[0];
        assert_eq!(p.choix.get("Origine").map(String::as_str), Some("Chinoise"));
        assert!(!p.choix.contains_key("Action"));
        // Le fichier est réécrit converti.
        assert!(
            fs::read_to_string(data.join(FICHIER))
                .unwrap()
                .contains("Chinoise")
        );
    }
}
