//! Les créations (« rushes ») : une image PNG et sa fiche JSON par création, dans `data\creations\`, comme en v2
//! (`app/plateau.py`, partie galerie). La v2 et la v3 partagent ce dossier : même nom de fichier, même fiche.
//!
//! Identifiant : `AAAAMMJJ-HHMMSS-xxxxxx` (date locale, puis six chiffres hexadécimaux tirés au hasard). Il est
//! toujours contrôlé en entier avant de devenir un chemin : un identifiant venu de l'interface ne sort jamais du
//! dossier.
//!
//! Fiche (clés de la v2) : `id`, `date` (`AAAA-MM-JJTHH:MM:SS`, heure locale), `largeur`, `hauteur`, puis ce que
//! donne celui qui enregistre (`type`, `idee`, `prompt`, `workflow`, `modele`, `seed`, `duree_s`…).

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};
use serde::Serialize;
use serde_json::{Map, Value};

/// Le dossier des créations, dans `data\`.
pub const DOSSIER: &str = "creations";
/// Profondeur de recherche des textes d'origine (parents et références), comme en v2.
pub const PROFONDEUR_ORIGINE: usize = 20;

const SIGNATURE_PNG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

/// Le dossier des créations de ce `data\`.
pub fn dossier(data: &Path) -> PathBuf {
    data.join(DOSSIER)
}

/// Vrai si `id` a la forme d'un identifiant de création (`AAAAMMJJ-HHMMSS-xxxxxx`, hexadécimal en minuscules).
pub fn id_valide(id: &str) -> bool {
    let octets = id.as_bytes();
    octets.len() == 22
        && octets.iter().enumerate().all(|(i, &c)| match i {
            8 | 15 => c == b'-',
            0..=14 => c.is_ascii_digit(),
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(&c),
        })
}

/// Largeur et hauteur d'un PNG, lues dans son en-tête.
///
/// # Errors
/// Les octets ne sont pas un PNG.
pub fn dimensions_png(png: &[u8]) -> Result<(u32, u32), String> {
    let entete = png
        .get(..24)
        .filter(|e| e.starts_with(&SIGNATURE_PNG) && &e[12..16] == b"IHDR")
        .ok_or_else(|| "l'image reçue n'est pas un PNG".to_owned())?;
    let largeur = u32::from_be_bytes([entete[16], entete[17], entete[18], entete[19]]);
    let hauteur = u32::from_be_bytes([entete[20], entete[21], entete[22], entete[23]]);
    Ok((largeur, hauteur))
}

fn suffixe_aleatoire() -> Result<String, String> {
    let mut octets = [0u8; 3];
    getrandom::fill(&mut octets).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    Ok(octets.iter().map(|o| format!("{o:02x}")).collect())
}

/// Le JSON écrit comme la v2 (`json.dumps(indent=4, ensure_ascii=False)`).
fn json_v2(valeur: &Value) -> Result<Vec<u8>, String> {
    let mut sortie = Vec::new();
    let formateur = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serialiseur = serde_json::Serializer::with_formatter(&mut sortie, formateur);
    valeur
        .serialize(&mut serialiseur)
        .map_err(|e| format!("fiche de création illisible : {e}"))?;
    Ok(sortie)
}

/// Enregistre une création : le PNG, puis sa fiche. Rend son identifiant.
///
/// # Errors
/// Les octets ne sont pas un PNG, ou le dossier ne s'écrit pas.
pub fn enregistrer(data: &Path, png: &[u8], infos: &Map<String, Value>) -> Result<String, String> {
    enregistrer_a(data, png, infos, Local::now().naive_local())
}

