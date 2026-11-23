//! « Proposer des directions » (nouveauté de la v3) : au Scénario, à partir d'une envie vague (« un clip mélancolique
//! en ville »), le LLM propose trois directions distinctes avant d'écrire le prompt. L'utilisateur en choisit une, la
//! retouche s'il veut, puis enchaîne sur ✨ Écrire le prompt ou 💬 M'aider à préciser.
//!
//! Même mécanique que l'écriture du Scénario : un généraliste (un spécialiste n'écrit que des prompts), une consigne
//! réglable dans Réglages, AS-18 avant et après, ✕ Annuler. La réponse est contrôlée (trois directions distinctes,
//! pas vides, pas déjà proposées) et redemandée une fois si elle a des défauts ; des deux essais, le meilleur est
//! gardé et ce qui reste s'affiche.

use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::llama::{self, Lancement};
use crate::plateau::{Contexte, Generateur};
use crate::prompt::{self, CLAUSE_MINEURS};
use crate::{creations, fiches, llm, securite, workflows};

/// Les ateliers qui proposent des directions.
pub const ATELIERS: [&str; 4] = ["image", "retouche", "clip", "sequence"];
/// Les directions d'une proposition.
pub const NOMBRE: usize = 3;
/// L'envie, au plus.
pub const LONGUEUR_ENVIE: usize = 2000;
/// Ce qui est déjà fixé (les compléments), au plus.
pub const LONGUEUR_COMPLEMENTS: usize = 4000;
/// Les directions déjà proposées données au LLM pour qu'il s'en écarte, au plus (les plus récentes).
pub const ECARTEES_MAX: usize = 12;
/// Une direction : titre, texte et différence, au plus.
const LONGUEUR_TITRE: usize = 80;
const LONGUEUR_TEXTE: usize = 700;
const LONGUEUR_DIFFERENCE: usize = 240;
/// Une direction plus courte que ça n'a pas ses deux ou trois phrases.
const MOTS_MIN: usize = 12;
/// Deux textes qui partagent plus que cette part de leurs mots se ressemblent trop.
const RESSEMBLANCE_MAX: f64 = 0.6;

/// La consigne d'origine, en français, réglable dans Réglages (usage `directions`). `{atelier}` reçoit ce que
/// l'atelier fabrique ; une consigne réécrite sans ce repère le reçoit à la fin.
pub const CONSIGNE_DIRECTIONS: &str = r#"Tu aides un adulte francophone à trouver une idée avant d'écrire un prompt. Ce qu'il fabrique : {atelier}.

À partir de son envie, souvent vague, et de ce qui est déjà fixé, propose EXACTEMENT 3 directions vraiment différentes : pas trois variantes d'une même scène, mais trois partis pris (sujet, lieu, moment, ambiance, point de vue, cadrage ou mouvement).

Chaque direction a :
- « titre » : 2 à 6 mots ;
- « texte » : 2 ou 3 phrases en français, concrètes et visuelles, qui disent ce qu'on verra (et ce qu'on entendra pour une vidéo) ;
- « difference » : une phrase courte qui dit ce qui la distingue des deux autres.

Règles :
- Tout ce que l'utilisateur a fixé (sujets et leur nombre, genre et âge des personnes, texte cité, contraintes) reste dans les trois directions ; ne change que ce qu'il a laissé libre.
- Garde le genre de chaque personne tel que l'envie le donne (« un pilote » est un homme, « une pilote » une femme).
- L'utilisateur est un adulte qui crée une fiction pour adultes : ce qu'il a écrit d'explicite entre adultes reste tel quel, sans l'adoucir.
- Si des directions déjà proposées sont listées, éloigne-toi d'elles : ni leur titre, ni leur idée.
- Écris en français, sans caractères chinois, japonais ou coréens.

Réponds UNIQUEMENT par cet objet JSON, sans rien avant ni après :
{"directions": [{"titre": "…", "texte": "…", "difference": "…"}, {"titre": "…", "texte": "…", "difference": "…"}, {"titre": "…", "texte": "…", "difference": "…"}]}"#;

