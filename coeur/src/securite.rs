//! Le filtre AS-18, comme en v2 (`app/securite.py`) : avant toute génération, un texte qui associe un personnage
//! mineur à un terme sensible est refusé, et un terme « toujours bloqué » est refusé partout.
//!
//! - Les textes d'une demande sont lus ensemble (idée, prompt, mots déclencheurs, textes d'origine…), sans fenêtre de
//!   proximité.
//! - Chaque texte est lu deux fois : tirets effacés (`n–ude` se lit `nude`), puis tirets visibles lus comme le trait
//!   d'union (`dix‑sept` reste deux mots, `10–12 ans` garde ses deux nombres). Une seule lecture qui bloque suffit.
//! - Normalisation : décomposition Unicode (NFKD), tout ce qui n'est pas ASCII effacé, minuscules. Comparaison sur des
//!   mots entiers ; espaces et tirets confondus dans les termes.
//! - Un mineur, c'est un mot de la liste `mineurs`, ou un âge de moins de 18 ans écrit en chiffres ou en lettres, en
//!   français ou en anglais (« 12 ans », « twelve-year-old », « aged 9 », « a girl of 14 »…).
//! - Le socle (`securite/socle.yaml`) est embarqué dans l'exe ; les ajouts de l'utilisateur (`reglages.json`, section
//!   `securite`) s'y ajoutent et ne peuvent rien en retirer. Socle ou ajouts mal formés : tout est refusé.
//!
//! Le filtre est rejoué sur les phrases témoins livrées (`securite/temoins.yaml`) et sur celles de l'utilisateur.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::{Dossier, reglages};

/// L'âge à partir duquel un personnage est adulte.
pub const AGE_MAJORITE: u64 = 18;
/// La longueur minimale d'un ajout de l'utilisateur (« a », « de » bloqueraient presque tout).
pub const LONGUEUR_MIN: usize = 3;
/// Ce qu'une phrase témoin doit donner.
pub const ATTENDUS: [&str; 2] = ["bloque", "passe"];
/// Les trois listes, dans cet ordre, et leur nom.
pub const LISTES: [(&str, &str); 3] = [
    ("mineurs", "Mots qui désignent un mineur"),
    ("sensibles", "Termes sensibles (bloqués avec un mineur)"),
    ("toujours_bloques", "Termes toujours bloqués"),
];
/// Le socle livré.
pub const SOCLE: &str = include_str!("../../securite/socle.yaml");
/// Les phrases témoins livrées.
pub const TEMOINS: &str = include_str!("../../securite/temoins.yaml");
/// La section de `reglages.json` qui garde les ajouts et les témoins de l'utilisateur.
pub const SECTION: &str = "securite";
const FICHIER_SOCLE: &str = "securite/socle.yaml";

/// `\s` de Python sur un texte ASCII (qui compte aussi les séparateurs `\x1C` à `\x1F`).
const S: &str = r"[\t\n\x0B\x0C\r\x1C-\x1F ]";
/// `[\s-]` de Python sur un texte ASCII.
const S_TIRET: &str = r"[\t\n\x0B\x0C\r\x1C-\x1F \-]";
/// Les personnes que suit un âge (« a girl of 14 », « une fille de 14 »).
const PERSONNE: &str = "(?:girl|boy|kid|child|daughter|son|student|pupil|teen|lad|lass|female|male|person|fille|garcon\
                        |enfant|gamin|gamine|eleve)";

/// Un refus du filtre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurSecurite(pub String);

impl fmt::Display for ErreurSecurite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ERREUR AS-18 · Génération bloquée : {}", self.0)
    }
}

/// Un terme qui a fait bloquer, et d'où il vient (`socle`, `ajouts`, ou `age` pour un âge reconnu).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Terme {
    pub terme: String,
    pub liste: String,
    pub etage: String,
}

/// Pourquoi des textes sont refusés.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refus {
    pub message: String,
    pub termes: Vec<Terme>,
}

/// Les ajouts de l'utilisateur.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ajouts {
    pub listes: HashMap<String, Vec<String>>,
    pub temoins: Vec<Temoin>,
}

