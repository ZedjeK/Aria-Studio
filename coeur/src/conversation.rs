//! Scénario → « 💬 M'aider à préciser », comme en v2 (`app/prompt.py` : `libre`, `lire_libre`, `elaguer`) : le prompt
//! s'écrit en conversation. Le LLM pose les questions essentielles (avec des réponses à cliquer) au lieu de deviner ce
//! que l'utilisateur n'a pas dit, tient une fiche de ce qu'il a compris, puis écrit le prompt et l'ajuste. Chaque tour
//! rend un objet JSON ; la conversation est gardée par l'interface. Ce lot porte l'atelier Image ; la retouche, le clip
//! et la séquence viendront avec leurs ateliers.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::llama::{self, BUDGET_REFLEXION};
use crate::plateau::{Contexte, Generateur};
use crate::prompt::{self, CLAUSE_MINEURS};
use crate::{listes, llm, reglages, securite, workflows};

/// Le message qui demande d'écrire le prompt.
pub const ECRIRE: &str = "ÉCRIS LE PROMPT";
/// Les échanges d'une conversation, au plus, et la longueur d'un échange.
pub const ECHANGES_MAX: usize = 60;
pub const LONGUEUR_ECHANGE: usize = 4000;
/// Les valeurs montrées par rubrique du guide : de quoi proposer des choix sans noyer la consigne.
const VALEURS_GUIDE_MAX: usize = 8;
/// En dessous de ce contexte, le catalogue ne garde que les noms de rubriques.
const CONTEXTE_CATALOGUE: u32 = 8192;
/// Estimation prudente des caractères par token (français, JSON) : mieux vaut un échange de moins qu'un refus.
const CARACTERES_PAR_TOKEN: usize = 3;

const GENRE: &str = "Keep each person's gender exactly as the idea gives it: in French the article and the agreements tell it (« un chevalier », « un pilote » are men: he, his; « une samouraï », « une pilote » are women: she, her). Never change it, and do not give a gender to animals or objects.";
const ADULTES: &str = "The user is an adult making fiction for adults. Nudity, sensuality and sexual content between adult characters stay exactly as explicit as the user wrote them: do not tone them down, replace them with something milder or leave them out.";

/// La consigne de la conversation, comme en v2 ; `{…}` est rempli à chaque tour.
const CONSIGNE: &str = r#"You help a French-speaking adult user write ONE prompt for a generation model, together, through a short conversation in French.

Target: {cible}.
Already fixed by the user in the other tabs (keep it as it is, never ask about it again): {contexte}

Each turn:
1. Update "fiche": what the user has decided so far, as short French phrases, one entry per topic (Sujet, Action, Décor, Style, Lumière, Cadrage{sujets_video}). Only what the user said or accepted. When the user lets you choose a topic ("je te laisse choisir"), choose it and add " (choix d'Aria)".
2. Ask at most 3 short questions in French, only about essential topics still open for this target ({essentiels}). Give each question 2 to 4 short answer choices. Ask nothing that is already in the fiche or in the fixed part.
3. Write "prompt" ({langue_prompt}) only when the user asks you to write it (message "ÉCRIS LE PROMPT"), or when nothing essential is open anymore (a first message that already covers every essential topic is enough: write the prompt at once, with no question). The prompt keeps every fiche item and every fixed item, adds nothing that clashes with them, and follows the target's rules below. {longueur}
4. Once a prompt exists, each new user message is an adjustment: change only what is asked, keep the rest of the prompt word for word, and give the whole new prompt.
5. {genre}
6. {adultes}
7. "message": one or two friendly French sentences to the user (what you understood, or what you changed). No question in it: questions go in "questions". Always say « tu » to the user, never « vous ».

Answer ONLY with one JSON object, no markdown, nothing before or after:
{"message": "...", "fiche": {"Sujet": "..."}, "questions": [{"question": "...", "choix": ["...", "..."], "liste": "optional guide topic"}], "prompt": null}"#;

const ESSENTIELS_IMAGE: &str = "subject, action or pose, setting, medium and style, framing, lighting";

