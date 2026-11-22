//! Les listes guidées du Scénario, comme en v2 (`app/listes.py`, `prompt/listes/*.md`) : lecture, règles et
//! assemblage du prompt.
//!
//! Chaque fichier est un bloc : des sous-listes en tableaux Markdown (Valeur | Prompt image | Remplace | Adulte |
//! Genre…) suivies d'une section « Incompatibilités ». Les fichiers sont livrés avec l'appli (les tests les lisent
//! tous). Règles appliquées à l'assemblage (l'interface les applique aussi, mais c'est ici qu'elles font foi) :
//! - Remplace : la valeur prend la place des sous-listes indiquées (une robe remplace le Bas) ;
//! - Incompatibilités : deux valeurs incompatibles sont refusées ensemble ;
//! - Genre : une valeur « femme » ou « homme » n'est proposée qu'à ce genre ;
//! - Avec <sous-liste> : la valeur n'est proposée qu'avec les valeurs indiquées de cette autre sous-liste ;
//! - Adulte : une valeur adulte est refusée pour un personnage mineur (ERREUR AS-18), puis le prompt final passe par
//!   le filtre AS-18 comme tous les autres.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::securite::{self, ErreurSecurite};
use crate::{Dossier, prompt, reglages, workflows};

/// Les fichiers livrés : `prompt/listes/<id>.md`.
const FICHIERS: [(&str, &str); 7] = [
    ("commun", include_str!("../../prompt/listes/commun.md")),
    ("creature", include_str!("../../prompt/listes/creature.md")),
    ("lieu", include_str!("../../prompt/listes/lieu.md")),
    (
        "mise-en-scene",
        include_str!("../../prompt/listes/mise-en-scene.md"),
    ),
    ("objet", include_str!("../../prompt/listes/objet.md")),
    ("personnage", include_str!("../../prompt/listes/personnage.md")),
    ("video", include_str!("../../prompt/listes/video.md")),
];

/// Un sujet du Scénario : son nom, ses blocs et le nom donné à défaut.
struct DefSujet {
    id: &'static str,
    nom: &'static str,
    blocs: &'static [&'static str],
    defaut: &'static str,
}

const SUJETS: [DefSujet; 4] = [
    DefSujet {
        id: "personnage",
        nom: "Personnage",
        blocs: &["personnage", "mise-en-scene", "commun"],
        defaut: "person",
    },
    DefSujet {
        id: "creature",
        nom: "Créature / animal",
        blocs: &["creature", "mise-en-scene", "commun"],
        defaut: "creature",
    },
    DefSujet {
        id: "objet",
        nom: "Objet",
        blocs: &["objet", "commun"],
        defaut: "object",
    },
    DefSujet {
        id: "lieu",
        nom: "Lieu / paysage",
        blocs: &["lieu", "mise-en-scene", "commun"],
        defaut: "landscape",
    },
];

/// Le bloc ajouté à tous les sujets quand le modèle cible est un modèle vidéo.
pub const PLAN: &str = "video";

/// Les accordéons de l'interface ; une sous-liste absente d'ici tombe dans « Autres ».
/// Les accordéons d'un bloc : (groupe, sous-listes).
type Accordeons = &'static [(&'static str, &'static [&'static str])];

const GROUPES: [(&str, Accordeons); 5] = [
    (
        "personnage",
        &[
            ("Identité", &["Genre", "Âge", "Origine"]),
            (
                "Corps",
                &[
                    "Silhouette",
                    "Taille",
                    "Poitrine",
                    "Hanches",
                    "Teint",
                    "Signe particulier",
                    "Motif de tatouage",
                ],
            ),
            (
                "Visage",
                &[
                    "Forme du visage",
                    "Forme des yeux",
                    "Couleur des yeux",
                    "Expression du regard",
                    "Expression du visage",
                    "Nez",
                    "Lèvres",
                    "Sourcils",
                    "Pilosité du visage",
                    "Trait marquant",
                ],
            ),
            (
                "Cheveux",
                &[
                    "Couleur des cheveux",
                    "Longueur des cheveux",
                    "Texture des cheveux",
                    "Coiffure",
                ],
            ),
            (
                "Tenue",
                &[
                    "Style vestimentaire",
                    "Haut",
                    "Bas",
                    "Chaussures",
                    "Tenue intime",
                    "Accessoires",
                    "Maquillage",
                ],
            ),
            ("Attitude", &["Pose", "Action"]),
        ],
    ),
    (
        "creature",
        &[
            (
                "Apparence",
                &[
                    "Type",
                    "Taille",
                    "Morphologie",
                    "Revêtement",
                    "Couleurs",
                    "Yeux",
                    "Signe distinctif",
                ],
            ),
            ("Attitude", &["Attitude", "Action"]),
        ],
    ),
    (
        "mise-en-scene",
        &[
            ("Lieu et moment", &["Lieu", "Moment", "Saison"]),
            ("Ambiance", &["Ambiance"]),
        ],
    ),
    (
        "commun",
        &[
            ("Style", &["Style", "Palette", "Finition"]),
            ("Lumière", &["Lumière"]),
            ("Cadrage", &["Cadrage", "Angle de vue", "Objectif"]),
        ],
    ),
    (
        PLAN,
        &[
            ("Caméra", &["Caméra"]),
            ("Voix", &["Langue", "Ton"]),
            ("Son", &["Ambiance sonore", "Musique"]),
        ],
    ),
];

/// Onglets du Scénario : `sujet` (par défaut), `decor` (décor et ambiance), `scene` (mise en scène).
const ONGLETS: [((&str, &str), &str); 6] = [
    (("personnage", "Attitude"), "scene"),
    (("creature", "Attitude"), "scene"),
    (("commun", "Cadrage"), "scene"),
    (("commun", "Style"), "decor"),
    (("commun", "Lumière"), "decor"),
    (("commun", "Autres"), "decor"),
];
const ONGLETS_BLOCS: [(&str, &str); 4] = [
    ("lieu", "decor"),
    ("mise-en-scene", "decor"),
    ("commun", "decor"),
    (PLAN, "scene"),
];

/// Une sous-liste proposée seulement si une autre a une valeur contenant ce texte.
/// Une paire de noms : (bloc, sous-liste) ou (sous-liste, texte).
type Paire = (&'static str, &'static str);

const CONDITIONS: [(Paire, Paire); 1] = [(
    ("personnage", "Motif de tatouage"),
    ("Signe particulier", "tatou"),
)];
/// Réunis en « wearing … ».
const VETEMENTS: [&str; 4] = ["Haut", "Bas", "Chaussures", "Tenue intime"];
/// Placé en tête du prompt.
const STYLE: (&str, &str) = ("commun", "Style");
/// L'âge précis, de 3 à 120 ans.
pub const AGE_PRECIS: (u32, u32) = (3, 120);
const LONGUEUR_TEXTE: usize = 200;
const LONGUEUR_PRECISIONS: usize = 500;
pub const LONGUEUR_ACTION: usize = 500;
/// Plusieurs répliques, une par ligne.
pub const LONGUEUR_REPLIQUE: usize = 600;
const LONGUEUR_MUSIQUE: usize = 200;
/// Familles d'origines (colonnes Famille et Origines).
pub const FAMILLES: [&str; 8] = [
    "europe",
    "asie-est",
    "asie-sud-est",
    "asie-sud",
    "moyen-orient",
    "afrique",
    "ameriques",
    "oceanie",
];
/// Colonne de cohérence avec une autre sous-liste du même fichier (« Avec Silhouette »).
const AVEC: &str = "Avec ";
/// Ce qui change d'un plan à l'autre : pas dans la description d'un personnage de séquence.
const ATTITUDE: [&str; 3] = ["Pose", "Action", "Attitude"];
/// Choisis plan par plan par le découpage : pas dans le décor commun d'une séquence.
const CAMERA: [&str; 3] = ["Cadrage", "Angle de vue", "Objectif"];

