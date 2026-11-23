//! La retouche et l'agrandissement d'une création, et l'import d'une image, comme en v2 (`app/plateau.py`,
//! `commander_retouche`, `commander_agrandissement`, `_retoucher`, `importer`).
//!
//! La retouche passe la source (recadrée au centre à un format si on le demande) au modèle de retouche avec la
//! consigne ; un masque peint limite le changement à sa zone, l'original revenant partout ailleurs avec un fondu.
//! L'agrandissement pose la source sur un canevas plus grand, fait remplir le reste, puis remet la source intacte.
//! La consigne part telle quelle, ou traduite par le LLM, qui voit l'image s'il a sa vision. Le filtre AS-18 lit la
//! consigne avec les idées et prompts de la source et de ses ancêtres : il ne lit pas les images.

use std::time::Instant;

use image::imageops::{self, FilterType};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::images::{self, ANCRAGES, TAILLE_MAX};
use crate::plateau::{
    self, ChoixLora, Commande, Contexte, Generateur, NOMBRE_MAX, Options, PREFIXE, Reecriture, Suivi,
};
use crate::prompt::{self, Vue};
use crate::workflows::{self, FORMATS, Workflow};
use crate::{creations, llm, reglages, rushes, securite};

/// La consigne d'un agrandissement : remplacer le gris par la suite de la scène (v2, mot pour mot).
pub const CONSIGNE_AGRANDISSEMENT: &str = "Replace the flat gray border areas with a natural continuation of the scene, as if the camera had captured a wider view: extend the environment, objects and lighting seamlessly, matching the style, colors and perspective. No gray areas, no frames, no borders must remain. Keep the existing picture unchanged.";
/// Le workflow de retouche de la v2 (`config.yaml`, `generation.retouche`).
pub const WORKFLOW_RETOUCHE: &str = "qwen_image_2_1_edit";
/// « Celui de l'image » : la source garde son format.
pub const FORMAT_IMAGE: &str = "image source";
/// Le fondu le plus large, en pixels.
pub const FONDU_MAX: u32 = 128;
const TACHE: &str = "retouche";

const fn fondu_retouche() -> u32 {
    24
}

const fn fondu_agrandissement() -> u32 {
    96
}

const fn un() -> u32 {
    1
}

const fn vrai() -> bool {
    true
}

fn centre() -> String {
    "centre".to_owned()
}

/// Une demande de retouche, comme celle de la v2.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeRetouche {
    pub source: String,
    pub instruction: String,
    /// Faire traduire la consigne par le LLM (absent : elle part telle quelle, comme la page Retouche de la v2).
    #[serde(default)]
    pub reecrire: bool,
    #[serde(default)]
    pub llm: Option<String>,
    #[serde(default)]
    pub workflow: Option<String>,
    /// Le masque peint, en data URL PNG : blanc ou opaque = zone à retoucher.
    #[serde(default)]
    pub masque: Option<String>,
    #[serde(default = "fondu_retouche")]
    pub fondu: u32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub modele: Option<String>,
    #[serde(default = "un")]
    pub nombre: u32,
    /// Recadre la source au centre à ce format avant la retouche ; absent ou « image source » : son format.
    #[serde(default)]
    pub format: Option<String>,
    /// Les LoRA de la famille du workflow.
    #[serde(default)]
    pub loras: Vec<ChoixLora>,
    /// Le dossier où ranger les résultats (« Ranger dans ») ; absent : à côté de la source.
    #[serde(default)]
    pub dossier: Option<String>,
}

/// Une demande d'agrandissement, comme celle de la v2.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeAgrandissement {
    pub source: String,
    pub largeur: u32,
    pub hauteur: u32,
    #[serde(default = "centre")]
    pub ancrage: String,
    /// Une précision facultative, ajoutée à la consigne (traduite par le LLM si `reecrire`).
    #[serde(default)]
    pub precision: String,
    #[serde(default = "vrai")]
    pub reecrire: bool,
    #[serde(default)]
    pub llm: Option<String>,
    #[serde(default)]
    pub workflow: Option<String>,
    #[serde(default = "fondu_agrandissement")]
    pub fondu: u32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub modele: Option<String>,
    #[serde(default = "un")]
    pub nombre: u32,
    /// Les LoRA de la famille du workflow.
    #[serde(default)]
    pub loras: Vec<ChoixLora>,
    /// Le dossier où ranger les résultats (« Ranger dans ») ; absent : à côté de la source.
    #[serde(default)]
    pub dossier: Option<String>,
}