/// [`enregistrer`] à une date donnée (pour les tests).
///
/// # Errors
/// Voir [`enregistrer`].
pub fn enregistrer_a(
    data: &Path,
    png: &[u8],
    infos: &Map<String, Value>,
    maintenant: NaiveDateTime,
) -> Result<String, String> {
    let (largeur, hauteur) = dimensions_png(png)?;
    let dossier = dossier(data);
    fs::create_dir_all(&dossier).map_err(|e| format!("dossier des créations : {e}"))?;
    // Le PNG est créé sans écraser : un identifiant déjà pris (même seconde, même tirage) en fait tirer un autre.
    let mut essais = 0;
    let id = loop {
        let id = format!("{}-{}", maintenant.format("%Y%m%d-%H%M%S"), suffixe_aleatoire()?);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dossier.join(format!("{id}.png")))
        {
            Ok(mut fichier) => {
                fichier
                    .write_all(png)
                    .and_then(|()| fichier.sync_all())
                    .map_err(|e| format!("image de la création : {e}"))?;
                break id;
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists && essais < 10 => essais += 1,
            Err(e) => return Err(format!("image de la création : {e}")),
        }
    };
    let mut fiche = Map::new();
    fiche.insert("id".to_owned(), Value::String(id.clone()));
    fiche.insert(
        "date".to_owned(),
        Value::String(maintenant.format("%Y-%m-%dT%H:%M:%S").to_string()),
    );
    fiche.insert("largeur".to_owned(), largeur.into());
    fiche.insert("hauteur".to_owned(), hauteur.into());
    for (cle, valeur) in infos {
        fiche.insert(cle.clone(), valeur.clone());
    }
    // La fiche passe par un fichier temporaire : une fiche à moitié écrite ne doit jamais apparaître dans la galerie.
    let temporaire = dossier.join(format!("{id}.json.tmp"));
    let ecrit = json_v2(&Value::Object(fiche)).and_then(|octets| {
        fs::write(&temporaire, octets)
            .and_then(|()| fs::rename(&temporaire, dossier.join(format!("{id}.json"))))
            .map_err(|e| format!("fiche de la création : {e}"))
    });
    if let Err(erreur) = ecrit {
        let _ = fs::remove_file(&temporaire);
        let _ = fs::remove_file(dossier.join(format!("{id}.png")));
        return Err(erreur);
    }
    Ok(id)
}

fn verifier_id(data: &Path, id: &str) -> Result<(), String> {
    if id_valide(id) && dossier(data).join(format!("{id}.json")).is_file() {
        Ok(())
    } else {
        Err(format!("Création inconnue : {id}"))
    }
}

/// La fiche d'une création.
///
/// # Errors
/// Identifiant mal formé ou inconnu, ou fiche illisible.
pub fn fiche(data: &Path, id: &str) -> Result<Value, String> {
    verifier_id(data, id)?;
    let texte = fs::read_to_string(dossier(data).join(format!("{id}.json")))
        .map_err(|e| format!("fiche de {id} : {e}"))?;
    serde_json::from_str(&texte).map_err(|e| format!("fiche de {id} illisible : {e}"))
}

/// Les fiches des `limite` créations les plus récentes, la plus récente en premier. Une fiche illisible est
/// sautée (la v2 s'arrêtait dessus).
pub fn liste(data: &Path, limite: usize) -> Vec<Value> {
    let Ok(lecture) = fs::read_dir(dossier(data)) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = lecture
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".json"))
                .filter(|id| id_valide(id))
                .map(str::to_owned)
        })
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.into_iter()
        .take(limite)
        .filter_map(|id| fiche(data, &id).ok())
        .collect()
}

/// Réécrit la fiche d'une création qui existe (rangement, archive, corbeille), par un fichier temporaire : une fiche
/// à moitié écrite ne doit jamais apparaître.
///
/// # Errors
/// Fiche sans identifiant, création inconnue, ou dossier qui ne s'écrit pas.
pub fn ecrire_fiche(data: &Path, fiche: &Value) -> Result<(), String> {
    let id = fiche
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Fiche de création sans identifiant".to_owned())?;
    verifier_id(data, id)?;
    let dossier = dossier(data);
    let temporaire = dossier.join(format!("{id}.json.tmp"));
    let ecrit = json_v2(fiche).and_then(|octets| {
        fs::write(&temporaire, octets)
            .and_then(|()| fs::rename(&temporaire, dossier.join(format!("{id}.json"))))
            .map_err(|e| format!("fiche de {id} : {e}"))
    });
    if ecrit.is_err() {
        let _ = fs::remove_file(&temporaire);
    }
    ecrit
}

/// Le chemin du PNG d'une création.
///
/// # Errors
/// Identifiant mal formé ou inconnu.
pub fn chemin_image(data: &Path, id: &str) -> Result<PathBuf, String> {
    verifier_id(data, id)?;
    Ok(dossier(data).join(format!("{id}.png")))
}

/// Supprime une création : son image, sa vidéo s'il y en a une, puis sa fiche.
///
/// # Errors
/// Identifiant mal formé ou inconnu, ou fichier qui ne s'efface pas.
pub fn supprimer(data: &Path, id: &str) -> Result<(), String> {
    verifier_id(data, id)?;
    let dossier = dossier(data);
    for extension in ["png", "mp4"] {
        if let Err(e) = fs::remove_file(dossier.join(format!("{id}.{extension}")))
            && e.kind() != ErrorKind::NotFound
        {
            return Err(format!("{id}.{extension} : {e}"));
        }
    }
    fs::remove_file(dossier.join(format!("{id}.json"))).map_err(|e| format!("{id}.json : {e}"))
}