/// Une valeur d'une sous-liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Valeur {
    pub nom: String,
    pub prompt: String,
    pub remplace: Vec<String>,
    pub adulte: bool,
    pub genre: Option<String>,
    /// Âge : le nom selon le genre (femme, homme, androgyne).
    pub prompts_genre: Option<BTreeMap<String, String>>,
    pub mineur: bool,
    /// Âge : la tranche lue dans « (18-25 ans) ».
    pub bornes: Option<(u32, u32)>,
    /// Origine : ses familles.
    pub familles: Vec<String>,
    /// Les familles pour lesquelles la valeur est proposée ; vide : toutes.
    pub origines: Vec<String>,
    pub description: String,
    /// Sous-liste → valeurs avec lesquelles celle-ci reste cohérente.
    pub avec: BTreeMap<String, Vec<String>>,
    /// Le groupe du menu (Pose, Action).
    pub categorie: String,
}

/// Une sous-liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SousListe {
    pub nom: String,
    pub note: String,
    pub valeurs: Vec<Valeur>,
}

impl SousListe {
    fn valeur(&self, nom: &str) -> Option<&Valeur> {
        self.valeurs.iter().find(|v| v.nom == nom)
    }
}

/// Une paire (sous-liste, valeur).
type Choix = (String, String);

/// Un fichier de listes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloc {
    pub id: String,
    pub titre: String,
    pub sous_listes: Vec<SousListe>,
    pub incompatibilites: Vec<(Choix, Choix)>,
}

impl Bloc {
    fn sous_liste(&self, nom: &str) -> Option<&SousListe> {
        self.sous_listes.iter().find(|s| s.nom == nom)
    }
}

// --- lecture ----------------------------------------------------------------------------------------------------

fn colonne(entete: &str) -> Option<&'static str> {
    Some(match entete {
        "Valeur" => "nom",
        "Prompt image" | "Prompt vidéo" => "prompt",
        "Remplace" => "remplace",
        "Adulte" => "adulte",
        "Genre" => "genre",
        "Mineur" => "mineur",
        "Prompt femme" => "femme",
        "Prompt homme" => "homme",
        "Prompt androgyne" => "androgyne",
        "Famille" => "famille",
        "Origines" => "origines",
        "Description" => "description",
        "Catégorie" => "categorie",
        _ => return None,
    })
}

fn cellules(ligne: &str) -> Vec<String> {
    let t = ligne.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_owned()).collect()
}

fn oui(texte: &str, ou: &str) -> Result<bool, String> {
    match texte {
        "" => Ok(false),
        "oui" => Ok(true),
        autre => Err(format!("{ou} : « {autre} » au lieu de « oui » ou vide")),
    }
}

fn familles(texte: &str, ou: &str) -> Result<Vec<String>, String> {
    let liste: Vec<String> = texte
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(str::to_owned)
        .collect();
    if let Some(inconnue) = liste.iter().find(|f| !FAMILLES.contains(&f.as_str())) {
        return Err(format!(
            "{ou} : famille « {inconnue} » inconnue ({})",
            FAMILLES.join(", ")
        ));
    }
    Ok(liste)
}

fn liste_de(texte: &str) -> Vec<String> {
    texte
        .split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
        .collect()
}

static TRANCHE: OnceLock<Option<Regex>> = OnceLock::new();
static ET_PLUS: OnceLock<Option<Regex>> = OnceLock::new();

/// La tranche d'âge écrite dans un nom : « (18-25 ans) », « (65 ans et plus) ».
fn bornes(nom: &str) -> Option<(u32, u32)> {
    let tranche = TRANCHE
        .get_or_init(|| Regex::new(r"\((\d+)-(\d+) ans\)").ok())
        .as_ref();
    let et_plus = ET_PLUS
        .get_or_init(|| Regex::new(r"\((\d+) ans et plus\)").ok())
        .as_ref();
    let mut trouvees = None;
    if let Some(p) = tranche.and_then(|r| r.captures(nom)) {
        trouvees = Some((p[1].parse().ok()?, p[2].parse().ok()?));
    }
    if let Some(p) = et_plus.and_then(|r| r.captures(nom)) {
        trouvees = Some((p[1].parse().ok()?, AGE_PRECIS.1));
    }
    trouvees
}

fn valeur(entete: &[String], cellules: &[String], ou: &str) -> Result<Valeur, String> {
    if cellules.len() != entete.len() {
        return Err(format!(
            "{ou} : {} colonnes au lieu de {}",
            cellules.len(),
            entete.len()
        ));
    }
    let c: BTreeMap<&str, &str> = entete
        .iter()
        .map(String::as_str)
        .zip(cellules.iter().map(String::as_str))
        .collect();
    let champ = |cle: &str| c.get(cle).copied().unwrap_or_default();
    let nom = champ("nom");
    if nom.is_empty() {
        return Err(format!("{ou} : valeur vide"));
    }
    let genre = Some(champ("genre")).filter(|g| !g.is_empty());
    if let Some(g) = genre
        && g != "femme"
        && g != "homme"
    {
        return Err(format!("{ou} : genre « {g} » inconnu (femme, homme ou vide)"));
    }
    let prompts_genre: BTreeMap<String, String> = ["femme", "homme", "androgyne"]
        .iter()
        .filter_map(|g| c.get(g).map(|p| ((*g).to_owned(), (*p).to_owned())))
        .collect();
    let avec = c
        .iter()
        .filter_map(|(cle, texte)| {
            let sous_liste = cle.strip_prefix(AVEC)?;
            let noms = liste_de(texte);
            (!noms.is_empty()).then(|| (sous_liste.to_owned(), noms))
        })
        .collect();
    Ok(Valeur {
        nom: nom.to_owned(),
        prompt: champ("prompt").to_owned(),
        remplace: liste_de(champ("remplace")),
        adulte: oui(champ("adulte"), ou)?,
        genre: genre.map(str::to_owned),
        prompts_genre: (!prompts_genre.is_empty()).then_some(prompts_genre),
        mineur: oui(champ("mineur"), ou)?,
        bornes: bornes(nom),
        familles: familles(champ("famille"), ou)?,
        origines: familles(champ("origines"), ou)?,
        description: champ("description").to_owned(),
        avec,
        categorie: champ("categorie").to_owned(),
    })
}