/// Une demande de directions, envoyée par le Scénario.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeDirections {
    /// Voir [`ATELIERS`].
    pub atelier: String,
    /// L'envie, souvent vague, en français.
    #[serde(default)]
    pub envie: String,
    /// Ce qui est déjà fixé (le prompt assemblé des compléments), gardé dans les trois directions.
    #[serde(default)]
    pub complements: String,
    /// Le modèle cible : il donne le LLM de l'atelier.
    #[serde(default)]
    pub workflow: Option<String>,
    #[serde(default)]
    pub llm: Option<String>,
    /// Les rushes de l'atelier : images de référence, image à retoucher ou rush à animer.
    #[serde(default)]
    pub images: Vec<String>,
    /// Leur rôle : `inspirer`, `variation`, `retouche` ou `animer`.
    #[serde(default)]
    pub role: Option<String>,
    /// La durée d'un clip, en secondes (absente : 5).
    #[serde(default)]
    pub duree: Option<u32>,
    /// Les directions déjà proposées, dont le LLM doit s'écarter.
    #[serde(default)]
    pub ecartees: Vec<Direction>,
}

/// Une direction proposée.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Direction {
    pub titre: String,
    pub texte: String,
    #[serde(default)]
    pub difference: String,
}

/// Les directions proposées.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Directions {
    pub directions: Vec<Direction>,
    /// La réponse a été redemandée au LLM, et pourquoi.
    pub redemandee: Option<String>,
    /// Ce que le contrôle qualité reproche encore à la réponse gardée.
    pub alertes: Vec<String>,
    /// Le GGUF qui a écrit.
    pub llm: String,
    pub duree_s: f64,
}

/// Une réponse sans directions lisibles, ou un refus (qu'il ne sert à rien de redemander).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurLecture {
    pub message: String,
    pub refus: bool,
}

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

fn champ(v: &Value, cles: &[&str]) -> String {
    cles.iter()
        .find_map(|c| v.get(*c))
        .map(|x| match x {
            Value::String(s) => s.clone(),
            autre => autre.to_string(),
        })
        .map(|s| prompt::nettoyer_markdown(&s))
        .unwrap_or_default()
}

/// Les directions d'une réponse du LLM : l'objet `{"directions": [...]}`, ou la liste seule.
///
/// # Errors
/// Réponse vide, sans JSON lisible, sans direction, ou refus du LLM.
pub fn lire_directions(texte: &str) -> Result<Vec<Direction>, ErreurLecture> {
    let texte = texte.rsplit_once("</think>").map_or(texte, |(_, apres)| apres);
    let morceau = |ouvre: char, ferme: char| {
        texte
            .find(ouvre)
            .zip(texte.rfind(ferme))
            .filter(|(debut, fin)| debut < fin)
            .and_then(|(debut, fin)| texte.get(debut..=fin))
            .and_then(|j| serde_json::from_str::<Value>(j).ok())
    };
    let liste = match morceau('{', '}') {
        Some(Value::Object(o)) => o.get("directions").and_then(Value::as_array).cloned(),
        _ => None,
    }
    .or_else(|| morceau('[', ']').and_then(|v| v.as_array().cloned()));
    let Some(liste) = liste else {
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
                    "Le LLM refuse cette demande : « {} ». Un autre LLM de la liste peut l'accepter.",
                    borne(texte, 160)
                ),
                refus: true,
            });
        }
        return Err(ErreurLecture {
            message: format!(
                "Réponse du LLM illisible (pas de directions en JSON) : « {} »",
                borne(texte, 160)
            ),
            refus: false,
        });
    };
    let directions: Vec<Direction> = liste
        .iter()
        .filter(|v| v.is_object())
        .map(|v| Direction {
            titre: borne(&champ(v, &["titre", "title", "nom"]), LONGUEUR_TITRE),
            texte: borne(&champ(v, &["texte", "text", "description"]), LONGUEUR_TEXTE),
            difference: borne(
                &champ(v, &["difference", "différence", "distinction"]),
                LONGUEUR_DIFFERENCE,
            ),
        })
        .filter(|d| !d.titre.is_empty() || !d.texte.is_empty())
        .collect();
    if directions.is_empty() {
        return Err(ErreurLecture {
            message: "Réponse du LLM sans direction".to_owned(),
            refus: false,
        });
    }
    Ok(directions)
}

/// Les mots d'un texte, en minuscules, sans les petits mots.
fn mots(texte: &str) -> HashSet<String> {
    texte
        .split(|c: char| !c.is_alphanumeric())
        .filter(|m| m.chars().count() > 3)
        .map(str::to_lowercase)
        .collect()
}

