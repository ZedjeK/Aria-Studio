//! Ce que les séquences et les essais de voix partagent pour MiniMax H3, comme en v2 (`app/sequences.py`,
//! `app/repliques.py`, `app/decoupage.py`) : le nombre d'images d'un clip, ses dimensions, les deux VAE du graphe,
//! l'élagage des branches débranchées, la langue d'une réplique et sa balise, les répliques d'un texte, et les champs
//! d'un prompt H3 (trois champs, ou les six du mode référence).

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

/// Images par seconde des clips de H3.
pub const IPS: u32 = 24;

/// Le nombre d'images d'un clip de `duree` secondes : arrondi à la grille 17k+5 de H3, comme le graphe du workflow.
pub fn images(duree: u32) -> u32 {
    let n = (duree * IPS).max(5);
    // n + (5 - n) mod 17, en restant dans les entiers non signés.
    n + (17 - (n + 12) % 17) % 17
}

/// Largeur et hauteur multiples de 32, comme `ImageScaleToTotalPixels` (mégapixels × 1024²).
pub fn dimensions(ratio: f64, megapixels: f64) -> (u32, u32) {
    let total = megapixels * 1024.0 * 1024.0;
    let arrondi = |x: f64| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positif, quelques milliers
        let n = (x / 32.0).round() as u32 * 32;
        n
    };
    (arrondi((total * ratio).sqrt()), arrondi((total / ratio).sqrt()))
}

/// Le rapport largeur / hauteur d'un format (« 16:9 (Widescreen) » → 1,78).
pub fn ratio_format(format: &str) -> Option<f64> {
    let (l, h) = format.split(' ').next()?.split_once(':')?;
    let (l, h): (f64, f64) = (l.parse().ok()?, h.parse().ok()?);
    (h > 0.0).then_some(l / h)
}

/// (VAE vidéo, VAE audio) du graphe H3.
///
/// # Errors
/// Le graphe n'a pas ses deux VAE.
pub fn vaes(api: &Value) -> Result<(String, String), String> {
    let noeuds: Vec<(&String, &Value)> = api
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, n)| n.get("class_type").and_then(Value::as_str) == Some("VAELoader"))
        .collect();
    let audio = noeuds
        .iter()
        .find(|(_, n)| {
            n.pointer("/inputs/vae_name")
                .and_then(Value::as_str)
                .is_some_and(|v| v.contains("audio"))
        })
        .map(|(id, _)| (*id).clone())
        .ok_or_else(|| "Graphe H3 sans VAE audio".to_owned())?;
    let video = noeuds
        .iter()
        .find(|(id, _)| **id != audio)
        .map(|(id, _)| (*id).clone())
        .ok_or_else(|| "Graphe H3 sans VAE vidéo".to_owned())?;
    Ok((video, audio))
}

/// Ne garde que les nœuds utiles aux sorties : les branches débranchées ne doivent pas être validées par ComfyUI.
pub fn elaguer(api: &Value) -> Value {
    let Some(noeuds) = api.as_object() else {
        return api.clone();
    };
    let mut utiles: Vec<String> = Vec::new();
    let mut a_voir: Vec<String> = noeuds
        .iter()
        .filter(|(_, n)| {
            matches!(
                n.get("class_type").and_then(Value::as_str),
                Some("SaveVideo" | "SaveImage" | "SaveAudio")
            )
        })
        .map(|(id, _)| id.clone())
        .collect();
    while let Some(id) = a_voir.pop() {
        if utiles.contains(&id) {
            continue;
        }
        if let Some(entrees) = noeuds
            .get(&id)
            .and_then(|n| n.get("inputs"))
            .and_then(Value::as_object)
        {
            for v in entrees.values() {
                if let Some([Value::String(source), _]) = v.as_array().map(Vec::as_slice)
                    && noeuds.contains_key(source)
                {
                    a_voir.push(source.clone());
                }
            }
        }
        utiles.push(id);
    }
    let garde: Map<String, Value> = noeuds
        .iter()
        .filter(|(id, _)| utiles.contains(id))
        .map(|(id, n)| (id.clone(), n.clone()))
        .collect();
    Value::Object(garde)
}