/// Un côté d'incompatibilité : « Sous-liste : Valeur », « Sous-liste : @Catégorie » (toutes les valeurs dont la
/// catégorie commence par ce texte), ou « Valeur » seule, qui doit alors être unique dans le fichier.
fn resoudre(cote: &str, sous_listes: &[SousListe], ou: &str) -> Result<Vec<Choix>, String> {
    if let Some((nom_sl, valeur)) = cote.split_once(" : ")
        && let Some(sl) = sous_listes.iter().find(|s| s.nom == nom_sl)
    {
        if let Some(prefixe) = valeur.strip_prefix('@') {
            let prefixe = prefixe.trim();
            let trouvees: Vec<Choix> = sl
                .valeurs
                .iter()
                .filter(|v| v.categorie.starts_with(prefixe))
                .map(|v| (sl.nom.clone(), v.nom.clone()))
                .collect();
            if trouvees.is_empty() {
                return Err(format!(
                    "{ou} : aucune valeur de {nom_sl} dans la catégorie « {prefixe} »"
                ));
            }
            return Ok(trouvees);
        }
        if sl.valeur(valeur).is_none() {
            return Err(format!("{ou} : « {valeur} » absent de la sous-liste {nom_sl}"));
        }
        return Ok(vec![(nom_sl.to_owned(), valeur.to_owned())]);
    }
    let trouves: Vec<&SousListe> = sous_listes.iter().filter(|s| s.valeur(cote).is_some()).collect();
    match trouves.as_slice() {
        [] => Err(format!("{ou} : valeur inconnue « {cote} »")),
        [sl] => Ok(vec![(sl.nom.clone(), cote.to_owned())]),
        plusieurs => Err(format!(
            "{ou} : « {cote} » est dans {} ; précise « Sous-liste : {cote} »",
            plusieurs
                .iter()
                .map(|s| s.nom.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Lit un fichier de listes.
///
/// # Errors
/// Une colonne, une valeur, une incompatibilité ou une référence invalide, avec le fichier et la ligne.
pub fn lire(id: &str, texte: &str) -> Result<Bloc, String> {
    let fichier = format!("{id}.md");
    let lignes: Vec<&str> = texte.lines().collect();
    let mut debut = 0;
    if lignes.first().is_some_and(|l| l.trim() == "---") {
        debut = (1..lignes.len())
            .find(|&i| lignes.get(i).is_some_and(|l| l.trim() == "---"))
            .map_or(lignes.len(), |i| i + 1);
    }
    let mut titre = id.to_owned();
    let mut sous_listes: Vec<SousListe> = Vec::new();
    let mut brutes: Vec<(String, String, String)> = Vec::new();
    let mut entete: Option<Vec<String>> = None;
    let mut dans_incompatibilites = false;
    let mut courante = false;
    for (i, ligne) in lignes.iter().enumerate().skip(debut) {
        let ou = format!("{fichier}:{}", i + 1);
        let t = ligne.trim();
        if let Some(reste) = t.strip_prefix("# ") {
            reste.trim().clone_into(&mut titre);
        } else if let Some(nom) = t.strip_prefix("## ") {
            let nom = nom.trim();
            dans_incompatibilites = nom == "Incompatibilités";
            entete = None;
            courante = false;
            if !dans_incompatibilites {
                if sous_listes.iter().any(|s| s.nom == nom) {
                    return Err(format!("{ou} : sous-liste « {nom} » en double"));
                }
                sous_listes.push(SousListe {
                    nom: nom.to_owned(),
                    note: String::new(),
                    valeurs: Vec::new(),
                });
                courante = true;
            }
        } else if dans_incompatibilites && let Some(reste) = t.strip_prefix("- ") {
            let (gauche, droite) = reste
                .split_once('↔')
                .ok_or_else(|| format!("{ou} : incompatibilité sans « ↔ »"))?;
            brutes.push((gauche.trim().to_owned(), droite.trim().to_owned(), ou));
        } else if courante
            && t.starts_with('>')
            && let Some(sl) = sous_listes.last_mut()
        {
            let ajout = t.trim_start_matches(['>', ' ']);
            sl.note = format!("{} {ajout}", sl.note).trim().to_owned();
        } else if courante
            && t.starts_with('|')
            && let Some(sl) = sous_listes.last_mut()
        {
            let c = cellules(t);
            if let Some(e) = &entete {
                if c.iter().all(|x| x.chars().all(|ch| "-: ".contains(ch))) {
                    continue; // la ligne de séparation
                }
                let v = valeur(e, &c, &ou)?;
                if sl.valeur(&v.nom).is_some() {
                    return Err(format!("{ou} : valeur « {} » en double dans {}", v.nom, sl.nom));
                }
                sl.valeurs.push(v);
                continue;
            }
            if let Some(inconnue) = c.iter().find(|x| colonne(x).is_none() && !x.starts_with(AVEC)) {
                return Err(format!("{ou} : colonne inconnue « {inconnue} »"));
            }
            entete = Some(
                c.iter()
                    .map(|x| colonne(x).map_or_else(|| x.clone(), str::to_owned))
                    .collect(),
            );
        }
    }
    for sl in &sous_listes {
        if sl.valeurs.is_empty() {
            return Err(format!("{fichier} : sous-liste « {} » sans valeur", sl.nom));
        }
        for v in &sl.valeurs {
            if let Some(cible) = v
                .remplace
                .iter()
                .find(|c| !sous_listes.iter().any(|s| &s.nom == *c))
            {
                return Err(format!(
                    "{fichier} : « {} » remplace « {cible} », sous-liste inconnue",
                    v.nom
                ));
            }
            for (reference, noms) in &v.avec {
                let Some(autre) = sous_listes.iter().find(|s| &s.nom == reference) else {
                    return Err(format!(
                        "{fichier} : colonne « {AVEC}{reference} », sous-liste inconnue"
                    ));
                };
                if let Some(inconnu) = noms.iter().find(|n| autre.valeur(n).is_none()) {
                    return Err(format!(
                        "{fichier} : « {} » ({}) cite « {inconnu} », absent de {reference}",
                        v.nom, sl.nom
                    ));
                }
            }
        }
    }
    let mut incompatibilites = Vec::new();
    for (gauche, droite, ou) in brutes {
        for g in resoudre(&gauche, &sous_listes, &ou)? {
            for d in resoudre(&droite, &sous_listes, &ou)? {
                incompatibilites.push((g.clone(), d));
            }
        }
    }
    let titre = titre
        .find(" (")
        .filter(|_| titre.ends_with(')'))
        .map_or(titre.clone(), |i| titre[..i].trim_end().to_owned());
    Ok(Bloc {
        id: id.to_owned(),
        titre,
        sous_listes,
        incompatibilites,
    })
}

static BLOCS: OnceLock<Result<BTreeMap<String, Bloc>, String>> = OnceLock::new();

/// Les blocs livrés, lus une fois.
///
/// # Errors
/// Un fichier livré est invalide (les tests le vérifient).
pub fn blocs() -> Result<&'static BTreeMap<String, Bloc>, String> {
    BLOCS
        .get_or_init(|| {
            FICHIERS
                .iter()
                .map(|(id, texte)| lire(id, texte).map(|b| ((*id).to_owned(), b)))
                .collect()
        })
        .as_ref()
        .map_err(Clone::clone)
}

// --- structure pour l'interface -----------------------------------------------------------------------------------

fn def_sujet(sujet: &str) -> Result<&'static DefSujet, String> {
    SUJETS
        .iter()
        .find(|s| s.id == sujet)
        .ok_or_else(|| format!("Sujet inconnu : {sujet}"))
}

fn bloc(id: &str) -> Result<&'static Bloc, String> {
    blocs()?
        .get(id)
        .ok_or_else(|| format!("Liste absente de prompt/listes : {id}.md"))
}

/// Les sous-listes d'un bloc pour un sujet : pour un sujet Lieu, le lieu est le sujet lui-même.
fn sous_listes(sujet: &str, id_bloc: &str) -> Result<Vec<&'static SousListe>, String> {
    let exclue = (sujet == "lieu" && id_bloc == "mise-en-scene").then_some("Lieu");
    Ok(bloc(id_bloc)?
        .sous_listes
        .iter()
        .filter(|s| Some(s.nom.as_str()) != exclue)
        .collect())
}

fn onglet(id_bloc: &str, groupe: &str) -> &'static str {
    ONGLETS
        .iter()
        .find(|((b, g), _)| *b == id_bloc && *g == groupe)
        .map(|(_, o)| *o)
        .or_else(|| ONGLETS_BLOCS.iter().find(|(b, _)| *b == id_bloc).map(|(_, o)| *o))
        .unwrap_or("sujet")
}