/// La part de mots partagés par deux textes (indice de Jaccard), de 0 à 1.
fn ressemblance(a: &str, b: &str) -> f64 {
    let (a, b) = (mots(a), mots(b));
    let union = a.union(&b).count();
    if union == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)] // quelques dizaines de mots
    let part = a.intersection(&b).count() as f64 / union as f64;
    part
}

fn meme_titre(a: &str, b: &str) -> bool {
    let propre = |t: &str| espaces(t).to_lowercase();
    !a.trim().is_empty() && propre(a) == propre(b)
}

fn se_ressemblent(a: &Direction, b: &Direction) -> bool {
    meme_titre(&a.titre, &b.titre) || ressemblance(&a.texte, &b.texte) > RESSEMBLANCE_MAX
}

/// Ce que le contrôle qualité reproche à une proposition, en phrases que l'utilisateur et le LLM lisent.
pub fn defauts(directions: &[Direction], ecartees: &[Direction]) -> Vec<String> {
    let mut sortie = Vec::new();
    if directions.len() != NOMBRE {
        sortie.push(format!(
            "il faut exactement {NOMBRE} directions, la réponse en donne {}",
            directions.len()
        ));
    }
    if directions
        .iter()
        .any(|d| d.titre.is_empty() || d.texte.is_empty())
    {
        sortie.push("une direction n'a pas de titre ou pas de texte".to_owned());
    }
    if directions
        .iter()
        .any(|d| !d.texte.is_empty() && d.texte.split_whitespace().count() < MOTS_MIN)
    {
        sortie.push("une direction est trop courte : deux ou trois phrases attendues".to_owned());
    }
    let paires = directions
        .iter()
        .enumerate()
        .any(|(i, a)| directions.iter().skip(i + 1).any(|b| se_ressemblent(a, b)));
    if paires {
        sortie.push("deux directions se ressemblent trop".to_owned());
    }
    if directions
        .iter()
        .any(|d| ecartees.iter().any(|e| se_ressemblent(d, e)))
    {
        sortie.push("une direction reprend une direction déjà proposée".to_owned());
    }
    sortie
}

/// Ce que l'atelier fabrique, pour la consigne.
fn atelier_de(atelier: &str, duree: u32, images: bool) -> String {
    match atelier {
        "retouche" => "une retouche d'image : ce qui change dans l'image jointe, le reste gardé".to_owned(),
        "clip" if images => format!(
            "un clip vidéo de {duree} s avec le son, qui anime l'image jointe (action, mouvement de caméra, ambiance \
             sonore)"
        ),
        "clip" => {
            format!("un clip vidéo de {duree} s avec le son (action, mouvement de caméra, ambiance sonore)")
        }
        "sequence" => {
            "une courte séquence vidéo en plusieurs plans, avec le son : chaque direction donne le fil de \
                       l'histoire, du début à la fin"
                .to_owned()
        }
        _ if images => "une image fixe, à partir des images jointes".to_owned(),
        _ => "une image fixe".to_owned(),
    }
}

/// La consigne : celle de Réglages pour ce GGUF, sinon celle d'origine ; l'atelier à son repère ; la clause
/// verrouillée sur les mineurs toujours à la fin.
pub fn consigne(reecrite: Option<&str>, atelier: &str) -> String {
    let base = reecrite
        .filter(|c| !c.trim().is_empty())
        .unwrap_or(CONSIGNE_DIRECTIONS)
        .trim_end();
    let texte = if base.contains("{atelier}") {
        base.replace("{atelier}", atelier)
    } else {
        format!("{base}\n\nCe qu'il fabrique : {atelier}.")
    };
    format!("{texte}\n\n{CLAUSE_MINEURS}")
}

/// Le message de l'utilisateur : son envie, ce qui est fixé, et les directions à éviter.
fn message(d: &DemandeDirections, ecartees: &[Direction]) -> String {
    let envie = d.envie.trim();
    let mut lignes = vec![if envie.is_empty() {
        "Envie : aucune, pars de ce qui est déjà fixé.".to_owned()
    } else {
        format!("Envie : {envie}")
    }];
    let fixe = d.complements.trim();
    lignes.push(if fixe.is_empty() {
        "Déjà fixé : rien.".to_owned()
    } else {
        format!("Déjà fixé, à garder dans les trois directions : {fixe}")
    });
    if !ecartees.is_empty() {
        lignes.push("Directions déjà proposées, à ne pas reprendre :".to_owned());
        lignes.extend(ecartees.iter().map(|e| format!("- {} : {}", e.titre, e.texte)));
    }
    lignes.join("\n")
}

