//! Le Montage, premier lot : tourner un clip, comme en v2 (`app/plateau.py` : `commander_clip`, `_tourner`). Un clip
//! MiniMax H3 (avec le son), en animant un rush ou à partir d'un texte ; le prompt est tourné tel quel (il s'écrit au
//! Scénario, ou à la main). Chaque clip est une création : sa vidéo, sa première image (vignette) et sa fiche.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::images;
use crate::plateau::{self, ChoixLora, Commande, Contexte, Generateur, Options, Suivi};
use crate::workflows::{self, FORMATS, Workflow};
use crate::{comfy, creations, reglages, retouche, securite};

/// Une résolution de MiniMax H3. Les steps sont ceux du checkpoint ; `steps` est celui de la mesure : `estimation_s`
/// a été mesuré à ces steps sur la RTX 4080 de l'auteur de la v2, pour 5 s, à chaud.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Qualite {
    pub cle: String,
    pub nom: String,
    pub aide: String,
    pub megapixels: f64,
    pub steps: u32,
    pub estimation_s: f64,
}

/// (clé, nom, aide, mégapixels, steps de la mesure, secondes mesurées).
const QUALITES: [(&str, &str, &str, f64, u32, f64); 3] = [
    ("rapide", "0,4 MP", "rapide", 0.4, 8, 80.0),
    ("compromis", "0,6 MP", "compromis", 0.6, 12, 170.0),
    (
        "qualite",
        "1 MP",
        "≈ natif de H3 (0,98 MP), le plus fin et le plus lent",
        0.98,
        12,
        425.0,
    ),
];
/// La qualité proposée d'abord.
pub const QUALITE_DEFAUT: &str = "compromis";
/// Les durées proposées : H3 accepte de 4 à 15 s.
pub const DUREES: [u32; 4] = [5, 8, 10, 15];
/// Le temps d'un clip croît avec sa durée en (D / 5)^EXPOSANT (relevé de la v2).
pub const EXPOSANT_DUREE: f64 = 1.57;
/// Les clips d'une même commande, au plus.
pub const CLIPS_MAX: u32 = 20;
/// Les modes : animer un rush, ou partir d'un texte ; et la tâche de leur workflow.
const MODES: [(&str, &str); 2] = [("animer", "image_vers_video"), ("texte", "texte_vers_video")];
/// Les workflows d'origine de la v2, quand aucun favori n'est choisi.
const DEFAUTS: [(&str, &str); 2] = [
    ("image_vers_video", "minimax_h3_i2v"),
    ("texte_vers_video", "minimax_h3_t2v"),
];
const PREFIXE: &str = "aria_montage";

/// Le temps d'un clip de 5 s à 12 steps pour ces mégapixels : interpolé sur les trois mesures (échelle log-log, la
/// pente du segment voisin au-delà des extrémités).
fn temps_a_12_steps(mp: f64) -> f64 {
    let mut points: Vec<(f64, f64)> = QUALITES
        .iter()
        .map(|(_, _, _, m, steps, s)| (*m, s * 12.0 / f64::from(*steps)))
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let i = usize::from(mp > points[1].0);
    let ((x0, y0), (x1, y1)) = (points[i], points[i + 1]);
    let pente = (y1 / y0).ln() / (x1 / x0).ln();
    y0 * (mp / x0).powf(pente)
}

/// Le workflow d'une tâche vidéo : le favori de l'utilisateur, sinon celui de la v2.
pub fn workflow_de(contexte: &Contexte, tache: &str) -> String {
    reglages::section(&contexte.dossier, "favoris")
        .and_then(|f| f.get("generation")?.get(tache)?.as_str().map(str::to_owned))
        .filter(|id| workflows::obtenir(id).is_ok())
        .or_else(|| {
            DEFAUTS
                .iter()
                .find(|(t, _)| *t == tache)
                .map(|(_, w)| (*w).to_owned())
        })
        .unwrap_or_default()
}

