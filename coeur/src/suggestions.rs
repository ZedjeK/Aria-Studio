//! Ce que la page source d'un checkpoint conseille (steps, CFG), comme en v2 (`app/sources/steps.py`).
//!
//! Aucune source n'a de champ « steps » (Civitai : jamais dans les métadonnées des images d'exemple ; Hugging Face :
//! texte libre). On cherche donc un nombre à côté du mot « step » (ou « CFG ») dans le nom du fichier, de la version et
//! la description. C'est une SUGGESTION : elle n'est jamais appliquée sans validation, et elle se déclare peu fiable
//! dès qu'elle n'est pas univoque. Forme gardée au registre : `{statut: unique|ambigu|aucun, valeurs, extraits}`.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

/// Les bornes du réglage « Steps imposés ».
const STEPS: (u64, u64) = (1, 150);
/// Les bornes du réglage « CFG ».
const CFG: (f64, f64) = (0.0, 30.0);
/// Les caractères gardés de part et d'autre d'un nombre trouvé.
const RAYON_EXTRAIT: usize = 45;
const EXTRAITS_MAX: usize = 6;

// Le module `regex` n'a ni regard arrière ni regard avant : les bords des nombres sont vérifiés à la main.
/// « 8-step », « 8 steps », « 8stepTURBO ».
static N_STEP: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?i)(\d{1,3})\s*[-_ ]?steps?").ok());
/// « steps: 8 » (« Step 1 » d'un tutoriel ne passe pas).
static STEPS_N: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\bsteps?\s*[:=]\s*(\d{1,3})").ok());
/// « CFG 1.5 », « cfg: 1 », « CFG scale 4 ».
static CFG_N: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)\bcfg(?:\s*scale)?\s*(?:[:=]|of|at|to|around)?\s*(\d{1,2}(?:[.,]\d{1,2})?)").ok()
});
/// « 1.5 CFG ».
static N_CFG: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)(\d{1,2}(?:[.,]\d{1,2})?)\s*cfg\b").ok());
/// Une balise HTML, sans « < » dedans.
static BALISES: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"<[^<>]+>").ok());