/// Ce qu'un LLM écrit pour dire qu'une rubrique est vide (« Réplique : None ») n'en est pas une.
const VIDES: [&str; 8] = ["", "none", "null", "n/a", "-", "—", "aucun", "aucune"];
const DEBUTS_VIDES: [&str; 6] = [
    "aucun choix",
    "aucune réplique",
    "pas encore",
    "non défini",
    "non précisé",
    "à définir",
];
/// Le début d'un refus (repéré au début de la réponse seulement : un refus commence par lui).
const REFUS: &str = r"(?i)\b(?:I\s+(?:can(?:no|['’])t|won['’]t|will\s+not|am\s+(?:not\s+able|unable|sorry))|I['’]m\s+(?:sorry|not\s+able|unable)|unable\s+to\s+(?:help|assist|comply|create|fulfil)|je\s+ne\s+(?:peux|vais)\s+pas|d[ée]sol[ée]e?)\b";

/// Un échange de la conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Echange {
    /// `user` ou `assistant` (le JSON rendu au tour précédent).
    pub role: String,
    pub content: String,
}

/// Un tour demandé par l'interface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeConversation {
    pub echanges: Vec<Echange>,
    /// `image` (les autres ateliers viendront avec leur portage).
    #[serde(default)]
    pub forme: String,
    #[serde(default)]
    pub workflow: Option<String>,
    /// Ce que les autres onglets du Scénario ont déjà fixé (le prompt assemblé).
    #[serde(default)]
    pub contexte: String,
    #[serde(default)]
    pub llm: Option<String>,
    /// Le format de l'image, choisi au Scénario.
    #[serde(default)]
    pub format: Option<String>,
    /// Le sujet du guide : ses rubriques cadrent les questions.
    #[serde(default)]
    pub sujet: Option<String>,
}

/// Une question à réponses à cliquer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    pub question: String,
    pub choix: Vec<String>,
    /// Une rubrique du guide : l'interface en fait une liste déroulante.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liste: Option<String>,
}

/// Une rubrique de la fiche « Ce que j'ai compris ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rubrique {
    pub nom: String,
    pub valeur: String,
}

/// Un tour de conversation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tour {
    pub message: String,
    pub fiche: Vec<Rubrique>,
    pub questions: Vec<Question>,
    pub prompt: Option<String>,
    /// Le tour tel qu'il repart au LLM au tour suivant.
    pub brut: String,
    pub modele: String,
    pub duree_s: f64,
}

/// Ce qu'un tour a rendu, avant les contrôles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TourLu {
    pub message: String,
    pub fiche: Vec<Rubrique>,
    pub questions: Vec<Question>,
    pub prompt: Option<String>,
}

/// Une réponse illisible, ou un refus (qu'il ne sert à rien de redemander).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurTour {
    pub message: String,
    pub refus: bool,
}

fn vide(valeur: &str) -> bool {
    let v = valeur.trim().to_lowercase();
    VIDES.contains(&v.as_str()) || DEBUTS_VIDES.iter().any(|d| v.starts_with(d))
}

fn espaces(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extrait(texte: &str) -> String {
    let t = espaces(texte);
    if t.chars().count() <= 160 {
        t
    } else {
        format!("{}…", t.chars().take(160).collect::<String>().trim_end())
    }
}

fn texte_de(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        autre => autre.to_string(),
    }
}