/// Les résolutions, avec les mégapixels choisis dans les réglages du workflow animer (`resolution_<clé>`, comme en v2).
pub fn qualites(contexte: &Contexte) -> Vec<Qualite> {
    let id = workflow_de(contexte, "image_vers_video");
    let reglage = reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&id).and_then(Value::as_object).cloned())
        .unwrap_or_default();
    QUALITES
        .iter()
        .map(|(cle, nom, aide, mp, steps, estimation)| {
            let choisi = reglage
                .get(&format!("resolution_{cle}"))
                .and_then(Value::as_f64)
                .filter(|m| (0.1..=4.0).contains(m));
            match choisi {
                Some(m) if (m - mp).abs() > f64::EPSILON => Qualite {
                    cle: (*cle).to_owned(),
                    nom: format!("{} MP", (m * 100.0).round() / 100.0).replace('.', ","),
                    aide: "réglé dans Réglages".to_owned(),
                    megapixels: m,
                    steps: *steps,
                    estimation_s: (temps_a_12_steps(m) * f64::from(*steps) / 12.0).round(),
                },
                _ => Qualite {
                    cle: (*cle).to_owned(),
                    nom: (*nom).to_owned(),
                    aide: (*aide).to_owned(),
                    megapixels: *mp,
                    steps: *steps,
                    estimation_s: *estimation,
                },
            }
        })
        .collect()
}

/// Ce que propose le Montage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsMontage {
    pub qualites: Vec<Qualite>,
    pub defaut_qualite: String,
    pub durees: Vec<u32>,
    pub nombre_max: u32,
    pub exposant_duree: f64,
    pub formats: Vec<String>,
    /// Les workflows prêts pour animer un rush, et pour partir d'un texte.
    pub animer: Options,
    pub texte: Options,
}

/// Ce que propose le Montage.
pub fn options(contexte: &Contexte) -> OptionsMontage {
    let pour = |tache: &str| plateau::options_pour(contexte, tache, workflow_de(contexte, tache));
    OptionsMontage {
        qualites: qualites(contexte),
        defaut_qualite: QUALITE_DEFAUT.to_owned(),
        durees: DUREES.to_vec(),
        nombre_max: CLIPS_MAX,
        exposant_duree: EXPOSANT_DUREE,
        formats: FORMATS.iter().map(|f| (*f).to_owned()).collect(),
        animer: pour("image_vers_video"),
        texte: pour("texte_vers_video"),
    }
}

/// Une demande de clip, comme celle de la v2.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeClip {
    #[serde(default)]
    pub idee: String,
    /// Tourné tel quel.
    pub prompt: String,
    /// `animer` (un rush) ou `texte`.
    pub mode: String,
    /// Le rush à animer.
    #[serde(default)]
    pub source: Option<String>,
    pub qualite: String,
    pub duree: u32,
    /// Texte → vidéo : un format ; animer : un format (recadrage au centre) ou « image source ».
    pub format: String,
    #[serde(default)]
    pub seed: Option<u64>,
    pub nombre: u32,
    #[serde(default)]
    pub modele: Option<String>,
    #[serde(default)]
    pub loras: Vec<ChoixLora>,
    /// Le LLM qui a écrit le prompt au Scénario (noté sur la fiche).
    #[serde(default)]
    pub ecrit_par: Option<String>,
}