/// Les accordéons d'un bloc : (groupe, sous-listes), dans l'ordre de l'interface.
fn groupes(sujet: &str, id_bloc: &str) -> Result<Vec<(String, Vec<&'static SousListe>)>, String> {
    let presentes = sous_listes(sujet, id_bloc)?;
    let definis = GROUPES
        .iter()
        .find(|(b, _)| *b == id_bloc)
        .map(|(_, g)| *g)
        .unwrap_or_default();
    let mut sortie: Vec<(String, Vec<&SousListe>)> = definis
        .iter()
        .map(|(nom, noms)| {
            (
                (*nom).to_owned(),
                noms.iter()
                    .filter_map(|n| presentes.iter().find(|s| s.nom == *n).copied())
                    .collect(),
            )
        })
        .collect();
    let reste: Vec<&SousListe> = presentes
        .iter()
        .filter(|s| !definis.iter().any(|(_, noms)| noms.contains(&s.nom.as_str())))
        .copied()
        .collect();
    if !reste.is_empty() {
        let nom = if definis.is_empty() {
            bloc(id_bloc)?.titre.clone()
        } else {
            "Autres".to_owned()
        };
        sortie.push((nom, reste));
    }
    Ok(sortie.into_iter().filter(|(_, s)| !s.is_empty()).collect())
}

/// Un sujet proposé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sujet {
    pub id: String,
    pub nom: String,
}

/// Les sujets du Scénario.
pub fn sujets() -> Vec<Sujet> {
    SUJETS
        .iter()
        .map(|s| Sujet {
            id: s.id.to_owned(),
            nom: s.nom.to_owned(),
        })
        .collect()
}

/// Une condition d'affichage d'une sous-liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub sous_liste: String,
    pub contient: String,
}

/// Une sous-liste pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SousListePublique {
    pub nom: String,
    pub note: String,
    /// Toutes ses valeurs sont adultes : elle disparaît pour un mineur.
    pub adulte: bool,
    pub condition: Option<Condition>,
    pub valeurs: Vec<Valeur>,
}

/// Un accordéon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Groupe {
    pub nom: String,
    pub onglet: String,
    pub sous_listes: Vec<SousListePublique>,
}

/// Un bloc pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlocPublic {
    pub id: String,
    pub titre: String,
    /// Le bloc Plan : affiché seulement pour un modèle vidéo.
    pub video: bool,
    pub groupes: Vec<Groupe>,
}

/// Les longueurs maximales des champs libres.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Longueurs {
    pub action: usize,
    pub replique: usize,
}

/// Ce que l'interface montre pour un sujet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Structure {
    pub sujet: String,
    pub nom: String,
    pub blocs: Vec<BlocPublic>,
    /// [[bloc, sous-liste, valeur], [bloc, sous-liste, valeur]].
    pub incompatibilites: Vec<[[String; 3]; 2]>,
    pub age_precis: (u32, u32),
    pub longueurs: Longueurs,
}

/// Blocs, accordéons, valeurs et incompatibilités d'un sujet. Le bloc Plan (vidéo) est toujours livré, marqué
/// `video` : l'interface ne l'affiche que pour un modèle vidéo.
///
/// # Errors
/// Sujet inconnu, ou liste absente.
pub fn structure(sujet: &str) -> Result<Structure, String> {
    let def = def_sujet(sujet)?;
    let mut sortie = Vec::new();
    let mut incompatibilites = Vec::new();
    for id_bloc in def.blocs.iter().copied().chain(std::iter::once(PLAN)) {
        let b = bloc(id_bloc)?;
        let presentes = sous_listes(sujet, id_bloc)?;
        let groupes = groupes(sujet, id_bloc)?
            .into_iter()
            .map(|(nom, liste)| Groupe {
                onglet: onglet(id_bloc, &nom).to_owned(),
                sous_listes: liste
                    .into_iter()
                    .map(|sl| SousListePublique {
                        nom: sl.nom.clone(),
                        note: sl.note.clone(),
                        adulte: sl.valeurs.iter().all(|v| v.adulte),
                        condition: CONDITIONS
                            .iter()
                            .find(|((b, n), _)| *b == id_bloc && *n == sl.nom)
                            .map(|(_, (sous_liste, contient))| Condition {
                                sous_liste: (*sous_liste).to_owned(),
                                contient: (*contient).to_owned(),
                            }),
                        valeurs: sl.valeurs.clone(),
                    })
                    .collect(),
                nom,
            })
            .collect();
        sortie.push(BlocPublic {
            id: id_bloc.to_owned(),
            titre: b.titre.clone(),
            video: id_bloc == PLAN,
            groupes,
        });
        let present = |n: &str| presentes.iter().any(|s| s.nom == n);
        incompatibilites.extend(
            b.incompatibilites
                .iter()
                .filter(|(g, d)| present(&g.0) && present(&d.0))
                .map(|(g, d)| {
                    [
                        [id_bloc.to_owned(), g.0.clone(), g.1.clone()],
                        [id_bloc.to_owned(), d.0.clone(), d.1.clone()],
                    ]
                }),
        );
    }
    Ok(Structure {
        sujet: sujet.to_owned(),
        nom: def.nom.to_owned(),
        blocs: sortie,
        incompatibilites,
        age_precis: AGE_PRECIS,
        longueurs: Longueurs {
            action: LONGUEUR_ACTION,
            replique: LONGUEUR_REPLIQUE,
        },
    })
}

// --- assemblage ---------------------------------------------------------------------------------------------------