/// Le workflow de retouche par défaut : le favori de l'utilisateur (`favoris.generation.retouche`) s'il est encore
/// livré, sinon celui de la v2.
pub fn workflow_defaut(contexte: &Contexte) -> String {
    reglages::section(&contexte.dossier, "favoris")
        .and_then(|f| f.get("generation")?.get(TACHE)?.as_str().map(str::to_owned))
        .filter(|id| workflows::obtenir(id).is_ok())
        .unwrap_or_else(|| WORKFLOW_RETOUCHE.to_owned())
}

/// Ce que la page Retouche propose : ses workflows, les formats et les LLM.
pub fn options(contexte: &Contexte) -> Options {
    crate::plateau::options_pour(contexte, TACHE, workflow_defaut(contexte))
}

/// Le workflow de retouche demandé, sinon celui par défaut ; prêt à tourner.
///
/// # Errors
/// Workflow inconnu, d'une autre tâche, ou incomplet.
pub fn workflow_retouche(contexte: &Contexte, demande: Option<&str>) -> Result<Workflow, String> {
    let id = demande.map_or_else(|| workflow_defaut(contexte), str::to_owned);
    let indisponible = || format!("Workflow indisponible pour retoucher : {id}");
    let wf = workflows::obtenir(&id).map_err(|_| indisponible())?;
    if wf.tache() != TACHE || !wf.manquants(&contexte.modeles).is_empty() {
        return Err(indisponible());
    }
    Ok(wf)
}

fn reglages_de(contexte: &Contexte, wf: &Workflow) -> Option<Map<String, Value>> {
    reglages::section(&contexte.dossier, "workflows")?
        .remove(&wf.id)
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
}

fn valider_modele(contexte: &Contexte, wf: &Workflow, modele: Option<&str>) -> Result<(), String> {
    let prets = wf.variantes(&contexte.modeles, reglages_de(contexte, wf).as_ref());
    match modele {
        Some(m) if !prets.iter().any(|p| p == m) => {
            Err(format!("Modèle absent de modeles/{} : {m}", wf.dossier_modeles()))
        }
        None if prets.is_empty() && wf.entree("steps").is_some() => Err(
            "Checkpoint à valider : ses steps ne sont pas connus ; valide-les avant de t'en servir"
                .to_owned(),
        ),
        _ => Ok(()),
    }
}

fn valider_llm(contexte: &Contexte, wf: &Workflow, llm: Option<&str>) -> Result<String, String> {
    let choisi = llm.filter(|l| !l.is_empty()).map_or_else(
        || llm::de_workflow(&contexte.dossier, &contexte.modeles, &wf.id),
        str::to_owned,
    );
    if llm::disponibles(&contexte.modeles).contains(&choisi) {
        Ok(choisi)
    } else {
        Err(format!("GGUF absent de modeles/llm : {choisi}"))
    }
}

fn valider_nombre(nombre: u32) -> Result<(), String> {
    if (1..=NOMBRE_MAX).contains(&nombre) {
        Ok(())
    } else {
        Err(format!("Le nombre d'images doit être entre 1 et {NOMBRE_MAX}"))
    }
}

/// Les LoRA d'une demande, contrôlées ; leurs mots déclencheurs passent au filtre AS-18 avec la consigne, puis sont
/// retenus pour la prochaine fois.
fn loras_de(
    contexte: &Contexte,
    wf: &Workflow,
    loras: &[ChoixLora],
    texte: &str,
    origine: &[String],
) -> Result<Vec<ChoixLora>, String> {
    let loras = plateau::valider_loras(contexte, wf, loras)?;
    if !loras.is_empty() {
        let mut textes: Vec<&str> = vec![texte];
        textes.extend(origine.iter().map(String::as_str));
        textes.extend(plateau::textes_loras(&loras));
        securite::verifier(&contexte.dossier, &textes).map_err(|e| e.to_string())?;
        plateau::memoriser_loras(contexte, wf, &loras)?;
    }
    Ok(loras)
}

fn verifier(contexte: &Contexte, texte: &str, origine: &[String]) -> Result<(), String> {
    let mut textes: Vec<&str> = vec![texte];
    textes.extend(origine.iter().map(String::as_str));
    securite::verifier(&contexte.dossier, &textes).map_err(|e| e.to_string())
}