/// Contrôle une demande de clip comme la v2, et rend la commande à mettre en file.
///
/// # Errors
/// Le message de la v2 : prompt vide, mode, résolution, durée ou format inconnus, nombre hors bornes, rush manquant,
/// workflow incomplet, modèle absent, LoRA invalide, ou refus du filtre AS-18.
pub fn commande_clip(contexte: &Contexte, d: &DemandeClip, id: String) -> Result<Commande, String> {
    let prompt = d.prompt.trim();
    if prompt.is_empty() {
        return Err("Le prompt est vide : écris-le ici, ou au Scénario".to_owned());
    }
    let tache = MODES
        .iter()
        .find(|(m, _)| *m == d.mode)
        .map(|(_, t)| *t)
        .ok_or_else(|| format!("Mode inconnu : {}", d.mode))?;
    if !QUALITES.iter().any(|(c, ..)| *c == d.qualite) {
        return Err(format!("Résolution inconnue : {}", d.qualite));
    }
    if !DUREES.contains(&d.duree) {
        let durees: Vec<String> = DUREES.iter().map(u32::to_string).collect();
        return Err(format!("Durée proposée : {} s", durees.join(", ")));
    }
    if !(1..=CLIPS_MAX).contains(&d.nombre) {
        return Err(format!("De 1 à {CLIPS_MAX} clips à la fois"));
    }
    let animer = d.mode == "animer";
    let source = d.source.clone().filter(|s| !s.is_empty());
    if animer && source.is_none() {
        return Err("Choisis le rush à animer".to_owned());
    }
    let image_source = animer && d.format == retouche::FORMAT_IMAGE;
    if !FORMATS.contains(&d.format.as_str()) && !image_source {
        return Err(format!("Format inconnu : {}", d.format));
    }
    let workflow = workflow_de(contexte, tache);
    let wf = workflows::obtenir(&workflow).map_err(|_| format!("Workflow vidéo absent : {workflow}"))?;
    if !wf.manquants(&contexte.modeles).is_empty() {
        return Err(format!("Workflow vidéo incomplet : {workflow}"));
    }
    let reglage = reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&workflow).and_then(Value::as_object).cloned());
    let prets = wf.variantes(&contexte.modeles, reglage.as_ref());
    match &d.modele {
        Some(m) if !prets.contains(m) => {
            return Err(format!("Modèle absent de modeles/{} : {m}", wf.dossier_modeles()));
        }
        None if prets.is_empty() && wf.entree("steps").is_some() => {
            return Err(
                "Checkpoint à valider : ses steps ne sont pas connus ; valide-les dans Modèles → Ma collection".to_owned(),
            );
        }
        _ => {}
    }
    let origine = match &source {
        Some(s) if animer => creations::textes_origine(&contexte.data, std::slice::from_ref(s))?,
        _ => Vec::new(),
    };
    let idee = if d.idee.trim().is_empty() {
        prompt
    } else {
        d.idee.trim()
    };
    let loras = plateau::valider_loras(contexte, &wf, &d.loras)?;
    let mut textes: Vec<&str> = vec![idee, prompt];
    textes.extend(origine.iter().map(String::as_str));
    textes.extend(plateau::textes_loras(&loras));
    securite::verifier(&contexte.dossier, &textes).map_err(|e| e.to_string())?;
    plateau::memoriser_loras(contexte, &wf, &loras)?;
    let mut c = Commande::nouvelle(id, "clip", idee.to_owned(), workflow, &d.format);
    c.prompt = prompt.to_owned();
    c.llm = d.ecrit_par.as_deref().map(|l| l.chars().take(200).collect());
    c.seed = d.seed;
    c.nombre = d.nombre;
    c.modele.clone_from(&d.modele);
    c.parent = if animer { source } else { None };
    c.cadrage = (animer && !image_source).then(|| d.format.clone());
    c.loras = loras;
    c.mode_video = Some(d.mode.clone());
    c.qualite = Some(d.qualite.clone());
    c.duree = Some(d.duree);
    Ok(c)
}

/// L'image à animer, telle quelle ou recadrée au centre au format choisi.
fn image_de_depart(contexte: &Contexte, c: &Commande) -> Result<Vec<u8>, String> {
    let parent = c
        .parent
        .as_deref()
        .ok_or_else(|| "Rush à animer manquant".to_owned())?;
    let chemin = creations::chemin_image(&contexte.data, parent)?;
    let octets = std::fs::read(&chemin).map_err(|e| format!("Image de {parent} illisible : {e}"))?;
    match &c.cadrage {
        Some(format) => images::png(&images::recadrer_centre(&images::ouvrir(&octets)?, format)?),
        None => Ok(octets),
    }
}