/// Ce que le Scénario demande d'assembler.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeAssemblage {
    pub sujet: String,
    /// {bloc: {sous-liste: valeur}}.
    #[serde(default)]
    pub choix: Map<String, Value>,
    #[serde(default)]
    pub age: Option<u32>,
    /// Le texte à écrire dans l'image.
    #[serde(default)]
    pub texte: String,
    /// Déjà traduites en anglais.
    #[serde(default)]
    pub precisions: String,
    #[serde(default)]
    pub workflow: Option<String>,
    /// Vidéo : l'action, déjà traduite.
    #[serde(default)]
    pub action: String,
    /// Vidéo : les répliques, une par ligne, gardées mot pour mot.
    #[serde(default)]
    pub replique: String,
    /// Vidéo : faux, personne ne parle.
    #[serde(default)]
    pub dialogue: Option<bool>,
    /// Vidéo : la musique, déjà traduite.
    #[serde(default)]
    pub musique: String,
    /// Vidéo : faux, pas de musique de fond ; vrai, `musique` ou la liste.
    #[serde(default)]
    pub avec_musique: Option<bool>,
    /// Le sujet vient des images (Mélanger, Redessiner) : les choix de l'onglet Sujet sont écartés.
    #[serde(default)]
    pub sans_sujet: bool,
}

/// Le prompt assemblé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assemblage {
    pub prompt: String,
    pub mots: usize,
    pub mots_max: Option<u64>,
    pub alertes: Vec<String>,
    pub mineur: bool,
    pub tranche: Option<String>,
    /// Des précisions ou une action à traduire avant d'assembler.
    pub a_traduire: bool,
    pub video: bool,
}

fn article(texte: &str) -> String {
    let voyelle = texte.chars().next().is_some_and(|c| "aeiouAEIOU".contains(c));
    format!("{} {texte}", if voyelle { "an" } else { "a" })
}

fn majuscule(texte: &str) -> String {
    let mut c = texte.chars();
    c.next()
        .map_or_else(String::new, |p| p.to_uppercase().chain(c).collect())
}

fn phrase(morceau: &str) -> String {
    let fin = if morceau.ends_with(['.', '!', '?']) {
        ""
    } else {
        "."
    };
    format!("{}{fin}", majuscule(morceau))
}

/// La balise d'une réplique pour un modèle vidéo (guide de H3) : `<d>[Langue] réplique</d>`, sans chevrons dedans.
pub fn balise(replique: &str, langue: &str) -> String {
    let texte: String = replique.chars().filter(|c| *c != '<' && *c != '>').collect();
    let texte = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("<d>[{langue}] {texte}</d>")
}

/// Un modèle texte → vidéo ?
pub fn est_video(workflow: Option<&str>) -> bool {
    workflow
        .and_then(|w| workflows::obtenir(w).ok())
        .is_some_and(|w| w.tache() == "texte_vers_video")
}

/// Une valeur choisie, rangée dans l'ordre des fichiers.
struct Choisie {
    bloc: &'static str,
    sous_liste: &'static str,
    valeur: &'static Valeur,
}

/// Les choix contrôlés, dans l'ordre des blocs du sujet puis des fichiers.
fn selection(sujet: &DefSujet, choix: &Map<String, Value>, video: bool) -> Result<Vec<Choisie>, String> {
    let mut autorises: Vec<&'static str> = sujet.blocs.to_vec();
    if video {
        autorises.push(PLAN);
    }
    for id_bloc in choix.keys() {
        if !autorises.contains(&id_bloc.as_str()) {
            return Err(format!(
                "La liste « {id_bloc} » n'est pas proposée pour le sujet {}",
                sujet.nom
            ));
        }
    }
    let mut sortie = Vec::new();
    for id_bloc in autorises {
        let Some(valeurs) = choix.get(id_bloc).and_then(Value::as_object) else {
            continue;
        };
        let presentes = sous_listes(sujet.id, id_bloc)?;
        for (nom_sl, nom_valeur) in valeurs {
            if nom_valeur.as_str().is_none_or(str::is_empty) {
                continue;
            }
            if !presentes.iter().any(|s| &s.nom == nom_sl) {
                return Err(format!("Sous-liste inconnue : {nom_sl} ({id_bloc})"));
            }
        }
        for sl in presentes {
            let Some(nom_valeur) = valeurs
                .get(&sl.nom)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
            else {
                continue;
            };
            let valeur = sl
                .valeur(nom_valeur)
                .ok_or_else(|| format!("Valeur inconnue : « {nom_valeur} » ({})", sl.nom))?;
            sortie.push(Choisie {
                bloc: id_bloc,
                sous_liste: sl.nom.as_str(),
                valeur,
            });
        }
    }
    Ok(sortie)
}

/// La valeur choisie d'une sous-liste (elle vient des listes livrées : rien ne la lie à la sélection).
fn trouver(selection: &[Choisie], bloc: &str, sous_liste: &str) -> Option<&'static Valeur> {
    selection
        .iter()
        .find(|c| c.bloc == bloc && c.sous_liste == sous_liste)
        .map(|c| c.valeur)
}

fn tranche_d_age(age: u32) -> Result<&'static Valeur, String> {
    bloc("personnage")?
        .sous_liste("Âge")
        .into_iter()
        .flat_map(|s| s.valeurs.iter())
        .find(|t| t.bornes.is_some_and(|(a, b)| (a..=b).contains(&age)))
        .ok_or_else(|| format!("Aucune tranche d'âge ne couvre {age} ans"))
}

fn qui_parle(sujet: &str, genre: Option<&str>) -> &'static str {
    match (sujet, genre) {
        ("personnage", Some("femme")) => "She says",
        ("personnage", Some("homme")) => "He says",
        ("personnage", _) => "They say",
        ("creature", _) => "The creature says",
        _ => "A voice-over says",
    }
}

/// L'onglet du Scénario d'une sous-liste (sujet, decor ou scene), rangée comme [`structure`] la range.
fn onglet_de(sujet: &str, id_bloc: &str, nom_sl: &str) -> String {
    groupes(sujet, id_bloc)
        .unwrap_or_default()
        .iter()
        .find(|(_, liste)| liste.iter().any(|s| s.nom == nom_sl))
        .map_or_else(|| "sujet".to_owned(), |(nom, _)| onglet(id_bloc, nom).to_owned())
}

/// `quoi` : « Action trop longue », « Précisions trop longues »…
fn trop_long(texte: &str, max: usize, quoi: &str) -> Result<(), String> {
    if texte.chars().count() > max {
        Err(format!("{quoi} (plus de {max} caractères)"))
    } else {
        Ok(())
    }
}

