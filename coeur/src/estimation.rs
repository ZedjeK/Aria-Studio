//! Temps estimé d'un tournage du Montage, comme en v2 (`app/estimation.py`) : calculé d'après la VRAM libre, les steps
//! et le poids du checkpoint, la résolution, la durée et le nombre de clips, sans tenir compte des clips déjà tournés.
//!
//! Point de départ : le tableau de [`montage::QUALITES`], mesuré sur la RTX 4080 de l'auteur de la v2 (16 Go) pour 5 s,
//! à chaud, avec le checkpoint d'origine. Il reste exact sur cette machine de référence ; le calcul suit cet ordre :
//! 1. la VRAM libre ;
//! 2. les steps du checkpoint (validés dans Modèles, ou clairs dans sa fiche) : le tableau est mesuré aux steps de sa
//!    résolution, le temps suit au prorata ;
//! 3. le poids du modèle vidéo : quand lui et le travail du clip dépassent la VRAM libre, ComfyUI décharge une partie
//!    en RAM et chaque step ralentit. Le facteur compare cette machine à la machine de référence ; la résolution
//!    (mégapixels) entre dans ce travail, et dans le temps du tableau ;
//! 4. la durée : (D / 5)^[`montage::EXPOSANT_DUREE`] ;
//! 5. le nombre de clips : autant de clips, un après l'autre.
//!
//! Hypothèses non mesurées (comme en v2) : la forme du ralentissement ([`VRAM_COEF`]), la VRAM de travail d'un clip
//! ([`TRAVAIL_GO_PAR_MP`]) et la vitesse brute de la carte (une 4090 sera plus rapide qu'une 4080 : ce n'est pas
//! modélisé).

use std::fs;

use serde::Serialize;
use serde_json::Value;

use crate::montage::{self, CLIPS_MAX, DUREES, EXPOSANT_DUREE};
use crate::plateau::Contexte;
use crate::{reglages, workflows};

const GO: f64 = 1_073_741_824.0;
/// La VRAM libre de la machine de référence : RTX 4080 de 16 Go, bureau compris.
pub const LIBRE_REF_GO: f64 = 15.0;
/// Le checkpoint d'origine de MiniMax H3 (`minimax_h3_fl2va_pruned_int8_convrot.safetensors`).
pub const POIDS_REF_GO: f64 = 19.5;
/// La VRAM de travail (activations) d'un clip de 5 s, par mégapixel : une hypothèse.
pub const TRAVAIL_GO_PAR_MP: f64 = 6.0;
/// Le ralentissement quand toute la charge est déchargée en RAM : une hypothèse.
pub const VRAM_COEF: f64 = 1.0;

/// L'estimation d'un tournage, et ce qui a servi au calcul.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimation {
    /// `calcul` (VRAM lue) ou `tableau_d_origine` (VRAM non lue : le tableau seul).
    pub source: String,
    /// Secondes d'un clip, et de la commande entière.
    pub clip_s: u64,
    pub total_s: u64,
    pub nombre: u32,
    pub checkpoint: String,
    pub poids_go: f64,
    pub steps: u64,
    /// Ses steps ne sont pas connus : le tableau tel quel, et le tournage sera refusé.
    pub a_valider: bool,
    pub vram_libre_go: Option<f64>,
    pub facteur_vram: f64,
}

/// La part de la charge qui ne tient pas en VRAM (0 : tout y tient).
fn decharge(besoin_go: f64, libre_go: f64) -> f64 {
    if besoin_go <= 0.0 {
        return 0.0;
    }
    (besoin_go - libre_go).max(0.0) / besoin_go
}

/// Le ralentissement de cette machine, divisé par celui de la machine de référence pour le même clip : 1 sur la
/// machine de référence, moins de 1 avec plus de VRAM libre ou un checkpoint plus léger, plus de 1 avec moins.
pub fn facteur_vram(poids_go: f64, libre_go: f64, megapixels: f64, duree: f64) -> f64 {
    let travail = TRAVAIL_GO_PAR_MP * megapixels * (duree / 5.0).sqrt();
    let ici = VRAM_COEF.mul_add(decharge(poids_go + travail, libre_go), 1.0);
    let reference = VRAM_COEF.mul_add(decharge(POIDS_REF_GO + travail, LIBRE_REF_GO), 1.0);
    ici / reference
}

