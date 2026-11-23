//! Le découpage d'une séquence en plans, comme en v2 (`app/decoupage.py`) : au Scénario, atelier Séquence, le LLM écrit
//! le prompt de chaque plan, tous ensemble (✨ Tout écrire) ou un seul (✨ d'un plan), puis les garde-fous d'ici le
//! contrôlent : étiquettes de plan retirées, répliques inventées retirées, réplique du plan mot pour mot à son moment,
//! description du personnage remise. Le stockage, les graphes et le tournage restent dans `sequences`.
//!
//! AS-18 : le fil de l'histoire, le personnage, le décor, la contrainte, la musique, les notes, les répliques et les
//! textes d'origine des rushes passent par le filtre avant l'écriture, puis avec tout ce que le LLM a écrit.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Instant;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::llama::Demande;
use crate::plateau::{Contexte, Generateur};
use crate::sequences::{DEPARTS, ENCHAINEMENTS, LONGUEUR_CONTRAINTE, PLANS_MAX};
use crate::workflows::{self, FORMATS, Workflow};
use crate::{
    conversation, creations, fiches, h3, listes, llm, montage, prompt, reglages, scenario, securite,
};

/// Le fil de l'histoire, le personnage et le décor, au plus (en caractères).
pub const LONGUEUR_SYNOPSIS: usize = 2000;
/// « Ce qui se passe » dans un plan.
pub const LONGUEUR_NOTE: usize = 500;
/// La réplique d'un plan.
pub const LONGUEUR_REPLIQUE: usize = 300;
/// Le prompt déjà écrit d'un plan, donné en contexte à ses voisins.
pub const LONGUEUR_PROMPT_PLAN: usize = 2000;
/// La musique d'ambiance de la séquence.
pub const LONGUEUR_MUSIQUE: usize = 200;
/// Débit de parole confortable : estimation d'Aria, aucun guide de H3 n'en donne.
pub const MOTS_PAR_SECONDE: f64 = 2.5;
/// Le moment de la réplique dans le plan. Le guide de H3 suit une chronologie et cale une réplique sur ce qu'on voit,
/// jamais sur un minutage.
pub const MOMENTS: [(&str, &str); 3] = [
    ("debut", "at the start of the shot, before the action begins"),
    ("pendant", "in the middle of the action, while it goes on"),
    ("fin", "at the end of the shot, after the action, as a reaction"),
];
/// Le type d'un plan, tel que le LLM le lit.
const TYPES: [(&str, &str); 5] = [
    ("rush", "start from the picture"),
    ("texte", "start"),
    ("prolonger", "continue"),
    ("couper", "cut"),
    ("transition", "transition"),
];
/// La longueur d'un prompt de plan sans modèle cible, en mots.
const MOTS_DEFAUT: u64 = 110;
/// Un clip plus long porte plus d'actions : chaque seconde au-delà de 5 s ajoute ces mots (estimation de la v2).
const MOTS_PAR_SECONDE_EN_PLUS: u64 = 8;
const DUREE_DE_BASE: u32 = 5;

/// La consigne d'origine du découpage, mot pour mot celle de la v2 (`prompt.CONSIGNE_DECOUPAGE`) : tout le
/// découpage en un seul appel, pour que le LLM voie l'histoire entière. `{mots_max}` et `{regle_personnage}` sont
/// remplis ici.
pub const CONSIGNE_DECOUPAGE: &str = r#"You are the storyboard writer for an audio-video generation model: it creates the picture AND the sound together.
The user gives, often in French, a short story, the look of the character and of the setting, and the list of shots with their type and duration. Write ONE English prompt per shot.

Rules for every shot:
1. A short production brief in the present tense: shot size, what the character does, the setting, ONE named camera movement with its speed, then concrete sounds. 40 to {mots_max} words, plain prose, and never more than the word limit given in the shot's line.
2. One continuous take: never write "cut to", never switch place or angle inside a shot (the next shot does that).
3. Dialogue: use only the lines in quotes from the story or the shot notes, and each shot's "line", word for word. Write every spoken line inside a <d> tag that starts with its language, after the speaker's description and an ID in parentheses, (S1) for the character: The character (S1) says: <d>[French] Je descends ici.</d> No ellipses; double quotes are only for text visible on screen, never for a spoken line. A shot's "line" belongs to that shot, at the moment it gives: tie it to the visible action in the timeline of the brief (at the start: the character speaks, then the action begins; in the middle: the line falls during the action; at the end: after the action, as a reaction), never to timecodes. A line is spoken in the language it is written in (a French line is spoken in French), whatever the character's origin. Never translate a line, never invent one: a shot without a line has nobody speaking, nobody murmurs or says anything.
4. Tell the story in order, spread over the shots; follow each shot's note when there is one. Few actions per shot: fit its duration.
5. Shot types:
- start: the first shot; it sets up the character and the place.
- start from the picture: it animates the reference picture; describe only what moves (motion, camera, sounds).
- continue: the same take goes on from the last image of the previous shot: same place, same light, same framing at first. Begin with "Continuous shot, same scene." A continue shot only receives the last images of the shot before it, never the reference picture: when the next shot is a continue shot, keep the character's face in the frame until the end of this one.
- cut: a new angle or a new moment of the story.
- transition: ONE continuous camera movement travelling from the last image of the previous shot to the given arrival; the scene changes progressively over the whole clip, never all at once at the end.
6. Character: {regle_personnage} Keep each person's gender exactly as the idea gives it: in French the article and the agreements tell it (« un chevalier », « un pilote » are men: he, his; « une samouraï », « une pilote » are women: she, her). Never change it, and do not give a gender to animals or objects.
7. Keep the look of the setting (place, light, style) unless a shot's note says otherwise.
8. Keep everything the user wrote (story events, character, setting, constraint, notes): never contradict, weaken or leave it out. No background music unless the user asks for it.
9. The user is an adult making fiction for adults. Nudity, sensuality and sexual content between adult characters stay exactly as explicit as the user wrote them: do not tone them down, replace them with something milder or leave them out.

Answer with exactly one block per shot to write (every shot, unless the user asks for one only), in order, and nothing else:
SHOT 1
FR: one short French sentence summarising the shot
EN: the English prompt"#;

/// La règle du personnage quand une image de référence le montre.
pub const REGLE_AVEC_REFERENCE: &str = r#"a reference picture gives the character's face, age, body and outfit: never describe them, call her or him simply "the character"."#;

/// La règle du personnage sans image : sa description est recopiée dans les plans de départ et coupés.
pub const REGLE_SANS_REFERENCE: &str = r#"no picture shows the character: in every start and cut shot, copy the character description given by the user word for word; in continue and transition shots, call her or him "the character"."#;

/// Le format en trois champs de H3, ajouté à la consigne d'un modèle qui le lit ainsi.
pub const FORMAT_CHAMPS: &str = r#"Output format of the target model (it replaces the plain-prose layout above, and the sounds move to overall_soundscape; every other rule stays):
exactly these three fields, one per paragraph, in this order, with these exact labels.
integrated_multimodal_description: [Shot 1] then the whole picture-and-action brief with the dialogue, as described above, but without the sounds: they go in overall_soundscape. No timestamp on Shot 1. Add a second shot only if the user asks for a cut: "[Shot 2] At 00:03.500, the camera cuts to ...".
overall_soundscape: one paragraph of 1 to 4 sentences: ambient sounds, sounds of the actions, non-verbal human sounds. Never put dialogue or music in it. Write N/A only if the user asks for silence.
non_diegetic_music: the background score only if the user asks for it (1 to 3 sentences: instruments, tempo, rhythm, dynamics, no mood words); otherwise N/A.
The word limit counts the three fields together."#;