/// Le prompt anglais assemblé à partir des choix, après contrôle des règles et du filtre AS-18. Pour un modèle vidéo,
/// le prompt devient un brief de production : rendu, sujet, action, caméra, réplique, sons.
///
/// # Errors
/// Une règle des listes, une longueur, ou le filtre AS-18 (`ERREUR AS-18 · …`).
#[allow(clippy::too_many_lines)] // l'assemblage de la v2, gardé d'un tenant pour se relire à côté d'elle
pub fn assembler(dossier: &Dossier, d: &DemandeAssemblage) -> Result<Assemblage, String> {
    let def = def_sujet(&d.sujet)?;
    let video = est_video(d.workflow.as_deref());
    let mut choix = d.choix.clone();
    let (mut action, mut replique) = (d.action.trim().to_owned(), d.replique.trim().to_owned());
    if !video {
        choix.remove(PLAN); // le Plan reste en mémoire dans l'interface, mais ne compte que pour la vidéo
        action.clear();
        replique.clear();
    }
    let mut selection = selection(def, &choix, video)?;
    let sans_sujet = d.sans_sujet && !video;
    let mut age = d.age;
    if sans_sujet {
        age = None; // l'âge précis est un choix de sujet
        selection.retain(|c| !(c.bloc == def.id && onglet_de(def.id, c.bloc, c.sous_liste) == "sujet"));
    }
    let mut texte = d.texte.trim().to_owned();
    let precisions = d.precisions.trim().to_owned();
    let musique = d.musique.split_whitespace().collect::<Vec<_>>().join(" ");
    if musique.chars().count() > LONGUEUR_MUSIQUE {
        return Err(format!(
            "Musique trop longue (plus de {LONGUEUR_MUSIQUE} caractères)"
        ));
    }
    let mut alertes = Vec::new();
    if video && !texte.is_empty() {
        alertes.push("Texte dans l'image ignoré : il ne concerne que les images".to_owned());
        texte.clear();
    }
    if texte.chars().count() > LONGUEUR_TEXTE {
        return Err(format!(
            "Texte dans l'image trop long (plus de {LONGUEUR_TEXTE} caractères)"
        ));
    }
    trop_long(&precisions, LONGUEUR_PRECISIONS, "Précisions trop longues")?;
    trop_long(&action, LONGUEUR_ACTION, "Action trop longue")?;
    trop_long(&replique, LONGUEUR_REPLIQUE, "Réplique trop longue")?;

    // Genre et âge.
    let genre_v = trouver(&selection, "personnage", "Genre");
    let genre = genre_v.map(|g| g.nom.to_lowercase());
    let mut tranche = trouver(&selection, "personnage", "Âge");
    if let Some(age) = age {
        if def.id != "personnage" {
            return Err("L'âge précis ne concerne que les personnages".to_owned());
        }
        if !(AGE_PRECIS.0..=AGE_PRECIS.1).contains(&age) {
            return Err(format!(
                "Âge précis entre {} et {} ans",
                AGE_PRECIS.0, AGE_PRECIS.1
            ));
        }
        let juste = tranche_d_age(age)?;
        if tranche.is_some_and(|t| t != juste) {
            alertes.push(format!(
                "Tranche d'âge ajustée à « {} » pour {age} ans",
                juste.nom
            ));
        }
        tranche = Some(juste);
        selection.retain(|c| !(c.bloc == "personnage" && c.sous_liste == "Âge"));
        let age_sl = bloc("personnage")?
            .sous_liste("Âge")
            .ok_or_else(|| "Sous-liste Âge absente".to_owned())?;
        selection.push(Choisie {
            bloc: "personnage",
            sous_liste: age_sl.nom.as_str(),
            valeur: juste,
        });
    }
    let mineur =
        tranche.is_some_and(|t| t.mineur) || age.is_some_and(|a| u64::from(a) < securite::AGE_MAJORITE);

    // Règles.
    for c in &selection {
        let v = c.valeur;
        if let (Some(g), Some(genre)) = (&v.genre, &genre)
            && (genre == "femme" || genre == "homme")
            && g != genre
        {
            let nom_genre = genre_v.map_or(genre.as_str(), |g| g.nom.as_str());
            return Err(format!(
                "« {} » ({}) n'est pas proposé pour le genre {nom_genre}",
                v.nom, c.sous_liste
            ));
        }
        for (reference, noms) in &v.avec {
            if let Some(r) = trouver(&selection, c.bloc, reference)
                && !noms.contains(&r.nom)
            {
                return Err(format!(
                    "« {} » ({}) ne va pas avec « {} » ({reference})",
                    v.nom, c.sous_liste, r.nom
                ));
            }
        }
        if mineur && v.adulte {
            return Err(ErreurSecurite(format!(
                "« {} » ({}) est réservé aux personnages adultes",
                v.nom, c.sous_liste
            ))
            .to_string());
        }
    }
    let blocs_vus: Vec<&str> = def.blocs.iter().copied().chain(video.then_some(PLAN)).collect();
    for id_bloc in blocs_vus {
        for ((sl_g, val_g), (sl_d, val_d)) in &bloc(id_bloc)?.incompatibilites {
            let g = trouver(&selection, id_bloc, sl_g);
            let dr = trouver(&selection, id_bloc, sl_d);
            if g.is_some_and(|g| &g.nom == val_g) && dr.is_some_and(|dr| &dr.nom == val_d) {
                return Err(format!(
                    "« {val_g} » ({sl_g}) et « {val_d} » ({sl_d}) sont incompatibles"
                ));
            }
        }
    }
    for ((id_bloc, nom_sl), (requise, contient)) in CONDITIONS {
        if trouver(&selection, id_bloc, nom_sl).is_some() {
            let ok = trouver(&selection, id_bloc, requise)
                .is_some_and(|r| r.nom.to_lowercase().contains(contient));
            if !ok {
                selection.retain(|c| !(c.bloc == id_bloc && c.sous_liste == nom_sl));
                alertes.push(format!(
                    "{nom_sl} ignoré : il faut d'abord un {} qui convient",
                    requise.to_lowercase()
                ));
            }
        }
    }
    let mut i = 0;
    while i < selection.len() {
        let (bloc_c, nom_v, remplace) = {
            let c = &selection[i];
            (c.bloc, c.valeur.nom.clone(), c.valeur.remplace.clone())
        };
        for cible in remplace {
            if let Some(pos) = selection
                .iter()
                .position(|c| c.bloc == bloc_c && c.sous_liste == cible)
            {
                let remplacee = selection.remove(pos);
                alertes.push(format!(
                    "{cible} (« {} ») remplacé par « {nom_v} »",
                    remplacee.valeur.nom
                ));
                if pos < i {
                    i -= 1;
                }
            }
        }
        i += 1;
    }

    // Le sujet.
    let (nom, ignorees): (Option<String>, Vec<&str>) = if sans_sujet {
        (None, Vec::new())
    } else if def.id == "personnage" {
        let mut nom = match tranche {
            Some(t) => t
                .prompts_genre
                .as_ref()
                .and_then(|p| {
                    p.get(genre.as_deref().unwrap_or("androgyne"))
                        .or_else(|| p.get("androgyne"))
                })
                .cloned()
                .unwrap_or_default(),
            None => genre_v.map_or_else(|| def.defaut.to_owned(), |g| g.prompt.clone()),
        };
        if let Some(a) = age {
            nom.push_str(&format!(", {a} years old"));
        }
        (Some(nom), vec!["Genre", "Âge", "Motif de tatouage"])
    } else {
        let premiere: &'static str = sous_listes(def.id, def.id)?
            .first()
            .copied()
            .map(|s| s.nom.as_str())
            .unwrap_or_default();
        let principal = trouver(&selection, def.id, premiere).filter(|p| !p.prompt.is_empty());
        (
            Some(principal.map_or_else(|| def.defaut.to_owned(), |p| p.prompt.clone())),
            vec![premiere],
        )
    };

    // Les détails, dans l'ordre des fichiers.
    let (mut details, mut rendu, mut vetements) = (Vec::new(), Vec::new(), Vec::new());
    let mut position_vetements = None;
    for id_bloc in def.blocs {
        for sl in sous_listes(def.id, id_bloc)? {
            let Some(v) = trouver(&selection, id_bloc, &sl.nom) else {
                continue;
            };
            if v.prompt.is_empty()
                || (*id_bloc == def.id && ignorees.contains(&sl.nom.as_str()))
                || (*id_bloc, sl.nom.as_str()) == STYLE
            {
                continue;
            }
            let mut morceau = v.prompt.clone();
            if *id_bloc == "personnage" && VETEMENTS.contains(&sl.nom.as_str()) {
                position_vetements.get_or_insert(details.len());
                vetements.push(morceau);
                continue;
            }
            if *id_bloc == "personnage" && sl.nom == "Signe particulier" {
                if let Some(motif) = trouver(&selection, "personnage", "Motif de tatouage") {
                    morceau.push_str(&format!(" featuring {}", article(&motif.prompt)));
                }
            } else if *id_bloc == "personnage" && sl.nom == "Accessoires" {
                morceau = format!("with {morceau}");
            }
            if *id_bloc == "commun" {
                rendu.push(morceau);
            } else {
                details.push(morceau);
            }
        }
    }
    if let Some(pos) = position_vetements {
        details.insert(pos, format!("wearing {}", vetements.join(", ")));
    }

    let mut phrases = Vec::new();
    if let Some(style) = trouver(&selection, STYLE.0, STYLE.1) {
        phrases.push(format!("{}.", majuscule(&style.prompt)));
    }
    let sujet_phrase = match &nom {
        None if details.is_empty() => String::new(),
        None => phrase(&details.join(", ")),
        Some(nom) => {
            let mut morceaux = vec![article(nom)];
            morceaux.extend(details.iter().cloned());
            format!("{}.", majuscule(&morceaux.join(", ")))
        }
    };
    if video {
        // Brief de production (bonnes pratiques de H3) : le cadrage d'abord, puis le sujet, l'action, la caméra, le son.
        if !rendu.is_empty() {
            phrases.push(format!("{}.", majuscule(&rendu.join(", "))));
        }
        phrases.push(sujet_phrase);
        if !action.is_empty() {
            phrases.push(phrase(&action));
        }
        if !precisions.is_empty() {
            phrases.push(phrase(&precisions));
        }
        let plan: BTreeMap<&str, &str> = selection
            .iter()
            .filter(|c| c.bloc == PLAN && !c.valeur.prompt.is_empty())
            .map(|c| (c.sous_liste, c.valeur.prompt.as_str()))
            .collect();
        if let Some(camera) = plan.get("Caméra") {
            phrases.push(phrase(camera));
        }
        let lignes: Vec<String> = if d.dialogue == Some(false) {
            Vec::new()
        } else {
            replique
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(|l| l.replace('"', "'"))
                .collect()
        };
        if let Some((premiere, suite)) = lignes.split_first() {
            let ton = plan.get("Ton").map(|t| format!(", {t}")).unwrap_or_default();
            let parle_en = plan.get("Langue").copied().unwrap_or("French");
            let suite: String = suite
                .iter()
                .map(|l| format!(" Then: {}", balise(l, parle_en)))
                .collect();
            phrases.push(format!(
                "{} in {parle_en}{ton}: {}{suite}",
                qui_parle(def.id, genre.as_deref()),
                balise(premiere, parle_en)
            ));
        } else if d.dialogue == Some(false) {
            phrases.push("Nobody speaks.".to_owned());
        } else if plan.contains_key("Langue") || plan.contains_key("Ton") {
            alertes.push("Langue et ton ignorés : écris d'abord une réplique".to_owned());
        }
        if let Some(sons) = plan.get("Ambiance sonore") {
            phrases.push(format!("Sounds: {sons}."));
        }
        let liste = plan.get("Musique").copied();
        let sans_musique_liste = liste.is_some_and(|l| l.starts_with("no "));
        if d.avec_musique == Some(false)
            || (d.avec_musique == Some(true) && musique.is_empty() && sans_musique_liste)
        {
            phrases.push("No background music.".to_owned());
        } else if d.avec_musique == Some(true) && !musique.is_empty() {
            phrases.push(format!("Background music: {}.", musique.trim_end_matches('.')));
        } else if let Some(l) = liste {
            phrases.push(if sans_musique_liste {
                phrase(l)
            } else {
                format!("Music: {l}.")
            });
        } else if d.avec_musique == Some(true) {
            phrases.push("Background music that fits the scene.".to_owned());
        }
    } else {
        if !sujet_phrase.is_empty() {
            phrases.push(sujet_phrase);
        }
        if !rendu.is_empty() {
            phrases.push(format!("{}.", majuscule(&rendu.join(", "))));
        }
        if !texte.is_empty() {
            phrases.push(format!(
                "The text \"{}\" appears in the image.",
                texte.replace('"', "'")
            ));
        }
        if !precisions.is_empty() {
            phrases.push(phrase(&precisions));
        }
    }
    let prompt = phrases.join(" ");

    // Le mineur est signalé explicitement : AS-18 ne dépend pas de la façon dont la tranche est formulée.
    let mut textes = vec![prompt.as_str()];
    if mineur {
        textes.push("minor");
    }
    securite::verifier(dossier, &textes).map_err(|e| e.to_string())?;

    let mots = prompt.split_whitespace().count();
    let mots_max = d.workflow.as_deref().and_then(|w| {
        let wf = workflows::obtenir(w).ok()?;
        let reglage = reglages::section(dossier, "workflows")
            .and_then(|s| s.get(w).and_then(Value::as_object).cloned());
        prompt::mots_max_de(Some(&wf), reglage.as_ref())
    });
    if let Some(max) = mots_max
        && mots as u64 > max
    {
        alertes.push(format!(
            "{mots} mots : au-delà de {max}, ce modèle risque d'ignorer la fin du prompt"
        ));
    }
    Ok(Assemblage {
        prompt,
        mots,
        mots_max,
        alertes,
        mineur,
        tranche: tranche.map(|t| t.nom.clone()),
        a_traduire: !precisions.is_empty() || !action.is_empty(),
        video,
    })
}