fn verifier(d: &DemandeDirections) -> Result<(), String> {
    if !ATELIERS.contains(&d.atelier.as_str()) {
        return Err("Atelier inconnu : image, retouche, clip ou séquence".to_owned());
    }
    if d.envie.chars().count() > LONGUEUR_ENVIE {
        return Err(format!("Envie de {LONGUEUR_ENVIE} caractères au plus"));
    }
    if d.complements.chars().count() > LONGUEUR_COMPLEMENTS {
        return Err(format!(
            "Compléments de {LONGUEUR_COMPLEMENTS} caractères au plus"
        ));
    }
    if d.envie.trim().is_empty() && d.complements.trim().is_empty() && d.images.is_empty() {
        return Err("Écris d'abord une envie, même vague".to_owned());
    }
    Ok(())
}

/// Le GGUF qui propose : celui choisi, sinon celui du modèle cible ; un généraliste (un spécialiste n'écrit que des
/// prompts, comme pour le découpage et la conversation).
fn llm_de(contexte: &Contexte, d: &DemandeDirections) -> String {
    let workflow = d.workflow.clone().filter(|w| !w.is_empty()).or_else(|| {
        (d.atelier == "sequence" || d.atelier == "clip")
            .then(|| {
                crate::montage::workflow_de(
                    contexte,
                    if d.atelier == "clip" && !d.images.is_empty() {
                        "image_vers_video"
                    } else {
                        "texte_vers_video"
                    },
                )
            })
            .filter(|w| !w.is_empty())
    });
    let demande = d
        .llm
        .clone()
        .filter(|l| !l.is_empty())
        .or_else(|| workflow.map(|w| llm::de_workflow(&contexte.dossier, &contexte.modeles, &w)));
    let nom = llm::nom(&contexte.dossier, &contexte.modeles, demande.as_deref());
    if fiches::specialiste(&nom).is_some() {
        llm::defaut(&contexte.dossier, &contexte.modeles)
    } else {
        nom
    }
}

/// Une proposition lue : ses directions (nettoyées des caractères CJK inattendus), ce qui lui manque, et si elle
/// en avait.
type Lue = (Vec<Direction>, Vec<String>, bool);

fn lire_et_juger(texte: &str, entrees: &[&str], ecartees: &[Direction]) -> Result<Lue, ErreurLecture> {
    let brutes = lire_directions(texte)?;
    let nettoyer = |s: &str| prompt::garde_cjk(s, entrees);
    let mut cjk = false;
    let directions: Vec<Direction> = brutes
        .into_iter()
        .map(|d| {
            let propre = Direction {
                titre: nettoyer(&d.titre),
                texte: nettoyer(&d.texte),
                difference: nettoyer(&d.difference),
            };
            cjk |= propre != d;
            propre
        })
        .collect();
    let manques = defauts(&directions, ecartees);
    Ok((directions, manques, cjk))
}