/// Une traduction de la contrainte : le texte de l'utilisateur et son anglais.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Traduction {
    pub fr: String,
    pub en: String,
}

/// Un plan tel que l'atelier l'envoie.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PlanDemande {
    pub enchainement: String,
    pub duree: Option<u32>,
    pub note: Option<String>,
    pub rush: Option<String>,
    pub replique: Option<String>,
    pub moment: Option<String>,
    pub off: bool,
    pub prompt: Option<String>,
}

/// ✨ Tout écrire, ou ✨ d'un plan (`plan` : son numéro, 0 pour le premier).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DemandeDecoupage {
    /// Le fil de l'histoire (facultatif), en français, répliques entre guillemets.
    pub synopsis: String,
    /// L'état du Scénario : le personnage et le décor en sont tirés.
    pub sujet: String,
    pub choix: Map<String, Value>,
    pub age: Option<u32>,
    pub plans: Vec<PlanDemande>,
    /// Le rush du personnage.
    pub reference: Option<String>,
    /// Impérative sur tous les plans.
    pub contrainte: String,
    pub llm: Option<String>,
    /// La musique d'ambiance ; rien : pas de musique de fond.
    pub musique: Option<String>,
    pub plan: Option<usize>,
    /// La traduction déjà faite de la contrainte, reprise si elle vaut toujours.
    pub contrainte_en: Option<Traduction>,
    /// Le format de la séquence (départ du texte) : le LLM compose pour ce cadre.
    pub format: Option<String>,
}

/// Un plan contrôlé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub enchainement: String,
    pub duree: u32,
    pub note: String,
    pub rush: Option<String>,
    pub replique: String,
    pub moment: String,
    pub off: bool,
    pub prompt: String,
}

/// Un plan écrit : le plan, son résumé en français et son prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanEcrit {
    #[serde(flatten)]
    pub plan: Plan,
    pub idee: String,
}

/// Ce que rend l'écriture.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decoupe {
    pub plans: Vec<PlanEcrit>,
    pub cible: Option<usize>,
    pub alertes: Vec<String>,
    pub duree_s: f64,
    pub reference: Option<String>,
    pub contrainte: String,
    pub contrainte_en: Option<Traduction>,
    /// Le GGUF qui a écrit.
    pub llm: String,
}

/// Un bloc de la réponse : le résumé français et le prompt anglais d'un plan.
pub type Bloc = (String, String);

// --- outils de texte ------------------------------------------------------------------------------------------------

fn espaces(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn motif(r: &'static LazyLock<Option<Regex>>) -> Option<&'static Regex> {
    let r: &Option<Regex> = r;
    r.as_ref()
}

static PONCTUATION: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"[^\w\s]").ok());

/// Le texte sans casse ni ponctuation, pour comparer deux répliques.
pub fn simple(texte: &str) -> String {
    let bas = texte.to_lowercase();
    let propre = motif(&PONCTUATION).map_or_else(|| bas.clone(), |r| r.replace_all(&bas, " ").into_owned());
    espaces(&propre)
}

/// Un extrait d'au plus `longueur` caractères, coupé à un mot.
pub fn extrait(texte: &str, longueur: usize) -> String {
    let texte = espaces(texte);
    if texte.chars().count() <= longueur {
        return texte;
    }
    let debut: String = texte.chars().take(longueur).collect();
    let coupe = debut.rsplit_once(' ').map_or(debut.as_str(), |(avant, _)| avant);
    format!("{coupe}…")
}

fn majuscule(texte: &str) -> String {
    let mut c = texte.chars();
    c.next()
        .map_or_else(String::new, |p| p.to_uppercase().chain(c).collect())
}

fn minuscule(texte: &str) -> String {
    let mut c = texte.chars();
    c.next()
        .map_or_else(String::new, |p| p.to_lowercase().chain(c).collect())
}

fn vers_le_bas(texte: &str, mut i: usize) -> usize {
    i = i.min(texte.len());
    while !texte.is_char_boundary(i) {
        i -= 1;
    }
    i
}

// --- répliques ------------------------------------------------------------------------------------------------------

/// Remet, mot pour mot, les répliques de la note que le LLM aurait perdues, dans la balise de H3.
pub fn garder_repliques(note: &str, prompt_plan: &str) -> String {
    let dedans = simple(prompt_plan);
    let mut morceaux = vec![prompt_plan.trim_end().to_owned()];
    morceaux.extend(
        h3::repliques(note)
            .into_iter()
            .filter(|r| !dedans.contains(&simple(r)))
            .map(|r| format!("The character (S1) says: {}", h3::balise(&r))),
    );
    morceaux.join(" ")
}

/// Retire les phrases où le LLM fait dire une réplique absente de `source` (le fil, les notes, les répliques des
/// plans). Un texte entre guillemets présent dans la source (réplique, enseigne) reste. Rend le prompt et les répliques
/// retirées.
pub fn sans_repliques_inventees(source: &str, prompt_plan: &str) -> (String, Vec<String>) {
    let permis = simple(source);
    let mut texte = prompt_plan.to_owned();
    let mut retirees = Vec::new();
    loop {
        let Some(inventee) = h3::reperes(&texte)
            .into_iter()
            .find(|r| !permis.contains(&simple(&r.contenu)))
        else {
            return (espaces(&texte), retirees);
        };
        let debut = h3::fins_de_phrase(&texte[..inventee.debut])
            .last()
            .copied()
            .unwrap_or(0);
        // La réplique peut finir la phrase (« Salut. » ou « Salut.</d> ») : on repart avant son point.
        let recul = if inventee.balise { 6 } else { 2 };
        let depuis = vers_le_bas(&texte, inventee.fin.saturating_sub(recul));
        let fin = h3::fin_de_phrase_depuis(&texte, depuis).unwrap_or(texte.len());
        let fin = fin.max(inventee.fin).min(texte.len());
        retirees.push(inventee.contenu);
        texte = format!("{} {}", &texte[..debut], &texte[fin..]);
    }
}

static PAROLE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"(?i)(\b(?:says?|saying|said|speaks?|replies|asks?|whispers?|murmurs?|shouts?|exclaims?|answers?|calls out)\b"#,
        r#"(?P<entre>[^.!?"«“<>]{0,80}?)[:,]?\s*)"#,
        r#"(?:«\s{0,20}([^»]{1,1000}?)\s{0,20}»|"([^"]{1,1000})"|“([^”]{1,1000})”)"#,
    ))
    .ok()
});
static TEXTE_VISIBLE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(sign|signs|text|label|banner|neon|screen|poster|board|written|caption|subtitles?)\b")
        .ok()
});
static COUPURES_SUJET: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r#"[.!?,;"”»“«]"#).ok());

