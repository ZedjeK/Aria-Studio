//! Les fiches de workflow (`workflows\<id>\fiche.yaml`, format 2), comme en v2 (`app/fiches.py`) : ce qu'il faut
//! savoir d'un modèle avant de l'installer (licence, matériel, profil de prompt, sources) et les entrées du graphe
//! ComfyUI que l'appli remplit. Une fiche qui ne passe pas les contrôles est écartée, avec ses problèmes dits.
//!
//! Les cinq workflows livrés sont embarqués dans l'exe : [`LIVRES`].
//!
//! Différence de lecture avec la v2 : PyYAML lisait `verifiee: 2026-10-26` comme une date ; le YAML 1.2 la lit comme
//! un texte, accepté ici s'il est une vraie date `AAAA-MM-JJ`.

use chrono::NaiveDate;
use serde_json::Value;

/// Le seul format de fiche lu.
pub const FORMAT: u64 = 2;
/// Les styles de prompt d'un modèle.
pub const STYLES: [&str; 3] = ["prose", "instruction", "brief_video"];
/// Les sections de la consigne du LLM, dans l'ordre du gabarit.
pub const SECTIONS_CONSIGNE: [&str; 5] = ["plan", "contenu", "dialogue", "texte", "interdits"];

/// Un workflow livré : son identifiant, sa fiche et son graphe ComfyUI (format API), tels qu'ils sont dans le dépôt.
pub struct Livre {
    pub id: &'static str,
    pub fiche: &'static str,
    pub api: &'static str,
}

macro_rules! livre {
    ($id:literal) => {
        Livre {
            id: $id,
            fiche: include_str!(concat!("../../workflows/", $id, "/fiche.yaml")),
            api: include_str!(concat!("../../workflows/", $id, "/api.json")),
        }
    };
}

/// Les workflows livrés avec l'appli.
pub const LIVRES: [Livre; 5] = [
    livre!("krea2_turbo"),
    livre!("minimax_h3_i2v"),
    livre!("minimax_h3_t2v"),
    livre!("qwen_image_2_1"),
    livre!("qwen_image_2_1_edit"),
];

/// Les problèmes d'une fiche de workflow ; aucun si elle est bonne.
pub fn problemes_workflow(fiche: &Value) -> Vec<String> {
    if fiche.get("format").and_then(Value::as_u64) != Some(FORMAT) {
        return vec![format!("format : {FORMAT} attendu (fiche d'un ancien format)")];
    }
    let mut p: Vec<String> = ["id", "nom", "tache", "famille", "fichiers", "entrees"]
        .iter()
        .filter(|c| !rempli(fiche.get(**c)))
        .map(|c| format!("{c} manquant"))
        .collect();
    if let Some(reserve) = fiche.get("reserve") {
        let ok = reserve.as_str() == Some("retouche")
            && fiche.get("tache").and_then(Value::as_str) == Some("retouche");
        if !ok {
            p.push(
                "reserve : « retouche », et seulement sur un workflow de tâche retouche (ou retire la clé)"
                    .to_owned(),
            );
        }
    }
    p.extend(problemes_licence(fiche.get("licence")));
    p.extend(problemes_materiel(fiche.get("materiel")));
    p.extend(problemes_profil(fiche.get("profil_prompt")));
    p.extend(problemes_civitai(fiche.get("civitai")));
    p.extend(problemes_hugging_face(fiche.get("hugging_face")));
    p
}

