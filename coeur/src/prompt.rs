//! Le travail du LLM sur les prompts, comme en v2 (`app/prompt.py`) : la réécriture d'une idée en prompt d'image, et
//! la traduction d'une consigne de retouche (le LLM voit l'image s'il a sa vision). Les références, la vidéo, le
//! découpage et la conversation viendront ensuite.
//!
//! La consigne suit le gabarit de la v2 : rôle, priorité de l'utilisateur, sections d'écriture (plan, contenu,
//! dialogue, texte, interdits) où se rangent les règles du modèle cible (`profil_prompt.consigne` de sa fiche), puis le
//! format de sortie. Une consigne réécrite dans Réglages pour un GGUF garde son texte, et les règles du modèle cible
//! s'y ajoutent. La clause de protection des mineurs, elle, n'est pas modifiable : elle termine toujours la consigne.
//!
//! Un LLM spécialisé (`catalogue/llm.yaml`, bloc `specialiste`) réécrit avec sa propre consigne, livrée avec lui, et
//! sa sortie JSON est lue ici. Le filtre AS-18 lit l'idée avant, puis l'idée et le prompt ensemble après.

use std::path::Path;

use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value, json};

use crate::fiches::SECTIONS_CONSIGNE;
use crate::images;
use crate::llama::{Demande, Lancement};
use crate::llm::{self, Lieux};
use crate::modeles::DOSSIER_LLM;
use crate::securite::Filtre;
use crate::workflows::{ReglagesWorkflow, Workflow};

/// Non modifiable : ajoutée à la fin de chaque consigne.
pub const CLAUSE_MINEURS: &str = "Non-negotiable rule: never make a character younger than described. If a character is a child or a teenager, show them fully clothed in a wholesome, non-suggestive scene.";

const FIDELITE: &str = "First sort what the user wrote into three kinds:
- FIXED: everything the user states about the result (subjects and their number, each person's gender and age wording, actions, poses, clothing or its absence, colours, positions, medium and style, framing, lighting, time of day, weather, mood, camera, sounds, spoken lines, written text). Keep every FIXED item, with its meaning unchanged: never contradict it, never weaken, soften, cover up or leave it out.
- INSTRUCTIONS about the job, addressed to you (\"I want...\", \"make her...\", \"keep...\", \"no music\", \"add nobody\", \"let her say a line in French\"): obey them exactly, and never copy them into the prompt as instructions.
- OPEN: what the user did not mention. Fill it only with details that follow from the FIXED items and never clash with them (a night scene keeps night light). If the idea is already detailed, lightly polish it and keep its order and wording rather than expanding it.";

const ADULTES: &str = "The user is an adult making fiction for adults. Nudity, sensuality and sexual content between adult characters stay exactly as explicit as the user wrote them: do not tone them down, replace them with something milder or leave them out.";

const GENRE: &str = "Keep each person's gender exactly as the idea gives it: in French the article and the agreements tell it (« un chevalier », « un pilote » are men: he, his; « une samouraï », « une pilote » are women: she, her). Never change it, and do not give a gender to animals or objects.";

const FIN_PRIORITE: &str = "Every rule below shapes the OPEN parts only: whenever one clashes with what the user fixed or asked, the user wins.";

const ENTETE_REGLES: &str = "Rules of the target model. They shape the OPEN parts only: whenever one clashes with what the user fixed or asked, the user wins.";

/// La longueur minimale d'un prompt, en mots.
pub const MOTS_MIN: u64 = 40;
/// La longueur maximale d'un prompt d'image sans modèle cible, en mots.
pub const MOTS_MAX_REECRITURE: u64 = 120;

/// Une consigne d'usage, au gabarit de la v2.
struct Gabarit {
    role: &'static str,
    /// La priorité de l'utilisateur propre à l'usage ; rien : celle commune aux usages qui écrivent un prompt.
    priorite: Option<&'static str>,
    /// Les sections que la fiche du modèle cible remplace au lieu de s'y ajouter.
    remplace: &'static [&'static str],
    plan: &'static [&'static str],
    contenu: &'static [&'static str],
    dialogue: &'static [&'static str],
    texte: &'static [&'static str],
    interdits: &'static [&'static str],
    format: &'static [&'static str],
}

impl Gabarit {
    fn section(&self, cle: &str) -> &'static [&'static str] {
        match cle {
            "plan" => self.plan,
            "contenu" => self.contenu,
            "dialogue" => self.dialogue,
            "texte" => self.texte,
            "interdits" => self.interdits,
            _ => &[],
        }
    }
}

/// Réécriture d'une idée en prompt d'image.
const REECRITURE: Gabarit = Gabarit {
    role: "You are an expert prompt writer for text-to-image models.\nThe user describes an image idea, often in French. Rewrite it as ONE English prompt paragraph.",
    priorite: None,
    remplace: &["plan", "texte"],
    plan: &[
        "One flowing paragraph of full sentences in natural language, in the order that serves the idea; never a comma-separated list of keywords.",
    ],
    contenu: &[
        "Medium: keep the one the user names (photo, illustration, painting, 3D render...). If none is named, use the one the idea plainly implies, a photograph for a real-world scene; never switch to painting or illustration on your own.",
        "Make the OPEN parts visual: setting, lighting, composition, in grounded words that serve the idea.",
    ],
    dialogue: &[],
    texte: &[
        "If the idea asks for written text in the image, keep that text exactly, in quotes. Otherwise mention no text.",
    ],
    interdits: &["Add characters, animals or objects the idea does not imply."],
    format: &[
        "{mots_min} to {mots_max} words, plain prose, no lists, no headings, no markdown (no asterisks), no quotes around the whole prompt.",
        "Output only the prompt: no preamble, no explanation, no translation notes.",
    ],
};

/// Retouche : changer franchement ce qui est demandé, garder le reste d'une seule phrase, sans le redécrire (d'après
/// la consigne du réécrivain de retouche de Qwen-Image 2.1).
const RETOUCHE: Gabarit = Gabarit {
    role: "You translate image-editing instructions for an instruction-based image editing model.\nThe user writes an instruction, often in French. Rewrite it as a short English imperative instruction (one or two sentences).",
    priorite: Some("Edit exactly what the user names. Keep the user's own action, target and details."),
    remplace: &["plan", "texte"],
    plan: &[
        "Lead with the change the user names, and make it clear and unmistakable, never faint.",
        "Then everything the user did not name stays as it is: say so in one short clause (\"keep everything else unchanged\"), without re-describing it. If the user names something to keep, say so explicitly.",
    ],
    contenu: &[],
    dialogue: &[],
    texte: &["If the instruction asks for written text, keep that text exactly, in quotes."],
    interdits: &["Add another change, a style or a clean-up the user did not ask for."],
    format: &[
        "One or two sentences.",
        "Output only the instruction: no preamble, no explanation.",
    ],
};