/// Idées et prompts des créations données et de leurs ancêtres (parents, références), pour le filtre AS-18 : il ne
/// lit pas les images, alors une création faite avec un personnage mineur le reste quand on la réutilise.
///
/// # Errors
/// Une des créations données n'existe pas (un ancêtre effacé est simplement ignoré).
pub fn textes_origine(data: &Path, ids: &[String]) -> Result<Vec<String>, String> {
    let mut textes = Vec::new();
    let mut vus: Vec<String> = Vec::new();
    let mut a_voir: Vec<String> = ids.to_vec();
    while vus.len() < PROFONDEUR_ORIGINE {
        let Some(id) = a_voir.pop() else { break };
        if vus.contains(&id) {
            continue;
        }
        let f = fiche(data, &id)?;
        vus.push(id);
        for cle in ["idee", "prompt"] {
            if let Some(texte) = f.get(cle).and_then(Value::as_str).filter(|t| !t.is_empty()) {
                textes.push(texte.to_owned());
            }
        }
        let parent = f.get("parent").and_then(Value::as_str);
        let references = f
            .get("references")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str);
        for ancetre in parent.into_iter().chain(references) {
            if verifier_id(data, ancetre).is_ok() {
                a_voir.push(ancetre.to_owned());
            }
        }
    }
    Ok(textes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Un PNG minimal : signature et en-tête IHDR (le reste n'est pas lu).
    fn png(largeur: u32, hauteur: u32) -> Vec<u8> {
        let mut octets = SIGNATURE_PNG.to_vec();
        octets.extend_from_slice(&13u32.to_be_bytes());
        octets.extend_from_slice(b"IHDR");
        octets.extend_from_slice(&largeur.to_be_bytes());
        octets.extend_from_slice(&hauteur.to_be_bytes());
        octets.extend_from_slice(&[8, 6, 0, 0, 0]);
        octets
    }

    fn infos(valeur: Value) -> Map<String, Value> {
        valeur.as_object().cloned().unwrap()
    }

    fn date(texte: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(texte, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn identifiants() {
        assert!(id_valide("20261122-140305-0a9fbc"));
        assert!(!id_valide("20261122-140305-0A9FBC"));
        assert!(!id_valide("20261122-140305-0a9fb"));
        assert!(!id_valide("20261122-140305-0a9fbc\n"));
        assert!(!id_valide("../../reglages"));
        assert!(!id_valide("2026112a-140305-0a9fbc"));
        assert!(!id_valide("20261122_140305-0a9fbc"));
    }

    #[test]
    fn dimensions() {
        assert_eq!(dimensions_png(&png(1328, 1328)).unwrap(), (1328, 1328));
        assert_eq!(dimensions_png(&png(1664, 928)).unwrap(), (1664, 928));
        assert!(dimensions_png(b"GIF89a").is_err());
        assert!(dimensions_png(&png(1, 1)[..20]).is_err());
    }

    #[test]
    fn enregistre_comme_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let id = enregistrer_a(
            temp.path(),
            &png(1328, 1328),
            &infos(json!({"type": "creation", "prompt": "a red fox, été", "seed": 42, "duree_s": 12.5})),
            date("2026-11-22T14:03:05"),
        )
        .unwrap();
        assert!(id.starts_with("20261122-140305-"));
        assert!(id_valide(&id));
        let dossier = temp.path().join("creations");
        assert_eq!(
            fs::read(dossier.join(format!("{id}.png"))).unwrap(),
            png(1328, 1328)
        );
        let texte = fs::read_to_string(dossier.join(format!("{id}.json"))).unwrap();
        // Indentation de quatre espaces et accents écrits tels quels, comme json.dumps de la v2.
        assert!(texte.contains("\n    \"date\": \"2026-11-22T14:03:05\""));
        assert!(texte.contains("été"));
        let f = fiche(temp.path(), &id).unwrap();
        assert_eq!(f["id"], json!(id));
        assert_eq!(f["largeur"], json!(1328));
        assert_eq!(f["hauteur"], json!(1328));
        assert_eq!(f["seed"], json!(42));
        assert_eq!(f["duree_s"], json!(12.5));
        assert!(!dossier.join(format!("{id}.json.tmp")).exists());
    }

    #[test]
    fn refuse_ce_qui_n_est_pas_un_png() {
        let temp = tempfile::tempdir().unwrap();
        assert!(enregistrer(temp.path(), b"pas une image", &Map::new()).is_err());
        assert!(liste(temp.path(), 10).is_empty());
    }

    #[test]
    fn liste_la_plus_recente_d_abord() {
        let temp = tempfile::tempdir().unwrap();
        let ancienne =
            enregistrer_a(temp.path(), &png(8, 8), &Map::new(), date("2026-11-01T10:00:00")).unwrap();
        let recente =
            enregistrer_a(temp.path(), &png(8, 8), &Map::new(), date("2026-11-22T10:00:00")).unwrap();
        let milieu =
            enregistrer_a(temp.path(), &png(8, 8), &Map::new(), date("2026-11-10T10:00:00")).unwrap();
        // Une fiche illisible et un fichier étranger sont ignorés.
        let dossier = temp.path().join("creations");
        fs::write(dossier.join("20261123-000000-abcdef.json"), "{").unwrap();
        fs::write(dossier.join("notes.json"), "{}").unwrap();
        let ids: Vec<Value> = liste(temp.path(), 10)
            .into_iter()
            .map(|f| f["id"].clone())
            .collect();
        assert_eq!(ids, vec![json!(recente), json!(milieu), json!(ancienne)]);
        assert_eq!(liste(temp.path(), 2).len(), 1); // la plus récente est illisible : sautée, pas remplacée
        assert!(liste(&temp.path().join("absent"), 10).is_empty());
    }

    #[test]
    fn un_identifiant_ne_sort_pas_du_dossier() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("reglages.json"), "{}").unwrap();
        assert!(fiche(temp.path(), "../reglages").is_err());
        assert!(chemin_image(temp.path(), "../reglages").is_err());
        assert!(supprimer(temp.path(), "../reglages").is_err());
        assert!(temp.path().join("reglages.json").exists());
        assert_eq!(
            fiche(temp.path(), "20261122-140305-0a9fbc").unwrap_err(),
            "Création inconnue : 20261122-140305-0a9fbc"
        );
    }

    #[test]
    fn reecrit_une_fiche_existante() {
        let temp = tempfile::tempdir().unwrap();
        let id = enregistrer(temp.path(), &png(8, 8), &infos(json!({"prompt": "a fox"}))).unwrap();
        let mut f = fiche(temp.path(), &id).unwrap();
        f["archive"] = json!(true);
        ecrire_fiche(temp.path(), &f).unwrap();
        let relue = fiche(temp.path(), &id).unwrap();
        assert_eq!(relue["archive"], json!(true));
        assert_eq!(relue["prompt"], json!("a fox"));
        assert!(ecrire_fiche(temp.path(), &json!({"id": "20200101-000000-000000"})).is_err());
        assert!(ecrire_fiche(temp.path(), &json!({"prompt": "x"})).is_err());
    }

    #[test]
    fn supprime_image_video_et_fiche() {
        let temp = tempfile::tempdir().unwrap();
        let id = enregistrer(temp.path(), &png(8, 8), &Map::new()).unwrap();
        let dossier = temp.path().join("creations");
        fs::write(dossier.join(format!("{id}.mp4")), b"clip").unwrap();
        assert!(chemin_image(temp.path(), &id).unwrap().is_file());
        supprimer(temp.path(), &id).unwrap();
        assert_eq!(fs::read_dir(&dossier).unwrap().count(), 0);
        assert!(supprimer(temp.path(), &id).is_err());
    }

    #[test]
    fn textes_des_ancetres() {
        let temp = tempfile::tempdir().unwrap();
        let d = temp.path();
        let grand_parent = enregistrer(
            d,
            &png(8, 8),
            &infos(json!({"idee": "idée 1", "prompt": "prompt 1"})),
        )
        .unwrap();
        let reference =
            enregistrer(d, &png(8, 8), &infos(json!({"idee": "", "prompt": "prompt ref"}))).unwrap();
        let parent = enregistrer(
            d,
            &png(8, 8),
            &infos(
                json!({"idee": "idée 2", "prompt": null, "parent": grand_parent, "references": [reference]}),
            ),
        )
        .unwrap();
        // Un ancêtre effacé ou mal écrit est ignoré.
        let enfant = enregistrer(
            d,
            &png(8, 8),
            &infos(json!({"prompt": "prompt 3", "parent": parent, "references": ["20200101-000000-000000", "../x"]})),
        )
        .unwrap();
        let mut textes = textes_origine(d, std::slice::from_ref(&enfant)).unwrap();
        textes.sort();
        assert_eq!(
            textes,
            vec!["idée 1", "idée 2", "prompt 1", "prompt 3", "prompt ref"]
        );
        assert!(textes_origine(d, &["20200101-000000-000000".to_owned()]).is_err());
        assert!(textes_origine(d, &[]).unwrap().is_empty());
    }
}