/// Une phrase témoin et ce qu'elle doit donner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Temoin {
    pub texte: String,
    pub attendu: String,
}

/// Un témoin rejoué.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EtatTemoin {
    pub texte: String,
    pub attendu: String,
    /// Livré avec Aria (sinon, ajouté par l'utilisateur).
    pub livre: bool,
    pub obtenu: String,
    pub ok: bool,
    pub termes: Vec<Terme>,
}

/// Les tirets visibles : la ponctuation « tiret » d'Unicode (hors le trait d'union ASCII) et le signe moins. Le
/// trait d'union conditionnel (U+00AD) n'en est pas un.
fn est_tiret(c: char) -> bool {
    matches!(
        c,
        '\u{058A}' | '\u{05BE}' | '\u{1400}' | '\u{1806}' | '\u{2010}'
            ..='\u{2015}'
                | '\u{2212}'
                | '\u{2E17}'
                | '\u{2E1A}'
                | '\u{2E3A}'
                | '\u{2E3B}'
                | '\u{2E40}'
                | '\u{2E5D}'
                | '\u{301C}'
                | '\u{3030}'
                | '\u{30A0}'
                | '\u{FE31}'
                | '\u{FE32}'
                | '\u{FE58}'
                | '\u{FE63}'
                | '\u{FF0D}'
                | '\u{10EAD}'
    )
}

/// Un espace au sens de Python sur de l'ASCII.
fn espace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0B' | '\x0C' | '\r' | '\x1C'..='\x1F' | ' ')
}