/// Le personnage et le décor d'une séquence, séparés, pour le découpage en plans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescriptionSequence {
    pub personnage: String,
    pub decor: String,
    pub mineur: bool,
}

/// {personnage, décor, mineur} à partir des choix du Scénario, après contrôle de toutes les règles (AS-18 compris).
///
/// # Errors
/// Celles d'[`assembler`].
pub fn description_sequence(
    dossier: &Dossier,
    sujet: &str,
    choix: &Map<String, Value>,
    age: Option<u32>,
) -> Result<DescriptionSequence, String> {
    let mut choix = choix.clone();
    choix.remove(PLAN);
    let complet = assembler(
        dossier,
        &DemandeAssemblage {
            sujet: sujet.to_owned(),
            choix: choix.clone(),
            age,
            ..DemandeAssemblage::default()
        },
    )?;
    let propre: Map<String, Value> = choix
        .get(sujet)
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(sl, _)| !ATTITUDE.contains(&sl.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    let personnage = if propre.is_empty() && age.is_none() {
        String::new()
    } else {
        let mut seul = Map::new();
        if !propre.is_empty() {
            seul.insert(sujet.to_owned(), Value::Object(propre));
        }
        assembler(
            dossier,
            &DemandeAssemblage {
                sujet: sujet.to_owned(),
                choix: seul,
                age,
                ..DemandeAssemblage::default()
            },
        )?
        .prompt
    };
    let def = def_sujet(sujet)?;
    let autres: Map<String, Value> = choix.into_iter().filter(|(b, _)| b != sujet).collect();
    let selection = selection(def, &autres, false)?;
    let style = trouver(&selection, STYLE.0, STYLE.1).filter(|s| !s.prompt.is_empty());
    let morceaux: Vec<&str> = selection
        .iter()
        .filter(|c| {
            (c.bloc, c.sous_liste) != STYLE && !c.valeur.prompt.is_empty() && !CAMERA.contains(&c.sous_liste)
        })
        .map(|c| c.valeur.prompt.as_str())
        .collect();
    let decor = [
        style
            .map(|s| format!("{}.", majuscule(&s.prompt)))
            .unwrap_or_default(),
        if morceaux.is_empty() {
            String::new()
        } else {
            format!("{}.", majuscule(&morceaux.join(", ")))
        },
    ]
    .into_iter()
    .filter(|x| !x.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    Ok(DescriptionSequence {
        personnage,
        decor,
        mineur: complet.mineur,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dossier() -> (tempfile::TempDir, Dossier) {
        let temp = tempfile::tempdir().unwrap();
        let d = Dossier::a(temp.path());
        (temp, d)
    }

    fn demande(sujet: &str, choix: Value) -> DemandeAssemblage {
        DemandeAssemblage {
            sujet: sujet.to_owned(),
            choix: choix.as_object().cloned().unwrap_or_default(),
            ..DemandeAssemblage::default()
        }
    }

    #[test]
    fn listes_livrees_lisibles() {
        let b = blocs().unwrap();
        assert_eq!(b.len(), FICHIERS.len());
        let perso = &b["personnage"];
        let age = perso.sous_liste("Âge").unwrap();
        let ado = age.valeurs.iter().find(|v| v.mineur).unwrap();
        assert!(ado.bornes.is_some() && ado.prompts_genre.as_ref().unwrap().contains_key("femme"));
        for s in sujets() {
            let st = structure(&s.id).unwrap();
            assert!(st.blocs.iter().any(|b| b.video));
            assert!(st.blocs.iter().all(|b| !b.groupes.is_empty()));
        }
        assert!(structure("inconnu").is_err());
        // Pour un sujet Lieu, le lieu de la mise en scène est le sujet lui-même.
        let lieu = structure("lieu").unwrap();
        let mes = lieu.blocs.iter().find(|b| b.id == "mise-en-scene").unwrap();
        assert!(
            mes.groupes
                .iter()
                .flat_map(|g| &g.sous_listes)
                .all(|s| s.nom != "Lieu")
        );
    }

    #[test]
    fn lecture_d_un_fichier() {
        let texte = "---\ntitre: x\n---\n# Essai (v2)\n\n## Genre\n\n| Valeur | Prompt image |\n| --- | --- |\n| Femme | woman |\n| Homme | man |\n\n## Bas\n\n> Une note\n\n| Valeur | Prompt image | Remplace |\n| --- | --- | --- |\n| Robe | a dress | Genre |\n\n## Incompatibilités\n\n- Femme ↔ Bas : Robe\n";
        let b = lire("essai", texte).unwrap();
        assert_eq!(b.titre, "Essai");
        assert_eq!(b.sous_listes.len(), 2);
        assert_eq!(b.sous_liste("Bas").unwrap().note, "Une note");
        assert_eq!(
            b.incompatibilites,
            vec![(("Genre".into(), "Femme".into()), ("Bas".into(), "Robe".into()))]
        );
        assert!(
            lire("x", "## A\n\n| Valeur | Inconnue |\n| - | - |\n| a | b |\n")
                .unwrap_err()
                .contains("colonne inconnue")
        );
        assert!(lire("x", "## A\n\n| Valeur | Adulte |\n| - | - |\n| a | peut-être |\n").is_err());
        assert!(lire("x", "## A\n\n| Valeur |\n| - |\n\n## Incompatibilités\n- a ↔ b\n").is_err());
        assert_eq!(bornes("Adulte (18-25 ans)"), Some((18, 25)));
        assert_eq!(bornes("Senior (65 ans et plus)"), Some((65, 120)));
    }

    #[test]
    fn assemblage_d_un_personnage() {
        let (_t, d) = dossier();
        let r = assembler(
            &d,
            &demande("personnage", json!({"personnage": {"Genre": "Femme"}})),
        )
        .unwrap();
        assert!(r.prompt.starts_with("A woman"), "{}", r.prompt);
        assert!(!r.mineur && !r.video);
        let mut a = demande("personnage", json!({"personnage": {"Genre": "Femme"}}));
        a.age = Some(30);
        a.texte = "OPEN".into();
        a.precisions = "under the rain".into();
        let r = assembler(&d, &a).unwrap();
        assert!(r.prompt.contains("30 years old") && r.prompt.contains("The text \"OPEN\" appears"));
        assert!(r.a_traduire);
        assert!(r.tranche.is_some());
        a.age = Some(130);
        assert!(assembler(&d, &a).is_err());
        a.age = Some(12);
        a.texte.clear();
        a.precisions.clear();
        let r = assembler(&d, &a).unwrap();
        assert!(r.mineur);
        // Une valeur adulte refusée pour un mineur.
        let adulte = bloc("personnage")
            .unwrap()
            .sous_listes
            .iter()
            .find_map(|s| {
                s.valeurs
                    .iter()
                    .find(|v| v.adulte)
                    .map(|v| (s.nom.clone(), v.nom.clone()))
            })
            .unwrap();
        let mut valeurs = Map::new();
        valeurs.insert(adulte.0, Value::String(adulte.1));
        let mut choix = Map::new();
        choix.insert("personnage".to_owned(), Value::Object(valeurs));
        let mut m = DemandeAssemblage {
            sujet: "personnage".to_owned(),
            choix,
            ..DemandeAssemblage::default()
        };
        m.age = Some(12);
        assert!(assembler(&d, &m).unwrap_err().starts_with("ERREUR AS-18"));
        assert!(assembler(&d, &demande("objet", json!({"personnage": {"Genre": "Femme"}}))).is_err());
        assert!(
            assembler(
                &d,
                &demande("personnage", json!({"personnage": {"Genre": "Robot"}}))
            )
            .is_err()
        );
    }

    #[test]
    fn assemblage_d_un_lieu_et_sans_sujet() {
        let (_t, d) = dossier();
        let r = assembler(&d, &demande("lieu", json!({}))).unwrap();
        assert_eq!(r.prompt, "A landscape.");
        let mut s = demande("personnage", json!({"personnage": {"Genre": "Femme"}}));
        s.sans_sujet = true;
        assert_eq!(assembler(&d, &s).unwrap().prompt, "");
        assert_eq!(balise("Salut <toi>", "French"), "<d>[French] Salut toi</d>");
        assert_eq!(article("owl"), "an owl");
        assert_eq!(phrase("it rains"), "It rains.");
    }

    #[test]
    fn description_d_une_sequence() {
        let (_t, d) = dossier();
        let choix = json!({"personnage": {"Genre": "Homme"}})
            .as_object()
            .cloned()
            .unwrap();
        let s = description_sequence(&d, "personnage", &choix, Some(40)).unwrap();
        assert!(s.personnage.contains("40 years old"));
        assert!(!s.mineur);
    }
}
