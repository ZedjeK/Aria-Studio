//! Le LLM regarde ce qui est sorti (nouveauté de la v3) : **vérifier une image** (elle respecte l'intention ? sinon
//! les écarts et un prompt corrigé, à relancer en un clic) et **choisir les meilleures** d'un lot. Il voit l'image par
//! sa vision (`mmproj`) : sans LLM qui voit, rien ne se fait, et l'interface dit quoi installer.
//!
//! Les deux passent par l'orchestrateur (un seul moteur sur la carte : ComfyUI vidé pendant que le LLM regarde), sur
//! un seul lancement du LLM pour tout un lot. ✕ Annuler les arrête ; le filtre AS-18 lit les textes avant et après ;
//! la réponse JSON mal formée, ou un prompt corrigé qui rate le contrôle qualité, est redemandée une fois.

use std::time::Instant;

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::directions::ErreurLecture;
use crate::llama::{self, Lancement};
use crate::montage::DemandeClip;
use crate::plateau::{ChoixLora, Contexte, Demande, Generateur, LIBERTE_DEFAUT};
use crate::prompt::{self, Attentes, CLAUSE_MINEURS, Vue};
use crate::workflows::{self, Workflow};
use crate::{creations, fiches, llm, reglages, securite};

/// Les images d'un lot, au plus (vérifiées d'affilée, ou comparées entre elles).
pub const IMAGES_MAX: usize = 8;
/// Les images à comparer pour en choisir les meilleures, au moins.
pub const CHOIX_MIN: usize = 2;
const LONGUEUR_VERDICT: usize = 300;
const LONGUEUR_ECART: usize = 200;
const ECARTS_MAX: usize = 8;
const LONGUEUR_PROMPT: usize = 4000;
const LONGUEUR_RAISON: usize = 240;

/// Le message quand aucun LLM installé ne voit les images.
pub const SANS_VISION: &str = "Aucun LLM installé ne voit les images : installe un LLM avec son projecteur de vision \
     (le fichier « mmproj » de sa famille) dans Modèles → LLM, et laisse sa vision active dans Réglages → LLM.";

/// La consigne de la vérification (réglable dans Réglages, sous « Vérification d'une image »).
pub const CONSIGNE_VERIFICATION: &str = r#"Tu vérifies une image produite par un modèle d'image ou de vidéo, en la comparant à ce qui était voulu : l'idée de l'utilisateur et le prompt envoyé au modèle.

Regarde l'image telle qu'elle est vraiment, puis compare : les sujets (nombre, genre, apparence, vêtements, pose), l'action, le décor, le cadrage et l'angle, la lumière et l'ambiance, le style, et le texte écrit dans l'image s'il en était demandé (mot pour mot, orthographe comprise).

Réponds UNIQUEMENT par un objet JSON, sans rien avant ni après :
{"fidele": true, "verdict": "une phrase en français", "ecarts": ["un écart précis, en français"], "prompt": "le prompt corrigé"}

- fidele : true si l'image respecte l'intention, même avec de petits défauts sans importance ; false sinon.
- verdict : une phrase courte qui résume, en français.
- ecarts : ce qui manque ou est faux, du plus important au moins important ; une liste vide si l'image est fidèle.
- prompt : si l'image n'est pas fidèle, le prompt réécrit pour corriger ces écarts. Garde ce qui marche dans le prompt d'origine, renforce ou précise ce qui a été manqué, n'ajoute rien que l'idée ne demande pas. Une chaîne vide si l'image est fidèle.
- Ne décris pas l'image en entier : seulement les écarts."#;

/// La consigne du choix des meilleures (réglable dans Réglages, sous « Choix des meilleures images »).
pub const CONSIGNE_CHOIX: &str = r#"Tu compares plusieurs images produites pour la même intention (l'idée et le prompt donnés). Elles sont numérotées dans l'ordre : Image 1, Image 2…

Choisis celles qui respectent le mieux l'intention : les sujets, l'action, le décor, le cadrage, le style, et le texte demandé dans l'image s'il y en a. Ne départage par la beauté qu'à intention égale.

Réponds UNIQUEMENT par un objet JSON, sans rien avant ni après :
{"meilleures": [{"image": 1, "raison": "une phrase en français"}]}

- De une à {nombre} images au plus, de la meilleure à la moins bonne.
- raison : ce qui la place devant les autres, en une phrase."#;

// --- le LLM qui voit -------------------------------------------------------------------------------------------------

fn voit(contexte: &Contexte, nom: &str) -> bool {
    llm::projecteur(&contexte.dossier, &contexte.modeles, nom)
        .0
        .is_some()
}

/// Le LLM qui regarde : celui choisi (il doit voir) ; sinon, parmi les généralistes installés qui voient, celui du
/// modèle de la création, le LLM par défaut, puis le premier venu. Un spécialiste n'écrit que des prompts.
///
/// # Errors
/// LLM choisi absent ou sans vision, ou aucun LLM installé ne voit.
pub fn llm_qui_voit(
    contexte: &Contexte,
    choisi: Option<&str>,
    workflow: Option<&str>,
) -> Result<String, String> {
    if let Some(c) = choisi.filter(|c| !c.is_empty()) {
        crate::scenario::verifier_llm(contexte, Some(c))?;
        return if voit(contexte, c) {
            Ok(c.to_owned())
        } else {
            Err(format!(
                "{c} ne voit pas les images : installe son projecteur de vision (« mmproj ») dans Modèles → LLM, ou \
                 choisis un LLM qui voit"
            ))
        };
    }
    let disponibles = llm::disponibles(&contexte.modeles);
    let mut candidats: Vec<String> = Vec::new();
    if let Some(w) = workflow.filter(|w| !w.is_empty()) {
        candidats.push(llm::de_workflow(&contexte.dossier, &contexte.modeles, w));
    }
    candidats.push(llm::defaut(&contexte.dossier, &contexte.modeles));
    candidats.extend(disponibles.iter().cloned());
    candidats
        .into_iter()
        .find(|n| disponibles.contains(n) && fiches::specialiste(n).is_none() && voit(contexte, n))
        .ok_or_else(|| SANS_VISION.to_owned())
}