/// La langue d'une réplique d'après son écriture ; écriture latine : le français, langue de l'utilisateur.
pub fn langue(replique: &str) -> &'static str {
    let dans = |debut: u32, fin: u32| replique.chars().any(|c| (debut..=fin).contains(&u32::from(c)));
    if dans(0x3040, 0x30FF) {
        "Japanese"
    } else if dans(0xAC00, 0xD7AF) {
        "Korean"
    } else if dans(0x4E00, 0x9FFF) {
        "Mandarin Chinese"
    } else if dans(0x0400, 0x04FF) {
        "Russian"
    } else {
        "French"
    }
}

/// La balise d'une réplique (guide de H3) : `<d>[Langue] réplique</d>`, la langue lue dans la réplique.
pub fn balise(replique: &str) -> String {
    crate::listes::balise(replique, langue(replique))
}

// --- répliques ------------------------------------------------------------------------------------------------------

/// Une réplique : la balise `<d>[Langue] …</d>`, ou « », "", “” (ce que tu écris, les anciens prompts). Bornées : un
/// « <d> » ou un « « » sans fermeture ne relit qu'au plus 1 000 caractères.
static REPLIQUES: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r#"(?is)<d>\s{0,20}(?:\[[^\]]{0,40}\]\s{0,20})?(.{1,1000}?)\s{0,20}</d>|«\s{0,20}([^»]{1,1000}?)\s{0,20}»|"([^"]{1,1000})"|“([^”]{1,1000})”"#,
    )
    .ok()
});

fn motif(r: &'static LazyLock<Option<Regex>>) -> Option<&'static Regex> {
    let r: &Option<Regex> = r;
    r.as_ref()
}

/// Le texte contient-il une réplique (une balise ou un guillemet) ?
pub fn parle(texte: &str) -> bool {
    texte.to_lowercase().contains("<d>") || texte.contains(['«', '"', '“'])
}

/// Les répliques d'un texte, sans leurs guillemets ni leur balise.
pub fn repliques(texte: &str) -> Vec<String> {
    motif(&REPLIQUES).map_or_else(Vec::new, |r| {
        r.captures_iter(texte)
            .filter_map(|c| {
                (1..=4)
                    .find_map(|i| c.get(i))
                    .map(|m| m.as_str().trim().to_owned())
            })
            .collect()
    })
}

// --- champs d'un prompt H3 -------------------------------------------------------------------------------------------

pub const CHAMP_DESCRIPTION: &str = "integrated_multimodal_description";
static CHAMPS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)(integrated_multimodal_description|overall_soundscape|non_diegetic_music)\s*:").ok()
});
/// Le libellé du personnage de référence (guide du mode référence de H3).
pub const SUJET: &str = "<Subject 1>";
static SIX_CHAMPS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*subject_definitions\s*:").ok());
static LE_PERSONNAGE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\bthe (?:main )?character\b").ok());
static PLAN: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?i)\[Shot (\d+)\]").ok());
static SHOT_1: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?i)^\[Shot 1\]\s*").ok());