/// Les répliques écrites entre guillemets derrière un verbe de parole (« says: "Salut" ») passent dans la balise. Un
/// texte déjà en balise, ou un guillemet sans verbe de parole (une enseigne), n'est pas touché.
pub fn vers_balises(texte: &str) -> String {
    let (Some(parole), Some(visible), Some(coupures)) =
        (motif(&PAROLE), motif(&TEXTE_VISIBLE), motif(&COUPURES_SUJET))
    else {
        return texte.to_owned();
    };
    parole
        .replace_all(texte, |c: &regex::Captures<'_>| {
            let tout = c.get(0).map_or("", |m| m.as_str());
            let debut = c.get(0).map_or(0, |m| m.start());
            // Le sujet du verbe est avant lui (« A neon sign above the door says "OPEN" »).
            let avant = &texte[vers_le_bas(texte, debut.saturating_sub(80))..debut];
            let sujet = coupures.split(avant).last().unwrap_or("");
            let entre = c.name("entre").map_or("", |m| m.as_str());
            if visible.is_match(&format!("{sujet}{entre}")) {
                return tout.to_owned();
            }
            let dit = (3..=5).find_map(|i| c.get(i)).map_or("", |m| m.as_str());
            let tete = c.get(1).map_or("", |m| m.as_str()).trim_end_matches([' ', ',']);
            if tete.trim_end().ends_with(':') {
                format!("{tete} {}", h3::balise(dit))
            } else {
                format!("{tete}: {}", h3::balise(dit))
            }
        })
        .into_owned()
}

// --- contrôle de la demande -----------------------------------------------------------------------------------------

fn moment_texte(moment: &str) -> &'static str {
    MOMENTS
        .iter()
        .find(|(cle, _)| *cle == moment)
        .map_or(MOMENTS[1].1, |(_, t)| *t)
}

fn type_texte(enchainement: &str) -> &'static str {
    TYPES
        .iter()
        .find(|(cle, _)| *cle == enchainement)
        .map_or("cut", |(_, t)| *t)
}

/// Les plans, propres. `cible` : le seul plan à écrire, les autres servent de contexte. `est_image` dit si un rush
/// est une image (un clip est refusé) et existe.
///
/// # Errors
/// Le message de la v2.
pub fn valider_plans(
    synopsis: &str,
    personnage: &str,
    decor: &str,
    plans: &[PlanDemande],
    reference: Option<&str>,
    cible: Option<usize>,
    est_image: &dyn Fn(&str) -> Result<(), String>,
) -> Result<Vec<Plan>, String> {
    if [synopsis, personnage, decor]
        .iter()
        .any(|t| t.chars().count() > LONGUEUR_SYNOPSIS)
    {
        return Err(format!(
            "Fil de l'histoire de {LONGUEUR_SYNOPSIS} caractères au plus"
        ));
    }
    if !(2..=PLANS_MAX).contains(&plans.len()) {
        return Err(format!("De 2 à {PLANS_MAX} plans"));
    }
    if cible.is_some_and(|k| k >= plans.len()) {
        return Err("Ce plan n'existe pas".to_owned());
    }
    let mut propres = Vec::new();
    for (k, p) in plans.iter().enumerate() {
        let nom = format!("Plan {}", k + 1);
        let permis: &[&str] = if k == 0 { &DEPARTS } else { &ENCHAINEMENTS };
        if !permis.contains(&p.enchainement.as_str()) {
            return Err(format!("{nom} : enchaînement possible : {}", permis.join(", ")));
        }
        if p.enchainement == "rush" && reference.is_none() {
            return Err("Plan 1 : pour partir d'un rush, choisis l'image de référence".to_owned());
        }
        let duree = p.duree.unwrap_or(5);
        if !montage::DUREES.contains(&duree) {
            let durees: Vec<String> = montage::DUREES.iter().map(u32::to_string).collect();
            return Err(format!("{nom} : durée proposée : {} s", durees.join(", ")));
        }
        let note = p.note.as_deref().unwrap_or_default().trim().to_owned();
        if note.chars().count() > LONGUEUR_NOTE {
            return Err(format!(
                "{nom} : « ce qui se passe » de {LONGUEUR_NOTE} caractères au plus"
            ));
        }
        let replique = espaces(p.replique.as_deref().unwrap_or_default())
            .trim_matches(|c: char| "«»\"“” ".contains(c))
            .to_owned();
        if replique.chars().count() > LONGUEUR_REPLIQUE {
            return Err(format!(
                "{nom} : réplique de {LONGUEUR_REPLIQUE} caractères au plus"
            ));
        }
        let moment = p
            .moment
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "pendant".to_owned());
        if !MOMENTS.iter().any(|(m, _)| *m == moment) {
            return Err(format!(
                "{nom} : la réplique se dit au début, pendant l'action ou à la fin"
            ));
        }
        let rush = match p.enchainement.as_str() {
            "rush" => reference.map(str::to_owned),
            "transition" => p.rush.clone().filter(|r| !r.is_empty()),
            _ => None,
        };
        if let Some(r) = &rush {
            est_image(r).map_err(|e| format!("{nom} : {e}"))?;
        }
        let off = p.off && !replique.is_empty();
        let prompt: String = espaces(p.prompt.as_deref().unwrap_or_default())
            .chars()
            .take(LONGUEUR_PROMPT_PLAN)
            .collect();
        propres.push(Plan {
            enchainement: p.enchainement.clone(),
            duree,
            note,
            rush,
            replique,
            moment,
            off,
            prompt,
        });
    }
    if let Some(r) = reference {
        est_image(r)?;
    }
    let vide = |p: &Plan| p.note.is_empty() && p.replique.is_empty();
    if synopsis.trim().is_empty() {
        match cible {
            None if propres.iter().all(vide) => {
                return Err("Écris le fil de l'histoire, ou ce qui se passe dans les plans".to_owned());
            }
            Some(k) if propres.get(k).is_some_and(vide) => {
                return Err(format!(
                    "Plan {} : écris ce qui s'y passe (ou le fil de l'histoire)",
                    k + 1
                ));
            }
            _ => {}
        }
    }
    Ok(propres)
}

/// La longueur conseillée d'un clip : celle du modèle jusqu'à 5 s, puis quelques mots de plus par seconde.
pub fn mots_pour_duree(mots_max: u64, duree: u32) -> u64 {
    if duree <= DUREE_DE_BASE {
        mots_max
    } else {
        mots_max + MOTS_PAR_SECONDE_EN_PLUS * u64::from(duree - DUREE_DE_BASE)
    }
}

/// La réplique d'un plan, telle que le LLM la reçoit : mot pour mot, sa langue, son moment, la voix off.
pub fn ligne_replique(p: &Plan) -> String {
    let off = if p.off {
        ", as an off-screen voiceover (the character's lips stay closed)"
    } else {
        ""
    };
    format!(
        "line, word for word, in its <d> tag: {}, spoken {}{off}",
        h3::balise(&p.replique),
        moment_texte(&p.moment)
    )
}

/// Ce que l'histoire donne au LLM : le fil, le personnage, le décor, la contrainte et la musique.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Histoire<'a> {
    pub synopsis: &'a str,
    pub personnage: &'a str,
    pub decor: &'a str,
    pub reference: bool,
    pub contrainte: &'a str,
    pub musique: Option<&'a str>,
}