/// Ce que l'interface sait avant de proposer « Vérifier » et « Choisir les meilleures ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatVision {
    /// Le LLM qui regardera ; rien : aucun ne voit.
    pub llm: Option<String>,
    /// Pourquoi rien ne peut se faire, et quoi installer.
    pub explication: Option<String>,
}

/// Le LLM qui regarderait, ou pourquoi il n'y en a pas.
pub fn etat_vision(contexte: &Contexte, workflow: Option<&str>) -> EtatVision {
    match llm_qui_voit(contexte, None, workflow) {
        Ok(nom) => EtatVision {
            llm: Some(nom),
            explication: None,
        },
        Err(e) => EtatVision {
            llm: None,
            explication: Some(e),
        },
    }
}

// --- lecture des réponses ---------------------------------------------------------------------------------------------

fn espaces(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn borne(texte: &str, max: usize) -> String {
    let t = espaces(texte);
    if t.chars().count() <= max {
        t
    } else {
        format!("{}…", t.chars().take(max).collect::<String>().trim_end())
    }
}

/// L'objet JSON d'une réponse (après une éventuelle réflexion), ou pourquoi il n'y en a pas.
fn objet(texte: &str, quoi: &str) -> Result<Map<String, Value>, ErreurLecture> {
    let texte = texte.rsplit_once("</think>").map_or(texte, |(_, apres)| apres);
    let trouve = texte
        .find('{')
        .zip(texte.rfind('}'))
        .filter(|(debut, fin)| debut < fin)
        .and_then(|(debut, fin)| texte.get(debut..=fin))
        .and_then(|j| serde_json::from_str::<Value>(j).ok());
    if let Some(Value::Object(o)) = trouve {
        return Ok(o);
    }
    if texte.trim().is_empty() {
        return Err(ErreurLecture {
            message: "Réponse du LLM vide".to_owned(),
            refus: false,
        });
    }
    let debut: String = texte.trim().chars().take(300).collect();
    if regex::Regex::new(crate::conversation::REFUS).is_ok_and(|r| r.is_match(&debut)) {
        return Err(ErreurLecture {
            message: format!(
                "Le LLM refuse cette demande : « {} ». Un autre LLM qui voit peut l'accepter.",
                borne(texte, 160)
            ),
            refus: true,
        });
    }
    Err(ErreurLecture {
        message: format!(
            "Réponse du LLM illisible (pas de {quoi} en JSON) : « {} »",
            borne(texte, 160)
        ),
        refus: false,
    })
}

fn texte_de(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => prompt::nettoyer_markdown(s),
        Some(Value::Null) | None => String::new(),
        Some(autre) => autre.to_string(),
    }
}

/// Une vérification telle que le LLM l'a rendue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lue {
    pub fidele: bool,
    pub verdict: String,
    pub ecarts: Vec<String>,
    pub prompt: String,
}

/// La vérification d'une réponse : `{"fidele", "verdict", "ecarts", "prompt"}` (quelques variantes de noms tolérées).
///
/// # Errors
/// Réponse vide, sans JSON lisible, sans verdict ni fidélité, ou refus du LLM.
pub fn lire_verification(texte: &str) -> Result<Lue, ErreurLecture> {
    let o = objet(texte, "vérification")?;
    let premier = |cles: &[&str]| cles.iter().find_map(|c| o.get(*c));
    let fidele = match premier(&["fidele", "fidèle", "faithful", "ok"]) {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::String(s)) => match s.trim().to_lowercase().as_str() {
            "true" | "oui" | "yes" | "vrai" => Some(true),
            "false" | "non" | "no" | "faux" => Some(false),
            _ => None,
        },
        _ => None,
    };
    let verdict = borne(
        &texte_de(premier(&["verdict", "resume", "résumé", "summary"])),
        LONGUEUR_VERDICT,
    );
    let ecarts: Vec<String> = premier(&["ecarts", "écarts", "differences", "issues"])
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .map(|e| borne(&texte_de(Some(e)), LONGUEUR_ECART))
                .filter(|e| !e.is_empty())
                .take(ECARTS_MAX)
                .collect()
        })
        .unwrap_or_default();
    let prompt_corrige = borne(
        &texte_de(premier(&[
            "prompt",
            "prompt_corrige",
            "prompt corrigé",
            "corrected_prompt",
        ])),
        LONGUEUR_PROMPT,
    );
    let Some(fidele) = fidele.or_else(|| (!verdict.is_empty()).then_some(ecarts.is_empty())) else {
        return Err(ErreurLecture {
            message: "Réponse du LLM sans verdict".to_owned(),
            refus: false,
        });
    };
    Ok(Lue {
        fidele,
        verdict,
        ecarts,
        prompt: if fidele { String::new() } else { prompt_corrige },
    })
}

/// Les images retenues d'une réponse de choix : leur numéro (de 1 à `nombre_images`) et leur raison, sans doublon.
///
/// # Errors
/// Réponse vide, sans JSON lisible, ou refus du LLM.
pub fn lire_choix(texte: &str, nombre_images: usize) -> Result<Vec<(usize, String)>, ErreurLecture> {
    let o = objet(texte, "choix")?;
    let liste = ["meilleures", "best", "choix", "images"]
        .iter()
        .find_map(|c| o.get(*c))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut sortie: Vec<(usize, String)> = Vec::new();
    for v in &liste {
        let numero = match v
            .get("image")
            .or_else(|| v.get("numero"))
            .or_else(|| v.get("index"))
        {
            Some(Value::Number(n)) => n.as_u64().and_then(|n| usize::try_from(n).ok()),
            Some(Value::String(s)) => s
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok(),
            _ => None,
        };
        if let Some(n) = numero.filter(|n| (1..=nombre_images).contains(n))
            && !sortie.iter().any(|(m, _)| *m == n)
        {
            let raison = borne(
                &texte_de(v.get("raison").or_else(|| v.get("reason"))),
                LONGUEUR_RAISON,
            );
            sortie.push((n, raison));
        }
    }
    Ok(sortie)
}