fn espaces(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// (description, ambiance, musique) d'un prompt H3 en trois champs. Un champ absent vaut rien ; le `[Shot 1]` de tête
/// de la description est retiré ([`assembler_champs`] le remet). Sans aucun champ, tout est la description.
pub fn separer_champs(texte: &str) -> (String, Option<String>, Option<String>) {
    let reperes: Vec<(usize, usize, String)> = motif(&CHAMPS).map_or_else(Vec::new, |r| {
        r.captures_iter(texte)
            .filter_map(|c| {
                let tout = c.get(0)?;
                Some((tout.start(), tout.end(), c.get(1)?.as_str().to_lowercase()))
            })
            .collect()
    });
    let mut morceaux: Vec<(String, String)> = Vec::new();
    let mut ajouter = |nom: &str, valeur: &str| match morceaux.iter_mut().find(|(n, _)| n == nom) {
        Some((_, v)) => {
            v.push(' ');
            v.push_str(valeur);
        }
        None => morceaux.push((nom.to_owned(), valeur.to_owned())),
    };
    match reperes.first() {
        None => ajouter(CHAMP_DESCRIPTION, texte),
        Some((debut, _, _)) if !texte[..*debut].trim().is_empty() => {
            ajouter(CHAMP_DESCRIPTION, &texte[..*debut])
        }
        Some(_) => {}
    }
    for (i, (_, fin, nom)) in reperes.iter().enumerate() {
        let suite = reperes.get(i + 1).map_or(texte.len(), |r| r.0);
        ajouter(nom, &texte[*fin..suite]);
    }
    let propre = |nom: &str| {
        morceaux
            .iter()
            .find(|(n, _)| n == nom)
            .map(|(_, v)| espaces(v))
            .filter(|v| !v.is_empty())
    };
    let description = propre(CHAMP_DESCRIPTION).unwrap_or_default();
    let description = motif(&SHOT_1).map_or_else(
        || description.clone(),
        |r| r.replace(&description, "").into_owned(),
    );
    (
        description,
        propre("overall_soundscape"),
        propre("non_diegetic_music"),
    )
}

/// Les trois champs d'un prompt H3, un par paragraphe.
pub fn assembler_champs(description: &str, ambiance: Option<&str>, musique: Option<&str>) -> String {
    let mut champs = vec![
        format!("{CHAMP_DESCRIPTION}: [Shot 1] {description}")
            .trim_end()
            .to_owned(),
    ];
    if let Some(a) = ambiance.filter(|a| !a.is_empty()) {
        champs.push(format!("overall_soundscape: {a}"));
    }
    if let Some(m) = musique.filter(|m| !m.is_empty()) {
        champs.push(format!("non_diegetic_music: {m}"));
    }
    champs.join("\n\n")
}

/// Ajoute une phrase (contrainte, mots déclencheurs) à la description d'un prompt en champs, sans toucher aux autres
/// champs ; rien si le prompt n'est pas en champs.
pub fn dans_la_description(texte: &str, ajout: &str, debut: bool) -> Option<String> {
    let c = motif(&CHAMPS)?.captures(texte)?;
    let repere = c.get(0)?;
    if c.get(1)?.as_str().to_lowercase() != CHAMP_DESCRIPTION {
        return None;
    }
    let (description, ambiance, musique) = separer_champs(&texte[repere.start()..]);
    let description = if debut {
        format!("{ajout} {description}")
    } else {
        format!("{description} {ajout}")
    };
    Some(format!(
        "{}{}",
        &texte[..repere.start()],
        assembler_champs(description.trim(), ambiance.as_deref(), musique.as_deref())
    ))
}

/// Le prompt d'un plan (trois champs ou prose) mis aux six champs du mode référence de H3 : le personnage de
/// <Picture 1> devient <Subject 1>, la voix de <Audio 1> est déclarée, et chaque « the character » de la description
/// est remplacé par le libellé. Sans référence, ou déjà en six champs, le texte ne change pas.
pub fn en_reference(texte: &str, personnage: bool, voix: bool) -> String {
    if !(personnage || voix) || motif(&SIX_CHAMPS).is_some_and(|r| r.is_match(texte)) {
        return texte.trim_end().to_owned();
    }
    let (mut description, ambiance, musique) = separer_champs(texte.trim());
    let qui = if personnage { SUJET } else { "the main speaker" };
    let (mut definitions, mut conservation, mut taches) = (Vec::new(), Vec::new(), Vec::new());
    if personnage {
        match motif(&LE_PERSONNAGE) {
            Some(r) if r.is_match(&description) => {
                description = r.replace_all(&description, SUJET).into_owned()
            }
            _ => description = format!("{SUJET} is the main character of this shot. {description}"),
        }
        let mut plans: Vec<u32> = vec![1];
        if let Some(r) = motif(&PLAN) {
            plans.extend(
                r.captures_iter(&description)
                    .filter_map(|c| c.get(1)?.as_str().parse().ok()),
            );
        }
        plans.sort_unstable();
        plans.dedup();
        let liste: Vec<String> = plans.iter().map(|n| format!("[Shot {n}]")).collect();
        definitions.push(format!("{SUJET} is the main character shown in <Picture 1>."));
        conservation.push(format!(
            "{SUJET} (appears in {}): fully_preserved - the character's identity, face, hair, body and outfit are retained.",
            liste.join(", ")
        ));
        taches.push("reference generation");
    }
    if voix {
        definitions.push(format!("<Audio 1> is the voice-timbre reference for {qui} (S1)."));
        conservation.push(format!(
            "<Audio 1>: reference - its vocal timbre guides the dialogue delivery of {qui} without copying the original signal."
        ));
        taches.push("audio reference");
    }
    let resume = if voix {
        format!(
            "The target video shows {qui} in the scene described below, speaking with the voice timbre of <Audio 1>."
        )
    } else {
        format!("The target video shows {qui} in the scene described below.")
    };
    let mut champs = vec![
        ("subject_definitions", definitions.join("\n")),
        ("summary", format!("[{}] {resume}", taches.join(" + "))),
        ("retention_analysis", conservation.join("\n")),
        (
            "detailed_description",
            format!("[Shot 1] {description}").trim_end().to_owned(),
        ),
    ];
    if let Some(a) = ambiance {
        champs.push(("overall_soundscape", a));
    }
    champs.push(("non_diegetic_music", musique.unwrap_or_else(|| "N/A".to_owned())));
    champs
        .iter()
        .map(|(nom, valeur)| format!("{nom}:\n{valeur}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests_champs {
    use super::*;

    #[test]
    fn repliques_lues() {
        assert!(parle("She says: <d>[French] Salut.</d>") && parle("« Salut »") && !parle("A quiet street."));
        assert_eq!(
            repliques("Il dit « Bonjour » puis <d>[French] Au revoir.</d> et \"OK\""),
            ["Bonjour", "Au revoir.", "OK"]
        );
    }

    #[test]
    fn champs_separes_et_reassembles() {
        let t = "integrated_multimodal_description: [Shot 1] The character walks.\n\noverall_soundscape: rain\n\nnon_diegetic_music: piano";
        let (d, a, m) = separer_champs(t);
        assert_eq!(
            (d.as_str(), a.as_deref(), m.as_deref()),
            ("The character walks.", Some("rain"), Some("piano"))
        );
        assert_eq!(assembler_champs(&d, a.as_deref(), m.as_deref()), t);
        assert_eq!(separer_champs("A cat sleeps.").0, "A cat sleeps.");
        let ajoute = dans_la_description(t, "Keep the red coat.", false).unwrap();
        assert!(ajoute.contains("The character walks. Keep the red coat."));
        assert!(dans_la_description("A cat sleeps.", "x", false).is_none());
    }

    #[test]
    fn mode_reference() {
        assert_eq!(en_reference("A cat sleeps. ", false, false), "A cat sleeps.");
        let r = en_reference("The character walks in the rain.", true, true);
        assert!(
            r.starts_with("subject_definitions:\n<Subject 1> is the main character shown in <Picture 1>.")
        );
        assert!(r.contains("[Shot 1] <Subject 1> walks in the rain."));
        assert!(r.contains("[reference generation + audio reference]"));
        assert!(r.ends_with("non_diegetic_music:\nN/A"));
        let sans = en_reference("A dog barks.", true, false);
        assert!(sans.contains("[Shot 1] <Subject 1> is the main character of this shot. A dog barks."));
        assert_eq!(en_reference(&r, true, true), r.trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn calculs_de_h3() {
        // Grille 17k+5, comme la v2 : 5 s à 24 i/s = 120 → 124.
        assert_eq!(images(5), 124);
        assert_eq!(images(8), 192);
        assert_eq!((images(5) - 5) % 17, 0);
        assert_eq!(dimensions(1.0, 0.4), (640, 640));
        let (l, h) = dimensions(16.0 / 9.0, 0.6);
        assert_eq!((l % 32, h % 32), (0, 0));
        assert!(l > h);
        assert_eq!(
            ratio_format("16:9 (Widescreen)").map(|r| (r * 100.0).round()),
            Some(178.0)
        );
        assert!(ratio_format("carré").is_none());
    }

    #[test]
    fn graphe_elague_et_vaes() {
        let api = json!({
            "1": {"class_type": "VAELoader", "inputs": {"vae_name": "h3_video_vae.safetensors"}},
            "2": {"class_type": "VAELoader", "inputs": {"vae_name": "h3_audio_vae.safetensors"}},
            "3": {"class_type": "CreateVideo", "inputs": {"images": ["1", 0]}},
            "4": {"class_type": "SaveVideo", "inputs": {"video": ["3", 0]}},
            "5": {"class_type": "LoadImage", "inputs": {"image": "x.png"}}
        });
        assert_eq!(vaes(&api).unwrap(), ("1".to_owned(), "2".to_owned()));
        let garde = elaguer(&api);
        let mut ids: Vec<&String> = garde.as_object().unwrap().keys().collect();
        ids.sort();
        assert_eq!(ids, ["1", "3", "4"]);
    }

    #[test]
    fn langues_et_balises() {
        assert_eq!(langue("Je descends ici."), "French");
        assert_eq!(langue("こんにちは"), "Japanese");
        assert_eq!(langue("안녕하세요"), "Korean");
        assert_eq!(langue("你好"), "Mandarin Chinese");
        assert_eq!(langue("Привет"), "Russian");
        assert_eq!(balise("Salut <toi>"), "<d>[French] Salut toi</d>");
    }
}