/// Tourne un clip par seed : la vidéo et sa vignette, sur le prompt tel quel (les mots déclencheurs des LoRA en tête).
///
/// # Errors
/// Filtre AS-18, workflow ou résolution inconnus, ComfyUI qui échoue ou ne rend pas la vidéo, ou annulation.
pub fn executer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    suivi: &Suivi<'_>,
    c: &Commande,
) -> Result<(), String> {
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let origine = creations::textes_origine(&contexte.data, c.parent.as_slice())?;
    let mut textes: Vec<&str> = vec![c.idee.as_str(), c.prompt.as_str()];
    textes.extend(origine.iter().map(String::as_str));
    textes.extend(plateau::textes_loras(&c.loras));
    filtre.verifier(&textes).map_err(|e| e.to_string())?;
    let wf: Workflow = workflows::obtenir(&c.workflow)?;
    let reglage = reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&c.workflow).and_then(Value::as_object).cloned());
    let cle = c.qualite.as_deref().unwrap_or(QUALITE_DEFAUT);
    let qualite = qualites(contexte)
        .into_iter()
        .find(|q| q.cle == cle)
        .ok_or_else(|| format!("Résolution inconnue : {cle}"))?;
    let duree = c.duree.unwrap_or(DUREES[0]);
    let animer = c.mode_video.as_deref() == Some("animer");
    let modele = c.modele.clone().or_else(|| {
        wf.regle(&contexte.modeles, reglage.as_ref())
            .get("modele")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let base = match c.seed {
        Some(seed) => seed,
        None => plateau::seed_au_hasard()?,
    };
    let source = if animer {
        Some(generateur.televerser(&format!("aria_{}.png", c.id), &image_de_depart(contexte, c)?)?)
    } else {
        None
    };
    for i in 0..c.nombre {
        if suivi.annulee() {
            return Err(crate::comfy_serveur::ANNULEE.to_owned());
        }
        let seed = base.wrapping_add(u64::from(i));
        suivi.etape(&if c.nombre > 1 {
            format!("Tournage {}/{}", i + 1, c.nombre)
        } else {
            "Tournage".to_owned()
        });
        suivi.progression(0.0);
        let debut = Instant::now();
        let mut valeurs = Map::new();
        valeurs.insert(
            "prompt".to_owned(),
            json!(plateau::avec_declencheurs(&c.prompt, &c.loras)),
        );
        valeurs.insert("seed".to_owned(), json!(seed));
        valeurs.insert("duree".to_owned(), json!(duree));
        valeurs.insert("megapixels".to_owned(), json!(qualite.megapixels));
        valeurs.insert("modele".to_owned(), json!(c.modele));
        valeurs.insert("prefixe".to_owned(), json!(PREFIXE));
        if !c.loras.is_empty() {
            valeurs.insert("loras".to_owned(), json!(c.loras));
        }
        match &source {
            Some(nom) => {
                valeurs.insert("image".to_owned(), json!(nom));
            }
            None => {
                valeurs.insert("ratio".to_owned(), json!(c.format));
            }
        }
        let mut api = wf.preparer(&valeurs, &contexte.modeles, reglage.as_ref(), &filtre)?;
        wf.ajouter_affiche(&mut api, PREFIXE)?;
        let fichiers = generateur.generer(&api, &mut |v, m| suivi.progression_de(v, m), suivi.drapeau())?;
        let noms: Vec<String> = fichiers.iter().map(|(n, _)| n.clone()).collect();
        let (nom_video, nom_affiche) = comfy::video_et_affiche(&noms);
        let octets = |nom: Option<&String>| {
            nom.and_then(|n| fichiers.iter().find(|(f, _)| f == n).map(|(_, o)| o.clone()))
        };
        let (Some(mp4), Some(affiche)) = (octets(nom_video), octets(nom_affiche)) else {
            return Err("ComfyUI n'a pas renvoyé la vidéo attendue".to_owned());
        };
        let infos = json!({
            "type": "clip",
            "mode_video": c.mode_video,
            "idee": c.idee,
            "prompt": c.prompt,
            "llm": c.llm,
            "workflow": c.workflow,
            "qualite": cle,
            "duree": duree,
            "megapixels": qualite.megapixels,
            "steps": modele.as_deref().map(|m| wf.steps_de(reglage.as_ref(), m)),
            "modele": modele,
            "loras": c.loras,
            "format": if animer { retouche::FORMAT_IMAGE.to_owned() } else { c.format.clone() },
            "cadrage": c.cadrage,
            "seed": seed,
            "parent": c.parent,
            "duree_s": (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0,
        });
        let mut infos = infos.as_object().cloned().unwrap_or_default();
        if c.cadrage.is_none() {
            infos.remove("cadrage");
        }
        let id = creations::enregistrer_clip(&contexte.data, &mp4, &affiche, &infos)?;
        suivi.image(id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dossier;
    use crate::comfy_serveur::Fichier;
    use crate::llama::{self, Lancement};
    use crate::modeles;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    fn contexte(temp: &tempfile::TempDir) -> Contexte {
        let modeles_dir = temp.path().join("modeles");
        for id in ["minimax_h3_i2v", "minimax_h3_t2v"] {
            let wf = workflows::obtenir(id).unwrap();
            for f in wf.manquants(&modeles_dir) {
                let chemin = modeles::destination(
                    &modeles_dir,
                    f["dossier"].as_str().unwrap(),
                    f["nom"].as_str().unwrap(),
                )
                .unwrap();
                fs::create_dir_all(chemin.parent().unwrap()).unwrap();
                fs::write(chemin, b"poids").unwrap();
            }
        }
        let dossier = Dossier::a(temp.path());
        let i2v = workflows::obtenir("minimax_h3_i2v").unwrap();
        // Le checkpoint d'origine de H3 attend ses steps (8 ou 12 selon les sources) : validés pour la famille.
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

    fn demande(mode: &str) -> DemandeClip {
        DemandeClip {
            idee: String::new(),
            prompt: "A fox runs in the snow.".to_owned(),
            mode: mode.to_owned(),
            source: None,
            qualite: "rapide".to_owned(),
            duree: 5,
            format: "16:9 (Widescreen)".to_owned(),
            seed: Some(7),
            nombre: 2,
            modele: None,
            loras: Vec::new(),
            ecrit_par: None,
        }
    }

    fn png(largeur: u32, hauteur: u32) -> Vec<u8> {
        images::png(&image::RgbImage::new(largeur, hauteur)).unwrap()
    }

    struct Faux {
        graphes: Vec<Value>,
        televerses: Vec<String>,
    }

    impl Generateur for Faux {
        fn ecrire(&mut self, _: Lancement, _: &llama::Demande) -> Result<llama::Reponse, String> {
            Err("pas de LLM ici".to_owned())
        }

        fn televerser(&mut self, nom: &str, _: &[u8]) -> Result<String, String> {
            self.televerses.push(nom.to_owned());
            Ok(nom.to_owned())
        }

        fn generer(
            &mut self,
            api: &Value,
            _: &mut dyn FnMut(u64, u64),
            _: &AtomicBool,
        ) -> Result<Vec<Fichier>, String> {
            self.graphes.push(api.clone());
            Ok(vec![
                ("aria_montage_00001_.mp4".to_owned(), b"video".to_vec()),
                ("aria_montage_affiche_00001_.png".to_owned(), png(64, 36)),
            ])
        }
    }

    #[test]
    fn controle_d_un_clip_comme_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let c = commande_clip(&ctx, &demande("texte"), "a1".to_owned()).unwrap();
        assert_eq!(
            (c.genre.as_str(), c.workflow.as_str(), c.idee.as_str()),
            ("clip", "minimax_h3_t2v", "A fox runs in the snow.")
        );
        assert_eq!(
            (c.duree, c.qualite.as_deref(), c.mode_video.as_deref()),
            (Some(5), Some("rapide"), Some("texte"))
        );
        let erreur = |changer: fn(&mut DemandeClip)| {
            let mut d = demande("texte");
            changer(&mut d);
            commande_clip(&ctx, &d, "x".to_owned()).unwrap_err()
        };
        assert!(erreur(|d| d.prompt = "  ".into()).contains("prompt est vide"));
        assert!(erreur(|d| d.mode = "danser".into()).contains("Mode inconnu"));
        assert!(erreur(|d| d.qualite = "4k".into()).contains("Résolution inconnue"));
        assert!(erreur(|d| d.duree = 7).contains("5, 8, 10, 15"));
        assert!(erreur(|d| d.nombre = 21).contains("De 1 à 20"));
        assert!(erreur(|d| d.format = "5:4".into()).contains("Format inconnu"));
        assert!(erreur(|d| d.mode = "animer".into()).contains("rush à animer"));
        assert!(erreur(|d| d.prompt = "a twelve-year-old girl, nude".into()).starts_with("ERREUR AS-18"));
        let vide = tempfile::tempdir().unwrap();
        let sans = Contexte {
            modeles: vide.path().join("modeles"),
            ..ctx.clone()
        };
        assert!(
            commande_clip(&sans, &demande("texte"), "x".to_owned())
                .unwrap_err()
                .contains("incomplet")
        );
    }

    #[test]
    fn tourner_deux_clips_d_un_rush() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let rush = creations::enregistrer(&ctx.data, &png(80, 60), &Map::new()).unwrap();
        let mut d = demande("animer");
        d.source = Some(rush.clone());
        d.format = "1:1 (Square)".to_owned();
        let c = commande_clip(&ctx, &d, "a1".to_owned()).unwrap();
        assert_eq!(c.cadrage.as_deref(), Some("1:1 (Square)"));
        let partage = plateau::Partage::default();
        partage.ajouter(c.clone());
        let suivi = Suivi::de(&partage, "a1");
        let mut faux = Faux {
            graphes: Vec::new(),
            televerses: Vec::new(),
        };
        executer(&ctx, &mut faux, &suivi, &c).unwrap();
        assert_eq!(faux.televerses.len(), 1, "le rush est téléversé une fois");
        assert_eq!(faux.graphes.len(), 2);
        assert!(faux.graphes[0].get("aria_affiche").is_some());
        let ids = partage.commande("a1").unwrap().images;
        assert_eq!(ids.len(), 2);
        let fiche = creations::fiche(&ctx.data, &ids[0]).unwrap();
        assert_eq!(
            (
                fiche["type"].as_str(),
                fiche["seed"].as_u64(),
                fiche["parent"].as_str()
            ),
            (Some("clip"), Some(7), Some(rush.as_str()))
        );
        assert!(
            ctx.data
                .join("creations")
                .join(format!("{}.mp4", ids[0]))
                .is_file()
        );
    }

    #[test]
    fn resolutions_reglees() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        assert_eq!(qualites(&ctx)[1].megapixels, 0.6);
        reglages::modifier_section(&ctx.dossier, "workflows", |s| {
            s.insert("minimax_h3_i2v".to_owned(), json!({"resolution_compromis": 0.8}));
        })
        .unwrap();
        let q = &qualites(&ctx)[1];
        assert_eq!(
            (q.megapixels, q.nom.as_str(), q.aide.as_str()),
            (0.8, "0,8 MP", "réglé dans Réglages")
        );
        assert!(q.estimation_s > 170.0 && q.estimation_s < 425.0);
        let o = options(&ctx);
        assert_eq!(o.durees, DUREES);
        assert!(o.animer.workflows.iter().any(|w| w.id == "minimax_h3_i2v"));
    }
}