/// Le message envoyé au LLM : le fil, le personnage (sauf avec une image de référence), le décor, la contrainte, la
/// langue de chaque réplique, puis la liste des plans avec leur type, durée, ce qui s'y passe, leur réplique et leur
/// arrivée (`arrivees` : ce que dit la fiche du rush d'arrivée de chaque transition). `cible` : un seul plan à écrire,
/// avec le prompt déjà écrit de ses voisins.
pub fn demande_decoupage(
    h: &Histoire<'_>,
    plans: &[Plan],
    arrivees: &[Option<String>],
    cible: Option<usize>,
    mots: &[u64],
) -> String {
    let mut lignes = vec![
        if h.synopsis.trim().is_empty() {
            "Story: told by the shots below, in order.".to_owned()
        } else {
            format!("Story: {}", h.synopsis.trim())
        },
        format!(
            "Character: {}",
            if h.reference {
                "shown by the reference picture"
            } else if h.personnage.trim().is_empty() {
                "free"
            } else {
                h.personnage.trim()
            }
        ),
        format!(
            "Setting and look: {}",
            if h.decor.trim().is_empty() {
                "free"
            } else {
                h.decor.trim()
            }
        ),
    ];
    if !h.contrainte.is_empty() {
        lignes.push(format!(
            "Mandatory constraint, respected in EVERY shot: {}",
            h.contrainte
        ));
    }
    let mut toutes: Vec<String> = h3::repliques(h.synopsis);
    for p in plans {
        for r in h3::repliques(&p.note) {
            if !toutes.contains(&r) {
                toutes.push(r);
            }
        }
    }
    let parlent = !toutes.is_empty() || plans.iter().any(|p| !p.replique.is_empty());
    if !parlent {
        lignes.push("Nobody speaks in the whole film: no dialogue, no voice-over.".to_owned());
    } else if !toutes.is_empty() {
        let balises: Vec<String> = toutes.iter().map(|r| h3::balise(r)).collect();
        lignes.push(format!(
            "Spoken lines, each in its <d> tag with its language: {}",
            balises.join("; ")
        ));
    }
    lignes.push(h.musique.map_or_else(
        || "No background music.".to_owned(),
        |m| format!("Background music for the whole film, the same in every shot: {m}"),
    ));
    lignes.push("Shots:".to_owned());
    for (k, p) in plans.iter().enumerate() {
        let mut morceaux = vec![format!(
            "{}. {}, {} s",
            k + 1,
            type_texte(&p.enchainement),
            p.duree
        )];
        if let Some(m) = mots.get(k) {
            morceaux.push(format!("at most {m} words"));
        }
        if !p.note.is_empty() {
            morceaux.push(format!("note: {}", p.note));
        }
        if parlent && !p.replique.is_empty() {
            morceaux.push(ligne_replique(p));
        }
        if p.enchainement == "transition" {
            let arrivee = arrivees
                .get(k)
                .and_then(Clone::clone)
                .map(|a| extrait(&a, 400))
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| "the next moment of the story".to_owned());
            morceaux.push(format!("arrival: {arrivee}"));
        }
        lignes.push(morceaux.join(", "));
    }
    if let Some(c) = cible {
        lignes.push(format!(
            "Write ONLY shot {}: the other shots are context, keep continuity with them.",
            c + 1
        ));
        for j in [c.checked_sub(1), Some(c + 1)].into_iter().flatten() {
            if let Some(p) = plans.get(j).filter(|p| !p.prompt.is_empty()) {
                lignes.push(format!(
                    "Shot {} as already written: {}",
                    j + 1,
                    extrait(&p.prompt, 700)
                ));
            }
        }
    }
    lignes.join("\n")
}

// --- lecture de la réponse ------------------------------------------------------------------------------------------

static HABILLAGE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?m)\*\*|^\s*#+\s*|^\s*-{3,}\s*$").ok());
static ENTETE_PLAN: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)SHOT\s+(\d+)\s*:?[ \t]*\r?\n").ok());
static CORPS_PLAN: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?is)^\s*FR\s*:\s*(.+?)\n\s*EN\s*:\s*(.+)$").ok());

/// Les blocs SHOT n / FR / EN de la réponse, pour les plans demandés ; refuse une réponse incomplète. Le LLM habille
/// souvent les étiquettes (**FR:**, ### SHOT 1) et sépare les plans par --- : on l'enlève. `exact` faux (un seul plan
/// demandé) : un plan en trop est ignoré, un seul bloc mal numéroté est pris.
///
/// # Errors
/// « Llama a rendu n plan(s) au lieu de m ».
pub fn lire_blocs(texte: &str, numeros: &[usize], exact: bool) -> Result<Vec<Bloc>, String> {
    let texte = motif(&HABILLAGE).map_or_else(|| texte.to_owned(), |r| r.replace_all(texte, "").into_owned());
    let mut blocs: BTreeMap<usize, Bloc> = BTreeMap::new();
    if let (Some(entete), Some(corps)) = (motif(&ENTETE_PLAN), motif(&CORPS_PLAN)) {
        let entetes: Vec<(usize, usize, usize)> = entete
            .captures_iter(&texte)
            .filter_map(|c| {
                let tout = c.get(0)?;
                Some((tout.start(), tout.end(), c.get(1)?.as_str().parse().ok()?))
            })
            .collect();
        for (i, (_, fin, numero)) in entetes.iter().enumerate() {
            let suite = entetes.get(i + 1).map_or(texte.len(), |e| e.0);
            if let Some(c) = corps.captures(&texte[*fin..suite]) {
                let fr = c.get(1).map_or("", |m| m.as_str()).trim().to_owned();
                let en = c.get(2).map_or("", |m| m.as_str()).trim().to_owned();
                blocs.insert(*numero, (fr, en));
            }
        }
    }
    if !exact && blocs.len() == 1 && numeros.len() == 1 {
        return Ok(blocs.into_values().collect());
    }
    let bons = if exact {
        blocs.keys().copied().collect::<Vec<_>>() == numeros
    } else {
        numeros.iter().all(|n| blocs.contains_key(n))
    };
    if !bons {
        return Err(format!(
            "Llama a rendu {} plan(s) au lieu de {}",
            blocs.len(),
            numeros.len()
        ));
    }
    Ok(numeros.iter().filter_map(|n| blocs.get(n).cloned()).collect())
}

static ETIQUETTE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*(?:start from the picture|start|cut|transition)\s*[:—-]\s*").ok());
static ETIQUETTE_INTERNE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i),\s*start from the picture\s*:\s*").ok());
static ETIQUETTE_PLAN: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:a|an)\s+(?:start|cut|transition|continue)\s+shot\s*[,:—-]\s*(\w)").ok()
});

/// Retire les types de plan recopiés par le LLM (« Cut: », « start from the picture: », « A cut shot, ») : H3 n'a
/// pas à les lire.
pub fn sans_etiquette(texte: &str) -> String {
    let mut t = motif(&ETIQUETTE).map_or_else(|| texte.to_owned(), |r| r.replace_all(texte, "").into_owned());
    if let Some(r) = motif(&ETIQUETTE_INTERNE) {
        t = r.replace_all(&t, ", ").into_owned();
    }
    if let Some(r) = motif(&ETIQUETTE_PLAN) {
        t = r
            .replace_all(&t, |c: &regex::Captures<'_>| {
                c.get(1).map_or_else(String::new, |m| m.as_str().to_uppercase())
            })
            .into_owned();
    }
    majuscule(t.trim())
}

/// Sans image de référence, l'identité tient au texte : la description du personnage, si le LLM l'a omise, est remise
/// en tête du plan.
pub fn avec_personnage(personnage: &str, texte: &str) -> String {
    let description = personnage.trim().trim_end_matches('.');
    if description.is_empty() {
        return texte.to_owned();
    }
    let debut: String = simple(description).chars().take(60).collect();
    if simple(texte).contains(&debut) {
        return texte.to_owned();
    }
    format!("The character is {}. {texte}", minuscule(description))
}