/// Contrôle une demande de retouche comme la v2, et rend la commande à mettre en file.
///
/// # Errors
/// Le message de la v2 : format inconnu ou combiné à un masque, source inconnue, nombre hors bornes, consigne vide,
/// refus du filtre AS-18, workflow indisponible, modèle ou LLM absent, masque illisible.
pub fn commande_retouche(contexte: &Contexte, d: &DemandeRetouche, id: String) -> Result<Commande, String> {
    let format = d.format.clone().filter(|f| f != FORMAT_IMAGE && !f.is_empty());
    if let Some(f) = &format
        && !FORMATS.contains(&f.as_str())
    {
        return Err(format!("Format inconnu : {f}"));
    }
    if format.is_some() && d.masque.is_some() {
        return Err(
            "Un format ne se combine pas avec un masque : retouche sans masque, ou garde le format de l'image"
                .to_owned(),
        );
    }
    let origine = creations::textes_origine(&contexte.data, std::slice::from_ref(&d.source))?;
    valider_nombre(d.nombre)?;
    let instruction = d.instruction.trim().to_owned();
    if instruction.is_empty() {
        return Err("Écris ta consigne de retouche".to_owned());
    }
    verifier(contexte, &instruction, &origine)?;
    let wf = workflow_retouche(contexte, d.workflow.as_deref())?;
    valider_modele(contexte, &wf, d.modele.as_deref())?;
    let loras = loras_de(contexte, &wf, &d.loras, &instruction, &origine)?;
    let llm = if d.reecrire {
        Some(valider_llm(contexte, &wf, d.llm.as_deref())?)
    } else {
        None
    };
    let masque = d.masque.as_deref().map(images::depuis_data_url).transpose()?;
    let mut c = Commande::nouvelle(id, "retouche", instruction.clone(), wf.id.clone(), "retouche");
    c.prompt = if d.reecrire { String::new() } else { instruction };
    c.reecrire = d.reecrire;
    c.llm = llm;
    c.seed = d.seed;
    c.nombre = d.nombre;
    c.modele = d.modele.clone();
    c.parent = Some(d.source.clone());
    c.masque = masque.is_some();
    c.masque_png = masque;
    c.fondu = d.fondu.min(FONDU_MAX);
    c.cadrage = format;
    c.loras = loras;
    c.dossier = rushes::valider_dossier(&contexte.data, d.dossier.as_deref(), "images")?;
    Ok(c)
}

/// Contrôle une demande d'agrandissement comme la v2, et rend la commande à mettre en file.
///
/// # Errors
/// Le message de la v2 : source inconnue, nombre hors bornes, taille trop petite ou trop grande, ancrage inconnu,
/// refus du filtre AS-18, workflow indisponible, modèle ou LLM absent.
pub fn commande_agrandissement(
    contexte: &Contexte,
    d: &DemandeAgrandissement,
    id: String,
) -> Result<Commande, String> {
    let fiche = creations::fiche(&contexte.data, &d.source)?;
    valider_nombre(d.nombre)?;
    let origine = creations::textes_origine(&contexte.data, std::slice::from_ref(&d.source))?;
    let dimension = |cle: &str| {
        fiche
            .get(cle)
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0)
    };
    let (l, h) = (dimension("largeur"), dimension("hauteur"));
    if d.largeur < l || d.hauteur < h || (d.largeur, d.hauteur) == (l, h) {
        return Err(format!("La nouvelle taille doit être plus grande que {l}×{h}"));
    }
    if d.largeur.max(d.hauteur) > TAILLE_MAX {
        return Err(format!("Taille maximale : {TAILLE_MAX} px de côté"));
    }
    if !ANCRAGES.iter().any(|(nom, _)| *nom == d.ancrage) {
        return Err(format!("Ancrage inconnu : {}", d.ancrage));
    }
    let precision = d.precision.trim().to_owned();
    verifier(contexte, &precision, &origine)?;
    let consigne = if precision.is_empty() {
        Some(CONSIGNE_AGRANDISSEMENT.to_owned())
    } else if d.reecrire {
        None // la précision sera traduite par le LLM, puis ajoutée à la consigne
    } else {
        Some(format!("{CONSIGNE_AGRANDISSEMENT} {precision}"))
    };
    let wf = workflow_retouche(contexte, d.workflow.as_deref())?;
    valider_modele(contexte, &wf, d.modele.as_deref())?;
    let loras = loras_de(contexte, &wf, &d.loras, &precision, &origine)?;
    let llm = if consigne.is_none() {
        Some(valider_llm(contexte, &wf, d.llm.as_deref())?)
    } else {
        None
    };
    let mut c = Commande::nouvelle(id, "agrandissement", precision, wf.id.clone(), "agrandi");
    c.reecrire = consigne.is_none();
    c.prompt = consigne.unwrap_or_default();
    c.llm = llm;
    c.seed = d.seed;
    c.nombre = d.nombre;
    c.modele = d.modele.clone();
    c.parent = Some(d.source.clone());
    c.fondu = d.fondu.min(FONDU_MAX);
    c.largeur = Some(d.largeur);
    c.hauteur = Some(d.hauteur);
    c.ancrage = Some(d.ancrage.clone());
    c.loras = loras;
    c.dossier = rushes::valider_dossier(&contexte.data, d.dossier.as_deref(), "images")?;
    Ok(c)
}