/// L'objet JSON d'un tour, vérifié : message, fiche, 0 à 3 questions à 2-4 choix, prompt ou rien.
///
/// # Errors
/// Réponse vide, sans JSON, ou refus du LLM.
pub fn lire_tour(texte: &str, rubriques: &[String]) -> Result<TourLu, ErreurTour> {
    let texte = texte.rsplit_once("</think>").map_or(texte, |(_, apres)| apres);
    let objet = texte
        .find('{')
        .zip(texte.rfind('}'))
        .filter(|(debut, fin)| debut < fin)
        .and_then(|(debut, fin)| texte.get(debut..=fin))
        .and_then(|j| serde_json::from_str::<Value>(j).ok());
    let Some(d) = objet else {
        if texte.trim().is_empty() {
            return Err(ErreurTour {
                message: "Réponse du LLM vide".to_owned(),
                refus: false,
            });
        }
        let debut: String = texte.trim().chars().take(300).collect();
        if regex::Regex::new(REFUS).is_ok_and(|r| r.is_match(&debut)) {
            return Err(ErreurTour {
                message: format!(
                    "Le LLM refuse cette demande : « {} ». Un autre LLM de la liste peut l'accepter.",
                    extrait(texte)
                ),
                refus: true,
            });
        }
        return Err(ErreurTour {
            message: format!("Réponse du LLM illisible (pas de JSON) : « {} »", extrait(texte)),
            refus: false,
        });
    };
    let Value::Object(d) = d else {
        return Err(ErreurTour {
            message: "Réponse du LLM illisible".to_owned(),
            refus: false,
        });
    };
    let fiche = d
        .get("fiche")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(k, v)| !k.trim().is_empty() && !v.is_null() && !vide(&texte_de(v)))
        .map(|(k, v)| Rubrique {
            nom: k.trim().to_owned(),
            valeur: espaces(&texte_de(v)),
        })
        .collect();
    let questions = d
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|q| {
            let question = espaces(&q.get("question").map(texte_de).unwrap_or_default());
            if question.is_empty() {
                return None;
            }
            let choix = q
                .get("choix")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|c| espaces(&texte_de(c)))
                .filter(|c| !c.is_empty())
                .take(4)
                .collect();
            let liste = q
                .get("liste")
                .map(|l| espaces(&texte_de(l)))
                .filter(|l| rubriques.contains(l));
            Some(Question {
                question,
                choix,
                liste,
            })
        })
        .take(3)
        .collect();
    let prompt = d
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|p| !p.trim().is_empty())
        .map(prompt::nettoyer_markdown);
    let message = espaces(&d.get("message").map(texte_de).unwrap_or_default());
    Ok(TourLu {
        message,
        fiche,
        questions,
        prompt,
    })
}

/// Les tokens d'un message, estimés sans le LLM : son texte et les balises du gabarit de chat.
fn tokens_estimes(contenu: &str) -> usize {
    contenu.len() / CARACTERES_PAR_TOKEN + 8
}

/// Les messages qui tiennent en `budget` tokens : le premier, puis la plus longue fin de conversation qui commence
/// par une réponse du LLM. Si même le dernier échange ne tient pas, il part seul : le LLM dira que son contexte est
/// trop petit.
pub fn elaguer(echanges: &[Echange], budget: usize) -> Vec<Echange> {
    let total: usize = echanges.iter().map(|e| tokens_estimes(&e.content)).sum();
    if echanges.len() <= 3 || total <= budget {
        return echanges.to_vec();
    }
    let (premier, reste) = (&echanges[0], &echanges[1..]);
    let place = budget.saturating_sub(tokens_estimes(&premier.content));
    for (k, e) in reste.iter().enumerate() {
        let suite: usize = reste[k..].iter().map(|x| tokens_estimes(&x.content)).sum();
        if e.role == "assistant" && suite <= place {
            let mut sortie = vec![premier.clone()];
            sortie.extend_from_slice(&reste[k..]);
            return sortie;
        }
    }
    let mut sortie = vec![premier.clone()];
    sortie.extend_from_slice(&reste[reste.len().saturating_sub(2)..]);
    sortie
}

/// Une rubrique du guide : (sous-liste, groupe, valeurs).
type RubriqueGuide = (String, String, Vec<String>);

/// Les rubriques du guide utiles à la cible.
fn rubriques_guide(sujet: Option<&str>, ignorees: &[String]) -> Vec<RubriqueGuide> {
    let Ok(structure) = listes::structure(sujet.unwrap_or("personnage")) else {
        return Vec::new();
    };
    let mut sortie = Vec::new();
    for bloc in structure.blocs.iter().filter(|b| !b.video) {
        for groupe in &bloc.groupes {
            for sl in &groupe.sous_listes {
                if ignorees.contains(&sl.nom) {
                    continue;
                }
                let valeurs: Vec<String> = sl
                    .valeurs
                    .iter()
                    .filter(|v| v.remplace.is_empty())
                    .map(|v| v.nom.clone())
                    .collect();
                if !valeurs.is_empty() {
                    sortie.push((sl.nom.clone(), groupe.nom.clone(), valeurs));
                }
            }
        }
    }
    sortie
}