/// Combien d'images retenir au plus dans un lot : la moitié (arrondie au-dessus), trois au plus.
pub fn retenues_max(nombre_images: usize) -> usize {
    nombre_images.div_ceil(2).clamp(1, 3)
}

// --- l'intention d'une création -------------------------------------------------------------------------------------

/// Ce qui était voulu pour une création, lu sur sa fiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intention {
    pub genre: String,
    pub idee: String,
    pub prompt: String,
    pub workflow: Option<String>,
}

/// L'intention d'une création ; une image importée n'en a pas.
///
/// # Errors
/// Création inconnue, image importée, ou fiche sans idée ni prompt.
pub fn intention(contexte: &Contexte, id: &str) -> Result<Intention, String> {
    let f = creations::fiche(&contexte.data, id)?;
    let texte = |cle: &str| {
        f.get(cle)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let genre = texte("type");
    if genre == "import" {
        return Err("Image importée : pas d'intention à comparer".to_owned());
    }
    let (idee, prompt) = (texte("idee"), texte("prompt"));
    if idee.is_empty() && prompt.is_empty() {
        return Err("Fiche sans idée ni prompt : pas d'intention à comparer".to_owned());
    }
    Ok(Intention {
        genre,
        idee,
        prompt,
        workflow: Some(texte("workflow")).filter(|w| !w.is_empty()),
    })
}

/// La ligne de ce qui était voulu, pour le message.
fn ligne_intention(i: &Intention) -> String {
    let mut lignes = Vec::new();
    if !i.idee.is_empty() && i.idee != i.prompt {
        lignes.push(format!("Idée de l'utilisateur : {}", i.idee));
    }
    if !i.prompt.is_empty() {
        lignes.push(format!("Prompt envoyé au modèle : {}", i.prompt));
    }
    lignes.join("\n")
}

/// Le modèle de la création et ses réglages.
fn cible(contexte: &Contexte, workflow: Option<&str>) -> (Option<Workflow>, Option<Map<String, Value>>) {
    let wf = workflow.and_then(|w| workflows::obtenir(w).ok());
    let regle = wf.as_ref().and_then(|w| {
        reglages::section(&contexte.dossier, "workflows")
            .and_then(|s| s.get(&w.id).and_then(Value::as_object).cloned())
    });
    (wf, regle)
}

/// Ce qu'on attend du prompt corrigé : la langue du modèle, sa longueur conseillée (pas pour un clip, dont la
/// longueur suit la durée).
fn attentes(contexte: &Contexte, i: &Intention) -> Attentes {
    let (wf, regle) = cible(contexte, i.workflow.as_deref());
    Attentes {
        mots_max: if i.genre == "clip" {
            None
        } else {
            prompt::mots_max_de(wf.as_ref(), regle.as_ref())
        },
        anglais: prompt::lit_l_anglais(wf.as_ref()),
    }
}

// --- consignes --------------------------------------------------------------------------------------------------------

/// La consigne de la vérification : celle de Réglages pour ce GGUF, sinon celle d'origine ; les règles du modèle
/// cible pour le prompt corrigé ; la clause verrouillée sur les mineurs toujours à la fin.
pub fn consigne_verification(
    reecrite: Option<&str>,
    workflow: Option<&Workflow>,
    attentes: &Attentes,
) -> String {
    let base = reecrite
        .filter(|c| !c.trim().is_empty())
        .unwrap_or(CONSIGNE_VERIFICATION)
        .trim_end();
    let mut cible = vec![format!(
        "Le prompt corrigé est écrit pour {} : {}.",
        workflow
            .and_then(|w| w.fiche.get("nom").and_then(Value::as_str))
            .unwrap_or("le même modèle"),
        if attentes.anglais {
            "en anglais, même si l'idée est en français"
        } else {
            "dans la langue du prompt d'origine"
        }
    )];
    if let Some(m) = attentes.mots_max {
        cible.push(format!("Il fait au plus {m} mots."));
    }
    let regles = prompt::bloc_modele(&prompt::sections_prompt(workflow));
    if !regles.is_empty() {
        cible.push(regles);
    }
    format!("{base}\n\n{}\n\n{CLAUSE_MINEURS}", cible.join("\n"))
}

/// La consigne du choix : celle de Réglages pour ce GGUF, sinon celle d'origine ; le nombre d'images à retenir ;
/// la clause verrouillée.
pub fn consigne_choix(reecrite: Option<&str>, nombre: usize) -> String {
    let base = reecrite
        .filter(|c| !c.trim().is_empty())
        .unwrap_or(CONSIGNE_CHOIX)
        .trim_end();
    let texte = if base.contains("{nombre}") {
        base.replace("{nombre}", &nombre.to_string())
    } else {
        format!("{base}\n\nRetiens de une à {nombre} images au plus.")
    };
    format!("{texte}\n\n{CLAUSE_MINEURS}")
}

// --- vérifier --------------------------------------------------------------------------------------------------------

/// La vérification d'une création, gardée sur sa fiche (`verification`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    pub fidele: bool,
    pub verdict: String,
    pub ecarts: Vec<String>,
    /// Le prompt corrigé, si l'image n'est pas fidèle.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Ce que le contrôle qualité a trouvé (réponse redemandée, prompt resté imparfait).
    #[serde(default)]
    pub alertes: Vec<String>,
    pub llm: String,
    pub date: String,
}

/// Une création vérifiée, ou pourquoi elle ne l'a pas été.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verifiee {
    pub id: String,
    pub verification: Option<Verification>,
    pub erreur: Option<String>,
}

/// Le résultat d'une vérification de lot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verifications {
    pub resultats: Vec<Verifiee>,
    pub llm: String,
    pub duree_s: f64,
}

impl Verifications {
    /// En une ligne, pour la file du Plateau.
    pub fn resume(&self) -> String {
        let fideles = self
            .resultats
            .iter()
            .filter(|r| r.verification.as_ref().is_some_and(|v| v.fidele))
            .count();
        let ecarts = self
            .resultats
            .iter()
            .filter(|r| r.verification.as_ref().is_some_and(|v| !v.fidele))
            .count();
        let erreurs = self.resultats.iter().filter(|r| r.erreur.is_some()).count();
        let mut morceaux = vec![format!("{fideles} fidèle{}", if fideles > 1 { "s" } else { "" })];
        if ecarts > 0 {
            morceaux.push(format!("{ecarts} avec des écarts (prompt corrigé proposé)"));
        }
        if erreurs > 0 {
            morceaux.push(format!(
                "{erreurs} non vérifiée{}",
                if erreurs > 1 { "s" } else { "" }
            ));
        }
        format!("Vérification : {}", morceaux.join(", "))
    }
}