/// Images de référence (« S'inspirer ») : une nouvelle scène, les références citées par leur numéro.
const REFERENCES: Gabarit = Gabarit {
    role: "You write instructions for an image model that receives reference pictures, numbered Picture 1, Picture 2, Picture 3.\nThe user describes, often in French, a new image to create from those references (« image 1 » means Picture 1). Rewrite it as ONE English instruction.",
    priorite: None,
    remplace: &["texte"],
    plan: &[
        "Describe what is new, as the user asks: scene, pose, action, setting, lighting, framing, style.",
    ],
    contenu: &[
        "Refer to the references by their number: \"the woman from Picture 1\", \"the dragon from Picture 2\". Do not re-describe what the references already show (face, hair, clothes, colors): the model sees them.",
        "Ask explicitly to keep the identity (face, hair) and outfit of the characters from the references, unless the user asks to change them, but NOT their pose, seat, props or background: the characters take the new pose and place described by the user.",
    ],
    dialogue: &[],
    texte: &["If the user asks for written text in the image, keep that text exactly, in quotes."],
    interdits: &[],
    format: &[
        "1 to 4 sentences, at most 90 words, plain prose, no markdown.",
        "Output only the instruction: no preamble, no explanation.",
    ],
};

/// Le nom que le modèle cible donne à ses images de référence (`profil_prompt.nom_image` de sa fiche, `<image{n}>`
/// pour Qwen-Image 2.1) ; rien : « Picture N », le nom d'origine.
pub fn nom_image(workflow: Option<&Workflow>) -> Option<String> {
    workflow?
        .fiche
        .pointer("/profil_prompt/nom_image")
        .and_then(Value::as_str)
        .filter(|n| *n != "Picture {n}" && n.contains("{n}"))
        .map(str::to_owned)
}