/// La réplique d'un plan, mot pour mot, à son moment. Écrite par le LLM : rien à faire. Perdue : remise dans la
/// chronologie du brief, après la phrase d'ouverture (début), au milieu (pendant l'action) ou à la fin.
pub fn placer_replique(texte: &str, replique: &str, moment: &str, off: bool) -> String {
    if replique.is_empty() || simple(texte).contains(&simple(replique)) {
        return texte.to_owned();
    }
    let dit = if off {
        format!(
            "The character (S1) says in an off-screen voiceover: {} The character's lips stay closed.",
            h3::balise(replique)
        )
    } else {
        format!("The character (S1) says: {}", h3::balise(replique))
    };
    let fins = h3::fins_de_phrase(texte);
    let coupure = if moment == "fin" || fins.len() < 2 {
        texte.len()
    } else if moment == "debut" {
        fins[0]
    } else {
        fins[(fins.len() - 1) / 2]
    };
    espaces(&format!("{} {dit} {}", &texte[..coupure], &texte[coupure..]))
}

/// Une réplique trop longue pour son plan : H3 la presse ou la coupe.
pub fn alerte_replique(k: usize, p: &Plan) -> Vec<String> {
    if p.replique.is_empty() {
        return Vec::new();
    }
    let mots = p.replique.split_whitespace().count();
    // Quelques mots par seconde : un entier petit, arrondi vers le bas comme la page.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let permis = (MOTS_PAR_SECONDE * f64::from(p.duree)).floor() as usize;
    if mots > permis {
        vec![format!(
            "Plan {k} : réplique de {mots} mots pour {} s (environ {permis} au plus) : elle risque d'être pressée ou coupée",
            p.duree
        )]
    } else {
        Vec::new()
    }
}

/// Ce qui a le droit d'être dit (ou écrit entre guillemets) : le reste est une réplique inventée par le LLM. Sans
/// aucune réplique nulle part, tout texte entre guillemets du LLM en est une.
pub fn source_repliques(synopsis: &str, contrainte: &str, plans: &[Plan]) -> String {
    let parle = !h3::repliques(synopsis).is_empty()
        || plans
            .iter()
            .any(|p| !h3::repliques(&p.note).is_empty() || !p.replique.is_empty());
    if !parle {
        return String::new();
    }
    let mut morceaux = vec![synopsis.to_owned(), contrainte.to_owned()];
    morceaux.extend(plans.iter().map(|p| p.note.clone()));
    morceaux.extend(
        plans
            .iter()
            .filter(|p| !p.replique.is_empty())
            .map(|p| format!("\"{}\"", p.replique)),
    );
    morceaux.join(" ")
}

/// Les garde-fous du prompt d'un plan écrit par le LLM : étiquettes et Markdown retirés, répliques inventées retirées,
/// « Continuous shot » d'un plan prolongé, description du personnage, et la réplique du plan mot pour mot, à son
/// moment. Rend le prompt et les alertes. `champs` : le prompt est en trois champs (H3) ; les garde-fous
/// s'appliquent à la description, l'ambiance et la musique sont remises telles quelles.
pub fn finir_plan(
    k: usize,
    p: &Plan,
    en: &str,
    personnage: &str,
    source: &str,
    champs: bool,
) -> (String, Vec<String>) {
    if champs {
        let (description, ambiance, musique) = h3::separer_champs(&prompt::nettoyer_markdown(en));
        let (texte, alertes) = finir_plan(k, p, &description, personnage, source, false);
        return (
            h3::assembler_champs(&texte, ambiance.as_deref(), musique.as_deref()),
            alertes,
        );
    }
    let (mut texte, inventees) =
        sans_repliques_inventees(source, &sans_etiquette(&prompt::nettoyer_markdown(en)));
    let mut alertes: Vec<String> = inventees
        .iter()
        .map(|r| format!("Plan {k} : réplique inventée par Llama, retirée : « {r} »"))
        .collect();
    if p.enchainement == "prolonger" && !texte.to_lowercase().starts_with("continuous shot") {
        texte = format!("Continuous shot, same scene. {texte}");
    }
    if p.enchainement == "texte" || p.enchainement == "couper" {
        texte = avec_personnage(personnage, &texte);
    }
    texte = garder_repliques(&p.note, &texte);
    texte = placer_replique(&texte, &p.replique, &p.moment, p.off);
    texte = vers_balises(&texte);
    alertes.extend(alerte_replique(k, p));
    (texte, alertes)
}

// --- l'écriture -----------------------------------------------------------------------------------------------------

/// La consigne du découpage pour ce LLM : celle de Réglages s'il en a une, sinon celle d'origine ; les règles du
/// modèle cible à la suite, le format en trois champs s'il le lit ainsi, la longueur, la règle du personnage et le
/// cadre de la séquence ; toujours suivie de la clause verrouillée.
pub fn consigne(
    reecrite: Option<&str>,
    mots_max: u64,
    regles: &[(String, Vec<String>)],
    champs: bool,
    reference: bool,
    format: Option<&str>,
) -> String {
    let mut texte = reecrite.unwrap_or(CONSIGNE_DECOUPAGE).trim_end().to_owned();
    let bloc = prompt::bloc_modele(regles);
    if !bloc.is_empty() {
        texte.push_str("\n\n");
        texte.push_str(&bloc);
    }
    if champs {
        texte.push_str("\n\n");
        texte.push_str(FORMAT_CHAMPS);
    }
    let texte = texte
        .replace("{mots_max}", &mots_max.to_string())
        .replace("{mots_min}", &prompt::MOTS_MIN.to_string());
    let mut texte = format!("{texte}\n\n{}", prompt::CLAUSE_MINEURS);
    let regle = if reference {
        REGLE_AVEC_REFERENCE
    } else {
        REGLE_SANS_REFERENCE
    };
    texte = if texte.contains("{regle_personnage}") {
        texte.replace("{regle_personnage}", regle)
    } else {
        format!("{texte}\n\nCharacter: {regle}") // consigne retouchée sans le repère
    };
    let cadre = conversation::cadre_format(format, "plan");
    if !cadre.is_empty() {
        texte.push_str("\n\n");
        texte.push_str(&cadre);
    }
    texte
}

/// Le modèle vidéo du découpage (texte vers vidéo) et ses réglages.
fn workflow_video(contexte: &Contexte) -> Option<(Workflow, Option<Map<String, Value>>)> {
    let id = montage::workflow_de(contexte, "texte_vers_video");
    let wf = workflows::obtenir(&id).ok()?;
    let reglage = reglages::section(&contexte.dossier, "workflows")
        .and_then(|s| s.get(&id).and_then(Value::as_object).cloned());
    Some((wf, reglage))
}

/// Ce modèle lit son prompt en trois champs (`profil_prompt.format_champs` de sa fiche).
fn champs_video(wf: &Workflow) -> bool {
    wf.fiche
        .pointer("/profil_prompt/format_champs")
        .is_some_and(|v| v.as_bool() == Some(true))
}