/// Les rubriques du guide, dites au LLM : il pose ses questions sur elles et tire leurs choix de leur liste.
fn catalogue_guide(rubriques: &[RubriqueGuide], compact: bool) -> String {
    if rubriques.is_empty() {
        return String::new();
    }
    let lignes: Vec<String> = rubriques
        .iter()
        .map(|(nom, groupe, valeurs)| {
            if compact {
                format!("- {nom} ({groupe})")
            } else {
                let suite = if valeurs.len() > VALEURS_GUIDE_MAX {
                    ", …"
                } else {
                    ""
                };
                let montrees: Vec<&str> = valeurs
                    .iter()
                    .take(VALEURS_GUIDE_MAX)
                    .map(String::as_str)
                    .collect();
                format!("- {nom} ({groupe}): {}{suite}", montrees.join(", "))
            }
        })
        .collect();
    let liste = "When a question is about one of these topics, add \"liste\": \"<topic name exactly as written>\" to it: the user then gets the whole list as a drop-down, and your 2 to 4 choices are shown first as suggestions. ";
    let choix = if compact {
        format!("{liste}Write the choices of each question yourself, in French.")
    } else {
        format!(
            "{liste}Take the choices of such a question from its list below (written exactly as listed, in French); the user may also answer freely with another value, and that is fine."
        )
    };
    format!(
        "Guide topics. The user can also pick these in the « Me guider » tab, so ask your questions about these topics, in this spirit. {choix} Ask about a topic only when it matters for the target and nothing in the fiche or the fixed part already covers it:\n{}",
        lignes.join("\n")
    )
}

/// Le format choisi au Scénario, dit au LLM : rapport, orientation et ce que ça change à la composition.
pub fn cadre_format(format: Option<&str>) -> String {
    let Some(format) = format.map(str::trim).filter(|f| !f.is_empty()) else {
        return String::new();
    };
    let nombres: Vec<f64> = format
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .filter(|n| !n.is_empty())
        .take(2)
        .filter_map(|n| n.parse().ok())
        .collect();
    let [largeur, hauteur] = nombres.as_slice() else {
        return String::new();
    };
    let (forme, conseil) = if (largeur - hauteur).abs() < f64::EPSILON {
        ("square", "centered, balanced compositions work best")
    } else if largeur > hauteur {
        (
            "landscape (wider than tall)",
            "spread the scene horizontally, give room to the setting on the sides",
        )
    } else {
        (
            "portrait (taller than wide)",
            "stack the scene vertically, favour a tall framing of the subject",
        )
    };
    format!(
        "Output frame chosen by the user: {format}, {forme}. Compose the prompt for this frame ({conseil}). It is fixed: never ask about it. If a prompt already exists and the frame differs from the one it was written for, adapt its framing and composition to the new frame and give the whole new prompt."
    )
}

