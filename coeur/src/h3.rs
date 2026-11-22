//! Ce que les séquences et les essais de voix partagent pour MiniMax H3, comme en v2 (`app/sequences.py`,
//! `app/repliques.py`, `app/decoupage.py`) : le nombre d'images d'un clip, ses dimensions, les deux VAE du graphe,
//! l'élagage des branches débranchées, la langue d'une réplique et sa balise.

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