/// Ajoute une image personnelle aux créations, pour la retoucher ou l'agrandir : réduite à 4 096 px de côté au plus.
///
/// # Errors
/// Image illisible, ou dossier qui ne s'écrit pas.
pub fn importer(
    contexte: &Contexte,
    octets: &[u8],
    nom: &str,
    dossier: Option<&str>,
) -> Result<String, String> {
    let dossier = rushes::valider_dossier(&contexte.data, dossier, "images")?;
    let mut image = images::ouvrir(octets)?;
    let (l, h) = image.dimensions();
    if l.max(h) > TAILLE_MAX {
        let f = f64::from(TAILLE_MAX) / f64::from(l.max(h));
        let taille = |v: u32| {
            // Des dimensions sous 4 096 px.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let t = (f64::from(v) * f).round().max(1.0) as u32;
            t
        };
        image = imageops::resize(&image, taille(l), taille(h), FilterType::Lanczos3);
    }
    let nom_origine: String = nom.chars().take(120).collect();
    let infos = json!({
        "type": "import",
        "idee": "",
        "prompt": "",
        "llm": null,
        "workflow": null,
        "format": "importée",
        "seed": null,
        "parent": null,
        "nom_origine": nom_origine,
        "duree_s": 0,
    });
    let mut infos = infos.as_object().cloned().unwrap_or_default();
    if let Some(d) = dossier {
        infos.insert("dossier".to_owned(), json!(d));
    }
    creations::enregistrer(&contexte.data, &images::png(&image)?, &infos)
}

/// La traduction de la consigne, contrôlée : la consigne et le LLM qui l'a écrite.
fn traduire_controle(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    c: &Commande,
    wf: &Workflow,
    vue: Option<&Vue>,
) -> Result<(prompt::Controle, String), String> {
    let ecriture = prompt::preparer_traduction(&contexte.lieux(), &c.idee, c.llm.as_deref(), Some(wf), vue)?;
    let attentes = ecriture.attentes(Some(wf));
    let lancement = ecriture.lancement.clone();
    let controle = prompt::ecrire_controle(&ecriture, &c.idee, &attentes, |d| {
        generateur.ecrire(lancement.clone(), d).map(|r| r.texte)
    })?;
    Ok((controle, ecriture.modele()))
}