/// Propose trois directions pour l'envie de l'utilisateur.
///
/// # Errors
/// Demande invalide, LLM absent, filtre AS-18, réponse illisible ou refus, annulation, ou llama-server qui échoue.
#[allow(clippy::too_many_lines)] // la mécanique d'écriture du Scénario, gardée d'un tenant
pub fn proposer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    d: &DemandeDirections,
) -> Result<Directions, String> {
    verifier(d)?;
    crate::scenario::verifier_llm(contexte, d.llm.as_deref())?;
    let ecartees: Vec<Direction> = d
        .ecartees
        .iter()
        .rev()
        .take(ECARTEES_MAX)
        .rev()
        .map(|e| Direction {
            titre: borne(&e.titre, LONGUEUR_TITRE),
            texte: borne(&e.texte, LONGUEUR_TEXTE),
            difference: String::new(),
        })
        .collect();
    let ids: Vec<String> = d
        .images
        .iter()
        .filter(|i| !i.is_empty())
        .take(workflows::REFERENCES_MAX)
        .cloned()
        .collect();
    // AS-18 : l'envie, ce qui est fixé, les directions déjà proposées et les textes d'origine des images (un rush né
    // d'un mineur le reste).
    let origines = creations::textes_origine(&contexte.data, &ids)?;
    let mut entrees: Vec<&str> = vec![d.envie.as_str(), d.complements.as_str()];
    entrees.extend(origines.iter().map(String::as_str));
    for e in &ecartees {
        entrees.push(e.titre.as_str());
        entrees.push(e.texte.as_str());
    }
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    filtre.verifier(&entrees).map_err(|e| e.to_string())?;
    let vues = ids
        .iter()
        .map(|id| crate::scenario::vue(contexte, id))
        .collect::<Result<Vec<_>, String>>()?;

    let nom = llm_de(contexte, d);
    let lancement: Lancement = llm::lancement(&contexte.lieux(), Some(&nom))?;
    let regle = llm::valeurs(&contexte.dossier, &nom);
    let duree = d.duree.filter(|s| (1..=60).contains(s)).unwrap_or(5);
    let reecrite = llm::consigne(&contexte.dossier, &nom, "directions");
    let systeme = consigne(
        reecrite.as_deref(),
        &atelier_de(&d.atelier, duree, !vues.is_empty()),
    );
    // Les images vont avec le message : le LLM les voit, ou lit leur fiche.
    let role = match (d.role.as_deref().filter(|r| !r.is_empty()), d.atelier.as_str()) {
        (Some(r), _) => r,
        (None, "retouche") => "retouche",
        (None, "clip") => "animer",
        (None, _) => "references",
    };
    let usage_vue = match role {
        "animer" | "variation" | "retouche" => role,
        _ => "references",
    };
    let contenu = prompt::montrer(
        &message(d, &ecartees),
        &vues,
        usage_vue,
        lancement.vision.is_some(),
        None,
    );
    let (temperature, echantillonnage) = regle.ecriture(false);
    let mut messages = vec![
        json!({"role": "system", "content": systeme}),
        json!({"role": "user", "content": contenu}),
    ];

    let debut = Instant::now();
    // Le meilleur essai : ses directions et ce qui leur manque encore.
    let mut meilleur: Option<(Vec<Direction>, Vec<String>)> = None;
    let mut redemandee: Option<String> = None;
    let mut derniere_erreur = "Réponse du LLM illisible".to_owned();
    for essai in 0..2 {
        let demande = llama::Demande {
            messages: Value::Array(messages.clone()),
            temperature,
            max_tokens: regle.max_tokens.max(900),
            reflechir: false,
            echantillonnage: echantillonnage.clone(),
            ..llama::Demande::default()
        };
        let reponse = match generateur.ecrire(lancement.clone(), &demande) {
            Ok(r) => r,
            // Une annulation arrête tout ; une panne au second essai garde le premier.
            Err(e) if essai == 0 || e == crate::comfy_serveur::ANNULEE => return Err(e),
            Err(_) => break,
        };
        let problemes: Vec<String> = match lire_et_juger(&reponse.texte, &entrees, &ecartees) {
            Err(e) if e.refus => return Err(e.message),
            Err(e) => {
                derniere_erreur = e.message;
                vec!["la réponse n'est pas l'objet JSON demandé".to_owned()]
            }
            Ok((directions, manques, cjk)) => {
                let mieux = meilleur.as_ref().is_none_or(|(_, m)| manques.len() < m.len());
                let mut problemes = manques.clone();
                if mieux {
                    meilleur = Some((directions, manques));
                }
                if cjk {
                    problemes.push("des caractères chinois, japonais ou coréens".to_owned());
                }
                problemes
            }
        };
        if problemes.is_empty() || essai == 1 {
            break;
        }
        redemandee = Some(problemes.join(" ; "));
        messages.push(json!({"role": "assistant", "content": reponse.texte}));
        messages.push(json!({"role": "user", "content": format!(
            "Ta réponse a ces défauts : {}. Recommence en les corrigeant. Réponds UNIQUEMENT par l'objet JSON demandé, \
             en français.",
            problemes.join(" ; ")
        )}));
    }
    let (mut directions, restants) = meilleur.ok_or(derniere_erreur)?;
    directions.truncate(NOMBRE);

    // AS-18 : tout ce que le LLM a écrit, relu avec ce qui l'a nourri (un mineur reste un mineur, reformulé ou non).
    let mut controles = entrees.clone();
    for p in &directions {
        controles.push(p.titre.as_str());
        controles.push(p.texte.as_str());
        controles.push(p.difference.as_str());
    }
    filtre.verifier(&controles).map_err(|e| e.to_string())?;
    let alertes = restants
        .iter()
        .map(|r| format!("Contrôle qualité : {r}"))
        .collect();
    Ok(Directions {
        directions,
        redemandee,
        alertes,
        llm: lancement
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

    fn direction(titre: &str, texte: &str) -> Direction {
        Direction {
            titre: titre.to_owned(),
            texte: texte.to_owned(),
            difference: "Elle se distingue.".to_owned(),
        }
    }

    const BONNE: &str = r#"Voici : {"directions": [
        {"titre": "Pluie de néons", "texte": "Une femme seule attend sous un abribus, les néons roses se reflètent dans les flaques. La caméra recule lentement pendant que le bus passe sans s'arrêter.", "difference": "La nuit et la couleur."},
        {"titre": "Dimanche gris", "texte": "Un vieil homme nourrit des pigeons sur une place vide, un matin de brume. Plan fixe large, on entend une cloche lointaine et le froissement des ailes.", "difference": "Le calme et le temps qui passe."},
        {"titre": "Dernier métro", "texte": "Deux amis se quittent sur un quai de métro désert. Travelling latéral le long de la rame qui s'éloigne, le grondement s'efface jusqu'au silence.", "difference": "Une séparation, le mouvement."}
    ]}"#;

    fn demande() -> DemandeDirections {
        DemandeDirections {
            atelier: "clip".to_owned(),
            envie: "un clip mélancolique en ville".to_owned(),
            ..DemandeDirections::default()
        }
    }

    #[test]
    fn lecture_des_directions() {
        let d = lire_directions(BONNE).unwrap();
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].titre, "Pluie de néons");
        assert!(d[1].texte.starts_with("Un vieil homme"));
        // La liste seule, des clés en anglais, du Markdown.
        let seule = lire_directions(r#"[{"title": "**Aube**", "text": "Un texte."}]"#).unwrap();
        assert_eq!(seule[0].titre, "Aube");
        assert!(lire_directions("").unwrap_err().message.contains("vide"));
        assert!(!lire_directions("Voici mes idées.").unwrap_err().refus);
        assert!(
            lire_directions("I'm sorry, I can't help with that.")
                .unwrap_err()
                .refus
        );
        assert!(lire_directions(r#"{"directions": []}"#).is_err());
    }

    #[test]
    fn controle_qualite() {
        let bonnes = lire_directions(BONNE).unwrap();
        assert!(defauts(&bonnes, &[]).is_empty());
        let deux = &bonnes[..2];
        assert!(defauts(deux, &[])[0].contains("exactement 3"));
        let mut courte = bonnes.clone();
        courte[2].texte = "Un métro.".to_owned();
        assert!(defauts(&courte, &[]).iter().any(|d| d.contains("trop courte")));
        let mut double = bonnes.clone();
        double[2] = double[0].clone();
        assert!(defauts(&double, &[]).iter().any(|d| d.contains("se ressemblent")));
        let deja = [direction("pluie de NÉONS", "Autre chose.")];
        assert!(
            defauts(&bonnes, &deja)
                .iter()
                .any(|d| d.contains("déjà proposée"))
        );
    }

    #[test]
    fn trois_directions_du_premier_coup() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            reponses: vec![Ok(BONNE.to_owned())],
            demandes: Vec::new(),
        };
        let r = proposer(&ctx, &mut faux, &demande()).unwrap();
        assert_eq!(r.directions.len(), 3);
        assert!(r.redemandee.is_none() && r.alertes.is_empty());
        assert_eq!(faux.demandes.len(), 1);
        let systeme = faux.demandes[0].messages[0]["content"].as_str().unwrap();
        assert!(systeme.contains("un clip vidéo de 5 s") && systeme.contains("Non-negotiable rule"));
        let message = faux.demandes[0].messages[1]["content"].as_str().unwrap();
        assert!(message.contains("Envie : un clip mélancolique en ville"));
    }

    #[test]
    fn redemandee_une_fois_puis_la_meilleure() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        // D'abord deux directions seulement, puis les trois.
        let deux = r#"{"directions": [{"titre": "A", "texte": "Une femme seule attend sous un abribus, les néons roses se reflètent dans les flaques de la rue."}, {"titre": "B", "texte": "Un vieil homme nourrit des pigeons sur une place vide, un matin de brume et de cloches."}]}"#;
        let mut faux = Faux {
            reponses: vec![Ok(deux.to_owned()), Ok(BONNE.to_owned())],
            demandes: Vec::new(),
        };
        let r = proposer(&ctx, &mut faux, &demande()).unwrap();
        assert_eq!(r.directions.len(), 3);
        assert!(r.redemandee.as_deref().unwrap().contains("exactement 3"));
        assert!(r.alertes.is_empty());
        let correction = faux.demandes[1].messages[3]["content"].as_str().unwrap();
        assert!(correction.contains("exactement 3"));
        // Deux fois incomplet : la meilleure réponse est gardée, avec son alerte.
        let mut faux = Faux {
            reponses: vec![Ok(deux.to_owned()), Ok("pas de JSON".to_owned())],
            demandes: Vec::new(),
        };
        let r = proposer(&ctx, &mut faux, &demande()).unwrap();
        assert_eq!(r.directions.len(), 2);
        assert!(r.alertes[0].starts_with("Contrôle qualité : il faut exactement 3"));
    }

    #[test]
    fn panne_annulation_et_refus() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let deux = r#"{"directions": [{"titre": "A", "texte": "Une femme seule attend sous un abribus, les néons roses se reflètent dans les flaques de la rue."}]}"#;
        // Une panne au second essai garde le premier.
        let mut faux = Faux {
            reponses: vec![Ok(deux.to_owned()), Err("llama-server a planté".to_owned())],
            demandes: Vec::new(),
        };
        assert_eq!(proposer(&ctx, &mut faux, &demande()).unwrap().directions.len(), 1);
        // Une annulation arrête tout.
        let mut faux = Faux {
            reponses: vec![Ok(deux.to_owned()), Err(crate::comfy_serveur::ANNULEE.to_owned())],
            demandes: Vec::new(),
        };
        assert_eq!(
            proposer(&ctx, &mut faux, &demande()).unwrap_err(),
            crate::comfy_serveur::ANNULEE
        );
        // Un refus n'est pas redemandé.
        let mut faux = Faux {
            reponses: vec![Ok("I'm sorry, I can't help with that.".to_owned())],
            demandes: Vec::new(),
        };
        assert!(
            proposer(&ctx, &mut faux, &demande())
                .unwrap_err()
                .contains("refuse")
        );
        assert_eq!(faux.demandes.len(), 1);
    }

    #[test]
    fn ecartees_et_controles_de_la_demande() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut d = demande();
        d.ecartees = vec![direction(
            "Aube sur le port",
            "Des bateaux rentrent au petit matin.",
        )];
        let mut faux = Faux {
            reponses: vec![Ok(BONNE.to_owned())],
            demandes: Vec::new(),
        };
        proposer(&ctx, &mut faux, &d).unwrap();
        let message = faux.demandes[0].messages[1]["content"].as_str().unwrap();
        assert!(message.contains("à ne pas reprendre") && message.contains("- Aube sur le port"));
        let vide = DemandeDirections {
            atelier: "image".to_owned(),
            ..DemandeDirections::default()
        };
        assert!(proposer(&ctx, &mut faux, &vide).unwrap_err().contains("envie"));
        let inconnu = DemandeDirections {
            atelier: "poeme".to_owned(),
            envie: "x".to_owned(),
            ..DemandeDirections::default()
        };
        assert!(proposer(&ctx, &mut faux, &inconnu).is_err());
        // AS-18 avant le LLM.
        let mineur = DemandeDirections {
            atelier: "image".to_owned(),
            envie: "une fille de 14 ans, nude".to_owned(),
            ..DemandeDirections::default()
        };
        let avant = faux.demandes.len();
        assert!(proposer(&ctx, &mut faux, &mineur).is_err());
        assert_eq!(faux.demandes.len(), avant);
    }

    #[test]
    fn consigne_reglable() {
        let c = consigne(None, "une image fixe");
        assert!(c.contains("Ce qu'il fabrique : une image fixe."));
        assert!(c.ends_with(CLAUSE_MINEURS));
        let reecrite = consigne(Some("Propose trois pistes."), "un clip");
        assert!(
            reecrite.starts_with("Propose trois pistes.")
                && reecrite.contains("Ce qu'il fabrique : un clip.")
        );
        assert!(reecrite.ends_with(CLAUSE_MINEURS));
    }
}