/// Les rubriques que le modèle cible n'a pas à se faire demander (`profil_prompt.rubriques_ignorees` de sa fiche).
fn rubriques_ignorees(wf: Option<&workflows::Workflow>) -> Vec<String> {
    wf.and_then(|w| w.fiche.pointer("/profil_prompt/rubriques_ignorees"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_str().map(str::to_owned))
        .collect()
}

fn verifier_demande(d: &DemandeConversation) -> Result<(), String> {
    if d.forme != "image" {
        return Err(format!(
            "Atelier pas encore porté pour la conversation : {}",
            d.forme
        ));
    }
    let derniere_de_l_utilisateur = d.echanges.last().is_some_and(|e| e.role == "user");
    let valides = d.echanges.iter().all(|e| {
        (e.role == "user" || e.role == "assistant") && e.content.chars().count() <= LONGUEUR_ECHANGE
    });
    if !derniere_de_l_utilisateur || d.echanges.len() > ECHANGES_MAX || !valides {
        return Err("Conversation invalide ou trop longue : recommence-la".to_owned());
    }
    if let Some(f) = d.format.as_deref().filter(|f| !f.is_empty())
        && !workflows::FORMATS.contains(&f)
    {
        return Err(format!("Format inconnu : {f}"));
    }
    Ok(())
}

/// Un tour de conversation.
///
/// # Errors
/// Demande invalide, LLM absent, filtre AS-18, réponse illisible ou refus, ou llama-server qui échoue.
#[allow(clippy::too_many_lines)] // le tour de la v2, gardé d'un tenant pour se relire à côté d'elle
pub fn tour(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    d: &DemandeConversation,
) -> Result<Tour, String> {
    verifier_demande(d)?;
    if let Some(l) = d.llm.as_deref().filter(|l| !l.is_empty())
        && !llm::disponibles(&contexte.modeles).iter().any(|x| x == l)
    {
        return Err(format!("GGUF absent de modeles/llm : {l}"));
    }
    let textes: Vec<&str> = d
        .echanges
        .iter()
        .filter(|e| e.role == "user")
        .map(|e| e.content.as_str())
        .collect();
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let mut entrees = vec![d.contexte.as_str()];
    entrees.extend(textes.iter().copied());
    filtre.verifier(&entrees).map_err(|e| e.to_string())?;

    let wf = d.workflow.as_deref().map(workflows::obtenir).transpose()?;
    let reglage = d.workflow.as_deref().and_then(|w| {
        reglages::section(&contexte.dossier, "workflows")
            .and_then(|s| s.get(w).and_then(Value::as_object).cloned())
    });
    // Un généraliste : un spécialiste ne tient pas de conversation (comme en v2).
    let demande_nom = d.llm.clone().filter(|l| !l.is_empty()).or_else(|| {
        d.workflow
            .as_deref()
            .map(|w| llm::de_workflow(&contexte.dossier, &contexte.modeles, w))
    });
    let nom = llm::nom(&contexte.modeles, demande_nom.as_deref());
    let nom = if crate::fiches::specialiste(&nom).is_some() {
        llm::defaut(&contexte.modeles)
    } else {
        nom
    };
    let lancement = llm::lancement(&contexte.lieux(), Some(&nom))?;
    let regle = llm::valeurs(&contexte.dossier, &nom);

    let nom_cible = wf
        .as_ref()
        .and_then(|w| w.fiche.get("nom").and_then(Value::as_str))
        .unwrap_or("the target model");
    let mots = prompt::mots_max_de(wf.as_ref(), reglage.as_ref());
    let systeme_base = CONSIGNE
        .replace("{cible}", &format!("{nom_cible}, a still image"))
        .replace(
            "{contexte}",
            if d.contexte.trim().is_empty() {
                "nothing yet"
            } else {
                d.contexte.trim()
            },
        )
        .replace("{sujets_video}", "")
        .replace("{essentiels}", ESSENTIELS_IMAGE)
        .replace("{langue_prompt}", "in English")
        .replace(
            "{longueur}",
            &mots.map(|m| format!("At most {m} words.")).unwrap_or_default(),
        )
        .replace("{genre}", GENRE)
        .replace("{adultes}", ADULTES);
    let mut systeme = systeme_base;
    let cadre = cadre_format(d.format.as_deref());
    if !cadre.is_empty() {
        systeme.push_str("\n\n");
        systeme.push_str(&cadre);
    }
    let ignorees = rubriques_ignorees(wf.as_ref());
    let rubriques = rubriques_guide(d.sujet.as_deref(), &ignorees);
    let guide = catalogue_guide(&rubriques, lancement.contexte < CONTEXTE_CATALOGUE);
    if !guide.is_empty() {
        systeme.push_str("\n\n");
        systeme.push_str(&guide);
    }
    systeme.push_str("\n\n");
    systeme.push_str(&prompt::bloc_usage(
        "reecriture",
        &prompt::sections_prompt(wf.as_ref()),
    ));
    systeme.push_str("\n\n");
    systeme.push_str(CLAUSE_MINEURS);
    let noms_rubriques: Vec<String> = rubriques.into_iter().map(|(n, _, _)| n).collect();

    let ecrire = d
        .echanges
        .last()
        .is_some_and(|e| e.role == "user" && e.content.trim() == ECRIRE);
    let max_tokens = 1200.max(600 + prompt::tokens_pour_mots(mots));
    let pense = lancement.reflexion && ecrire;
    let (temperature, echantillonnage) = regle.ecriture(pense);
    let reserve = max_tokens as usize
        + if lancement.reflexion {
            BUDGET_REFLEXION as usize
        } else {
            0
        };
    let budget = (lancement.contexte as usize).saturating_sub(tokens_estimes(&systeme) + reserve);
    let conversation = elaguer(&d.echanges, budget);
    let mut messages: Vec<Value> = vec![json!({"role": "system", "content": systeme})];
    messages.extend(
        conversation
            .iter()
            .map(|e| json!({"role": e.role, "content": e.content})),
    );

    let debut = Instant::now();
    let mut lu = None;
    for essai in 0..2 {
        let demande = llama::Demande {
            messages: Value::Array(messages.clone()),
            temperature,
            max_tokens,
            reflechir: ecrire,
            echantillonnage: echantillonnage.clone(),
            ..llama::Demande::default()
        };
        let reponse = generateur.ecrire(lancement.clone(), &demande)?;
        match lire_tour(&reponse.texte, &noms_rubriques) {
            Ok(t) => {
                let mut sorties: Vec<&str> = vec![t.message.as_str()];
                sorties.extend(t.fiche.iter().map(|r| r.valeur.as_str()));
                for q in &t.questions {
                    sorties.push(q.question.as_str());
                    sorties.extend(q.choix.iter().map(String::as_str));
                }
                if let Some(p) = &t.prompt {
                    sorties.push(p.as_str());
                }
                let cjk = sorties.iter().any(|s| prompt::garde_cjk(s, &entrees) != *s);
                if cjk && essai == 0 {
                    messages.push(json!({"role": "assistant", "content": reponse.texte}));
                    messages.push(json!({"role": "user", "content": "Answer ONLY with the JSON object described, and write ONLY in French or English: no Chinese, Japanese or Korean characters."}));
                    continue;
                }
                // Toujours là au second essai : on retire les caractères, plutôt que d'échouer.
                let nettoyer = |s: &str| prompt::garde_cjk(s, &entrees);
                lu = Some(TourLu {
                    message: nettoyer(&t.message),
                    fiche: t
                        .fiche
                        .into_iter()
                        .map(|r| Rubrique {
                            valeur: nettoyer(&r.valeur),
                            nom: r.nom,
                        })
                        .collect(),
                    questions: t
                        .questions
                        .into_iter()
                        .map(|q| Question {
                            question: nettoyer(&q.question),
                            choix: q.choix.iter().map(|c| nettoyer(c)).collect(),
                            liste: q.liste,
                        })
                        .collect(),
                    prompt: t.prompt.map(|p| nettoyer(&p)),
                });
                break;
            }
            Err(e) if e.refus || essai == 1 => return Err(e.message),
            Err(_) => {
                messages.push(json!({"role": "assistant", "content": reponse.texte}));
                messages
                    .push(json!({"role": "user", "content": "Answer ONLY with the JSON object described."}));
            }
        }
    }
    let TourLu {
        message,
        fiche,
        questions,
        prompt: ecrit,
    } = lu.ok_or_else(|| "Réponse du LLM illisible".to_owned())?;
    let mut controles = entrees.clone();
    controles.extend(fiche.iter().map(|r| r.valeur.as_str()));
    controles.push(ecrit.as_deref().unwrap_or_default());
    filtre.verifier(&controles).map_err(|e| e.to_string())?;
    let mut fiche_json = Map::new();
    for r in &fiche {
        fiche_json.insert(r.nom.clone(), json!(r.valeur));
    }
    let brut =
        json!({"message": message, "fiche": fiche_json, "questions": questions, "prompt": ecrit}).to_string();
    Ok(Tour {
        message,
        fiche,
        questions,
        prompt: ecrit,
        brut,
        modele: lancement
            .modele
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        duree_s: (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dossier;
    use crate::comfy_serveur::Fichier;
    use crate::llama::Lancement;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    struct Faux {
        reponses: Vec<String>,
        demandes: Vec<llama::Demande>,
    }

    impl Generateur for Faux {
        fn ecrire(&mut self, _: Lancement, demande: &llama::Demande) -> Result<llama::Reponse, String> {
            self.demandes.push(demande.clone());
            let texte = if self.reponses.is_empty() {
                String::new()
            } else {
                self.reponses.remove(0)
            };
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

    fn contexte(temp: &tempfile::TempDir) -> Contexte {
        let modeles = temp.path().join("modeles");
        fs::create_dir_all(modeles.join("llm")).unwrap();
        fs::write(modeles.join("llm/mistral.gguf"), crate::gguf::essai::gguf(&[12])).unwrap();
        Contexte {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles,
            moteurs: temp.path().join("moteurs"),
            port_llama: 8095,
        }
    }

    fn demande(messages: &[(&str, &str)]) -> DemandeConversation {
        DemandeConversation {
            echanges: messages
                .iter()
                .map(|(r, c)| Echange {
                    role: (*r).to_owned(),
                    content: (*c).to_owned(),
                })
                .collect(),
            forme: "image".to_owned(),
            workflow: Some("krea2_turbo".to_owned()),
            sujet: Some("personnage".to_owned()),
            format: Some("16:9 (Widescreen)".to_owned()),
            ..DemandeConversation::default()
        }
    }

    #[test]
    fn un_tour_avec_questions() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let reponse = r#"<think>…</think>Voici : {"message": "J'ai compris un renard.", "fiche": {"Sujet": "un  renard roux", "Décor": "aucun choix"}, "questions": [{"question": "Quel décor ?", "choix": ["Forêt", "Neige", "", "Ville", "Plage"], "liste": "Lieu"}, {"question": " "}], "prompt": null}"#;
        let mut faux = Faux {
            reponses: vec![reponse.to_owned()],
            demandes: Vec::new(),
        };
        let t = tour(&ctx, &mut faux, &demande(&[("user", "un renard")])).unwrap();
        assert_eq!(t.message, "J'ai compris un renard.");
        assert_eq!(
            t.fiche,
            vec![Rubrique {
                nom: "Sujet".into(),
                valeur: "un renard roux".into()
            }]
        );
        assert_eq!(t.questions.len(), 1);
        assert_eq!(t.questions[0].choix, ["Forêt", "Neige", "Ville", "Plage"]);
        assert_eq!(t.questions[0].liste.as_deref(), Some("Lieu"));
        assert!(t.prompt.is_none());
        let systeme = faux.demandes[0].messages[0]["content"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(systeme.contains("Krea 2, a still image") && systeme.contains("landscape (wider than tall)"));
        assert!(systeme.contains("Guide topics") && systeme.contains("Non-negotiable rule"));
        assert!(!faux.demandes[0].reflechir);
        assert!(t.brut.contains("\"fiche\":{\"Sujet\":\"un renard roux\"}"));
    }

    #[test]
    fn ecrire_le_prompt_et_redemander_le_json() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            reponses: vec![
                "Bien sûr, voici le prompt.".to_owned(),
                r#"{"message": "Voilà.", "fiche": {}, "questions": [], "prompt": "**A red fox** in snow"}"#
                    .to_owned(),
            ],
            demandes: Vec::new(),
        };
        let d = demande(&[("user", "un renard"), ("assistant", "{}"), ("user", ECRIRE)]);
        let t = tour(&ctx, &mut faux, &d).unwrap();
        assert_eq!(t.prompt.as_deref(), Some("A red fox in snow"));
        assert_eq!(faux.demandes.len(), 2);
        assert!(
            faux.demandes[0].reflechir,
            "le tour « ÉCRIS LE PROMPT » réfléchit"
        );
        let mut refus = Faux {
            reponses: vec!["I'm sorry, I can't help with that.".to_owned()],
            demandes: Vec::new(),
        };
        assert!(
            tour(&ctx, &mut refus, &demande(&[("user", "x")]))
                .unwrap_err()
                .contains("refuse")
        );
        assert_eq!(refus.demandes.len(), 1, "un refus n'est pas redemandé");
    }

    #[test]
    fn demandes_refusees() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            reponses: Vec::new(),
            demandes: Vec::new(),
        };
        assert!(tour(&ctx, &mut faux, &demande(&[("assistant", "x")])).is_err());
        let mut d = demande(&[("user", "x")]);
        d.forme = "clip".into();
        assert!(tour(&ctx, &mut faux, &d).is_err());
        let mut d = demande(&[("user", "x")]);
        d.format = Some("5:4".into());
        assert!(tour(&ctx, &mut faux, &d).is_err());
        assert!(
            tour(
                &ctx,
                &mut faux,
                &demande(&[("user", "a twelve-year-old girl, nude")])
            )
            .unwrap_err()
            .starts_with("ERREUR AS-18")
        );
        assert!(faux.demandes.is_empty());
    }

    #[test]
    fn elagage_d_une_longue_conversation() {
        let long = "x".repeat(300);
        let echanges: Vec<Echange> = (0..8)
            .map(|i| Echange {
                role: if i % 2 == 0 { "user" } else { "assistant" }.to_owned(),
                content: long.clone(),
            })
            .collect();
        assert_eq!(elaguer(&echanges, 10_000).len(), 8);
        let court = elaguer(&echanges, 400);
        assert_eq!(court[0], echanges[0]);
        assert_eq!(court[1].role, "assistant");
        assert!(court.len() < 8);
        assert!(cadre_format(Some("1:1 (Square)")).contains("square"));
        assert_eq!(cadre_format(None), "");
    }
}