/// Exécute une retouche ou un agrandissement : traduction de la consigne s'il y a lieu, source téléversée une fois,
/// une image par seed, recollage, fiche de la v2.
///
/// # Errors
/// Source illisible, refus du filtre, LLM ou ComfyUI en erreur, annulation, ou création qui ne s'écrit pas.
pub fn executer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    suivi: &Suivi<'_>,
    c: &Commande,
) -> Result<(), String> {
    let debut = Instant::now();
    let parent = c
        .parent
        .clone()
        .ok_or_else(|| "Retouche sans image de départ".to_owned())?;
    let octets = std::fs::read(creations::chemin_image(&contexte.data, &parent)?)
        .map_err(|e| format!("Image de {parent} illisible : {e}"))?;
    let mut source = images::ouvrir(&octets)?;
    if let Some(format) = &c.cadrage {
        source = images::recadrer_centre(&source, format)?;
    }
    let origine = creations::textes_origine(&contexte.data, std::slice::from_ref(&parent))?;
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let wf = workflows::obtenir(&c.workflow)?;
    let agrandir = c.genre == "agrandissement";
    let mut reecriture: Option<Reecriture> = None;
    let (consigne, llm_utilise) = if c.reecrire {
        suivi.etape("Traduction de la consigne");
        let vue = if agrandir {
            None
        } else {
            Some(Vue {
                png: images::pour_la_vision(&images::png(&source)?)?,
                texte: creations::description(&contexte.data, &creations::fiche(&contexte.data, &parent)?, 3),
            })
        };
        let essai = traduire_controle(contexte, generateur, c, &wf, vue.as_ref());
        if suivi.annulee() {
            return Err(crate::comfy_serveur::ANNULEE.to_owned());
        }
        let (traduction, llm, faite) = match essai {
            Ok((controle, modele)) if controle.reste.is_none() => {
                let etat = if controle.redemande.is_some() {
                    "redemandee"
                } else {
                    "faite"
                };
                (
                    controle.texte,
                    Some(modele),
                    Reecriture::de(etat, controle.redemande),
                )
            }
            // Repli : le LLM a raté deux fois, ou il a planté ; la consigne part telle quelle.
            Ok((controle, _)) => (
                c.idee.clone(),
                None,
                Reecriture::de("repli", controle.reste.map(|m| format!("contrôle qualité : {m}"))),
            ),
            Err(e) if e == crate::comfy_serveur::ANNULEE => return Err(e),
            Err(e) => (c.idee.clone(), None, Reecriture::de("repli", Some(e))),
        };
        let consigne = if agrandir {
            format!("{CONSIGNE_AGRANDISSEMENT} {traduction}")
        } else {
            traduction
        };
        suivi.prompt(&consigne);
        suivi.note(faite.note().as_deref());
        reecriture = Some(faite);
        (consigne, llm)
    } else {
        (c.prompt.clone(), None)
    };
    let mut textes: Vec<&str> = vec![c.idee.as_str(), consigne.as_str()];
    textes.extend(origine.iter().map(String::as_str));
    filtre.verifier(&textes).map_err(|e| e.to_string())?;

    let (entree, position) = if agrandir {
        let (l, h) = (c.largeur.unwrap_or(0), c.hauteur.unwrap_or(0));
        let ancrage = c.ancrage.as_deref().unwrap_or("centre");
        let (canevas, position) = images::canevas_agrandi(&source, l, h, ancrage)?;
        (canevas, Some(position))
    } else {
        (source.clone(), None)
    };
    let masque = match (&c.masque_png, agrandir) {
        (Some(m), false) => images::masque_net(m, source.dimensions())?,
        _ => None,
    };
    let reglages = reglages_de(contexte, &wf);
    let modele = c.modele.clone().or_else(|| {
        wf.regle(&contexte.modeles, reglages.as_ref())
            .get("modele")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    // Sans dossier choisi, le résultat reste à côté de sa source (comme en v2).
    let dossier = match &c.dossier {
        Some(d) => Some(d.clone()),
        None => creations::fiche(&contexte.data, &parent)?
            .get("dossier")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    let base = match c.seed {
        Some(seed) => seed,
        None => crate::plateau::seed_au_hasard()?,
    };
    let etape = if agrandir { "Agrandissement" } else { "Retouche" };
    // L'image d'entrée est téléversée une seule fois pour toutes les variantes.
    let nom = generateur.televerser(&format!("aria_{}.png", c.id), &images::png(&entree)?)?;
    for i in 0..c.nombre {
        if suivi.annulee() {
            return Err(crate::comfy_serveur::ANNULEE.to_owned());
        }
        let seed = base.wrapping_add(u64::from(i));
        suivi.etape(&if c.nombre > 1 {
            format!("{etape} {}/{}", i + 1, c.nombre)
        } else {
            etape.to_owned()
        });
        suivi.progression(0.0);
        let debut_image = Instant::now();
        let mut valeurs = Map::new();
        valeurs.insert(
            "prompt".to_owned(),
            json!(plateau::avec_declencheurs(&consigne, &c.loras)),
        );
        valeurs.insert("image".to_owned(), json!(nom));
        valeurs.insert("seed".to_owned(), json!(seed));
        valeurs.insert("modele".to_owned(), json!(c.modele));
        valeurs.insert("prefixe".to_owned(), json!(PREFIXE));
        if !c.loras.is_empty() {
            valeurs.insert("loras".to_owned(), json!(c.loras));
        }
        let api = wf.preparer(&valeurs, &contexte.modeles, reglages.as_ref(), &filtre)?;
        let fichiers = generateur.generer(&api, &mut |v, m| suivi.progression_de(v, m), suivi.drapeau())?;
        let brut = fichiers
            .into_iter()
            .find(|(n, _)| n.to_ascii_lowercase().ends_with(".png"))
            .map(|(_, o)| o)
            .ok_or_else(|| "ComfyUI n'a renvoyé aucune image".to_owned())?;
        suivi.etape("Recollage");
        let resultat = images::ouvrir(&brut)?;
        let final_ = match (position, &masque) {
            (Some(pos), _) => {
                let (l, h) = entree.dimensions();
                let ajuste = imageops::resize(&resultat, l, h, FilterType::Lanczos3);
                images::recoller_agrandissement(&source, pos, &ajuste, c.fondu)
            }
            (None, Some(m)) => images::recoller(&source, &resultat, m, c.fondu),
            (None, None) => {
                let (l, h) = source.dimensions();
                imageops::resize(&resultat, l, h, FilterType::Lanczos3)
            }
        };
        let ecoule = if i == 0 { debut } else { debut_image }.elapsed().as_secs_f64();
        let mut infos = json!({
            "type": c.genre,
            "idee": c.idee,
            "prompt": consigne,
            "llm": llm_utilise,
            "workflow": c.workflow,
            "modele": modele,
            "loras": c.loras,
            "format": c.format,
            "seed": seed,
            "parent": parent,
            "masque": masque.is_some(),
            "fondu": c.fondu,
            "ancrage": if agrandir { json!(c.ancrage) } else { Value::Null },
            "duree_s": (ecoule * 100.0).round() / 100.0,
        });
        if let Some(objet) = infos.as_object_mut() {
            if let Some(f) = &c.cadrage {
                objet.insert("cadrage".to_owned(), json!(f));
            }
            if let Some(d) = &dossier {
                objet.insert("dossier".to_owned(), json!(d));
            }
            if let Some(r) = &reecriture {
                r.noter(objet);
            }
        }
        let png = images::png(&final_)?;
        let id_creation =
            creations::enregistrer(&contexte.data, &png, infos.as_object().unwrap_or(&Map::new()))?;
        suivi.image(id_creation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comfy_serveur::Fichier;
    use crate::llama::{self, Lancement};
    use crate::plateau::{EtatCommande, Plateau};
    use crate::{Dossier, modeles};
    use image::{Rgb, RgbImage};
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Un dossier où le workflow de retouche est prêt (fichiers posés, steps validés).
    fn contexte(temp: &tempfile::TempDir) -> Contexte {
        let modeles_dir = temp.path().join("modeles");
        let wf = workflows::obtenir(WORKFLOW_RETOUCHE).unwrap();
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
        let dossier = Dossier::a(temp.path());
        if let Some(origine) = wf.modele_origine()
            && wf.entree("steps").is_some()
        {
            workflows::valider_checkpoint(&dossier, &modeles_dir, WORKFLOW_RETOUCHE, &origine, 25).unwrap();
        }
        Contexte {
            dossier,
            data: temp.path().join("data"),
            modeles: modeles_dir,
            moteurs: temp.path().join("moteurs"),
            port_llama: 8095,
        }
    }

    fn uni(l: u32, h: u32, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(l, h, Rgb(c))
    }

    fn source(ctx: &Contexte, infos: Value) -> String {
        let png = images::png(&uni(40, 20, [0, 0, 255])).unwrap();
        creations::enregistrer(&ctx.data, &png, infos.as_object().unwrap()).unwrap()
    }

    fn retouche(source: &str, instruction: &str) -> DemandeRetouche {
        DemandeRetouche {
            source: source.to_owned(),
            instruction: instruction.to_owned(),
            reecrire: false,
            llm: None,
            workflow: None,
            masque: None,
            fondu: 24,
            seed: Some(3),
            modele: None,
            nombre: 1,
            format: None,
            loras: Vec::new(),
            dossier: None,
        }
    }

    fn agrandissement(source: &str, largeur: u32, hauteur: u32) -> DemandeAgrandissement {
        DemandeAgrandissement {
            source: source.to_owned(),
            largeur,
            hauteur,
            ancrage: "centre".to_owned(),
            precision: String::new(),
            reecrire: false,
            llm: None,
            workflow: None,
            fondu: 8,
            seed: Some(3),
            modele: None,
            nombre: 1,
            loras: Vec::new(),
            dossier: None,
        }
    }

    #[test]
    fn controle_d_une_retouche_comme_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let s = source(&ctx, json!({"type": "creation", "prompt": "a blue sky"}));
        let id = || "c1".to_owned();
        let c = commande_retouche(&ctx, &retouche(&s, "  make it red "), id()).unwrap();
        assert_eq!(
            (c.genre.as_str(), c.prompt.as_str(), c.format.as_str()),
            ("retouche", "make it red", "retouche")
        );
        assert_eq!(c.parent.as_deref(), Some(s.as_str()));
        assert_eq!(
            commande_retouche(&ctx, &retouche(&s, " "), id()).unwrap_err(),
            "Écris ta consigne de retouche"
        );
        let mut d = retouche(&s, "x");
        d.format = Some("5:4".to_owned());
        assert_eq!(
            commande_retouche(&ctx, &d, id()).unwrap_err(),
            "Format inconnu : 5:4"
        );
        d.format = Some("16:9 (Widescreen)".to_owned());
        assert_eq!(
            commande_retouche(&ctx, &d, id()).unwrap().cadrage.as_deref(),
            Some("16:9 (Widescreen)")
        );
        d.masque = Some(format!("data:image/png;base64,{}", images::base64(b"x")));
        assert!(
            commande_retouche(&ctx, &d, id())
                .unwrap_err()
                .starts_with("Un format ne se combine pas")
        );
        d.format = Some(FORMAT_IMAGE.to_owned());
        assert!(commande_retouche(&ctx, &d, id()).unwrap().masque);
        let mut d = retouche(&s, "x");
        d.nombre = 0;
        assert_eq!(
            commande_retouche(&ctx, &d, id()).unwrap_err(),
            "Le nombre d'images doit être entre 1 et 50"
        );
        assert!(commande_retouche(&ctx, &retouche("20200101-000000-000000", "x"), id()).is_err());
        let mut d = retouche(&s, "x");
        d.reecrire = true;
        assert!(
            commande_retouche(&ctx, &d, id())
                .unwrap_err()
                .starts_with("GGUF absent de modeles/llm")
        );
        // Le filtre lit la consigne avec le texte d'origine de la source.
        let mineure = source(
            &ctx,
            json!({"type": "creation", "prompt": "a twelve-year-old girl"}),
        );
        assert!(
            commande_retouche(&ctx, &retouche(&mineure, "nude"), id())
                .unwrap_err()
                .starts_with("ERREUR AS-18")
        );
    }

    #[test]
    fn controle_d_un_agrandissement_comme_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let s = source(&ctx, json!({"type": "creation", "prompt": "a blue sky"}));
        let id = || "c1".to_owned();
        let c = commande_agrandissement(&ctx, &agrandissement(&s, 60, 20), id()).unwrap();
        assert_eq!(c.prompt, CONSIGNE_AGRANDISSEMENT);
        assert_eq!(
            (c.largeur, c.hauteur, c.format.as_str()),
            (Some(60), Some(20), "agrandi")
        );
        let mut d = agrandissement(&s, 60, 20);
        d.precision = " add the sea ".to_owned();
        assert_eq!(
            commande_agrandissement(&ctx, &d, id()).unwrap().prompt,
            format!("{CONSIGNE_AGRANDISSEMENT} add the sea")
        );
        assert_eq!(
            commande_agrandissement(&ctx, &agrandissement(&s, 40, 20), id()).unwrap_err(),
            "La nouvelle taille doit être plus grande que 40×20"
        );
        assert!(commande_agrandissement(&ctx, &agrandissement(&s, 30, 40), id()).is_err());
        assert!(commande_agrandissement(&ctx, &agrandissement(&s, 5000, 20), id()).is_err());
        let mut d = agrandissement(&s, 60, 20);
        d.ancrage = "nulle-part".to_owned();
        assert_eq!(
            commande_agrandissement(&ctx, &d, id()).unwrap_err(),
            "Ancrage inconnu : nulle-part"
        );
    }

    #[test]
    fn options_de_la_retouche() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let o = options(&ctx);
        assert_eq!(o.defaut, WORKFLOW_RETOUCHE);
        assert_eq!(o.workflows.len(), 1);
        assert_eq!(o.workflows[0].id, WORKFLOW_RETOUCHE);
        assert!(!o.workflows[0].modeles.is_empty());
    }

    #[test]
    fn import_d_une_image() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let grande = images::png(&uni(5000, 100, [9, 9, 9])).unwrap();
        let id = importer(&ctx, &grande, "photo.jpg", None).unwrap();
        let f = creations::fiche(&ctx.data, &id).unwrap();
        assert_eq!(
            (f["largeur"].clone(), f["hauteur"].clone()),
            (json!(4096), json!(82))
        );
        assert_eq!(
            (f["type"].clone(), f["nom_origine"].clone()),
            (json!("import"), json!("photo.jpg"))
        );
        assert!(importer(&ctx, b"rien", "x", None).is_err());
        assert!(f.get("dossier").is_none());
        // « Ranger dans » : un dossier des Rushes existant, sinon refusé.
        let petite = images::png(&uni(40, 30, [9, 9, 9])).unwrap();
        assert!(importer(&ctx, &petite, "a.png", Some("Renards")).is_err());
        rushes::creer_dossier(&ctx.data, "Renards", "images").unwrap();
        let rangee = importer(&ctx, &petite, "a.png", Some("Renards")).unwrap();
        assert_eq!(
            creations::fiche(&ctx.data, &rangee).unwrap()["dossier"],
            "Renards"
        );
    }

    /// Un ComfyUI de test : note les téléversements et les graphes, rend une image rouge de 64 × 64.
    struct Faux {
        televerses: Arc<Mutex<Vec<String>>>,
        graphes: Arc<Mutex<Vec<Value>>>,
    }

    impl Generateur for Faux {
        fn ecrire(&mut self, _: Lancement, _: &llama::Demande) -> Result<llama::Reponse, String> {
            Err("pas de LLM ici".to_owned())
        }

        fn televerser(&mut self, nom: &str, _: &[u8]) -> Result<String, String> {
            self.televerses.lock().unwrap().push(nom.to_owned());
            Ok(nom.to_owned())
        }

        fn generer(
            &mut self,
            api: &Value,
            _: &mut dyn FnMut(u64, u64),
            _: &AtomicBool,
        ) -> Result<Vec<Fichier>, String> {
            self.graphes.lock().unwrap().push(api.clone());
            Ok(vec![(
                "sortie.png".to_owned(),
                images::png(&uni(64, 64, [255, 0, 0]))?,
            )])
        }
    }

    fn attendre(plateau: &Plateau, id: &str) -> Commande {
        let fin = Instant::now() + Duration::from_secs(20);
        loop {
            let c = plateau.commandes().into_iter().find(|c| c.id == id).unwrap();
            if !matches!(c.etat, EtatCommande::Attente | EtatCommande::EnCours) || Instant::now() > fin {
                return c;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn lire_image(data: &std::path::Path, id: &str) -> RgbImage {
        images::ouvrir(&fs::read(creations::chemin_image(data, id).unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn retouche_avec_masque_puis_agrandissement() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let data = ctx.data.clone();
        let s = source(
            &ctx,
            json!({"type": "creation", "prompt": "a blue sky", "dossier": "Ciels"}),
        );
        let televerses = Arc::new(Mutex::new(Vec::new()));
        let graphes = Arc::new(Mutex::new(Vec::new()));
        let plateau = Plateau::demarrer(
            ctx,
            Faux {
                televerses: Arc::clone(&televerses),
                graphes: Arc::clone(&graphes),
            },
        );

        // Masque : la moitié gauche.
        let mut masque = image::GrayImage::new(40, 20);
        for x in 0..20 {
            for y in 0..20 {
                masque.put_pixel(x, y, image::Luma([255]));
            }
        }
        let mut octets = std::io::Cursor::new(Vec::new());
        masque.write_to(&mut octets, image::ImageFormat::Png).unwrap();
        let mut d = retouche(&s, "make it red");
        d.masque = Some(format!(
            "data:image/png;base64,{}",
            images::base64(octets.get_ref())
        ));
        d.fondu = 0;
        d.nombre = 2;
        let c = plateau.commander_retouche(&d).unwrap();
        let fin = attendre(&plateau, &c.id);
        assert_eq!(fin.etat, EtatCommande::Termine, "{:?}", fin.erreur);
        assert_eq!(fin.images.len(), 2);
        // La source n'est téléversée qu'une fois pour les deux variantes.
        assert_eq!(televerses.lock().unwrap().len(), 1);
        let wf = workflows::obtenir(WORKFLOW_RETOUCHE).unwrap();
        let (noeud, entree) = wf.entree("image").unwrap();
        assert_eq!(
            graphes.lock().unwrap()[0][noeud.as_str()]["inputs"][entree.as_str()],
            json!(format!("aria_{}.png", c.id))
        );
        let image = lire_image(&data, &fin.images[0]);
        assert_eq!(image.dimensions(), (40, 20));
        assert_eq!(image.get_pixel(5, 10), &Rgb([255, 0, 0]));
        assert_eq!(image.get_pixel(35, 10), &Rgb([0, 0, 255]));
        let f = creations::fiche(&data, &fin.images[0]).unwrap();
        assert_eq!(f["type"], json!("retouche"));
        assert_eq!(f["parent"], json!(s));
        assert_eq!(f["masque"], json!(true));
        assert_eq!(f["dossier"], json!("Ciels"));
        assert_eq!(f["prompt"], json!("make it red"));
        assert_eq!(creations::fiche(&data, &fin.images[1]).unwrap()["seed"], json!(4));

        let c = plateau
            .commander_agrandissement(&agrandissement(&s, 80, 20))
            .unwrap();
        let fin = attendre(&plateau, &c.id);
        assert_eq!(fin.etat, EtatCommande::Termine, "{:?}", fin.erreur);
        let image = lire_image(&data, &fin.images[0]);
        assert_eq!(image.dimensions(), (80, 20));
        // Au cœur de la source, l'original revient au pixel près.
        assert_eq!(image.get_pixel(40, 10), &Rgb([0, 0, 255]));
        let f = creations::fiche(&data, &fin.images[0]).unwrap();
        assert_eq!(
            (f["type"].clone(), f["ancrage"].clone()),
            (json!("agrandissement"), json!("centre"))
        );
    }
}