/// Ce que dit la fiche d'un rush (prompt, sinon idée).
fn ce_que_montre(contexte: &Contexte, id: &str) -> Option<String> {
    let f = creations::fiche(&contexte.data, id).ok()?;
    let texte = |cle: &str| {
        f.get(cle)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    texte("prompt").or_else(|| texte("idee"))
}

/// Un rush est une image existante (un clip est refusé).
fn image_existante(contexte: &Contexte, id: &str) -> Result<(), String> {
    if creations::fiche(&contexte.data, id)?
        .get("type")
        .and_then(Value::as_str)
        == Some("clip")
    {
        return Err("il faut une image, pas un clip".to_owned());
    }
    creations::chemin_image(&contexte.data, id).map(|_| ())
}

/// Ce que ✨ Tout écrire, ✨ d'un plan et 💬 d'un plan partagent : les plans contrôlés, le personnage, la contrainte,
/// les entrées passées par AS-18, ce qui a le droit d'être dit, et la demande au LLM (comme `preparer_ecriture` de la
/// v2).
pub struct Preparation {
    pub plans: Vec<Plan>,
    /// La description du personnage ; vide avec une image de référence (l'image fait l'identité).
    pub personnage: String,
    pub contrainte: String,
    pub reference: Option<String>,
    pub format: Option<String>,
    /// Tout ce qui part au LLM, avec les textes d'origine des rushes (et « minor » pour un mineur).
    pub entrees: Vec<String>,
    /// Ce qui a le droit d'être dit (le reste est une réplique inventée).
    pub source: String,
    /// Le message envoyé au LLM : l'histoire et les plans.
    pub demande: String,
    pub regle_personnage: &'static str,
    /// La longueur conseillée de chaque plan, et celle du modèle vidéo.
    pub mots: Vec<u64>,
    pub mots_max: u64,
    video: Option<(Workflow, Option<Map<String, Value>>)>,
}

impl Preparation {
    /// Le modèle vidéo lit son prompt en trois champs.
    pub fn champs(&self) -> bool {
        self.video.as_ref().is_some_and(|(wf, _)| champs_video(wf))
    }

    /// L'identifiant du modèle vidéo du découpage.
    pub fn workflow(&self) -> Option<&str> {
        self.video.as_ref().map(|(wf, _)| wf.id.as_str())
    }
}

/// Contrôle une demande de découpage, la passe par AS-18 et prépare la demande au LLM.
///
/// # Errors
/// Demande invalide (le message de la v2), LLM absent, ou filtre AS-18.
pub fn preparer(contexte: &Contexte, d: &DemandeDecoupage) -> Result<Preparation, String> {
    scenario::verifier_llm(contexte, d.llm.as_deref())?;
    let format = d.format.as_deref().filter(|f| !f.is_empty());
    if let Some(f) = format
        && !FORMATS.contains(&f)
    {
        return Err(format!("Format inconnu : {f}"));
    }
    let sujet = if d.sujet.is_empty() {
        "personnage"
    } else {
        d.sujet.as_str()
    };
    let desc = listes::description_sequence(&contexte.dossier, sujet, &d.choix, d.age)?;
    let reference = d.reference.as_deref().filter(|r| !r.is_empty());
    let est_image = |id: &str| image_existante(contexte, id);
    let plans = valider_plans(
        &d.synopsis,
        &desc.personnage,
        &desc.decor,
        &d.plans,
        reference,
        d.plan,
        &est_image,
    )?;
    let contrainte = espaces(&d.contrainte);
    if contrainte.chars().count() > LONGUEUR_CONTRAINTE {
        return Err(format!("Contrainte de {LONGUEUR_CONTRAINTE} caractères au plus"));
    }
    let musique = d.musique.as_deref().map(espaces).filter(|m| !m.is_empty());
    if musique
        .as_ref()
        .is_some_and(|m| m.chars().count() > LONGUEUR_MUSIQUE)
    {
        return Err(format!("Musique de {LONGUEUR_MUSIQUE} caractères au plus"));
    }
    // L'image fait l'identité : la décrire ferait dériver H3.
    let personnage = if reference.is_some() {
        String::new()
    } else {
        desc.personnage.clone()
    };
    let video = workflow_video(contexte);
    let mots_max = video
        .as_ref()
        .and_then(|(wf, r)| prompt::mots_max_de(Some(wf), r.as_ref()))
        .unwrap_or(MOTS_DEFAUT);
    let mots: Vec<u64> = plans.iter().map(|p| mots_pour_duree(mots_max, p.duree)).collect();

    // AS-18 : tout ce qui part au LLM, avec les textes d'origine des rushes.
    let mut rushes: Vec<String> = reference.map(str::to_owned).into_iter().collect();
    rushes.extend(plans.iter().filter_map(|p| p.rush.clone()));
    let mut entrees: Vec<String> = vec![
        d.synopsis.clone(),
        personnage.clone(),
        desc.decor.clone(),
        contrainte.clone(),
        musique.clone().unwrap_or_default(),
    ];
    entrees.extend(plans.iter().map(|p| p.note.clone()));
    entrees.extend(plans.iter().map(|p| p.replique.clone()));
    entrees.extend(creations::textes_origine(&contexte.data, &rushes)?);
    if desc.mineur {
        entrees.push("minor".to_owned());
    }
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let refs: Vec<&str> = entrees.iter().map(String::as_str).collect();
    filtre.verifier(&refs).map_err(|e| e.to_string())?;

    let source = source_repliques(&d.synopsis, &contrainte, &plans);
    let arrivees: Vec<Option<String>> = plans
        .iter()
        .map(|p| {
            (p.enchainement == "transition")
                .then(|| p.rush.as_deref().and_then(|r| ce_que_montre(contexte, r)))
                .flatten()
        })
        .collect();
    let histoire = Histoire {
        synopsis: &d.synopsis,
        personnage: &personnage,
        decor: &desc.decor,
        reference: reference.is_some(),
        contrainte: &contrainte,
        musique: musique.as_deref(),
    };
    let demande_texte = demande_decoupage(&histoire, &plans, &arrivees, d.plan, &mots);
    Ok(Preparation {
        regle_personnage: if reference.is_some() {
            REGLE_AVEC_REFERENCE
        } else {
            REGLE_SANS_REFERENCE
        },
        reference: reference.map(str::to_owned),
        format: format.map(str::to_owned),
        plans,
        personnage,
        contrainte,
        entrees,
        source,
        demande: demande_texte,
        mots,
        mots_max,
        video,
    })
}

/// Le LLM écrit les prompts des plans d'une séquence : tous ensemble (✨ Tout écrire), ou un seul (`plan`), les
/// autres servant de contexte. Puis les garde-fous d'ici les contrôlent, et la contrainte est traduite (une fois).
///
/// # Errors
/// Demande invalide (le message de la v2), LLM absent, filtre AS-18, réponse mal formée deux fois, ou llama-server
/// qui échoue.
#[allow(clippy::too_many_lines)] // l'écriture de la v2, gardée d'un tenant pour se relire à côté d'elle
pub fn decouper(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    d: &DemandeDecoupage,
) -> Result<Decoupe, String> {
    let Preparation {
        plans,
        personnage,
        contrainte,
        reference,
        format,
        entrees,
        source,
        demande: demande_texte,
        mots,
        mots_max,
        video,
        ..
    } = preparer(contexte, d)?;
    let reference = reference.as_deref();
    let format = format.as_deref();
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let refs: Vec<&str> = entrees.iter().map(String::as_str).collect();
    let cibles: Vec<usize> = d.plan.map_or_else(|| (0..plans.len()).collect(), |k| vec![k]);

    // Le LLM de ce modèle, sinon celui par défaut ; un spécialiste ne fait pas de découpage.
    let lieux = contexte.lieux();
    let choisi = d.llm.clone().filter(|l| !l.is_empty()).or_else(|| {
        video
            .as_ref()
            .map(|(wf, _)| llm::de_workflow(&contexte.dossier, &contexte.modeles, &wf.id))
    });
    let mut nom = llm::nom(&lieux.dossier, &lieux.modeles, choisi.as_deref());
    if fiches::specialiste(&nom).is_some() {
        nom = llm::defaut(&lieux.dossier, &lieux.modeles);
    }
    let lancement = llm::lancement(&lieux, Some(nom.as_str()))?;
    let regle = llm::valeurs(&contexte.dossier, &nom);
    let champs = video.as_ref().is_some_and(|(wf, _)| champs_video(wf));
    let regles = video
        .as_ref()
        .map(|(wf, _)| prompt::sections_prompt(Some(wf)))
        .unwrap_or_default();
    let plus_long = cibles
        .iter()
        .filter_map(|k| mots.get(*k))
        .copied()
        .max()
        .unwrap_or(mots_max);
    let reecrite = llm::consigne(&contexte.dossier, &nom, "decoupage");
    let systeme = consigne(
        reecrite.as_deref(),
        plus_long,
        &regles,
        champs,
        reference.is_some(),
        format,
    );
    // Chaque bloc : son prompt, et environ 40 tokens pour « SHOT n », la ligne FR et « EN: ».
    let besoin: u32 = cibles
        .iter()
        .map(|k| prompt::tokens_pour_mots(mots.get(*k).copied()).saturating_add(40))
        .fold(100, u32::saturating_add);
    let (temperature, echantillonnage) = regle.ecriture(lancement.reflexion);
    let demande = Demande {
        messages: json!([
            {"role": "system", "content": systeme},
            {"role": "user", "content": demande_texte},
        ]),
        temperature,
        max_tokens: regle.max_tokens.max(besoin),
        echantillonnage,
        ..Demande::default()
    };
    let debut = Instant::now();
    let numeros: Vec<usize> = cibles.iter().map(|k| k + 1).collect();
    // Une réponse mal formée est redemandée une fois.
    let mut essais = 0;
    let blocs = loop {
        let reponse = generateur.ecrire(lancement.clone(), &demande)?;
        match lire_blocs(&reponse.texte, &numeros, d.plan.is_none()) {
            Ok(b) => break b,
            Err(e) if essais == 1 => return Err(e),
            Err(_) => essais += 1,
        }
    };

    let mut alertes = Vec::new();
    let mut sortie = Vec::new();
    for (k, (fr, en)) in cibles.iter().zip(blocs) {
        let Some(p) = plans.get(*k) else { continue };
        let (texte, alertes_plan) = finir_plan(k + 1, p, &en, &personnage, &source, champs);
        let (fr, _) = sans_repliques_inventees(&source, &prompt::nettoyer_markdown(&fr));
        alertes.extend(alertes_plan);
        let compte = texte.split_whitespace().count();
        let conseil = mots.get(*k).copied().unwrap_or(mots_max);
        if u64::try_from(compte).unwrap_or(u64::MAX) > conseil + 20 {
            alertes.push(format!(
                "Plan {} : {compte} mots, au-delà des {conseil} conseillés",
                k + 1
            ));
        }
        let mut plan = p.clone();
        plan.prompt = texte;
        sortie.push(PlanEcrit { plan, idee: fr });
    }
    let tout: Vec<&str> = sortie.iter().map(|p| p.plan.prompt.as_str()).collect();
    let tout = tout.join(" ");
    if d.plan.is_none() {
        // Une réplique du fil : le LLM la place dans le plan qu'il veut.
        let dedans = simple(&tout);
        for r in h3::repliques(&d.synopsis) {
            if !dedans.contains(&simple(&r)) {
                alertes.push(format!(
                    "Réplique « {r} » absente du découpage : ajoute-la au plan voulu"
                ));
            }
        }
    }
    // Ensemble : un mineur des textes d'origine reste un mineur, même si le LLM l'a reformulé.
    let mut avec_tout = refs.clone();
    avec_tout.push(tout.as_str());
    filtre.verifier(&avec_tout).map_err(|e| e.to_string())?;

    // La contrainte en anglais, ajoutée telle quelle à chaque plan au tournage (où le LLM n'écrit plus).
    let contrainte_en = if contrainte.is_empty() {
        None
    } else {
        match d
            .contrainte_en
            .as_ref()
            .filter(|t| t.fr == contrainte && !t.en.trim().is_empty())
        {
            Some(t) => {
                let en: String = espaces(&t.en).chars().take(LONGUEUR_CONTRAINTE * 2).collect();
                let mut a_lire = vec![contrainte.as_str(), en.as_str()];
                a_lire.extend(refs.iter().copied());
                filtre.verifier(&a_lire).map_err(|e| e.to_string())?;
                Some(Traduction {
                    fr: contrainte.clone(),
                    en,
                })
            }
            None => {
                let en =
                    scenario::traduire_precisions(contexte, generateur, &contrainte, Some(nom.as_str()))?;
                Some(Traduction {
                    fr: contrainte.clone(),
                    en: prompt::nettoyer_markdown(&en),
                })
            }
        }
    };
    Ok(Decoupe {
        plans: sortie,
        cible: d.plan,
        alertes,
        duree_s: (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0,
        reference: reference.map(str::to_owned),
        contrainte,
        contrainte_en,
        llm: nom,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(enchainement: &str) -> Plan {
        Plan {
            enchainement: enchainement.to_owned(),
            duree: 5,
            note: String::new(),
            rush: None,
            replique: String::new(),
            moment: "pendant".to_owned(),
            off: false,
            prompt: String::new(),
        }
    }

    fn demande(enchainement: &str, note: &str) -> PlanDemande {
        PlanDemande {
            enchainement: enchainement.to_owned(),
            note: Some(note.to_owned()),
            ..PlanDemande::default()
        }
    }

    #[test]
    fn blocs_lus_et_habillage_retire() {
        let r = "### SHOT 1\n**FR:** Elle entre.\n**EN:** She walks in.\n---\nSHOT 2:\nFR: Elle s'assoit.\nEN: She sits down.\nThe rain falls.";
        let b = lire_blocs(r, &[1, 2], true).unwrap();
        assert_eq!(b[0], ("Elle entre.".to_owned(), "She walks in.".to_owned()));
        assert_eq!(b[1].1, "She sits down.\nThe rain falls.");
        assert!(
            lire_blocs(r, &[1, 2, 3], true)
                .unwrap_err()
                .contains("2 plan(s) au lieu de 3")
        );
        // Un seul plan demandé : un bloc mal numéroté est pris.
        let un = lire_blocs("SHOT 4\nFR: x\nEN: y", &[2], false).unwrap();
        assert_eq!(un, [("x".to_owned(), "y".to_owned())]);
    }

    #[test]
    fn etiquettes_retirees() {
        assert_eq!(sans_etiquette("Cut: the woman turns."), "The woman turns.");
        assert_eq!(sans_etiquette("A cut shot, the woman turns."), "The woman turns.");
        assert_eq!(
            sans_etiquette("Wide view, start from the picture: she smiles."),
            "Wide view, she smiles."
        );
    }

    #[test]
    fn replique_placee_a_son_moment() {
        let texte = "She enters. She sits. She drinks. She leaves.";
        let debut = placer_replique(texte, "Bonjour", "debut", false);
        assert!(debut.starts_with("She enters. The character (S1) says: <d>[French] Bonjour</d> She sits."));
        let fin = placer_replique(texte, "Bonjour", "fin", true);
        assert!(fin.ends_with(
            "says in an off-screen voiceover: <d>[French] Bonjour</d> The character's lips stay closed."
        ));
        let deja = "She says: <d>[French] Bonjour</d>";
        assert_eq!(placer_replique(deja, "Bonjour", "debut", false), deja);
    }

    #[test]
    fn personnage_remis() {
        let p = "A tall woman with red hair.";
        assert_eq!(
            avec_personnage(p, "She walks."),
            "The character is a tall woman with red hair. She walks."
        );
        assert_eq!(
            avec_personnage(p, "A tall woman with red hair walks."),
            "A tall woman with red hair walks."
        );
        assert_eq!(avec_personnage("", "She walks."), "She walks.");
    }

    #[test]
    fn repliques_inventees_retirees() {
        let (t, r) = sans_repliques_inventees(
            "Elle dit « Bonjour ».",
            "She enters. She says: <d>[French] Salut.</d> Then she says \"Bonjour\". She sits.",
        );
        assert_eq!(r, ["Salut."]);
        assert_eq!(t, "She enters. Then she says \"Bonjour\". She sits.");
        // Sans aucune réplique nulle part, tout texte entre guillemets en est une.
        let (t, r) = sans_repliques_inventees("", "She smiles. \"Hi.\" She leaves.");
        assert_eq!((t.as_str(), r.len()), ("She smiles. She leaves.", 1));
    }

    #[test]
    fn guillemets_vers_balise() {
        assert_eq!(
            vers_balises("She says \"Bonjour\" and leaves."),
            "She says: <d>[French] Bonjour</d> and leaves."
        );
        let enseigne = "A neon sign says \"OPEN\".";
        assert_eq!(vers_balises(enseigne), enseigne);
    }

    #[test]
    fn plan_fini_par_les_garde_fous() {
        let mut p = plan("prolonger");
        p.replique = "On y va".to_owned();
        p.moment = "fin".to_owned();
        let (t, alertes) = finir_plan(2, &p, "**Cut:** she stands up.", "", "\"On y va\"", false);
        assert!(t.starts_with("Continuous shot, same scene. She stands up."));
        assert!(t.ends_with("The character (S1) says: <d>[French] On y va</d>"));
        assert!(alertes.is_empty());
        // Trois champs : la description passe par les garde-fous, l'ambiance reste.
        let champs = "integrated_multimodal_description: [Shot 1] She runs.\n\noverall_soundscape: rain";
        let (t, _) = finir_plan(1, &plan("couper"), champs, "a tall woman", "", true);
        assert!(t.starts_with(
            "integrated_multimodal_description: [Shot 1] The character is a tall woman. She runs."
        ));
        assert!(t.ends_with("overall_soundscape: rain"));
    }

    #[test]
    fn alerte_d_une_replique_trop_longue() {
        let mut p = plan("couper");
        p.replique = "un deux trois quatre cinq six sept huit neuf dix onze douze treize".to_owned();
        assert_eq!(alerte_replique(3, &p).len(), 1);
        p.duree = 10;
        assert!(alerte_replique(3, &p).is_empty());
    }

    #[test]
    fn validation_comme_la_v2() {
        let image = |_: &str| -> Result<(), String> { Ok(()) };
        let plans = [demande("texte", "Elle entre"), demande("prolonger", "")];
        let propres = valider_plans("", "", "", &plans, None, None, &image).unwrap();
        assert_eq!((propres[0].duree, propres[0].moment.as_str()), (5, "pendant"));
        assert!(
            valider_plans("", "", "", &plans[..1], None, None, &image)
                .unwrap_err()
                .contains("De 2 à")
        );
        let mauvais = [demande("couper", "x"), demande("prolonger", "")];
        assert!(
            valider_plans("", "", "", &mauvais, None, None, &image)
                .unwrap_err()
                .starts_with("Plan 1 : enchaînement")
        );
        let rush = [demande("rush", "x"), demande("couper", "")];
        assert!(
            valider_plans("", "", "", &rush, None, None, &image)
                .unwrap_err()
                .contains("image de référence")
        );
        let vides = [demande("texte", ""), demande("couper", "")];
        assert!(
            valider_plans("", "", "", &vides, None, None, &image)
                .unwrap_err()
                .contains("fil de l'histoire")
        );
        assert!(
            valider_plans("", "", "", &vides, None, Some(1), &image)
                .unwrap_err()
                .starts_with("Plan 2 :")
        );
        assert!(
            valider_plans("Une histoire", "", "", &vides, None, Some(5), &image)
                .unwrap_err()
                .contains("n'existe pas")
        );
        let mut dure = demande("texte", "x");
        dure.duree = Some(7);
        assert!(
            valider_plans("", "", "", &[dure, demande("couper", "")], None, None, &image)
                .unwrap_err()
                .contains("durée proposée")
        );
    }

    #[test]
    fn demande_au_llm() {
        let mut deux = plan("couper");
        deux.note = "Il répond « Salut »".to_owned();
        deux.prompt = "He answers.".to_owned();
        let mut trois = plan("transition");
        trois.replique = "Viens".to_owned();
        let plans = [plan("texte"), deux, trois];
        let h = Histoire {
            synopsis: "Une rencontre.",
            personnage: "a tall woman",
            contrainte: "no text",
            ..Histoire::default()
        };
        let d = demande_decoupage(
            &h,
            &plans,
            &[None, None, Some("A street at night".to_owned())],
            Some(2),
            &[110, 110, 150],
        );
        assert!(d.starts_with("Story: Une rencontre.\nCharacter: a tall woman\nSetting and look: free"));
        assert!(d.contains("Mandatory constraint, respected in EVERY shot: no text"));
        assert!(d.contains("Spoken lines, each in its <d> tag with its language: <d>[French] Salut</d>"));
        assert!(d.contains("No background music."));
        assert!(d.contains("3. transition, 5 s, at most 150 words, line, word for word, in its <d> tag: <d>[French] Viens</d>"));
        assert!(d.contains("arrival: A street at night"));
        assert!(d.contains("Write ONLY shot 3"));
        assert!(d.contains("Shot 2 as already written: He answers."));
    }

    #[test]
    fn consigne_du_decoupage() {
        let c = consigne(None, 130, &[], true, true, Some("9:16 (Vertical)"));
        assert!(c.starts_with("You are the storyboard writer"));
        assert!(c.contains("40 to 130 words"));
        assert!(c.contains("never describe them"));
        assert!(c.contains("integrated_multimodal_description:"));
        assert!(c.contains("portrait"));
        assert!(!c.contains('{'));
        let propre = consigne(Some("Write shots."), 110, &[], false, false, None);
        assert!(propre.starts_with("Write shots.\n\n"));
        assert!(propre.ends_with(&format!("Character: {REGLE_SANS_REFERENCE}")));
        assert_eq!(mots_pour_duree(110, 5), 110);
        assert_eq!(mots_pour_duree(110, 10), 150);
    }
}