fn ids_valides(ids: &[String], min: usize) -> Result<Vec<String>, String> {
    let mut uniques: Vec<String> = Vec::new();
    for id in ids.iter().filter(|i| !i.is_empty()) {
        if !uniques.contains(id) {
            uniques.push(id.clone());
        }
    }
    if !(min..=IMAGES_MAX).contains(&uniques.len()) {
        return Err(format!("De {min} à {IMAGES_MAX} images à la fois"));
    }
    Ok(uniques)
}

fn nom_de(lancement: &Lancement) -> String {
    lancement
        .modele
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn maintenant() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Les défauts d'une vérification lue : un prompt corrigé qui manque, ou qui rate le contrôle qualité.
fn defauts_de(lue: &Lue, idee: &str, attentes: &Attentes) -> Vec<String> {
    if lue.fidele {
        return Vec::new();
    }
    if lue.prompt.is_empty() {
        return vec!["le prompt corrigé manque alors que l'image n'est pas fidèle".to_owned()];
    }
    prompt::defauts(&lue.prompt, idee, attentes)
        .iter()
        .map(|d| format!("prompt corrigé : {}", d.pour_toi()))
        .collect()
}

/// Ce qui ne dépend pas de l'image : le LLM lancé, son nom, ses réglages d'écriture.
struct Regard {
    lancement: Lancement,
    nom: String,
    temperature: f64,
    echantillonnage: Map<String, Value>,
    max_tokens: u32,
}

impl Regard {
    fn de(contexte: &Contexte, nom: &str) -> Result<Self, String> {
        let lancement = llm::lancement(&contexte.lieux(), Some(nom))?;
        if lancement.vision.is_none() {
            return Err(SANS_VISION.to_owned());
        }
        let regle = llm::valeurs(&contexte.dossier, nom);
        let (temperature, echantillonnage) = regle.ecriture(false);
        Ok(Self {
            nom: nom_de(&lancement),
            lancement,
            temperature,
            echantillonnage,
            max_tokens: regle.max_tokens.max(900),
        })
    }

    /// Une demande au LLM, sans réflexion (il regarde et répond court).
    fn demande(&self, messages: &[Value]) -> llama::Demande {
        llama::Demande {
            messages: Value::Array(messages.to_vec()),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            reflechir: false,
            echantillonnage: self.echantillonnage.clone(),
            ..llama::Demande::default()
        }
    }
}

/// Pourquoi une création n'a pas été vérifiée : à cause d'elle (le lot continue), ou du LLM (le lot s'arrête :
/// annulation, LLM qui ne répond pas).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Echec {
    Image(String),
    Lot(String),
}

impl From<String> for Echec {
    fn from(e: String) -> Self {
        Self::Image(e)
    }
}

/// Vérifie une création avec le LLM déjà choisi, et l'écrit sur sa fiche.
#[allow(clippy::too_many_lines)] // la mécanique d'écriture du Scénario, gardée d'un tenant
fn verifier_une(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    regard: &Regard,
    id: &str,
) -> Result<Verification, Echec> {
    let i = intention(contexte, id)?;
    let vue: Vue = crate::scenario::vue(contexte, id)?;
    // AS-18 : l'intention et les textes d'origine (une image née d'un mineur le reste).
    let origines = creations::textes_origine(&contexte.data, &[id.to_owned()])?;
    let mut entrees: Vec<&str> = vec![i.idee.as_str(), i.prompt.as_str()];
    entrees.extend(origines.iter().map(String::as_str));
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    filtre.verifier(&entrees).map_err(|e| e.to_string())?;

    let (wf, _) = cible(contexte, i.workflow.as_deref());
    let attentes = attentes(contexte, &i);
    let reecrite = llm::consigne(&contexte.dossier, &regard.nom, "verification");
    let systeme = consigne_verification(reecrite.as_deref(), wf.as_ref(), &attentes);
    let quoi = if i.genre == "clip" {
        "\n(L'image est la première image d'un clip : ne juge que ce qu'une image fixe peut montrer.)"
    } else {
        ""
    };
    let texte = format!("{}{quoi}\n\nVérifie l'image jointe.", ligne_intention(&i));
    let contenu = Value::Array(vec![
        prompt::image_url(&vue.png),
        json!({"type": "text", "text": texte}),
    ]);
    let mut messages = vec![
        json!({"role": "system", "content": systeme}),
        json!({"role": "user", "content": contenu}),
    ];

    let mut meilleur: Option<(Lue, Vec<String>)> = None;
    let mut redemandee: Option<String> = None;
    let mut derniere_erreur = "Réponse du LLM illisible".to_owned();
    for essai in 0..2 {
        let demande = regard.demande(&messages);
        let reponse = match generateur.ecrire(regard.lancement.clone(), &demande) {
            Ok(r) => r,
            // Une annulation ou un LLM qui ne répond pas arrêtent le lot ; une panne au second essai garde le premier.
            Err(e) if essai == 0 || e == crate::comfy_serveur::ANNULEE => return Err(Echec::Lot(e)),
            Err(_) => break,
        };
        let problemes: Vec<String> = match lire_verification(&reponse.texte) {
            Err(e) if e.refus => return Err(Echec::Image(e.message)),
            Err(e) => {
                derniere_erreur = e.message;
                vec!["la réponse n'est pas l'objet JSON demandé".to_owned()]
            }
            Ok(mut lue) => {
                lue.prompt = prompt::garde_cjk(&lue.prompt, &entrees);
                let manques = defauts_de(&lue, &i.idee, &attentes);
                if meilleur.as_ref().is_none_or(|(_, m)| manques.len() < m.len()) {
                    meilleur = Some((lue, manques.clone()));
                }
                manques
            }
        };
        if problemes.is_empty() || essai == 1 {
            break;
        }
        redemandee = Some(problemes.join(" ; "));
        messages.push(json!({"role": "assistant", "content": reponse.texte}));
        messages.push(json!({"role": "user", "content": format!(
            "Ta réponse a ces défauts : {}. Recommence en les corrigeant. Réponds UNIQUEMENT par l'objet JSON demandé.",
            problemes.join(" ; ")
        )}));
    }
    let (lue, restants) = meilleur.ok_or(derniere_erreur)?;

    // AS-18 : tout ce que le LLM a écrit, relu avec ce qui l'a nourri.
    let mut controles = entrees.clone();
    controles.push(lue.verdict.as_str());
    controles.extend(lue.ecarts.iter().map(String::as_str));
    controles.push(lue.prompt.as_str());
    filtre.verifier(&controles).map_err(|e| e.to_string())?;

    let mut alertes = Vec::new();
    if let Some(m) = &redemandee {
        alertes.push(format!("Contrôle qualité : réponse redemandée au LLM ({m})"));
    }
    alertes.extend(restants.iter().map(|r| format!("Contrôle qualité : {r}")));
    let verification = Verification {
        fidele: lue.fidele,
        verdict: lue.verdict,
        ecarts: lue.ecarts,
        prompt: Some(lue.prompt).filter(|p| !p.is_empty()),
        alertes,
        llm: regard.nom.clone(),
        date: maintenant(),
    };
    let mut fiche = creations::fiche(&contexte.data, id)?;
    if let Some(o) = fiche.as_object_mut() {
        o.insert("verification".to_owned(), json!(verification));
    }
    creations::ecrire_fiche(&contexte.data, &fiche)?;
    Ok(verification)
}