/// Les « Picture 1, Picture 2… » d'un texte dans le nom que le modèle cible leur donne.
pub fn avec_noms_images(texte: &str, nom_image: Option<&str>) -> String {
    let Some(nom) = nom_image else {
        return texte.to_owned();
    };
    match Regex::new(r"Picture (\d+)") {
        Ok(motif) => motif
            .replace_all(texte, |c: &regex::Captures<'_>| nom.replace("{n}", &c[1]))
            .into_owned(),
        Err(_) => texte.to_owned(),
    }
}

fn titre(cle: &str) -> &str {
    match cle {
        "plan" => "PROMPT OUTLINE",
        "contenu" => "CONTENT AND WORDING",
        "dialogue" => "DIALOGUE",
        "texte" => "VISIBLE TEXT",
        "interdits" => "NEVER",
        autre => autre,
    }
}

/// La priorité de l'utilisateur, commune aux usages qui écrivent un prompt.
fn priorite() -> String {
    format!("{FIDELITE}\n{GENRE}\n{ADULTES}\n{FIN_PRIORITE}")
}

/// Les règles du modèle cible, par section, telles que les donne `profil_prompt` de sa fiche : `consigne.<section>`
/// et, pour une fiche d'avant le gabarit, `regles` à plat (rangées sous « regles »).
pub fn sections_prompt(workflow: Option<&Workflow>) -> Vec<(String, Vec<String>)> {
    let Some(profil) = workflow.and_then(|w| w.fiche.get("profil_prompt")) else {
        return Vec::new();
    };
    let lignes = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .map(|l| {
                l.iter()
                    .map(|r| r.as_str().map_or_else(|| r.to_string(), str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut sections: Vec<(String, Vec<String>)> = SECTIONS_CONSIGNE
        .iter()
        .map(|cle| {
            (
                (*cle).to_owned(),
                lignes(profil.pointer(&format!("/consigne/{cle}"))),
            )
        })
        .filter(|(_, l)| !l.is_empty())
        .collect();
    let regles = lignes(profil.get("regles"));
    if !regles.is_empty() {
        sections.push(("regles".to_owned(), regles));
    }
    sections
}

fn de(sections: &[(String, Vec<String>)], cle: &str) -> Vec<String> {
    sections
        .iter()
        .find(|(c, _)| c == cle)
        .map(|(_, l)| l.clone())
        .unwrap_or_default()
}

fn puces(lignes: &[String]) -> String {
    lignes
        .iter()
        .map(|l| format!("- {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Une section du gabarit. Le plan est numéroté dès qu'il a plusieurs parties : leur ordre compte.
fn section(cle: &str, lignes: &[String]) -> String {
    if cle == "plan" {
        if let [seule] = lignes {
            return format!("{}:\n{seule}", titre(cle));
        }
        let numeros: Vec<String> = lignes
            .iter()
            .enumerate()
            .map(|(n, l)| format!("{}. {l}", n + 1))
            .collect();
        return format!("{}, in this order:\n{}", titre(cle), numeros.join("\n"));
    }
    format!("{}:\n{}", titre(cle), puces(lignes))
}

/// Les sections d'écriture d'un usage, celles du modèle cible rangées dedans : une section de la fiche remplace celle
/// du gabarit quand l'usage la dit remplaçable, et s'y ajoute sinon.
fn sections_usage(gabarit: &Gabarit, regles: &[(String, Vec<String>)]) -> Vec<String> {
    let mut blocs = Vec::new();
    for cle in SECTIONS_CONSIGNE {
        let du_modele = de(regles, cle);
        let du_gabarit: Vec<String> = gabarit.section(cle).iter().map(|l| (*l).to_owned()).collect();
        let lignes = if !du_modele.is_empty() && gabarit.remplace.contains(&cle) {
            du_modele
        } else {
            [du_gabarit, du_modele].concat()
        };
        if !lignes.is_empty() {
            blocs.push(section(cle, &lignes));
        }
    }
    let a_plat = de(regles, "regles");
    if !a_plat.is_empty() {
        blocs.push(format!("{ENTETE_REGLES}\n{}", puces(&a_plat)));
    }
    blocs
}

fn gabarit(g: &Gabarit, regles: &[(String, Vec<String>)]) -> String {
    let format: Vec<String> = g.format.iter().map(|l| (*l).to_owned()).collect();
    let mut blocs = vec![
        format!("ROLE:\n{}", g.role),
        format!(
            "USER PRIORITY:\n{}",
            g.priorite.map_or_else(priorite, str::to_owned)
        ),
    ];
    blocs.extend(sections_usage(g, regles));
    blocs.push(format!("OUTPUT FORMAT:\n{}", puces(&format)));
    blocs.join("\n\n")
}

/// Les règles du modèle cible en un bloc, pour une consigne réécrite dans Réglages : l'en-tête, puis les sections.
/// Vide sans règle.
fn bloc_modele(regles: &[(String, Vec<String>)]) -> String {
    if regles.is_empty() {
        return String::new();
    }
    let a_plat = de(regles, "regles");
    let tete = if a_plat.is_empty() {
        ENTETE_REGLES.to_owned()
    } else {
        format!("{ENTETE_REGLES}\n{}", puces(&a_plat))
    };
    let mut blocs = vec![tete];
    for cle in SECTIONS_CONSIGNE {
        let lignes = de(regles, cle);
        if !lignes.is_empty() {
            blocs.push(section(cle, &lignes));
        }
    }
    blocs.join("\n\n")
}

/// La consigne d'origine d'un usage, telle que Réglages la montre : le gabarit sans modèle cible, longueurs en repères
/// (`{mots_min}`, `{mots_max}`), sans la clause verrouillée ajoutée ensuite. Rien pour un usage pas encore porté.
pub fn consigne_d_origine(usage: &str) -> Option<String> {
    match usage {
        "reecriture" => Some(gabarit(&REECRITURE, &[])),
        "retouche" => Some(gabarit(&RETOUCHE, &[])),
        "references" => Some(gabarit(&REFERENCES, &[])),
        _ => None,
    }
}

/// La consigne de réécriture d'une idée pour ce modèle cible, toujours suivie de la clause verrouillée. `reecrite` :
/// celle de Réglages pour ce GGUF, qui garde son texte (les règles du modèle cible s'y ajoutent).
pub fn consigne_reecriture(
    reecrite: Option<&str>,
    mots_max: Option<u64>,
    regles: &[(String, Vec<String>)],
) -> String {
    consigne(&REECRITURE, reecrite, mots_max, regles)
}

/// La consigne d'une création à partir d'images de référence, les images nommées comme le modèle cible les nomme,
/// toujours suivie de la clause verrouillée. `reecrite` : celle de Réglages pour ce GGUF.
pub fn consigne_references(
    reecrite: Option<&str>,
    regles: &[(String, Vec<String>)],
    nom_image: Option<&str>,
) -> String {
    avec_noms_images(&consigne(&REFERENCES, reecrite, None, regles), nom_image)
}

/// La consigne de traduction d'une instruction de retouche pour ce modèle cible, toujours suivie de la clause
/// verrouillée. `reecrite` : celle de Réglages pour ce GGUF.
pub fn consigne_retouche(reecrite: Option<&str>, regles: &[(String, Vec<String>)]) -> String {
    consigne(&RETOUCHE, reecrite, None, regles)
}

fn consigne(
    g: &Gabarit,
    reecrite: Option<&str>,
    mots_max: Option<u64>,
    regles: &[(String, Vec<String>)],
) -> String {
    let texte = match reecrite {
        Some(propre) => {
            let mut t = propre.trim_end().to_owned();
            let bloc = bloc_modele(regles);
            if !bloc.is_empty() {
                t.push_str("\n\n");
                t.push_str(&bloc);
            }
            t
        }
        None => gabarit(g, regles),
    };
    let texte = texte
        .replace("{mots_max}", &mots_max.unwrap_or(MOTS_MAX_REECRITURE).to_string())
        .replace("{mots_min}", &MOTS_MIN.to_string());
    format!("{texte}\n\n{CLAUSE_MINEURS}")
}

/// La longueur conseillée par le modèle cible : celle de Réglages, sinon celle de sa fiche.
pub fn mots_max_de(workflow: Option<&Workflow>, reglages: Option<&ReglagesWorkflow>) -> Option<u64> {
    let workflow = workflow?;
    reglages
        .and_then(|r| r.get("mots_max"))
        .and_then(Value::as_u64)
        .filter(|m| (20..=1000).contains(m))
        .or_else(|| {
            workflow
                .fiche
                .pointer("/profil_prompt/mots_max")
                .and_then(Value::as_u64)
        })
}

/// Les tokens à prévoir pour un prompt anglais de `mots` mots (≈ 1,3 token par mot, plus une marge).
pub fn tokens_pour_mots(mots: Option<u64>) -> u32 {
    mots.map_or(0, |m| {
        u32::try_from(m.saturating_mul(3) / 2)
            .unwrap_or(u32::MAX)
            .saturating_add(64)
    })
}

/// Le prompt en prose simple : le LLM glisse parfois **gras**, *bruitages* ou titres, que les modèles n'ont pas à lire.
pub fn nettoyer_markdown(texte: &str) -> String {
    let mut texte = texte.to_owned();
    if let Ok(gras) = Regex::new(r"\*{1,2}([^*]+?)\*{1,2}") {
        texte = gras.replace_all(&texte, "$1").into_owned();
    }
    if let Ok(titres) = RegexBuilder::new(r"^\s*#+\s*").multi_line(true).build() {
        texte = titres.replace_all(&texte, "").into_owned();
    }
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cjk() -> Option<Regex> {
    Regex::new(
        "[\u{2e80}-\u{303f}\u{3040}-\u{30ff}\u{3400}-\u{4dbf}\u{4e00}-\u{9fff}\u{ac00}-\u{d7af}\u{f900}-\u{faff}\u{ff00}-\u{ffef}]",
    )
    .ok()
}

/// Le texte sans les caractères chinois, japonais ou coréens que le LLM a ajoutés de lui-même : si l'utilisateur en
/// a écrit, ils restent permis.
pub fn garde_cjk(texte: &str, entrees: &[&str]) -> String {
    let Some(motif) = cjk() else {
        return texte.to_owned();
    };
    if !motif.is_match(texte) || entrees.iter().any(|e| motif.is_match(e)) {
        return texte.to_owned();
    }
    motif
        .replace_all(texte, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Le bloc `specialiste` de ce GGUF s'il sert cet usage (`creation`, `retouche`) pour ce workflow.
pub fn specialiste_pour(modele: &str, usage: &str, workflow: Option<&Workflow>) -> Option<Value> {
    let s = crate::fiches::specialiste(modele)?;
    let id = workflow?.id.as_str();
    let sert = s.get("usage").and_then(Value::as_str) == Some(usage)
        && s.get("workflows")
            .and_then(Value::as_array)
            .is_some_and(|w| w.iter().any(|x| x.as_str() == Some(id)));
    sert.then_some(s)
}

/// Les messages et les paramètres d'un LLM spécialisé : sa consigne livrée (plus la clause verrouillée), l'image à
/// retoucher s'il la voit, et l'échantillonnage de son entraînement.
///
/// # Errors
/// Sa consigne n'est pas dans `modeles\llm`, ou son bloc est incomplet.
pub fn demande_specialiste(
    modeles: &Path,
    s: &Value,
    texte: &str,
    image: Option<&[u8]>,
) -> Result<Demande, String> {
    let nom = s
        .pointer("/consigne/nom")
        .and_then(Value::as_str)
        .ok_or_else(|| "Bloc spécialiste sans consigne".to_owned())?;
    let chemin = modeles.join(DOSSIER_LLM).join(llm::nom_de_fichier(nom)?);
    let propre = std::fs::read_to_string(&chemin).map_err(|_| {
        format!("Consigne du LLM spécialisé absente ({nom}) : réinstalle-le depuis l'onglet Modèles")
    })?;
    let systeme = format!("{}\n\n{CLAUSE_MINEURS}", propre.trim());
    let mut echantillonnage: Map<String, Value> = s
        .get("echantillonnage")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let temperature = echantillonnage
        .remove("temperature")
        .and_then(|t| t.as_f64())
        .unwrap_or(1.0);
    let max_tokens = s
        .get("max_tokens")
        .and_then(Value::as_u64)
        .and_then(|m| u32::try_from(m).ok())
        .ok_or_else(|| "Bloc spécialiste sans max_tokens".to_owned())?;
    Ok(Demande {
        messages: json!([
            {"role": "system", "content": systeme},
            {"role": "user", "content": match image {
                Some(png) => json!([image_url(png), {"type": "text", "text": texte}]),
                None => json!(texte),
            }},
        ]),
        temperature,
        max_tokens,
        seed: None,
        // Son max_tokens compte déjà sa réflexion.
        budget_reflexion: 0,
        reflechir: true,
        echantillonnage,
    })
}

/// La sortie d'un LLM spécialisé, sans sa réflexion (`<think>…</think>`) : le texte, ou la clé de son JSON.
///
/// # Errors
/// Réflexion trop longue, ou sortie hors format.
pub fn lire_specialiste(texte: &str, s: &Value) -> Result<String, String> {
    let illisible =
        || "Réponse du LLM spécialisé illisible : réflexion trop longue, ou sortie hors format".to_owned();
    let apres = texte.rsplit_once("</think>").map_or(texte, |(_, fin)| fin).trim();
    let valeur = if s.get("sortie").and_then(Value::as_str) == Some("texte") {
        Some(apres.to_owned())
    } else {
        let cle = s.get("cle").and_then(Value::as_str).unwrap_or_default();
        match (apres.find('{'), apres.rfind('}')) {
            (Some(debut), Some(fin)) if debut < fin => serde_json::from_str::<Value>(&apres[debut..=fin])
                .ok()
                .and_then(|v| v.get(cle).and_then(Value::as_str).map(str::to_owned)),
            _ => None,
        }
    };
    valeur
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .ok_or_else(illisible)
}

/// Les images d'une réécriture : le nombre d'images de référence (S'inspirer ; 0 sinon) et les images montrées au LLM
/// (les références, ou l'image de départ d'une variation).
#[derive(Debug, Clone, Copy, Default)]
pub struct Images<'a> {
    pub references: usize,
    pub vues: &'a [Vue],
}

/// Ce qu'il faut pour réécrire une idée : le LLM à lancer, la demande, et comment lire sa réponse.
#[derive(Debug, Clone, PartialEq)]
pub struct Ecriture {
    pub lancement: Lancement,
    pub demande: Demande,
    /// Le bloc du LLM spécialisé, s'il réécrit avec sa propre consigne.
    pub specialiste: Option<Value>,
}

impl Ecriture {
    /// Le nom du GGUF qui écrit.
    pub fn modele(&self) -> String {
        self.lancement
            .modele
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Prépare la réécriture d'une idée en prompt d'image pour ce workflow : le spécialiste du workflow s'il est choisi,
/// sinon le LLM demandé (un spécialiste d'autre chose cède la place au LLM par défaut) avec la consigne du gabarit.
///
/// # Errors
/// Nom de GGUF invalide, ou consigne du spécialiste absente.
pub fn preparer_reecriture(
    lieux: &Lieux,
    idee: &str,
    modele: Option<&str>,
    workflow: Option<&Workflow>,
    reglages_workflow: Option<&ReglagesWorkflow>,
    images: &Images<'_>,
) -> Result<Ecriture, String> {
    let demande_nom = llm::nom(&lieux.modeles, modele);
    // Le spécialiste ne réécrit qu'une idée seule (ni références, ni vidéo), comme en v2.
    if images.references == 0
        && let Some(s) = specialiste_pour(&demande_nom, "creation", workflow)
    {
        let lancement = llm::lancement(lieux, Some(&demande_nom))?;
        let demande = demande_specialiste(&lieux.modeles, &s, idee, None)?;
        return Ok(Ecriture {
            lancement,
            demande,
            specialiste: Some(s),
        });
    }
    let nom = if crate::fiches::specialiste(&demande_nom).is_some() {
        llm::defaut(&lieux.modeles)
    } else {
        demande_nom
    };
    let lancement = llm::lancement(lieux, Some(&nom))?;
    let regle = llm::valeurs(&lieux.dossier, &nom);
    let noms = nom_image(workflow);
    let voit = lancement.vision.is_some();
    let (systeme, contenu, mots) = if images.references > 0 {
        let reecrite = llm::consigne(&lieux.dossier, &nom, "references");
        let systeme = consigne_references(reecrite.as_deref(), &sections_prompt(workflow), noms.as_deref());
        let texte = format!("{idee}\n\n(Reference pictures: {})", images.references);
        let contenu = montrer(&texte, images.vues, "references", voit, noms.as_deref());
        (systeme, contenu, None)
    } else {
        let mots = mots_max_de(workflow, reglages_workflow);
        let reecrite = llm::consigne(&lieux.dossier, &nom, "reecriture");
        let systeme = consigne_reecriture(reecrite.as_deref(), mots, &sections_prompt(workflow));
        (systeme, montrer(idee, images.vues, "variation", voit, None), mots)
    };
    let (temperature, echantillonnage) = regle.ecriture(lancement.reflexion);
    Ok(Ecriture {
        demande: Demande {
            messages: json!([
                {"role": "system", "content": systeme},
                {"role": "user", "content": contenu},
            ]),
            temperature,
            max_tokens: regle.max_tokens.max(tokens_pour_mots(mots)),
            echantillonnage,
            ..Demande::default()
        },
        lancement,
        specialiste: None,
    })
}

/// Une image montrée au LLM : ses pixels (il la voit s'il a sa vision), sinon ce qu'en dit sa fiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vue {
    /// L'image en PNG, réduite à un mégapixel au plus ([`crate::images::pour_la_vision`]).
    pub png: Vec<u8>,
    /// Ce que montre l'image d'après sa fiche ; rien pour une image importée.
    pub texte: Option<String>,
}

/// Ce que le LLM doit faire de l'image, selon l'usage.
fn note_vue(usage: &str) -> &'static str {
    match usage {
        "retouche" => {
            "The picture is the image to edit: name what is really in it (subjects and their gender, objects, setting) so that the instruction fits it."
        }
        "variation" => {
            "The picture is the starting image, partly redrawn: keep its subjects (their gender, looks, clothing), setting and visual style unless the idea changes them."
        }
        "references" => {
            "The pictures are the reference pictures, in order (Picture 1, Picture 2…): take from each what the idea asks for, as it really looks."
        }
        _ => {
            "The picture is the first frame of the clip. Open with one short sentence anchoring it as it really is (visual style, subject, setting), then describe what happens from there. Keep the subjects' gender, looks and clothing, the setting and the visual style consistent (a painting stays a painting)."
        }
    }
}

fn image_url(png: &[u8]) -> Value {
    json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{}", images::base64(png))}})
}

/// Le message de l'utilisateur, avec les images si le LLM les voit, sinon ce qu'en disent leurs fiches ; tel quel
/// sans image, ou si aucune n'a de fiche (images importées).
pub fn montrer(texte: &str, vues: &[Vue], usage: &str, voit: bool, nom_image: Option<&str>) -> Value {
    if vues.is_empty() {
        return json!(texte);
    }
    let note = avec_noms_images(note_vue(usage), nom_image);
    if voit {
        let mut contenu: Vec<Value> = vues.iter().map(|v| image_url(&v.png)).collect();
        contenu.push(json!({"type": "text", "text": format!("{texte}\n\n({note})")}));
        return Value::Array(contenu);
    }
    let connues: Vec<(usize, &str)> = vues
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.texte.as_deref().map(|t| (i + 1, t)))
        .collect();
    match connues.as_slice() {
        [] => json!(texte),
        [(_, seule)] if vues.len() == 1 => json!(format!(
            "{texte}\n\n(You cannot see the picture; it was created from this prompt: {seule}\n{note})"
        )),
        _ => {
            let lignes: Vec<String> = connues
                .iter()
                .map(|(i, t)| format!("{}: {t}", avec_noms_images(&format!("Picture {i}"), nom_image)))
                .collect();
            json!(format!(
                "{texte}\n\n(You cannot see the pictures; they were created from these prompts:\n{}\n{note})",
                lignes.join("\n")
            ))
        }
    }
}

/// Prépare la traduction d'une instruction de retouche (ou d'une précision d'agrandissement, sans image) : le
/// spécialiste de retouche du workflow s'il est choisi et qu'il y a une image à voir, sinon le LLM demandé (un
/// spécialiste d'autre chose cède la place au LLM par défaut), qui voit l'image s'il a sa vision. Courte demande :
/// sans réflexion, 200 tokens.
///
/// # Errors
/// Nom de GGUF invalide, ou consigne du spécialiste absente.
pub fn preparer_traduction(
    lieux: &Lieux,
    instruction: &str,
    modele: Option<&str>,
    workflow: Option<&Workflow>,
    vue: Option<&Vue>,
) -> Result<Ecriture, String> {
    let demande_nom = llm::nom(&lieux.modeles, modele);
    if let Some(v) = vue
        && let Some(s) = specialiste_pour(&demande_nom, "retouche", workflow)
    {
        let lancement = llm::lancement(lieux, Some(&demande_nom))?;
        let demande = demande_specialiste(&lieux.modeles, &s, instruction, Some(&v.png))?;
        return Ok(Ecriture {
            lancement,
            demande,
            specialiste: Some(s),
        });
    }
    let nom = if crate::fiches::specialiste(&demande_nom).is_some() {
        llm::defaut(&lieux.modeles)
    } else {
        demande_nom
    };
    let lancement = llm::lancement(lieux, Some(&nom))?;
    let regle = llm::valeurs(&lieux.dossier, &nom);
    let reecrite = llm::consigne(&lieux.dossier, &nom, "retouche");
    let systeme = consigne_retouche(reecrite.as_deref(), &sections_prompt(workflow));
    let contenu = montrer(
        instruction,
        vue.map(std::slice::from_ref).unwrap_or_default(),
        "retouche",
        lancement.vision.is_some(),
        None,
    );
    Ok(Ecriture {
        demande: Demande {
            messages: json!([
                {"role": "system", "content": systeme},
                {"role": "user", "content": contenu},
            ]),
            temperature: regle.temperature_retouche,
            max_tokens: 200,
            reflechir: false,
            echantillonnage: regle.echantillonnage(),
            ..Demande::default()
        },
        lancement,
        specialiste: None,
    })
}

/// La consigne des précisions du Scénario, comme en v2 : une courte traduction à ajouter au prompt.
pub const PRECISIONS: &str = "You translate short details for a text-to-image prompt.
The user writes a detail, often in French. Translate it into a short English phrase that can be appended to an image prompt.

Rules:
1. Keep exactly the meaning: same elements, same details. Add nothing, remove nothing.
2. If the detail is already in English, return it unchanged.
3. If it contains text in quotes, keep that text exactly, in quotes.
4. Output only the translation: no preamble, no explanation.";

/// La demande de traduction des précisions du Scénario : un généraliste (un spécialiste cède la place au LLM par
/// défaut), courte demande sans réflexion, 200 tokens, à la température de traduction.
///
/// # Errors
/// Nom de GGUF invalide.
pub fn preparer_precisions(
    lieux: &Lieux,
    precisions: &str,
    modele: Option<&str>,
) -> Result<Ecriture, String> {
    let demande_nom = llm::nom(&lieux.modeles, modele);
    let nom = if crate::fiches::specialiste(&demande_nom).is_some() {
        llm::defaut(&lieux.modeles)
    } else {
        demande_nom
    };
    let lancement = llm::lancement(lieux, Some(&nom))?;
    let regle = llm::valeurs(&lieux.dossier, &nom);
    let systeme = format!("{PRECISIONS}\n\n{CLAUSE_MINEURS}");
    Ok(Ecriture {
        demande: Demande {
            messages: json!([
                {"role": "system", "content": systeme},
                {"role": "user", "content": precisions},
            ]),
            temperature: regle.temperature_traduction,
            max_tokens: 200,
            reflechir: false,
            echantillonnage: regle.echantillonnage(),
            ..Demande::default()
        },
        lancement,
        specialiste: None,
    })
}

/// Le prompt tiré de la réponse : sortie du spécialiste lue, ou texte nettoyé (markdown, CJK ajouté), puis l'idée et
/// le prompt relus ensemble par le filtre AS-18 (le LLM peut avoir ajouté un terme sensible à une idée avec un
/// mineur).
///
/// # Errors
/// Sortie illisible, ou refus du filtre.
pub fn lire_reecriture(
    ecriture: &Ecriture,
    idee: &str,
    reponse: &str,
    filtre: &Filtre,
) -> Result<String, String> {
    let prompt = match &ecriture.specialiste {
        Some(s) => lire_specialiste(reponse, s)?,
        None => garde_cjk(&nettoyer_markdown(reponse), &[idee]),
    };
    filtre
        .verifier(&[idee, prompt.as_str()])
        .map_err(|e| e.to_string())?;
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dossier;
    use crate::gguf::essai;
    use crate::securite::Ajouts;
    use crate::workflows;
    use std::fs;

    const T2I: &str = "Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf";

    fn lieux(temp: &tempfile::TempDir) -> Lieux {
        let l = Lieux {
            dossier: Dossier::a(temp.path()),
            modeles: temp.path().join("modeles"),
            moteurs: temp.path().join("moteurs"),
            port: 8095,
        };
        fs::create_dir_all(l.modeles.join(DOSSIER_LLM)).unwrap();
        l
    }

    fn filtre() -> Filtre {
        Filtre::livre(&Ajouts::default()).unwrap()
    }

    fn systeme(e: &Ecriture) -> String {
        e.demande.messages[0]["content"].as_str().unwrap().to_owned()
    }

    #[test]
    fn la_consigne_d_origine_suit_le_gabarit_de_la_v2() {
        let c = consigne_reecriture(None, None, &[]);
        assert!(c.starts_with("ROLE:\nYou are an expert prompt writer for text-to-image models."));
        assert!(c.contains("\n\nUSER PRIORITY:\nFirst sort what the user wrote into three kinds:"));
        assert!(c.contains("PROMPT OUTLINE:\nOne flowing paragraph"));
        assert!(c.contains("CONTENT AND WORDING:\n- Medium: keep"));
        assert!(c.contains("VISIBLE TEXT:\n- If the idea asks for written text"));
        assert!(c.contains("NEVER:\n- Add characters, animals or objects"));
        assert!(c.contains("OUTPUT FORMAT:\n- 40 to 120 words, plain prose"));
        assert!(c.ends_with(&format!("\n\n{CLAUSE_MINEURS}")));
        assert!(!c.contains("DIALOGUE"));
        assert!(!c.contains('{'));
    }

    #[test]
    fn les_regles_du_modele_cible_se_rangent_dans_les_sections() {
        let krea = workflows::obtenir("krea2_turbo").unwrap();
        let regles = sections_prompt(Some(&krea));
        assert!(regles.iter().any(|(c, _)| c == "contenu"));
        let c = consigne_reecriture(None, Some(150), &regles);
        // « contenu » s'ajoute au gabarit ; « texte » le remplace.
        assert!(c.contains("- Make the OPEN parts visual: setting, lighting, composition, in grounded words that serve the idea.\n- Group each subject"));
        assert!(c.contains("VISIBLE TEXT:\n- If the idea asks for written text in the image, keep that text exactly, in quotes.\n- If the idea asks for no written words"));
        assert!(!c.contains("Otherwise mention no text."));
        assert!(c.contains("40 to 150 words"));
        // Un plan de plusieurs parties est numéroté.
        let plan = vec![("plan".to_owned(), vec!["A".to_owned(), "B".to_owned()])];
        assert!(
            consigne_reecriture(None, None, &plan).contains("PROMPT OUTLINE, in this order:\n1. A\n2. B")
        );
        // Des règles à plat ont leur en-tête.
        let a_plat = vec![("regles".to_owned(), vec!["Use tags.".to_owned()])];
        assert!(consigne_reecriture(None, None, &a_plat).contains(&format!("{ENTETE_REGLES}\n- Use tags.")));
    }

    #[test]
    fn une_consigne_reecrite_garde_son_texte() {
        let krea = workflows::obtenir("krea2_turbo").unwrap();
        let c = consigne_reecriture(
            Some("Write {mots_max} words.  \n"),
            Some(150),
            &sections_prompt(Some(&krea)),
        );
        assert!(c.starts_with(&format!(
            "Write 150 words.\n\n{ENTETE_REGLES}\n\nCONTENT AND WORDING:\n- Group"
        )));
        assert!(c.ends_with(CLAUSE_MINEURS));
        assert_eq!(
            consigne_reecriture(Some("Seule."), None, &[]),
            format!("Seule.\n\n{CLAUSE_MINEURS}")
        );
    }

    #[test]
    fn longueurs() {
        let krea = workflows::obtenir("krea2_turbo").unwrap();
        assert_eq!(mots_max_de(Some(&krea), None), Some(150));
        let mut r = Map::new();
        r.insert("mots_max".to_owned(), json!(300));
        assert_eq!(mots_max_de(Some(&krea), Some(&r)), Some(300));
        r.insert("mots_max".to_owned(), json!(5000));
        assert_eq!(mots_max_de(Some(&krea), Some(&r)), Some(150));
        assert_eq!(mots_max_de(None, None), None);
        assert_eq!(tokens_pour_mots(Some(450)), 739);
        assert_eq!(tokens_pour_mots(None), 0);
    }

    #[test]
    fn nettoyage() {
        assert_eq!(
            nettoyer_markdown("## Prompt\n**A red fox** sits in *soft* snow.\n\n# Fin"),
            "Prompt A red fox sits in soft snow. Fin"
        );
        assert_eq!(garde_cjk("A fox 狐狸 in snow", &["un renard"]), "A fox in snow");
        assert_eq!(
            garde_cjk("A sign reading 東京", &["une enseigne « 東京 »"]),
            "A sign reading 東京"
        );
        assert_eq!(garde_cjk("Plain text", &[]), "Plain text");
    }

    #[test]
    fn sortie_d_un_specialiste() {
        let json_s = json!({"sortie": "json", "cle": "rewritten_prompt"});
        let texte_s = json!({"sortie": "texte"});
        assert_eq!(
            lire_specialiste(
                "<think>long</think>\n{\"rewritten_prompt\": \" A fox. \"}",
                &json_s
            )
            .unwrap(),
            "A fox."
        );
        assert_eq!(
            lire_specialiste("Voici : {\"rewritten_prompt\": \"A cat\"} fin", &json_s).unwrap(),
            "A cat"
        );
        assert!(lire_specialiste("<think>jamais fini", &json_s).is_err());
        assert!(lire_specialiste("{\"autre\": \"x\"}", &json_s).is_err());
        assert!(lire_specialiste("{\"rewritten_prompt\": \"  \"}", &json_s).is_err());
        assert_eq!(
            lire_specialiste("<think>x</think> A dog ", &texte_s).unwrap(),
            "A dog"
        );
    }

    #[test]
    fn reecriture_par_le_llm_general() {
        let temp = tempfile::tempdir().unwrap();
        let l = lieux(&temp);
        fs::write(l.modeles.join("llm").join("mistral.gguf"), essai::gguf(&[12])).unwrap();
        fs::write(
            l.dossier.reglages(),
            r#"{"llm": {"mistral.gguf": {"reflexion": false, "temperature_reecriture": 0.5, "max_tokens": 256}}}"#,
        )
        .unwrap();
        let krea = workflows::obtenir("krea2_turbo").unwrap();
        let e = preparer_reecriture(
            &l,
            "un renard dans la neige",
            Some("mistral.gguf"),
            Some(&krea),
            None,
            &Images::default(),
        )
        .unwrap();
        assert!(e.specialiste.is_none());
        assert_eq!(e.modele(), "mistral.gguf");
        assert!(!e.lancement.reflexion);
        assert!((e.demande.temperature - 0.5).abs() < f64::EPSILON);
        // 150 mots conseillés pour Krea 2 : la réponse doit pouvoir les contenir.
        assert_eq!(e.demande.max_tokens, 289);
        assert_eq!(e.demande.messages[1]["content"], json!("un renard dans la neige"));
        assert!(systeme(&e).contains("40 to 150 words"));
        assert!(systeme(&e).contains("Group each subject"));
        assert_eq!(
            lire_reecriture(&e, "un renard", "**A red fox** in snow 雪", &filtre()).unwrap(),
            "A red fox in snow"
        );
        let refus = lire_reecriture(
            &e,
            "a twelve-year-old girl",
            "a twelve-year-old girl, nude",
            &filtre(),
        );
        assert!(refus.unwrap_err().starts_with("ERREUR AS-18"));
    }

    #[test]
    fn reecriture_par_le_specialiste_du_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let l = lieux(&temp);
        let llm_dir = l.modeles.join("llm");
        fs::write(llm_dir.join(T2I), essai::gguf(&[12])).unwrap();
        fs::write(llm_dir.join("mistral.gguf"), essai::gguf(&[12])).unwrap();
        let qwen = workflows::obtenir("qwen_image_2_1").unwrap();
        // Sans sa consigne, il ne peut pas écrire.
        let absente = preparer_reecriture(&l, "un renard", Some(T2I), Some(&qwen), None, &Images::default())
            .unwrap_err();
        assert!(absente.starts_with("Consigne du LLM spécialisé absente"));
        fs::write(
            llm_dir.join("Qwen-Image-2.1-PE-T2I.consigne.txt"),
            "  Rewrite the prompt.\n",
        )
        .unwrap();
        let e =
            preparer_reecriture(&l, "un renard", Some(T2I), Some(&qwen), None, &Images::default()).unwrap();
        assert!(e.specialiste.is_some());
        assert_eq!(systeme(&e), format!("Rewrite the prompt.\n\n{CLAUSE_MINEURS}"));
        assert_eq!((e.demande.max_tokens, e.demande.budget_reflexion), (8192, 0));
        assert!((e.demande.temperature - 1.0).abs() < f64::EPSILON);
        assert_eq!(e.demande.echantillonnage["top_k"], json!(20));
        assert!(!e.demande.echantillonnage.contains_key("temperature"));
        assert!(e.lancement.reflexion);
        assert_eq!(
            lire_reecriture(
                &e,
                "un renard",
                "<think>…</think>{\"rewritten_prompt\": \"A fox\"}",
                &filtre()
            )
            .unwrap(),
            "A fox"
        );
        // Pour un autre workflow, le spécialiste cède la place au LLM par défaut.
        let krea = workflows::obtenir("krea2_turbo").unwrap();
        let e =
            preparer_reecriture(&l, "un renard", Some(T2I), Some(&krea), None, &Images::default()).unwrap();
        assert!(e.specialiste.is_none());
        assert_eq!(e.modele(), "mistral.gguf");
    }

    #[test]
    fn la_consigne_de_retouche_suit_son_gabarit() {
        let c = consigne_retouche(None, &[]);
        assert!(c.starts_with(
            "ROLE:
You translate image-editing instructions"
        ));
        assert!(c.contains(
            "USER PRIORITY:
Edit exactly what the user names."
        ));
        assert!(!c.contains("First sort what the user wrote"));
        assert!(c.contains(
            "PROMPT OUTLINE, in this order:
1. Lead with the change"
        ));
        assert!(c.contains(
            "OUTPUT FORMAT:
- One or two sentences."
        ));
        assert!(c.ends_with(CLAUSE_MINEURS));
    }

    fn vue(texte: Option<&str>) -> Vue {
        Vue {
            png: vec![0x89, b'P', b'N', b'G'],
            texte: texte.map(str::to_owned),
        }
    }

    #[test]
    fn images_montrees_ou_decrites() {
        assert_eq!(
            montrer("fais-la rire", &[], "retouche", true, None),
            json!("fais-la rire")
        );
        let vu = montrer("fais-la rire", &[vue(Some("a woman"))], "retouche", true, None);
        assert_eq!(vu[0]["type"], json!("image_url"));
        assert_eq!(vu[0]["image_url"]["url"], json!("data:image/png;base64,iVBORw=="));
        assert!(vu[1]["text"].as_str().unwrap().starts_with(
            "fais-la rire

(The picture is the image to edit"
        ));
        let decrit = montrer("fais-la rire", &[vue(Some("a woman"))], "retouche", false, None);
        assert!(
            decrit
                .as_str()
                .unwrap()
                .contains("You cannot see the picture; it was created from this prompt: a woman")
        );
        // Image importée, sans fiche : à l'aveugle.
        assert_eq!(
            montrer("fais-la rire", &[vue(None)], "retouche", false, None),
            json!("fais-la rire")
        );
        let deux = montrer(
            "mélange",
            &[vue(Some("a cat")), vue(Some("a dog"))],
            "references",
            false,
            None,
        );
        assert!(deux.as_str().unwrap().contains(
            "Picture 1: a cat
Picture 2: a dog"
        ));
        // Les images nommées comme le modèle cible les nomme.
        let qwen = montrer(
            "mélange",
            &[vue(Some("a cat")), vue(Some("a dog"))],
            "references",
            false,
            Some("<image{n}>"),
        );
        assert!(qwen.as_str().unwrap().contains(
            "<image1>: a cat
<image2>: a dog"
        ));
        assert!(qwen.as_str().unwrap().contains("in order (<image1>, <image2>…)"));
    }

    #[test]
    fn traduction_d_une_retouche() {
        let temp = tempfile::tempdir().unwrap();
        let l = lieux(&temp);
        let llm_dir = l.modeles.join("llm");
        fs::write(llm_dir.join("mistral.gguf"), essai::gguf(&[12])).unwrap();
        let edit = workflows::obtenir("qwen_image_2_1_edit").unwrap();
        // Sans vision : la fiche décrit l'image.
        let e = preparer_traduction(
            &l,
            "fais-la rire",
            Some("mistral.gguf"),
            Some(&edit),
            Some(&vue(Some("a woman"))),
        )
        .unwrap();
        assert!(e.specialiste.is_none());
        assert!(systeme(&e).starts_with(
            "ROLE:
You translate image-editing instructions"
        ));
        assert_eq!((e.demande.max_tokens, e.demande.reflechir), (200, false));
        assert!((e.demande.temperature - 0.2).abs() < f64::EPSILON);
        assert!(
            e.demande.messages[1]["content"]
                .as_str()
                .unwrap()
                .contains("a woman")
        );
        // Avec son projecteur, le LLM voit l'image.
        fs::write(llm_dir.join("mistral-mmproj-Q8_0.gguf"), b"x").unwrap();
        let e = preparer_traduction(
            &l,
            "fais-la rire",
            Some("mistral.gguf"),
            Some(&edit),
            Some(&vue(None)),
        )
        .unwrap();
        assert!(e.lancement.vision.is_some());
        assert_eq!(e.demande.messages[1]["content"][0]["type"], json!("image_url"));
        // Le spécialiste de retouche du workflow voit l'image avec sa propre consigne.
        let i2i = "Qwen-Image-2.1-PE-I2I.Q5_K_M.gguf";
        fs::write(llm_dir.join(i2i), essai::gguf(&[12])).unwrap();
        fs::write(llm_dir.join("Qwen-Image-2.1-PE-I2I.consigne.txt"), "Edit.").unwrap();
        let e = preparer_traduction(&l, "fais-la rire", Some(i2i), Some(&edit), Some(&vue(None))).unwrap();
        assert!(e.specialiste.is_some());
        assert_eq!(e.demande.messages[1]["content"][0]["type"], json!("image_url"));
        assert_eq!(e.demande.messages[1]["content"][1]["text"], json!("fais-la rire"));
        // Une précision d'agrandissement, sans image : jamais le spécialiste.
        let e = preparer_traduction(&l, "ajoute la mer", Some(i2i), Some(&edit), None).unwrap();
        assert!(e.specialiste.is_none());
        assert_eq!(e.modele(), "mistral.gguf");
    }

    #[test]
    fn reecriture_a_partir_de_references() {
        let temp = tempfile::tempdir().unwrap();
        let l = lieux(&temp);
        let llm_dir = l.modeles.join("llm");
        fs::write(llm_dir.join("mistral.gguf"), essai::gguf(&[12])).unwrap();
        fs::write(
            llm_dir.join("Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf"),
            essai::gguf(&[12]),
        )
        .unwrap();
        fs::write(llm_dir.join("Qwen-Image-2.1-PE-T2I.consigne.txt"), "Rewrite.").unwrap();
        let edit = workflows::obtenir("qwen_image_2_1_edit").unwrap();
        assert_eq!(nom_image(Some(&edit)).as_deref(), Some("<image{n}>"));
        let vues = [vue(Some("a cat")), vue(Some("a dog"))];
        let images = Images {
            references: 2,
            vues: &vues,
        };
        let e = preparer_reecriture(
            &l,
            "le chat de l'image 1 et le chien",
            Some("mistral.gguf"),
            Some(&edit),
            None,
            &images,
        )
        .unwrap();
        let consigne = systeme(&e);
        assert!(consigne.starts_with("ROLE:
You write instructions for an image model that receives reference pictures, numbered <image1>, <image2>, <image3>."));
        assert!(consigne.contains("\"the woman from <image1>\""));
        assert!(consigne.ends_with(CLAUSE_MINEURS));
        let message = e.demande.messages[1]["content"].as_str().unwrap();
        assert!(message.starts_with(
            "le chat de l'image 1 et le chien

(Reference pictures: 2)"
        ));
        assert!(message.contains("<image1>: a cat"));
        // Avec des références, jamais le spécialiste de la réécriture.
        let qwen = workflows::obtenir("qwen_image_2_1").unwrap();
        let e = preparer_reecriture(
            &l,
            "x",
            Some("Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf"),
            Some(&qwen),
            None,
            &images,
        )
        .unwrap();
        assert!(e.specialiste.is_none());
        // Une variation : la consigne de réécriture, l'image de départ décrite.
        let depart = [vue(Some("a red fox"))];
        let variation = Images {
            references: 0,
            vues: &depart,
        };
        let krea = workflows::obtenir("krea2_turbo").unwrap();
        let e = preparer_reecriture(
            &l,
            "en hiver",
            Some("mistral.gguf"),
            Some(&krea),
            None,
            &variation,
        )
        .unwrap();
        assert!(systeme(&e).starts_with(
            "ROLE:
You are an expert prompt writer"
        ));
        assert!(
            e.demande.messages[1]["content"]
                .as_str()
                .unwrap()
                .contains("it was created from this prompt: a red fox")
        );
    }
}
