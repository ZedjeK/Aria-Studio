//! Le travail du LLM sur les prompts, comme en v2 (`app/prompt.py`) : pour l'instant, la réécriture d'une idée en
//! prompt d'image. Les références, la retouche, la vidéo, le découpage et la conversation viendront ensuite.
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
        format!("USER PRIORITY:\n{}", priorite()),
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

/// La consigne de réécriture d'une idée pour ce modèle cible, toujours suivie de la clause verrouillée. `reecrite` :
/// celle de Réglages pour ce GGUF, qui garde son texte (les règles du modèle cible s'y ajoutent).
pub fn consigne_reecriture(
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
        None => gabarit(&REECRITURE, regles),
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

/// Le bloc `specialiste` de ce GGUF s'il réécrit les idées pour ce workflow.
pub fn specialiste_pour(modele: &str, workflow: Option<&Workflow>) -> Option<Value> {
    let s = crate::fiches::specialiste(modele)?;
    let id = workflow?.id.as_str();
    let sert = s.get("usage").and_then(Value::as_str) == Some("creation")
        && s.get("workflows")
            .and_then(Value::as_array)
            .is_some_and(|w| w.iter().any(|x| x.as_str() == Some(id)));
    sert.then_some(s)
}

/// Les messages et les paramètres d'un LLM spécialisé : sa consigne livrée (plus la clause verrouillée) et
/// l'échantillonnage de son entraînement.
///
/// # Errors
/// Sa consigne n'est pas dans `modeles\llm`, ou son bloc est incomplet.
pub fn demande_specialiste(modeles: &Path, s: &Value, texte: &str) -> Result<Demande, String> {
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
            {"role": "user", "content": texte},
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
) -> Result<Ecriture, String> {
    let demande_nom = llm::nom(&lieux.modeles, modele);
    if let Some(s) = specialiste_pour(&demande_nom, workflow) {
        let lancement = llm::lancement(lieux, Some(&demande_nom))?;
        let demande = demande_specialiste(&lieux.modeles, &s, idee)?;
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
    let mots = mots_max_de(workflow, reglages_workflow);
    let reecrite = llm::consigne(&lieux.dossier, &nom, "reecriture");
    let systeme = consigne_reecriture(reecrite.as_deref(), mots, &sections_prompt(workflow));
    let (temperature, echantillonnage) = regle.ecriture(lancement.reflexion);
    Ok(Ecriture {
        demande: Demande {
            messages: json!([
                {"role": "system", "content": systeme},
                {"role": "user", "content": idee},
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
        let absente = preparer_reecriture(&l, "un renard", Some(T2I), Some(&qwen), None).unwrap_err();
        assert!(absente.starts_with("Consigne du LLM spécialisé absente"));
        fs::write(
            llm_dir.join("Qwen-Image-2.1-PE-T2I.consigne.txt"),
            "  Rewrite the prompt.\n",
        )
        .unwrap();
        let e = preparer_reecriture(&l, "un renard", Some(T2I), Some(&qwen), None).unwrap();
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
        let e = preparer_reecriture(&l, "un renard", Some(T2I), Some(&krea), None).unwrap();
        assert!(e.specialiste.is_none());
        assert_eq!(e.modele(), "mistral.gguf");
    }
}