/// Vérifie des créations, d'affilée, sur un seul lancement du LLM. `avancer(fait, total)` est appelé avant chaque
/// image ; il rend `false` pour arrêter (annulation). Une création qui ne peut pas être vérifiée (image importée,
/// refus du filtre…) est notée comme telle, les autres continuent.
///
/// # Errors
/// Lot invalide, aucun LLM qui voit, annulation, ou LLM qui ne démarre pas.
pub fn verifier(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    ids: &[String],
    llm_choisi: Option<&str>,
    mut avancer: impl FnMut(usize, usize) -> bool,
) -> Result<Verifications, String> {
    let ids = ids_valides(ids, 1)?;
    crate::scenario::verifier_llm(contexte, llm_choisi)?;
    let premier = ids.iter().find_map(|id| intention(contexte, id).ok());
    let nom = llm_qui_voit(
        contexte,
        llm_choisi,
        premier.as_ref().and_then(|i| i.workflow.as_deref()),
    )?;
    let regard = Regard::de(contexte, &nom)?;
    let debut = Instant::now();
    let mut resultats = Vec::new();
    for (k, id) in ids.iter().enumerate() {
        if !avancer(k, ids.len()) {
            return Err(crate::comfy_serveur::ANNULEE.to_owned());
        }
        match verifier_une(contexte, generateur, &regard, id) {
            Ok(v) => resultats.push(Verifiee {
                id: id.clone(),
                verification: Some(v),
                erreur: None,
            }),
            Err(Echec::Lot(e)) => return Err(e),
            Err(Echec::Image(e)) => resultats.push(Verifiee {
                id: id.clone(),
                verification: None,
                erreur: Some(e),
            }),
        }
    }
    Ok(Verifications {
        resultats,
        llm: regard.nom,
        duree_s: (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0,
    })
}

// --- choisir les meilleures ---------------------------------------------------------------------------------------

/// Une image retenue, gardée sur sa fiche (`meilleure`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meilleure {
    /// Son rang parmi les retenues (1 : la meilleure).
    pub rang: usize,
    pub raison: String,
    /// Le lot comparé.
    pub lot: Vec<String>,
    pub llm: String,
    pub date: String,
}

/// Une image retenue, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Retenue {
    pub id: String,
    pub rang: usize,
    pub raison: String,
}

/// Le résultat d'un choix.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choix {
    pub meilleures: Vec<Retenue>,
    /// La réponse a été redemandée au LLM, et pourquoi.
    pub redemandee: Option<String>,
    pub llm: String,
    pub duree_s: f64,
}