/// Décomposé (NFKD), tirets visibles changés en `-` si demandé, tout ce qui n'est pas ASCII effacé, en minuscules.
pub fn normaliser(texte: &str, tirets: bool) -> String {
    texte
        .nfkd()
        .map(|c| if tirets && est_tiret(c) { '-' } else { c })
        .filter(char::is_ascii)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Espaces et tirets confondus en une seule espace.
fn tasser(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut separe = false;
    for c in texte.chars() {
        if espace(c) || c == '-' {
            separe = true;
        } else {
            if separe {
                sortie.push(' ');
            }
            separe = false;
            sortie.push(c);
        }
    }
    if separe {
        sortie.push(' ');
    }
    sortie
}

/// Un terme tel qu'il est comparé : sans accents, en minuscules, espaces et tirets confondus.
pub fn cle(terme: &str) -> String {
    tasser(&normaliser(terme, false)).trim_matches(espace).to_owned()
}

/// Le motif d'une liste : ses termes en mots entiers, les plus longs d'abord, une espace valant espaces et tirets.
fn motif(termes: &[&String]) -> Result<Regex, String> {
    let mut tries: Vec<&String> = termes.to_vec();
    tries.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    tries.dedup();
    let alternatives: Vec<String> = tries
        .iter()
        .map(|t| regex::escape(t).replace(' ', &format!("{S_TIRET}+")))
        .collect();
    Regex::new(&format!(r"\b({})\b", alternatives.join("|"))).map_err(|e| e.to_string())
}

/// Les nombres écrits en lettres que le filtre lit comme un âge.
fn nombres() -> HashMap<String, u64> {
    let mut n: HashMap<String, u64> = [
        ("un", 1),
        ("une", 1),
        ("deux", 2),
        ("trois", 3),
        ("quatre", 4),
        ("cinq", 5),
        ("six", 6),
        ("sept", 7),
        ("huit", 8),
        ("neuf", 9),
        ("dix", 10),
        ("onze", 11),
        ("douze", 12),
        ("treize", 13),
        ("quatorze", 14),
        ("quinze", 15),
        ("seize", 16),
        ("dix sept", 17),
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("eleven", 11),
        ("twelve", 12),
        ("thirteen", 13),
        ("fourteen", 14),
        ("fifteen", 15),
        ("sixteen", 16),
        ("seventeen", 17),
        ("dix huit", 18),
        ("dix neuf", 19),
        ("quatre vingt", 80),
        ("quatre vingts", 80),
        ("soixante et onze", 71),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    const UNITES_FR: [&str; 9] = [
        "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf",
    ];
    const DIZAINES_FR: [(&str, u64); 8] = [
        ("vingt", 20),
        ("trente", 30),
        ("quarante", 40),
        ("cinquante", 50),
        ("soixante", 60),
        ("septante", 70),
        ("huitante", 80),
        ("nonante", 90),
    ];
    const ADOS_FR: [(&str, u64); 10] = [
        ("dix", 10),
        ("onze", 11),
        ("douze", 12),
        ("treize", 13),
        ("quatorze", 14),
        ("quinze", 15),
        ("seize", 16),
        ("dix sept", 17),
        ("dix huit", 18),
        ("dix neuf", 19),
    ];
    for (dizaine, valeur) in DIZAINES_FR {
        for (i, unite) in (1u64..).zip(UNITES_FR) {
            n.insert(format!("{dizaine} {unite}"), valeur + i);
        }
        n.insert(format!("{dizaine} et un"), valeur + 1);
    }
    for (ado, valeur) in ADOS_FR {
        n.insert(format!("soixante {ado}"), 60 + valeur);
        n.insert(format!("quatre vingt {ado}"), 80 + valeur);
    }
    for (i, unite) in (1u64..).zip(UNITES_FR) {
        n.insert(format!("quatre vingt {unite}"), 80 + i);
    }
    for (dizaine, valeur) in DIZAINES_EN {
        for (i, unite) in (1u64..).zip(UNITES_EN) {
            n.insert(format!("{dizaine} {unite}"), valeur + i);
        }
    }
    n
}

const UNITES_EN: [&str; 9] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
];
const DIZAINES_EN: [(&str, u64); 8] = [
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
];

fn compose_anglais(cle: &str) -> bool {
    let mut mots = cle.split(' ');
    matches!(
        (mots.next(), mots.next(), mots.next()),
        (Some(d), Some(u), None) if DIZAINES_EN.iter().any(|(x, _)| *x == d) && UNITES_EN.contains(&u)
    )
}

/// Les motifs d'âge. Un nombre anglais à deux mots n'est un âge adulte que lié par un trait d'union
/// (« twenty-two ») ou devant « years » au pluriel (« twenty two years old ») : « twenty two-year-old girls » dit
/// vingt fillettes de deux ans. La v2 l'écrivait avec une anticipation (`(?=\s+years\b)`) ; ici, la forme avec une
/// espace est une branche à part du premier motif, qui inclut « years ». Dans les deux autres motifs, sans elle,
/// « aged twenty two » se lit 20 au lieu de 22 : le verdict (adulte) est le même.
fn motifs_ages(nombres: &HashMap<String, u64>) -> Result<[Regex; 3], String> {
    let mut formes: Vec<String> = nombres
        .keys()
        .map(|cle| {
            let mots: Vec<String> = cle.split(' ').map(regex::escape).collect();
            if compose_anglais(cle) {
                mots.join("-")
            } else {
                mots.join(&format!("{S_TIRET}+"))
            }
        })
        .collect();
    formes.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    let nombre = format!("(?P<a>[0-9]{{1,2}}|{})", formes.join("|"));
    let mut composes: Vec<String> = nombres
        .keys()
        .filter(|c| compose_anglais(c))
        .map(|c| c.replace(' ', &format!("{S}+")))
        .collect();
    composes.sort();
    let suffixe = format!(
        r"(?:yos?|y/os?|y\.o\.?|yrs?(?:{S_TIRET}*olds?)?|years?{S_TIRET}*olds?|years?{S}+of{S}+age|ans?)"
    );
    let premier = format!(
        r"\b(?:{nombre}{S}*-?{S}*{suffixe}|(?P<b>{}){S}+(?:years{S_TIRET}*olds?|years{S}+of{S}+age))\b",
        composes.join("|")
    );
    let deuxieme = format!(r"\bage[ds]?{S}*(?:of{S}+)?:?{S}*{nombre}\b");
    let troisieme = format!(r"\b{PERSONNE}{S}+(?:of|de){S}+{nombre}\b");
    let compiler = |m: &str| Regex::new(m).map_err(|e| e.to_string());
    Ok([compiler(&premier)?, compiler(&deuxieme)?, compiler(&troisieme)?])
}

/// Le filtre prêt à l'emploi : socle et ajouts réunis.
pub struct Filtre {
    /// Pour chaque liste, d'où vient chaque terme (`socle` l'emporte sur `ajouts`).
    etage: Vec<HashMap<String, &'static str>>,
    motifs: Vec<Regex>,
    ages: [Regex; 3],
    nombres: HashMap<String, u64>,
}

impl Filtre {
    /// Le filtre d'un socle et des ajouts de l'utilisateur.
    ///
    /// # Errors
    /// [`ErreurSecurite`] : le socle est absent, vide ou mal formé (tout sera alors refusé).
    pub fn depuis(socle: &str, ajouts: &Ajouts) -> Result<Self, ErreurSecurite> {
        let panne = || {
            ErreurSecurite(format!(
                "le socle du filtre est absent, vide ou illisible ({FICHIER_SOCLE}) : répare ce fichier ou \
                 réinstalle Aria"
            ))
        };
        let brut: Value = serde_saphyr::from_str(socle).map_err(|_| panne())?;
        let racine = brut.as_object().ok_or_else(panne)?;
        let mut etage = Vec::new();
        let mut motifs = Vec::new();
        for (liste, _) in LISTES {
            let termes = racine.get(liste).and_then(Value::as_array).ok_or_else(panne)?;
            if termes.is_empty() {
                return Err(panne());
            }
            let mut d: HashMap<String, &'static str> = HashMap::new();
            for t in ajouts.listes.get(liste).into_iter().flatten() {
                d.insert(cle(t), "ajouts");
            }
            for t in termes {
                let c = t.as_str().map(cle).filter(|c| !c.is_empty()).ok_or_else(panne)?;
                d.insert(c, "socle");
            }
            let cles: Vec<&String> = d.keys().collect();
            motifs.push(motif(&cles).map_err(|_| panne())?);
            etage.push(d);
        }
        let nombres = nombres();
        let ages = motifs_ages(&nombres).map_err(|_| panne())?;
        Ok(Self {
            etage,
            motifs,
            ages,
            nombres,
        })
    }

    /// Le filtre livré, avec les ajouts de l'utilisateur.
    ///
    /// # Errors
    /// Voir [`Filtre::depuis`].
    pub fn livre(ajouts: &Ajouts) -> Result<Self, ErreurSecurite> {
        Self::depuis(SOCLE, ajouts)
    }

    fn chercher(&self, i: usize, texte: &str) -> Option<Terme> {
        let trouve = self.motifs.get(i)?.captures(texte)?;
        let terme = cle(trouve.get(1)?.as_str());
        let etage = self
            .etage
            .get(i)
            .and_then(|e| e.get(&terme))
            .copied()
            .unwrap_or("socle");
        Some(Terme {
            liste: LISTES.get(i).map(|(l, _)| (*l).to_owned()).unwrap_or_default(),
            etage: etage.to_owned(),
            terme,
        })
    }

    fn valeur(&self, nombre: &str) -> Option<u64> {
        if nombre.bytes().all(|b| b.is_ascii_digit()) {
            return nombre.parse().ok();
        }
        self.nombres.get(&tasser(nombre)).copied()
    }

    fn mineur(&self, texte: &str) -> Option<Terme> {
        if let Some(trouve) = self.chercher(0, texte) {
            return Some(trouve);
        }
        for motif in &self.ages {
            for prise in motif.captures_iter(texte) {
                let nombre = prise
                    .name("a")
                    .or_else(|| prise.name("b"))
                    .map(|m| m.as_str())
                    .unwrap_or_default();
                if self.valeur(nombre).is_some_and(|v| v < AGE_MAJORITE) {
                    return Some(Terme {
                        terme: prise.get(0).map(|m| m.as_str().to_owned()).unwrap_or_default(),
                        liste: "mineurs".to_owned(),
                        etage: "age".to_owned(),
                    });
                }
            }
        }
        None
    }

    /// Ce qui refuse ces textes pris ensemble, ou rien s'ils passent.
    pub fn analyser(&self, textes: &[&str]) -> Option<Refus> {
        let brut = textes.join("\n");
        let mut lectures = vec![normaliser(&brut, false)];
        let avec_tirets = normaliser(&brut, true);
        if !lectures.contains(&avec_tirets) {
            lectures.push(avec_tirets);
        }
        for texte in &lectures {
            if let Some(interdit) = self.chercher(2, texte) {
                return Some(Refus {
                    message: format!("terme interdit « {} »", interdit.terme),
                    termes: vec![interdit],
                });
            }
            if let Some(mineur) = self.mineur(texte)
                && let Some(sensible) = self.chercher(1, texte)
            {
                return Some(Refus {
                    message: format!(
                        "personnage mineur (« {} ») associé à un terme sensible (« {} »)",
                        mineur.terme, sensible.terme
                    ),
                    termes: vec![mineur, sensible],
                });
            }
        }
        None
    }

    /// Refuse ces textes s'ils associent un mineur à un terme sensible.
    ///
    /// # Errors
    /// [`ErreurSecurite`] avec le motif du refus.
    pub fn verifier(&self, textes: &[&str]) -> Result<(), ErreurSecurite> {
        match self.analyser(textes) {
            Some(refus) => Err(ErreurSecurite(refus.message)),
            None => Ok(()),
        }
    }

    /// Rejoue les témoins livrés et ceux de l'utilisateur.
    pub fn temoins(&self, ajouts: &Ajouts) -> Vec<EtatTemoin> {
        let livres = temoins_livres().into_iter().map(|t| (t, true));
        let siens = ajouts.temoins.iter().cloned().map(|t| (t, false));
        livres
            .chain(siens)
            .map(|(t, livre)| {
                let refus = self.analyser(&[t.texte.as_str()]);
                let obtenu = if refus.is_some() { "bloque" } else { "passe" };
                EtatTemoin {
                    ok: obtenu == t.attendu,
                    obtenu: obtenu.to_owned(),
                    termes: refus.map(|r| r.termes).unwrap_or_default(),
                    livre,
                    texte: t.texte,
                    attendu: t.attendu,
                }
            })
            .collect()
    }
}

fn temoin(valeur: &Value) -> Option<Temoin> {
    let texte = valeur.get("texte")?.as_str()?;
    let attendu = valeur.get("attendu")?.as_str()?;
    ATTENDUS.contains(&attendu).then(|| Temoin {
        texte: texte.to_owned(),
        attendu: attendu.to_owned(),
    })
}

/// Les phrases témoins livrées (une entrée mal formée est ignorée).
pub fn temoins_livres() -> Vec<Temoin> {
    serde_saphyr::from_str::<Value>(TEMOINS)
        .ok()
        .and_then(|v| v.as_array().map(|l| l.iter().filter_map(temoin).collect()))
        .unwrap_or_default()
}

/// Les ajouts de l'utilisateur, lus dans la section `securite` de `reglages.json`. Absente : aucun ajout.
///
/// # Errors
/// [`ErreurSecurite`] : la section est mal formée (tout sera alors refusé).
pub fn lire_ajouts(
    section: Option<&serde_json::Map<String, Value>>,
    fichier_reglages: &Path,
) -> Result<Ajouts, ErreurSecurite> {
    let mut ajouts = Ajouts::default();
    let Some(section) = section.filter(|s| !s.is_empty()) else {
        return Ok(ajouts);
    };
    let panne = || {
        ErreurSecurite(format!(
            "tes ajouts au filtre sont mal formés (section « {SECTION} » de {}) : corrige-les ou retire cette section",
            fichier_reglages.display()
        ))
    };
    // Ce que Python tient pour « rien » : null, false, 0, texte vide, objet vide (une liste vide se lit comme vide).
    let vide = |v: &Value| {
        matches!(v, Value::Null | Value::Bool(false))
            || v.as_str() == Some("")
            || v.as_f64() == Some(0.0)
            || v.as_object().is_some_and(serde_json::Map::is_empty)
    };
    for (liste, _) in LISTES {
        let Some(valeur) = section.get(liste).filter(|v| !vide(v)) else {
            continue;
        };
        let termes = valeur.as_array().ok_or_else(panne)?;
        let mut lus = Vec::new();
        for t in termes {
            let terme = t.as_str().filter(|s| !cle(s).is_empty()).ok_or_else(panne)?;
            lus.push(terme.to_owned());
        }
        ajouts.listes.insert(liste.to_owned(), lus);
    }
    if let Some(valeur) = section.get("temoins").filter(|v| !vide(v)) {
        let liste = valeur.as_array().ok_or_else(panne)?;
        for t in liste {
            ajouts.temoins.push(temoin(t).ok_or_else(panne)?);
        }
    }
    Ok(ajouts)
}

/// Le filtre de ce dossier de données : le socle livré et les ajouts de l'utilisateur. Un `reglages.json` illisible
/// laisse le socle seul : le filtre n'est jamais affaibli.
///
/// # Errors
/// [`ErreurSecurite`] : socle ou ajouts mal formés.
pub fn filtre(dossier: &Dossier) -> Result<Filtre, ErreurSecurite> {
    let section = reglages::section(dossier, SECTION);
    let ajouts = lire_ajouts(section.as_ref(), &dossier.reglages())?;
    Filtre::livre(&ajouts)
}

/// Refuse ces textes s'ils associent un mineur à un terme sensible, d'après le filtre de ce dossier de données.
///
/// # Errors
/// [`ErreurSecurite`].
pub fn verifier(dossier: &Dossier, textes: &[&str]) -> Result<(), ErreurSecurite> {
    filtre(dossier)?.verifier(textes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn livre() -> Filtre {
        Filtre::livre(&Ajouts::default()).unwrap()
    }

    fn bloque(f: &Filtre, textes: &[&str]) -> bool {
        f.analyser(textes).is_some()
    }

    #[test]
    fn toutes_les_phrases_temoins_livrees_sont_conformes() {
        let f = livre();
        let etats = f.temoins(&Ajouts::default());
        assert!(etats.len() >= 20, "{}", etats.len());
        let ecarts: Vec<&EtatTemoin> = etats.iter().filter(|e| !e.ok).collect();
        assert!(ecarts.is_empty(), "{ecarts:#?}");
        assert!(etats.iter().any(|e| e.attendu == "bloque") && etats.iter().any(|e| e.attendu == "passe"));
    }

    #[test]
    fn le_message_est_celui_de_la_v2() {
        let e = ErreurSecurite("terme interdit « x »".to_owned());
        assert_eq!(
            e.to_string(),
            "ERREUR AS-18 · Génération bloquée : terme interdit « x »"
        );
        let refus = livre().analyser(&["a child", "nude"]).unwrap();
        assert_eq!(
            refus.message,
            "personnage mineur (« child ») associé à un terme sensible (« nude »)"
        );
        assert_eq!(
            refus
                .termes
                .iter()
                .map(|t| (t.terme.as_str(), t.etage.as_str()))
                .collect::<Vec<_>>(),
            vec![("child", "socle"), ("nude", "socle")]
        );
        let age = livre().analyser(&["a girl of 14", "nude"]).unwrap();
        assert_eq!(age.termes[0].etage, "age");
    }

    #[test]
    fn un_mineur_seul_ou_un_adulte_avec_un_terme_sensible_passe() {
        let f = livre();
        assert!(!bloque(&f, &["two children building a treehouse"]));
        assert!(!bloque(&f, &["a woman of 30", "nude"]));
        assert!(!bloque(&f, &["25 year old", "nude"]));
        assert!(!bloque(&f, &["one of 3 dancers", "nude"]));
        assert!(!bloque(&f, &["une femme de vingt-neuf ans", "nude"]));
        assert!(!bloque(&f, &["dix-huit ans", "nude"]));
    }

    #[test]
    fn les_formes_d_age_reconnues() {
        let f = livre();
        for age in [
            "a girl of 14",
            "une fille de 14",
            "12 yrs old",
            "16 years of age",
            "age of 15",
            "aged 9",
            "12 ans",
            "seventeen-year-old",
        ] {
            assert!(bloque(&f, &[age, "nude"]), "{age}");
        }
    }

    #[test]
    fn un_mot_qui_commence_par_old_n_est_pas_un_age() {
        let f = livre();
        for texte in ["12 oldsmobile", "12 oldschool", "12 yosemite"] {
            assert!(!bloque(&f, &[texte, "nude"]), "{texte}");
        }
    }

    #[test]
    fn un_nombre_anglais_a_deux_mots() {
        let f = livre();
        // Vingt fillettes de deux ans : bloque.
        assert!(bloque(&f, &["twenty two-year-old girls", "nude"]));
        // Un adulte de vingt-deux ans : passe.
        for adulte in [
            "twenty-two-year-old",
            "twenty two years old",
            "twenty-two years old",
            "twenty two years of age",
        ] {
            assert!(!bloque(&f, &[adulte, "nude"]), "{adulte}");
        }
    }

    #[test]
    fn un_mineur_cite_apres_un_adulte_bloque_toujours() {
        assert!(bloque(&livre(), &["a 30 year old man and a 9 year old", "nude"]));
    }

    fn francais(n: u64) -> String {
        const SIMPLES: [&str; 20] = [
            "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix", "onze",
            "douze", "treize", "quatorze", "quinze", "seize", "dix-sept", "dix-huit", "dix-neuf",
        ];
        const DIZAINES: [&str; 7] = ["", "", "vingt", "trente", "quarante", "cinquante", "soixante"];
        let simple = |i: u64| SIMPLES[usize::try_from(i).unwrap()].to_owned();
        match n {
            0..=19 => simple(n),
            20..=69 => {
                let d = DIZAINES[usize::try_from(n / 10).unwrap()];
                match n % 10 {
                    0 => d.to_owned(),
                    1 => format!("{d}-et-un"),
                    u => format!("{d}-{}", simple(u)),
                }
            }
            70 => "soixante-dix".to_owned(),
            71 => "soixante-et-onze".to_owned(),
            72..=79 => format!("soixante-{}", simple(n - 60)),
            80 => "quatre-vingts".to_owned(),
            81..=99 => format!("quatre-vingt-{}", simple(n - 80)),
            _ => unreachable!(),
        }
    }

    fn anglais(n: u64) -> String {
        const SIMPLES: [&str; 20] = [
            "zero",
            "one",
            "two",
            "three",
            "four",
            "five",
            "six",
            "seven",
            "eight",
            "nine",
            "ten",
            "eleven",
            "twelve",
            "thirteen",
            "fourteen",
            "fifteen",
            "sixteen",
            "seventeen",
            "eighteen",
            "nineteen",
        ];
        const DIZAINES: [&str; 10] = [
            "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
        ];
        let i = usize::try_from(n).unwrap();
        if n < 20 {
            return SIMPLES[i].to_owned();
        }
        match n % 10 {
            0 => DIZAINES[i / 10].to_owned(),
            u => format!("{}-{}", DIZAINES[i / 10], SIMPLES[usize::try_from(u).unwrap()]),
        }
    }

    #[test]
    fn tous_les_ages_de_1_a_99_bloquent_si_et_seulement_si_moins_de_18() {
        let f = livre();
        for n in 1..100u64 {
            let formes = [
                format!("{n} ans"),
                format!("{n} years old"),
                format!("{n}-year-old"),
                format!("{n} yo"),
                format!("a girl of {n}"),
                format!("aged {n}"),
                format!("{} ans", francais(n)),
                format!("{}-year-old", anglais(n)),
            ];
            for forme in &formes {
                assert_eq!(bloque(&f, &[forme, "nude"]), n < AGE_MAJORITE, "{forme}");
            }
        }
    }

    #[test]
    fn un_age_au_pluriel_est_lu_comme_au_singulier() {
        let f = livre();
        for n in 1..100u64 {
            for forme in [
                format!("{n}-year-olds"),
                format!("{n} yos"),
                format!("{n} y/os"),
                format!("{n} yrs olds"),
            ] {
                assert_eq!(bloque(&f, &[&forme, "nude"]), n < AGE_MAJORITE, "{forme}");
            }
        }
    }

    #[test]
    fn les_tirets_typographiques_valent_le_trait_d_union() {
        let f = livre();
        for tiret in [
            '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2212}', '\u{FE63}', '\u{FF0D}',
        ] {
            assert!(
                bloque(&f, &[&format!("dix{tiret}sept ans"), "nude"]),
                "dix{tiret}sept"
            );
            assert!(bloque(&f, &[&format!("10{tiret}12 ans"), "nude"]), "10{tiret}12");
            assert!(!bloque(&f, &[&format!("25{tiret}30 ans"), "nude"]), "25{tiret}30");
            assert!(
                !bloque(&f, &[&format!("vingt{tiret}neuf ans"), "nude"]),
                "vingt{tiret}neuf"
            );
        }
    }

    #[test]
    fn un_tiret_dans_un_mot_du_filtre_ne_le_cache_pas() {
        let f = livre();
        let socle: Value = serde_saphyr::from_str(SOCLE).unwrap();
        for (liste, prefixe) in [("sensibles", "a child, "), ("toujours_bloques", "")] {
            for terme in socle[liste].as_array().unwrap().iter().filter_map(Value::as_str) {
                let caracteres: Vec<char> = terme.chars().collect();
                for i in 1..caracteres.len() {
                    if caracteres[i - 1] == ' ' || caracteres[i] == ' ' {
                        continue;
                    }
                    for coupe in ['\u{00AD}', '\u{2013}', '\u{2011}', '\u{2212}'] {
                        let coupe_terme: String = caracteres[..i]
                            .iter()
                            .chain(std::iter::once(&coupe))
                            .chain(&caracteres[i..])
                            .collect();
                        let texte = format!("{prefixe}{coupe_terme}");
                        assert!(bloque(&f, &[&texte]), "{liste} : {texte:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn un_socle_absent_vide_ou_illisible_refuse_tout() {
        for socle in [
            "",
            "pas: [du yaml",
            "- une\n- liste\n",
            "mineurs: [enfant]\nsensibles: []\ntoujours_bloques: [x]\n",
        ] {
            let erreur = Filtre::depuis(socle, &Ajouts::default()).err().unwrap();
            assert!(erreur.0.contains("socle"), "{socle:?} : {erreur}");
        }
    }

    #[test]
    fn les_ajouts_comptent_et_ne_retirent_rien() {
        let section = json!({ "sensibles": ["Zorglub"], "mineurs": [], "temoins": [] });
        let ajouts = lire_ajouts(section.as_object(), Path::new("reglages.json")).unwrap();
        let f = Filtre::livre(&ajouts).unwrap();
        let refus = f.analyser(&["a child", "zorglub"]).unwrap();
        assert_eq!(refus.termes[1].etage, "ajouts");
        assert!(bloque(&f, &["a child", "nude"]), "le socle reste entier");
    }

    #[test]
    fn des_ajouts_mal_formes_refusent_tout() {
        let section = json!({ "sensibles": "pas une liste" });
        let erreur = lire_ajouts(section.as_object(), Path::new("C:/x/reglages.json")).unwrap_err();
        assert!(
            erreur.0.contains("reglages.json") && erreur.0.contains("« securite »"),
            "{erreur}"
        );
        let section = json!({ "temoins": [{ "texte": "x", "attendu": "peut-être" }] });
        assert!(lire_ajouts(section.as_object(), Path::new("r.json")).is_err());
        assert_eq!(lire_ajouts(None, Path::new("r.json")).unwrap(), Ajouts::default());
    }

    #[test]
    fn un_reglages_illisible_laisse_le_socle_seul() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        std::fs::write(dossier.reglages(), "{ cassé").unwrap();
        assert!(verifier(&dossier, &["un paysage de montagne"]).is_ok());
        assert!(verifier(&dossier, &["a child", "nude"]).is_err());
    }

    #[test]
    fn la_normalisation_efface_accents_et_non_ascii() {
        assert_eq!(normaliser("Élève ÇA", false), "eleve ca");
        assert_eq!(normaliser("n\u{2013}ude", false), "nude");
        assert_eq!(normaliser("dix\u{2011}sept", true), "dix-sept");
        assert_eq!(cle("  Under -  Age "), "under age");
    }
}