/// La taille du checkpoint sur le disque, sinon l'estimation de sa fiche (`materiel.vram_estimee_go`).
#[allow(clippy::cast_precision_loss)] // des dizaines de Go : la précision d'un f64 suffit largement
fn poids_go(contexte: &Contexte, wf: &workflows::Workflow, checkpoint: &str) -> f64 {
    let fichier = contexte.modeles.join(wf.dossier_modeles()).join(checkpoint);
    match fs::metadata(&fichier) {
        Ok(m) if m.is_file() && !checkpoint.is_empty() => m.len() as f64 / GO,
        _ => wf
            .fiche
            .pointer("/materiel/vram_estimee_go")
            .and_then(Value::as_f64)
            .unwrap_or(POIDS_REF_GO),
    }
}

fn arrondi(valeur: f64, decimales: i32) -> f64 {
    let p = 10f64.powi(decimales);
    (valeur * p).round() / p
}

fn secondes(valeur: f64) -> u64 {
    // Des secondes positives : quelques heures au plus.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = valeur.max(0.0).round() as u64;
    s
}

/// Les secondes d'un clip et de la commande entière, avec ce qui a servi au calcul. `libre_go` : la VRAM libre
/// (lue par l'appli) ; sans elle, le tableau d'origine seul.
///
/// # Errors
/// Mode, résolution, durée ou nombre de clips inconnus, workflow absent, ou checkpoint absent du dossier de la famille.
pub fn estimer(
    contexte: &Contexte,
    mode: &str,
    modele: Option<&str>,
    qualite: &str,
    duree: u32,
    nombre: u32,
    libre_go: Option<f64>,
) -> Result<Estimation, String> {
    let tache = match mode {
        "animer" => "image_vers_video",
        "texte" => "texte_vers_video",
        _ => return Err(format!("Mode inconnu : {mode}")),
    };
    if !DUREES.contains(&duree) || !(1..=CLIPS_MAX).contains(&nombre) {
        return Err("Mode, résolution, durée ou nombre de clips inconnus".to_owned());
    }
    let q = montage::qualites(contexte)
        .into_iter()
        .find(|q| q.cle == qualite)
        .ok_or_else(|| format!("Résolution inconnue : {qualite}"))?;
    let wf = workflows::obtenir(&montage::workflow_de(contexte, tache))?;
    let reglage = reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&wf.id).and_then(Value::as_object).cloned());
    let modele = modele.filter(|m| !m.is_empty());
    // Le nom sert de chemin pour lire le poids : un fichier déposé dans le dossier de la famille, rien d'autre.
    if let Some(m) = modele
        && !wf.presents(&contexte.modeles).iter().any(|p| p == m)
    {
        return Err(format!(
            "Checkpoint absent de modeles/{} : {m}",
            wf.dossier_modeles()
        ));
    }
    let checkpoint = modele
        .map(str::to_owned)
        .or_else(|| {
            wf.regle(&contexte.modeles, reglage.as_ref())
                .get("modele")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| wf.modele_origine())
        .unwrap_or_default();
    let a_valider = wf.a_valider(reglage.as_ref(), &checkpoint);
    // À valider : le tableau tel quel (le tournage sera refusé).
    let steps = match wf.steps_de(reglage.as_ref(), &checkpoint) {
        0 => u64::from(q.steps),
        n => n,
    };
    let poids = poids_go(contexte, &wf, &checkpoint);
    #[allow(clippy::cast_precision_loss)] // quelques dizaines de steps
    let prorata = steps as f64 / f64::from(q.steps);
    let mut clip = q.estimation_s * prorata * (f64::from(duree) / 5.0).powf(EXPOSANT_DUREE);
    let mut facteur = 1.0;
    if let Some(libre) = libre_go {
        facteur = facteur_vram(poids, libre, q.megapixels, f64::from(duree));
        clip *= facteur;
    }
    Ok(Estimation {
        source: if libre_go.is_some() {
            "calcul"
        } else {
            "tableau_d_origine"
        }
        .to_owned(),
        clip_s: secondes(clip),
        total_s: secondes(clip * f64::from(nombre)),
        nombre,
        checkpoint,
        poids_go: arrondi(poids, 1),
        steps,
        a_valider,
        vram_libre_go: libre_go.map(|l| arrondi(l, 1)),
        facteur_vram: arrondi(facteur, 2),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facteur_de_la_machine_de_reference() {
        // La machine de référence : 1 ; plus de VRAM libre ou un checkpoint plus léger : moins ; moins de VRAM : plus.
        assert!((facteur_vram(POIDS_REF_GO, LIBRE_REF_GO, 0.6, 5.0) - 1.0).abs() < 1e-9);
        assert!(facteur_vram(POIDS_REF_GO, 30.0, 0.6, 5.0) < 1.0);
        assert!(facteur_vram(10.0, LIBRE_REF_GO, 0.6, 5.0) < 1.0);
        assert!(facteur_vram(POIDS_REF_GO, 8.0, 0.6, 5.0) > 1.0);
        // Un checkpoint de 61,73 Go sur la machine de référence, 15 s en Compromis : environ 1,26 fois plus lent.
        let gros = facteur_vram(61.73, LIBRE_REF_GO, 0.6, 15.0);
        assert!((gros - 1.255).abs() < 0.01, "{gros}");
    }

    #[test]
    fn decharge_bornee() {
        assert!(decharge(10.0, 15.0).abs() < 1e-12);
        assert!((decharge(20.0, 15.0) - 0.25).abs() < 1e-12);
        assert!(decharge(0.0, 15.0).abs() < 1e-12);
    }

    fn contexte(temp: &tempfile::TempDir) -> Contexte {
        let modeles_dir = temp.path().join("modeles");
        for id in ["minimax_h3_i2v", "minimax_h3_t2v"] {
            let wf = workflows::obtenir(id).unwrap();
            for f in wf.manquants(&modeles_dir) {
                let chemin = crate::modeles::destination(
                    &modeles_dir,
                    f["dossier"].as_str().unwrap(),
                    f["nom"].as_str().unwrap(),
                )
                .unwrap();
                fs::create_dir_all(chemin.parent().unwrap()).unwrap();
                fs::write(chemin, b"poids").unwrap();
            }
        }
        let dossier = crate::Dossier::a(temp.path());
        let i2v = workflows::obtenir("minimax_h3_i2v").unwrap();
        workflows::valider_checkpoint(
            &dossier,
            &modeles_dir,
            &i2v.id,
            &i2v.modele_origine().unwrap(),
            12,
        )
        .unwrap();
        Contexte {
            dossier,
            data: temp.path().join("data"),
            modeles: modeles_dir,
            moteurs: temp.path().join("moteurs"),
            port_llama: 8095,
        }
    }

    #[test]
    fn estimation_d_un_tournage() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        // Sans VRAM lue : le tableau d'origine, aux steps du checkpoint (12, ceux de la mesure en Compromis).
        let e = estimer(&ctx, "animer", None, "compromis", 5, 3, None).unwrap();
        assert_eq!(
            (e.source.as_str(), e.clip_s, e.total_s, e.steps),
            ("tableau_d_origine", 170, 510, 12)
        );
        assert!(!e.a_valider && e.vram_libre_go.is_none());
        assert!((e.facteur_vram - 1.0).abs() < 1e-9);
        // Avec la VRAM lue : un checkpoint léger (le fichier de test pèse quelques octets) tourne plus vite.
        let c = estimer(&ctx, "animer", None, "compromis", 5, 1, Some(15.0)).unwrap();
        assert_eq!(c.source, "calcul");
        assert!(c.clip_s < 170 && c.facteur_vram < 1.0, "{c:?}");
        // La durée coûte plus que proportionnellement.
        let long = estimer(&ctx, "animer", None, "compromis", 15, 1, None).unwrap();
        assert!(long.clip_s > 3 * 170);
        // Refus de la v2.
        assert!(estimer(&ctx, "danser", None, "compromis", 5, 1, None).is_err());
        assert!(estimer(&ctx, "animer", None, "inconnue", 5, 1, None).is_err());
        assert!(estimer(&ctx, "animer", None, "compromis", 7, 1, None).is_err());
        assert!(estimer(&ctx, "animer", None, "compromis", 5, 0, None).is_err());
        assert!(
            estimer(
                &ctx,
                "animer",
                Some("../autre.safetensors"),
                "compromis",
                5,
                1,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn arrondis() {
        assert!((arrondi(19.53, 1) - 19.5).abs() < 1e-9);
        assert_eq!(secondes(170.4), 170);
        assert_eq!(secondes(-3.0), 0);
    }
}