/// Une valeur « non vide » au sens de la v2 : ni absente, ni nulle, ni fausse, ni zéro, ni vide.
fn rempli(valeur: Option<&Value>) -> bool {
    match valeur {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Un texte non vide une fois les espaces retirés.
fn texte(valeur: Option<&Value>) -> bool {
    valeur
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
}

/// Un nombre strictement positif, pas un booléen.
fn nombre(valeur: Option<&Value>) -> bool {
    valeur.and_then(Value::as_f64).is_some_and(|x| x > 0.0)
}

/// Une date `AAAA-MM-JJ` qui existe.
fn date(valeur: Option<&Value>) -> bool {
    valeur
        .and_then(Value::as_str)
        .is_some_and(|s| s.len() == 10 && NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok())
}

/// Les problèmes du bloc `licence` (commun aux fiches de workflow et de LLM).
pub fn problemes_licence(licence: Option<&Value>) -> Vec<String> {
    let Some(licence) = licence.and_then(Value::as_object) else {
        return vec![
            "licence : un bloc (id, nom, url, resume, commercial, territoires_exclus, verifiee)".to_owned(),
        ];
    };
    let mut p: Vec<String> = ["id", "nom", "url", "resume"]
        .iter()
        .filter(|c| !texte(licence.get(**c)))
        .map(|c| format!("licence.{c} manquant"))
        .collect();
    if !matches!(licence.get("commercial"), Some(Value::Bool(_) | Value::Null)) {
        p.push("licence.commercial : true, false ou null (à lire)".to_owned());
    }
    if !matches!(
        licence.get("territoires_exclus"),
        Some(Value::Array(_) | Value::Null)
    ) {
        p.push("licence.territoires_exclus : une liste (vide si aucun), ou null (à lire)".to_owned());
    }
    let composants_ok = match licence.get("composants") {
        None => true,
        Some(Value::Array(liste)) => liste.iter().all(|c| {
            c.as_object()
                .is_some_and(|o| o.contains_key("fichier") && o.contains_key("licence"))
        }),
        Some(_) => false,
    };
    if !composants_ok {
        p.push(
            "licence.composants : une liste de {fichier, licence} (fichiers sous une autre licence)"
                .to_owned(),
        );
    }
    if !date(licence.get("verifiee")) {
        p.push("licence.verifiee : la date de lecture de la licence (AAAA-MM-JJ)".to_owned());
    }
    p
}

/// Les problèmes du bloc `materiel`.
pub fn problemes_materiel(materiel: Option<&Value>) -> Vec<String> {
    let Some(materiel) = materiel.and_then(Value::as_object) else {
        return vec![
            "materiel : un bloc (vram_estimee_go, vram_min_go, vram_confort_go, mesure, cartes_verifiees)"
                .to_owned(),
        ];
    };
    let mut p = Vec::new();
    if !nombre(materiel.get("vram_estimee_go"))
        || materiel.get("vram_estimee_go").is_some_and(Value::is_boolean)
    {
        p.push("materiel.vram_estimee_go : un nombre de Go".to_owned());
    }
    for cle in ["vram_min_go", "vram_confort_go"] {
        let ok = match materiel.get(cle) {
            Some(Value::Null) => true,
            Some(v @ Value::Number(_)) => nombre(Some(v)),
            _ => false,
        };
        if !ok {
            p.push(format!(
                "materiel.{cle} : un nombre de Go mesuré, ou null tant que ce n'est pas mesuré"
            ));
        }
    }
    let mesure = materiel.get("mesure").filter(|m| !m.is_null());
    if let Some(mesure) = mesure {
        let ok = mesure
            .as_object()
            .is_some_and(|o| ["carte", "date", "methode"].iter().all(|c| o.contains_key(*c)));
        if !ok {
            p.push("materiel.mesure : null, ou {carte, date, methode}".to_owned());
        }
    }
    let mesure_annoncee = rempli(materiel.get("vram_min_go")) || rempli(materiel.get("vram_confort_go"));
    if mesure_annoncee && mesure.is_none() {
        p.push(
            "materiel : une VRAM minimale ou de confort doit dire comment elle a été mesurée (mesure)"
                .to_owned(),
        );
    }
    if !materiel.get("cartes_verifiees").is_some_and(Value::is_array) {
        p.push("materiel.cartes_verifiees : une liste (vide si aucune)".to_owned());
    }
    p
}

/// Les problèmes du bloc `profil_prompt`.
pub fn problemes_profil(profil: Option<&Value>) -> Vec<String> {
    let Some(profil) = profil.and_then(Value::as_object) else {
        return vec!["profil_prompt : un bloc (style, langue, regles…)".to_owned()];
    };
    let mut p = Vec::new();
    if !profil
        .get("style")
        .and_then(Value::as_str)
        .is_some_and(|s| STYLES.contains(&s))
    {
        p.push(format!("profil_prompt.style : {}", STYLES.join(", ")));
    }
    if !texte(profil.get("langue")) {
        p.push("profil_prompt.langue manquant".to_owned());
    }
    if let Some(mots) = profil.get("mots_max")
        && (!mots.as_i64().is_some_and(|m| (20..=1000).contains(&m)) || mots.is_f64())
    {
        p.push("profil_prompt.mots_max : un entier de 20 à 1000".to_owned());
    }
    for cle in ["regles", "conseils", "sources", "rubriques_ignorees"] {
        if profil.get(cle).is_some_and(|v| !v.is_array()) {
            p.push(format!("profil_prompt.{cle} : une liste"));
        }
    }
    let consigne_ok = match profil.get("consigne") {
        None => true,
        Some(Value::Object(sections)) => sections.iter().all(|(nom, lignes)| {
            SECTIONS_CONSIGNE.contains(&nom.as_str())
                && lignes
                    .as_array()
                    .is_some_and(|l| l.iter().all(|r| r.as_str().is_some_and(|s| !s.trim().is_empty())))
        }),
        Some(_) => false,
    };
    if !consigne_ok {
        p.push(format!(
            "profil_prompt.consigne : des listes de phrases, sous {}",
            SECTIONS_CONSIGNE.join(", ")
        ));
    }
    p
}

fn bases_valides(bloc: &Value) -> bool {
    bloc.get("bases")
        .and_then(Value::as_array)
        .is_some_and(|b| !b.is_empty() && b.iter().all(|x| x.as_str().is_some_and(|s| !s.trim().is_empty())))
}

/// Les problèmes du bloc `civitai` (facultatif).
pub fn problemes_civitai(civitai: Option<&Value>) -> Vec<String> {
    let Some(civitai) = civitai.filter(|c| !c.is_null()) else {
        return Vec::new();
    };
    if !bases_valides(civitai) {
        return vec!["civitai.bases : la liste des « baseModel » de Civitai de cette famille".to_owned()];
    }
    if civitai.get("nom_contient").is_some_and(|n| !texte(Some(n))) {
        return vec!["civitai.nom_contient : un mot (ou retire la clé)".to_owned()];
    }
    Vec::new()
}

/// Les problèmes du bloc `hugging_face` (facultatif).
pub fn problemes_hugging_face(hf: Option<&Value>) -> Vec<String> {
    let Some(hf) = hf.filter(|h| !h.is_null()) else {
        return Vec::new();
    };
    let depots = hf.get("bases").and_then(Value::as_array);
    if !bases_valides(hf)
        || depots.is_some_and(|b| b.iter().any(|x| !x.as_str().is_some_and(|s| s.contains('/'))))
    {
        return vec!["hugging_face.bases : la liste des dépôts d'origine (organisation/dépôt)".to_owned()];
    }
    Vec::new()
}

/// Une fiche lue dans son texte YAML.
///
/// # Errors
/// Le texte n'est pas du YAML lisible.
pub fn lire(texte: &str) -> Result<Value, String> {
    serde_saphyr::from_str::<Value>(texte).map_err(|e| format!("YAML illisible : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    fn livre(id: &str) -> Value {
        let l = LIVRES.iter().find(|l| l.id == id).unwrap();
        lire(l.fiche).unwrap()
    }

    #[test]
    fn chaque_dossier_de_workflows_est_livre() {
        let dossier = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../workflows");
        let sur_disque: BTreeSet<String> = std::fs::read_dir(dossier)
            .unwrap()
            .map(|e| e.unwrap())
            .filter(|e| e.path().join("fiche.yaml").is_file())
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        let livres: BTreeSet<String> = LIVRES.iter().map(|l| l.id.to_owned()).collect();
        assert_eq!(sur_disque, livres);
    }

    #[test]
    fn toutes_les_fiches_livrees_sont_valides() {
        for l in &LIVRES {
            let fiche = lire(l.fiche).unwrap();
            assert_eq!(problemes_workflow(&fiche), Vec::<String>::new(), "{}", l.id);
            assert_eq!(fiche["id"], l.id);
            let api: Value = serde_json::from_str(l.api).unwrap();
            assert!(api.is_object(), "{}", l.id);
        }
    }

    #[test]
    fn les_identifiants_de_noeuds_restent_des_textes() {
        // En YAML 1.1, '30:3' sans apostrophes serait un nombre sexagésimal : ici tout reste du texte.
        let krea = livre("krea2_turbo");
        assert_eq!(krea["entrees"]["seed"], json!(["30:3", "seed"]));
        let qwen = livre("qwen_image_2_1");
        assert_eq!(qwen["entrees"]["prompt"], json!(["459:452", "prompt"]));
        assert_eq!(krea["defauts"]["ratio"], "1:1 (Square)");
    }

    #[test]
    fn composants_et_licences_comme_en_v2() {
        for l in &LIVRES {
            let fiche = lire(l.fiche).unwrap();
            let fichiers: BTreeSet<&str> = fiche["fichiers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["nom"].as_str().unwrap())
                .collect();
            for c in fiche["licence"]["composants"].as_array().into_iter().flatten() {
                assert!(fichiers.contains(c["fichier"].as_str().unwrap()), "{}", l.id);
            }
        }
        assert_eq!(livre("qwen_image_2_1")["licence"]["commercial"], false);
        assert!(livre("krea2_turbo")["licence"]["commercial"].is_null());
        assert!(livre("krea2_turbo")["licence"]["territoires_exclus"].is_null());
        let h3 = livre("minimax_h3_t2v");
        let exclus = h3["licence"]["territoires_exclus"].to_string();
        assert!(
            exclus.contains("Union") || exclus.contains("UE") || exclus.contains("EU"),
            "{exclus}"
        );
    }

    #[test]
    fn aucun_minimum_sans_mesure() {
        for l in &LIVRES {
            let fiche = lire(l.fiche).unwrap();
            assert!(fiche["materiel"]["vram_min_go"].is_null(), "{}", l.id);
            assert!(fiche["materiel"]["mesure"].is_null(), "{}", l.id);
        }
    }

    fn bonne() -> Value {
        livre("krea2_turbo")
    }

    #[test]
    fn un_ancien_format_donne_un_seul_probleme() {
        let mut f = bonne();
        f.as_object_mut().unwrap().remove("format");
        assert_eq!(
            problemes_workflow(&f),
            vec!["format : 2 attendu (fiche d'un ancien format)"]
        );
    }

    #[test]
    fn la_licence_est_controlee() {
        let mut f = bonne();
        f["licence"].as_object_mut().unwrap().remove("url");
        f["licence"]["commercial"] = json!("peut-être");
        f["licence"]["verifiee"] = json!("bientôt");
        let p = problemes_workflow(&f);
        assert!(p.contains(&"licence.url manquant".to_owned()), "{p:?}");
        assert!(
            p.contains(&"licence.commercial : true, false ou null (à lire)".to_owned()),
            "{p:?}"
        );
        assert!(
            p.contains(&"licence.verifiee : la date de lecture de la licence (AAAA-MM-JJ)".to_owned()),
            "{p:?}"
        );
        let mut f = bonne();
        f["licence"]["verifiee"] = json!("2026-02-30");
        assert!(!problemes_workflow(&f).is_empty(), "une date qui n'existe pas");
        let mut f = bonne();
        f["licence"]["composants"] = json!([{ "fichier": "a.safetensors" }]);
        assert!(
            problemes_workflow(&f)
                .iter()
                .any(|x| x.starts_with("licence.composants"))
        );
    }

    #[test]
    fn le_materiel_est_controle() {
        let mut f = bonne();
        f["materiel"]["vram_min_go"] = json!(12);
        assert!(
            problemes_workflow(&f)
                .iter()
                .any(|x| x.contains("doit dire comment elle a été mesurée"))
        );
        f["materiel"]["mesure"] = json!({ "carte": "RTX 4080", "date": "2026-11-01", "methode": "paliers" });
        assert_eq!(problemes_workflow(&f), Vec::<String>::new());
        f["materiel"]["vram_estimee_go"] = json!(0);
        assert!(problemes_workflow(&f).contains(&"materiel.vram_estimee_go : un nombre de Go".to_owned()));
    }

    #[test]
    fn le_profil_est_controle() {
        let mut f = bonne();
        f["profil_prompt"]["style"] = json!("poème");
        f["profil_prompt"]["mots_max"] = json!(5);
        f["profil_prompt"]["regles"] = json!("une seule règle");
        let p = problemes_workflow(&f);
        assert!(
            p.contains(&"profil_prompt.style : prose, instruction, brief_video".to_owned()),
            "{p:?}"
        );
        assert!(
            p.contains(&"profil_prompt.mots_max : un entier de 20 à 1000".to_owned()),
            "{p:?}"
        );
        assert!(
            p.contains(&"profil_prompt.regles : une liste".to_owned()),
            "{p:?}"
        );
        let mut f = bonne();
        f["profil_prompt"]["consigne"] = json!({ "plan": ["a"], "inconnue": ["b"] });
        assert!(
            problemes_workflow(&f)
                .iter()
                .any(|x| x.starts_with("profil_prompt.consigne"))
        );
        f["profil_prompt"]["consigne"] =
            json!({ "plan": ["Sujet, puis décor."], "interdits": ["Pas de texte."] });
        assert_eq!(problemes_workflow(&f), Vec::<String>::new());
    }

    #[test]
    fn les_sources_sont_controlees() {
        let mut f = bonne();
        f["hugging_face"] = json!({ "bases": ["sans-barre"] });
        assert!(
            problemes_workflow(&f)
                .iter()
                .any(|x| x.starts_with("hugging_face.bases"))
        );
        let mut f = bonne();
        f["civitai"] = json!({ "bases": [] });
        assert!(
            problemes_workflow(&f)
                .iter()
                .any(|x| x.starts_with("civitai.bases"))
        );
        let mut f = bonne();
        f["reserve"] = json!("retouche");
        assert!(problemes_workflow(&f).iter().any(|x| x.starts_with("reserve")));
    }
}