/// Un motif compilé (toujours, ils sont fixes).
fn motif(r: &'static LazyLock<Option<Regex>>) -> Option<&'static Regex> {
    let r: &Option<Regex> = r;
    r.as_ref()
}

/// Le texte sans balises ni espaces en trop.
fn nettoyer(texte: &str) -> String {
    let sans = motif(&BALISES).map_or_else(|| texte.to_owned(), |r| r.replace_all(texte, " ").into_owned());
    sans.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Le passage autour de `debut..fin`, à `RAYON_EXTRAIT` caractères près.
fn extrait(texte: &str, debut: usize, fin: usize) -> String {
    let avant = texte[..debut]
        .char_indices()
        .rev()
        .nth(RAYON_EXTRAIT - 1)
        .map_or(0, |(i, _)| i);
    let apres = texte[fin..]
        .char_indices()
        .nth(RAYON_EXTRAIT)
        .map_or(texte.len(), |(i, _)| fin + i);
    format!(
        "{}{}{}",
        if avant > 0 { "…" } else { "" },
        &texte[avant..apres],
        if apres < texte.len() { "…" } else { "" }
    )
}

fn chiffre_ou_point(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit() || c == '.')
}

/// Après un CFG : des chiffres, points ou virgules suivis d'un chiffre le prolongent (« 1.5.2 », « 12,34 »).
fn prolonge(reste: &str) -> bool {
    reste
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .any(|c| c.is_ascii_digit())
}

/// Les nombres trouvés par ces motifs, lus (et gardés ou non) par `lire`, avec leurs extraits.
fn chercher<T: PartialEq + Copy>(
    textes: &[(&str, &str)],
    motifs: &[&'static LazyLock<Option<Regex>>],
    lire: impl Fn(&str, usize, usize, &str) -> Option<T>,
) -> (Vec<T>, Vec<Value>) {
    let mut valeurs = Vec::new();
    let mut extraits: Vec<Value> = Vec::new();
    for (origine, brut) in textes {
        let texte = nettoyer(brut);
        for r in motifs {
            let Some(r) = motif(*r) else { continue };
            for prise in r.captures_iter(&texte) {
                let (Some(tout), Some(nombre)) = (prise.get(0), prise.get(1)) else {
                    continue;
                };
                let Some(n) = lire(&texte, tout.start(), tout.end(), nombre.as_str()) else {
                    continue;
                };
                if !valeurs.contains(&n) {
                    valeurs.push(n);
                }
                let e = json!({"origine": origine, "texte": extrait(&texte, tout.start(), tout.end())});
                if !extraits.contains(&e) {
                    extraits.push(e);
                }
            }
        }
    }
    extraits.truncate(EXTRAITS_MAX);
    (valeurs, extraits)
}

fn statut(n: usize) -> &'static str {
    match n {
        0 => "aucun",
        1 => "unique",
        _ => "ambigu",
    }
}

/// Les steps conseillés : `textes` est une liste de (origine, texte).
pub fn steps(textes: &[(&str, &str)]) -> Value {
    let (mut valeurs, extraits) = chercher(textes, &[&N_STEP, &STEPS_N], |texte, debut, fin, nombre| {
        // « 1234 steps » ou « v1.8 steps » : le nombre ne commence pas là ; « steps: 1234 » : il ne finit pas là.
        let colle_avant = chiffre_ou_point(texte[..debut].chars().next_back());
        let colle_apres = texte[fin..].chars().next().is_some_and(|c| c.is_ascii_digit());
        let n: u64 = nombre.parse().ok()?;
        let premier = texte[debut..].starts_with(|c: char| c.is_ascii_digit());
        let bord = if premier { colle_avant } else { colle_apres };
        (!bord && (STEPS.0..=STEPS.1).contains(&n)).then_some(n)
    });
    valeurs.sort_unstable();
    json!({"statut": statut(valeurs.len()), "valeurs": valeurs, "extraits": extraits})
}

/// Le CFG conseillé, mêmes réserves.
pub fn cfg(textes: &[(&str, &str)]) -> Value {
    let (mut valeurs, extraits) = chercher(textes, &[&CFG_N, &N_CFG], |texte, debut, fin, nombre| {
        let premier = texte[debut..].starts_with(|c: char| c.is_ascii_digit());
        let bord = if premier {
            let avant = texte[..debut].chars().next_back();
            avant.is_some_and(|c| c.is_ascii_digit() || c == '.' || c == ',')
        } else {
            prolonge(&texte[fin..])
        };
        let n: f64 = nombre.replace(',', ".").parse().ok()?;
        (!bord && (CFG.0..=CFG.1).contains(&n)).then_some(n)
    });
    valeurs.sort_by(f64::total_cmp);
    json!({"statut": statut(valeurs.len()), "valeurs": valeurs, "extraits": extraits})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valeurs(v: &Value) -> Vec<f64> {
        v["valeurs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_f64().unwrap())
            .collect()
    }

    #[test]
    fn steps_d_une_page() {
        let v = steps(&[("nom du fichier", "krea-8stepTURBO.safetensors")]);
        assert_eq!((v["statut"].as_str(), valeurs(&v)), (Some("unique"), vec![8.0]));
        let v = steps(&[
            ("description", "<p>Use <b>4 steps</b> or steps: 8 for quality</p>"),
            ("nom", "v1.8 steps"),
        ]);
        assert_eq!(
            (v["statut"].as_str(), valeurs(&v)),
            (Some("ambigu"), vec![4.0, 8.0])
        );
        assert!(v["extraits"][0]["texte"].as_str().unwrap().contains("4 steps"));
        assert_eq!(
            steps(&[("x", "Step 1: install ComfyUI, 1234 steps, steps: 1234")])["statut"],
            "aucun"
        );
        assert_eq!(steps(&[("x", "rien")])["valeurs"], json!([]));
    }

    #[test]
    fn cfg_d_une_page() {
        let v = cfg(&[("description", "12 steps, CFG 1.5 ; or 1,5 cfg")]);
        assert_eq!((v["statut"].as_str(), valeurs(&v)), (Some("unique"), vec![1.5]));
        let v = cfg(&[("description", "cfg scale: 4, CFG 1.2.3, 99 cfg")]);
        assert_eq!(valeurs(&v), vec![4.0]);
    }

    #[test]
    fn extrait_borne() {
        let long = format!("{}8 steps{}", "é".repeat(100), "à".repeat(100));
        let e = steps(&[("x", long.as_str())])["extraits"][0]["texte"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(e.starts_with('…') && e.ends_with('…'));
        assert_eq!(e.chars().count(), 2 + 45 + "8 steps".len() + 45);
    }
}