/// Le message du choix : l'intention (une fois si elle est commune), puis les images numérotées.
fn message_choix(intentions: &[Intention], vues: &[Vue]) -> Value {
    let commune = intentions
        .windows(2)
        .all(|p| p[0].prompt == p[1].prompt && p[0].idee == p[1].idee);
    let texte = if commune {
        intentions.first().map(ligne_intention).unwrap_or_default()
    } else {
        intentions
            .iter()
            .enumerate()
            .map(|(k, i)| format!("Image {} :\n{}", k + 1, ligne_intention(i)))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let mut contenu: Vec<Value> = Vec::new();
    for (k, v) in vues.iter().enumerate() {
        contenu.push(json!({"type": "text", "text": format!("Image {}", k + 1)}));
        contenu.push(prompt::image_url(&v.png));
    }
    contenu.push(json!({"type": "text", "text": format!(
        "{texte}\n\nCompare les {} images et choisis les meilleures.",
        vues.len()
    )}));
    Value::Array(contenu)
}

/// Choisit, dans un lot de 2 à 8 créations, celles qui respectent le mieux l'intention, et le note sur leur fiche.
///
/// # Errors
/// Lot invalide, création sans intention, aucun LLM qui voit, filtre AS-18, réponse illisible ou refus, annulation,
/// ou LLM qui échoue.
pub fn choisir(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    ids: &[String],
    llm_choisi: Option<&str>,
) -> Result<Choix, String> {
    let ids = ids_valides(ids, CHOIX_MIN)?;
    crate::scenario::verifier_llm(contexte, llm_choisi)?;
    let intentions = ids
        .iter()
        .map(|id| intention(contexte, id))
        .collect::<Result<Vec<_>, String>>()?;
    let origines = creations::textes_origine(&contexte.data, &ids)?;
    let mut entrees: Vec<&str> = Vec::new();
    for i in &intentions {
        entrees.push(i.idee.as_str());
        entrees.push(i.prompt.as_str());
    }
    entrees.extend(origines.iter().map(String::as_str));
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    filtre.verifier(&entrees).map_err(|e| e.to_string())?;
    let vues = ids
        .iter()
        .map(|id| crate::scenario::vue(contexte, id))
        .collect::<Result<Vec<_>, String>>()?;

    let nom = llm_qui_voit(
        contexte,
        llm_choisi,
        intentions.first().and_then(|i| i.workflow.as_deref()),
    )?;
    let regard = Regard::de(contexte, &nom)?;
    let nombre = retenues_max(ids.len());
    let reecrite = llm::consigne(&contexte.dossier, &regard.nom, "choix");
    let mut messages = vec![
        json!({"role": "system", "content": consigne_choix(reecrite.as_deref(), nombre)}),
        json!({"role": "user", "content": message_choix(&intentions, &vues)}),
    ];
    let debut = Instant::now();
    let mut retenues: Vec<(usize, String)> = Vec::new();
    let mut redemandee: Option<String> = None;
    let mut derniere_erreur = "Réponse du LLM illisible".to_owned();
    for essai in 0..2 {
        let demande = regard.demande(&messages);
        let reponse = match generateur.ecrire(regard.lancement.clone(), &demande) {
            Ok(r) => r,
            Err(e) if essai == 0 || e == crate::comfy_serveur::ANNULEE => return Err(e),
            Err(_) => break,
        };
        let probleme = match lire_choix(&reponse.texte, ids.len()) {
            Err(e) if e.refus => return Err(e.message),
            Err(e) => {
                derniere_erreur = e.message;
                Some("la réponse n'est pas l'objet JSON demandé".to_owned())
            }
            Ok(lues) if lues.is_empty() => {
                derniere_erreur = "Le LLM n'a retenu aucune image".to_owned();
                Some(format!("aucune image retenue (numéros de 1 à {})", ids.len()))
            }
            Ok(mut lues) => {
                lues.truncate(nombre);
                retenues = lues;
                None
            }
        };
        let Some(p) = probleme else { break };
        if essai == 1 {
            break;
        }
        redemandee = Some(p.clone());
        messages.push(json!({"role": "assistant", "content": reponse.texte}));
        messages.push(json!({"role": "user", "content": format!(
            "Ta réponse a ce défaut : {p}. Recommence. Réponds UNIQUEMENT par l'objet JSON demandé."
        )}));
    }
    if retenues.is_empty() {
        return Err(derniere_erreur);
    }
    let mut controles = entrees.clone();
    controles.extend(retenues.iter().map(|(_, r)| r.as_str()));
    filtre.verifier(&controles).map_err(|e| e.to_string())?;

    // Sur les fiches : le choix précédent du lot s'efface, les retenues portent leur rang et leur raison.
    let date = maintenant();
    for (k, id) in ids.iter().enumerate() {
        let mut fiche = creations::fiche(&contexte.data, id)?;
        if let Some(o) = fiche.as_object_mut() {
            o.remove("meilleure");
            if let Some(rang) = retenues.iter().position(|(n, _)| *n == k + 1) {
                let raison = retenues.get(rang).map(|(_, r)| r.clone()).unwrap_or_default();
                o.insert(
                    "meilleure".to_owned(),
                    json!(Meilleure {
                        rang: rang + 1,
                        raison,
                        lot: ids.clone(),
                        llm: regard.nom.clone(),
                        date: date.clone(),
                    }),
                );
            }
        }
        creations::ecrire_fiche(&contexte.data, &fiche)?;
    }
    let meilleures = retenues
        .iter()
        .enumerate()
        .filter_map(|(rang, (n, raison))| {
            ids.get(n - 1).map(|id| Retenue {
                id: id.clone(),
                rang: rang + 1,
                raison: raison.clone(),
            })
        })
        .collect();
    Ok(Choix {
        meilleures,
        redemandee,
        llm: regard.nom,
        duree_s: (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0,
    })
}

// --- relancer avec le prompt corrigé ---------------------------------------------------------------------------------

/// La demande à remettre en file : une image du Plateau, ou un clip du Montage.
#[derive(Debug, Clone, PartialEq)]
pub enum Relance {
    Image(Demande),
    Clip(DemandeClip),
}

/// La même demande que la création, avec ce prompt (tourné tel quel, une nouvelle seed) : même workflow,
/// checkpoint, format, LoRA, références et dossier.
///
/// # Errors
/// Création inconnue, prompt vide, ou création qui ne se relance pas d'ici (retouche, plan de séquence, import).
pub fn relance(contexte: &Contexte, id: &str, prompt_corrige: &str) -> Result<Relance, String> {
    let prompt_corrige = prompt_corrige.trim();
    if prompt_corrige.is_empty() {
        return Err("Le prompt corrigé est vide".to_owned());
    }
    let f = creations::fiche(&contexte.data, id)?;
    let texte = |cle: &str| {
        f.get(cle)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    let loras: Vec<ChoixLora> = f
        .get("loras")
        .filter(|l| !l.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| format!("LoRA de la fiche illisibles : {e}"))?
        .unwrap_or_default();
    match texte("type").as_deref() {
        Some("creation") => {
            let references: Vec<String> = f
                .get("references")
                .and_then(Value::as_array)
                .map(|l| l.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default();
            Ok(Relance::Image(Demande {
                prompt: prompt_corrige.to_owned(),
                reecrire: false,
                llm: None,
                workflow: texte("workflow"),
                format: texte("format").unwrap_or_default(),
                seed: None,
                nombre: 1,
                modele: texte("modele"),
                mode_reference: if references.is_empty() {
                    None
                } else {
                    texte("mode_reference")
                },
                references,
                liberte: f.get("liberte").and_then(Value::as_f64).unwrap_or(LIBERTE_DEFAUT),
                loras,
                dossier: texte("dossier"),
                verifier: false,
            }))
        }
        Some("clip") => {
            let mode = texte("mode_video").unwrap_or_default();
            if mode != "animer" && mode != "texte" {
                return Err("Un plan ou un film de séquence se retourne depuis sa séquence, au Montage".to_owned());
            }
            let format = if mode == "animer" {
                texte("cadrage")
                    .or_else(|| texte("format"))
                    .unwrap_or_else(|| crate::retouche::FORMAT_IMAGE.to_owned())
            } else {
                texte("format").unwrap_or_default()
            };
            Ok(Relance::Clip(DemandeClip {
                idee: texte("idee").unwrap_or_default(),
                prompt: prompt_corrige.to_owned(),
                mode: mode.clone(),
                source: if mode == "animer" { texte("parent") } else { None },
                qualite: texte("qualite").unwrap_or_else(|| crate::montage::QUALITE_DEFAUT.to_owned()),
                duree: f
                    .get("duree")
                    .and_then(Value::as_u64)
                    .and_then(|d| u32::try_from(d).ok())
                    .unwrap_or(crate::montage::DUREES[0]),
                format,
                seed: None,
                nombre: 1,
                modele: texte("modele"),
                loras,
                ecrit_par: f
                    .pointer("/verification/llm")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                dossier: texte("dossier"),
            }))
        }
        Some("retouche" | "agrandissement") => Err(
            "Une retouche se relance depuis la page Retouche (le masque peint n'est pas gardé) : copie le prompt corrigé \
             dans sa consigne"
                .to_owned(),
        ),
        _ => Err("Cette création ne se relance pas d'ici".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dossier;
    use crate::comfy_serveur::Fichier;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    struct Faux {
        reponses: Vec<Result<String, String>>,
        demandes: Vec<llama::Demande>,
    }

    impl Generateur for Faux {
        fn ecrire(&mut self, _: Lancement, demande: &llama::Demande) -> Result<llama::Reponse, String> {
            self.demandes.push(demande.clone());
            let texte = if self.reponses.is_empty() {
                Ok(String::new())
            } else {
                self.reponses.remove(0)
            }?;
            Ok(llama::Reponse {
                texte,
                ..llama::Reponse::default()
            })
        }

        fn televerser(&mut self, _: &str, _: &[u8]) -> Result<String, String> {
            Err("pas d'image ici".to_owned())
        }

        fn generer(
            &mut self,
            _: &Value,
            _: &mut dyn FnMut(u64, u64),
            _: &AtomicBool,
        ) -> Result<Vec<Fichier>, String> {
            Err("pas d'image ici".to_owned())
        }
    }

    fn faux(reponses: &[&str]) -> Faux {
        Faux {
            reponses: reponses.iter().map(|r| Ok((*r).to_owned())).collect(),
            demandes: Vec::new(),
        }
    }

    /// Un LLM installé, avec (ou sans) le projecteur de vision de sa famille, et son serveur.
    fn contexte(temp: &tempfile::TempDir, vision: bool) -> Contexte {
        let modeles = temp.path().join("modeles");
        fs::create_dir_all(modeles.join("llm")).unwrap();
        fs::write(modeles.join("llm/mistral.gguf"), crate::gguf::essai::gguf(&[12])).unwrap();
        if vision {
            fs::write(modeles.join("llm/mistral-mmproj-f16.gguf"), b"projecteur").unwrap();
        }
        Contexte {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles,
            moteurs: temp.path().join("moteurs"),
            port_llama: 8095,
        }
    }

    fn png() -> Vec<u8> {
        crate::images::png(&image::RgbImage::new(64, 48)).unwrap()
    }

    fn creation(ctx: &Contexte, prompt: &str) -> String {
        let infos = json!({
            "type": "creation", "idee": "un renard dans la neige", "prompt": prompt,
            "workflow": "krea2_turbo", "format": "1:1 (Square)", "dossier": null,
        });
        creations::enregistrer(&ctx.data, &png(), infos.as_object().unwrap()).unwrap()
    }

    const PROMPT_CORRIGE: &str = "A red fox sitting in fresh snow at golden hour, its breath visible in the cold air, \
         pine trees in the background, gentle snowflakes falling, shallow depth of field, a calm winter mood, a detailed \
         natural photograph with fine texture on the fur and the snow, the fox looking straight at the camera";

    #[test]
    fn lecture_d_une_verification() {
        let v = lire_verification(
            r#"Voici : {"fidele": false, "verdict": "Le renard est gris.", "ecarts": ["renard gris au lieu de roux", ""], "prompt": "A red fox."}"#,
        )
        .unwrap();
        assert_eq!(
            (v.fidele, v.verdict.as_str(), v.ecarts.len(), v.prompt.as_str()),
            (false, "Le renard est gris.", 1, "A red fox.")
        );
        // Fidèle : le prompt proposé est ignoré.
        let v =
            lire_verification(r#"{"fidele": "oui", "verdict": "Conforme.", "ecarts": [], "prompt": "x"}"#)
                .unwrap();
        assert!(v.fidele && v.prompt.is_empty());
        assert!(lire_verification("").is_err());
        assert!(lire_verification(r#"{"autre": 1}"#).is_err());
        assert!(
            lire_verification("I'm sorry, I can't help with that.")
                .unwrap_err()
                .refus
        );
    }

    #[test]
    fn lecture_d_un_choix() {
        let lues = lire_choix(
            r#"{"meilleures": [{"image": 3, "raison": "le texte est juste"}, {"image": "Image 1"}, {"image": 3}, {"image": 9}]}"#,
            4,
        )
        .unwrap();
        assert_eq!(lues, [(3, "le texte est juste".to_owned()), (1, String::new())]);
        assert_eq!((retenues_max(2), retenues_max(4), retenues_max(8)), (1, 2, 3));
    }

    #[test]
    fn consignes_et_clause() {
        let a = Attentes {
            mots_max: Some(120),
            anglais: true,
        };
        let c = consigne_verification(None, None, &a);
        assert!(c.contains("en anglais") && c.contains("120 mots") && c.ends_with(CLAUSE_MINEURS));
        let reecrite = consigne_verification(Some("Ma consigne."), None, &a);
        assert!(reecrite.starts_with("Ma consigne.") && reecrite.ends_with(CLAUSE_MINEURS));
        assert!(consigne_choix(None, 2).contains("De une à 2 images"));
        assert!(consigne_choix(Some("Choisis."), 3).contains("de une à 3 images"));
    }

    #[test]
    fn llm_qui_voit_ou_pas() {
        let temp = tempfile::tempdir().unwrap();
        let sans = contexte(&temp, false);
        assert_eq!(llm_qui_voit(&sans, None, None).unwrap_err(), SANS_VISION);
        assert!(etat_vision(&sans, None).llm.is_none());
        assert!(
            llm_qui_voit(&sans, Some("mistral.gguf"), None)
                .unwrap_err()
                .contains("ne voit pas")
        );
        let temp = tempfile::tempdir().unwrap();
        let avec = contexte(&temp, true);
        assert_eq!(
            llm_qui_voit(&avec, None, Some("krea2_turbo")).unwrap(),
            "mistral.gguf"
        );
        assert_eq!(etat_vision(&avec, None).llm.as_deref(), Some("mistral.gguf"));
    }

    #[test]
    fn verifier_et_relancer() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, true);
        let id = creation(&ctx, "A fox in the snow.");
        // Premier essai : prompt corrigé trop court, redemandé ; le second passe.
        let mut g = faux(&[
            r#"{"fidele": false, "verdict": "Le renard est gris.", "ecarts": ["renard gris"], "prompt": "A red fox."}"#,
            format!(
                r#"{{"fidele": false, "verdict": "Le renard est gris.", "ecarts": ["renard gris"], "prompt": "{PROMPT_CORRIGE}"}}"#
            )
            .as_str(),
        ]);
        let v = verifier(&ctx, &mut g, std::slice::from_ref(&id), None, |_, _| true).unwrap();
        assert_eq!(g.demandes.len(), 2);
        let r = &v.resultats[0];
        let verification = r.verification.as_ref().unwrap();
        assert!(!verification.fidele && verification.prompt.as_deref() == Some(PROMPT_CORRIGE));
        assert!(verification.alertes[0].contains("redemandée"));
        assert_eq!(
            v.resume(),
            "Vérification : 0 fidèle, 1 avec des écarts (prompt corrigé proposé)"
        );
        // Sur la fiche.
        let fiche = creations::fiche(&ctx.data, &id).unwrap();
        assert_eq!(fiche["verification"]["fidele"], json!(false));
        assert_eq!(fiche["verification"]["llm"], json!("mistral.gguf"));
        // La même demande, avec le prompt corrigé.
        let Relance::Image(d) = relance(&ctx, &id, PROMPT_CORRIGE).unwrap() else {
            panic!("une image attendue");
        };
        assert_eq!(
            (
                d.prompt.as_str(),
                d.workflow.as_deref(),
                d.format.as_str(),
                d.reecrire,
                d.seed
            ),
            (PROMPT_CORRIGE, Some("krea2_turbo"), "1:1 (Square)", false, None)
        );
        assert!(relance(&ctx, &id, "  ").is_err());
    }

    #[test]
    fn verifier_s_arrete_ou_continue() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, true);
        let un = creation(&ctx, "A fox in the snow.");
        let importee = creations::enregistrer(
            &ctx.data,
            &png(),
            json!({"type": "import", "idee": "", "prompt": ""})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        // Une image importée n'a pas d'intention : notée, le reste continue.
        let mut g = faux(&[r#"{"fidele": true, "verdict": "Conforme.", "ecarts": []}"#]);
        let v = verifier(&ctx, &mut g, &[un.clone(), importee], None, |_, _| true).unwrap();
        assert!(v.resultats[0].verification.as_ref().unwrap().fidele);
        assert!(v.resultats[1].erreur.as_deref().unwrap().contains("importée"));
        assert_eq!(v.resume(), "Vérification : 1 fidèle, 1 non vérifiée");
        // ✕ Annuler.
        let mut g = faux(&[]);
        assert_eq!(
            verifier(&ctx, &mut g, std::slice::from_ref(&un), None, |_, _| false).unwrap_err(),
            crate::comfy_serveur::ANNULEE
        );
        // Sans vision : rien ne part au LLM.
        let temp = tempfile::tempdir().unwrap();
        let sans = contexte(&temp, false);
        let id = creation(&sans, "A fox.");
        let mut g = faux(&[]);
        assert_eq!(
            verifier(&sans, &mut g, &[id], None, |_, _| true).unwrap_err(),
            SANS_VISION
        );
        assert!(g.demandes.is_empty());
        assert!(verifier(&ctx, &mut faux(&[]), &[], None, |_, _| true).is_err());
    }

    #[test]
    fn choisir_les_meilleures() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, true);
        let ids: Vec<String> = (0..3).map(|_| creation(&ctx, "A fox in the snow.")).collect();
        // Une réponse sans image valable est redemandée.
        let mut g = faux(&[
            r#"{"meilleures": [{"image": 7}]}"#,
            r#"{"meilleures": [{"image": 2, "raison": "le renard regarde l'objectif"}, {"image": 1, "raison": "la neige"}]}"#,
        ]);
        let c = choisir(&ctx, &mut g, &ids, None).unwrap();
        assert!(c.redemandee.is_some());
        assert_eq!(
            c.meilleures
                .iter()
                .map(|m| (m.id.as_str(), m.rang))
                .collect::<Vec<_>>(),
            [(ids[1].as_str(), 1), (ids[0].as_str(), 2)]
        );
        let fiche = |id: &str| creations::fiche(&ctx.data, id).unwrap();
        assert_eq!(fiche(&ids[1])["meilleure"]["rang"], json!(1));
        assert!(fiche(&ids[2]).get("meilleure").is_none());
        // Un nouveau choix sur le lot efface l'ancien.
        let mut g = faux(&[r#"{"meilleures": [{"image": 3, "raison": "x"}]}"#]);
        choisir(&ctx, &mut g, &ids, None).unwrap();
        assert!(fiche(&ids[1]).get("meilleure").is_none());
        assert_eq!(fiche(&ids[2])["meilleure"]["rang"], json!(1));
        // Une seule image : rien à comparer.
        assert!(choisir(&ctx, &mut faux(&[]), &ids[..1], None).is_err());
    }
}
