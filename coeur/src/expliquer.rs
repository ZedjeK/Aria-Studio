//! « Expliquer une erreur » (nouveauté de la v3) : quand quelque chose échoue (une commande du Plateau, de la Retouche
//! ou du Montage, un moteur, un téléchargement, une installation, une ligne du Journal), Aria dit la cause probable et
//! les remèdes concrets dans l'appli.
//!
//! D'abord une **base de causes connues**, tenue ici à partir des messages que l'appli produit déjà : si l'une
//! correspond, elle est rendue telle quelle, sans LLM, aussitôt. Le LLM (un généraliste) ne sert que pour le reste, ou
//! pour approfondir une cause large (un moteur qui ne répond pas) avec la fin de son journal ; il reçoit les causes
//! connues comme repères. Sans LLM, ou quand le LLM est lui-même en cause, la base suffit, et Aria dit honnêtement
//! quand rien ne correspond.
//!
//! Rien de secret ne part au LLM : clés, jetons et chemins du profil sont masqués avant l'envoi. AS-18 lit les textes
//! avant et après ; ✕ Annuler arrête l'écriture ; la réponse JSON mal formée est redemandée une fois ; la consigne se
//! règle dans Réglages. Chaque explication est notée au Journal, avec l'erreur d'origine.

use std::path::Path;
use std::time::Instant;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::directions::ErreurLecture;
use crate::llama::{self, Lancement};
use crate::plateau::{Commande, Contexte, Generateur};
use crate::prompt::{self, CLAUSE_MINEURS};
use crate::{carte, fiches, journal, llm, securite};

/// Le message d'erreur lu, au plus (un message plus long est coupé).
pub const LONGUEUR_ERREUR: usize = 4000;
const LONGUEUR_CONTEXTE: usize = 1000;
/// La fin du journal d'un moteur donnée au LLM : ses dernières lignes, au plus tant de caractères.
const LIGNES_JOURNAL: usize = 40;
const LONGUEUR_JOURNAL: usize = 3000;
const LONGUEUR_CAUSE: usize = 500;
const LONGUEUR_REMEDE: usize = 300;
const REMEDES_MAX: usize = 4;
/// Là où une erreur peut être expliquée.
pub const ORIGINES: [&str; 8] = [
    "plateau",
    "retouche",
    "montage",
    "moteur",
    "telechargement",
    "installation",
    "journal",
    "autre",
];

/// La consigne d'origine (réglable dans Réglages) ; les repères d'Aria et la clause sur les mineurs s'y ajoutent.
pub const CONSIGNE_EXPLICATION: &str = r#"Tu aides un utilisateur d'Aria Studio, une appli Windows qui génère des images et des vidéos en local : ComfyUI fait tourner les modèles d'image et de vidéo, llama-server les LLM, sur une carte graphique NVIDIA. Une opération vient d'échouer.

Tu lis le message d'erreur et le contexte donné (l'endroit, la commande, la carte, l'état des moteurs, parfois la fin du journal du moteur), puis tu expliques en français, simplement, sans jargon inutile :
- la cause la plus probable, en une ou deux phrases ;
- ta certitude : « sure » si le message le dit clairement, « probable » si le contexte l'indique, « possible » si c'est une hypothèse ;
- de un à quatre remèdes concrets, du plus simple au plus lourd. Chacun est un geste précis : dans Aria Studio quand c'est possible (la page, l'onglet, le bouton ou le réglage, le modèle à installer), sinon dans Windows.

Appuie-toi sur les repères d'Aria ci-dessous quand le message y ressemble. N'invente jamais un menu, un bouton ou un réglage qui n'est ni dans les repères ni dans le contexte. Si rien ne permet de conclure, dis-le franchement et indique où en savoir plus (le journal du moteur, dans data\logs).

Réponds UNIQUEMENT par un objet JSON, sans texte autour :
{"cause": "…", "certitude": "sure", "remedes": ["…", "…"]}"#;

/// La certitude d'une explication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Certitude {
    Sure,
    Probable,
    Possible,
    /// Rien ne permet de conclure.
    Inconnue,
}

/// Une cause connue d'Aria : le motif (expression régulière, sans casse) qui la reconnaît dans un message d'erreur,
/// la cause dite à l'utilisateur et ses remèdes.
#[derive(Debug)]
pub struct CauseConnue {
    pub cle: &'static str,
    motif: &'static str,
    pub cause: &'static str,
    pub certitude: Certitude,
    pub remedes: &'static [&'static str],
    /// Le LLM est lui-même en cause : il n'explique pas sa propre panne.
    pub llm_en_cause: bool,
    /// Cause large (un moteur qui ne répond pas) : le LLM peut l'approfondir avec la fin du journal du moteur.
    pub large: bool,
}

/// Les causes connues, dans l'ordre où elles se cherchent (la première qui correspond l'emporte).
pub const CAUSES: &[CauseConnue] = &[
    CauseConnue {
        cle: "carte_occupee",
        motif: r"carte graphique était occupée|VRAM déjà occupée",
        cause: "Une autre appli occupait la carte graphique (sa mémoire vidéo) au moment de lancer : Aria a attendu, \
                puis s'est arrêtée.",
        certitude: Certitude::Sure,
        remedes: &[
            "Ferme les applis qui utilisent la carte graphique (jeu, logiciel vidéo ou d'IA, navigateur chargé), puis \
             relance.",
            "Si une autre copie d'Aria ou la v2 tourne encore, ferme-la.",
            "Réglages → Moteurs → « Libérer la carte » vide ce qu'Aria garde en mémoire vidéo.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "annule",
        motif: r"^\s*Annulé\b",
        cause: "Ce n'est pas une panne : l'opération a été annulée (✕ Annuler).",
        certitude: Certitude::Sure,
        remedes: &[
            "Relance l'opération si l'annulation n'était pas voulue.",
            "Un téléchargement annulé reprend là où il s'était arrêté.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "as18",
        motif: r"AS-18",
        cause: "Le filtre de protection des mineurs (AS-18) a bloqué la demande : elle associe un âge ou un mot de \
                mineur à un contenu sensible.",
        certitude: Certitude::Sure,
        remedes: &[
            "Reformule sans associer un âge mineur, ou un mot qui le suggère, à un contenu sensible.",
            "Les termes ajoutés au filtre se voient dans Réglages → Sécurité.",
            "Ce blocage est voulu : il ne se contourne pas.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "checkpoint_a_valider",
        motif: r"Checkpoint à valider|steps ne sont pas connus",
        cause: "Le checkpoint choisi n'a pas encore ses steps validés : Aria ne sait pas combien d'étapes il lui faut.",
        certitude: Certitude::Sure,
        remedes: &[
            "Ouvre Modèles → Ma collection, trouve ce checkpoint et valide ses steps (la valeur conseillée est \
             proposée).",
            "Ou choisis un autre checkpoint, déjà validé.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "checkpoint_incompatible",
        motif: r"ne se charge pas dans|size mismatch|state_dict|could not detect model type|header too large|unexpected key|missing key",
        cause: "Le checkpoint ne se charge pas : il n'est sans doute pas de la même famille que le workflow (une autre \
                architecture), ou son fichier est abîmé.",
        certitude: Certitude::Probable,
        remedes: &[
            "Reviens au checkpoint d'origine du workflow.",
            "Vérifie dans Modèles → Ma collection qu'il est rangé dans la bonne famille.",
            "S'il vient d'un téléchargement, retélécharge-le : un fichier tronqué ne se charge pas.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "modele_absent",
        motif: r"absente? de modeles/|Workflow (?:vidéo )?(?:absent|incomplet)|Modèle introuvable|Projecteur d'images introuvable|GGUF absent|LoRA absente",
        cause: "Un fichier de modèle manque : le checkpoint, la LoRA, un fichier du workflow ou le LLM demandé n'est \
                pas dans le dossier des modèles.",
        certitude: Certitude::Sure,
        remedes: &[
            "Installe-le depuis Modèles (onglet Workflows et LLM, ou l'onglet de son type), ou relance l'assistant : \
             Réglages → Moteurs → « Relancer l'assistant ».",
            "Si tes modèles sont ailleurs, vérifie le dossier des modèles dans Réglages → Emplacements et clés (le \
             changement compte au redémarrage).",
            "Pour une LoRA ou un checkpoint retiré du disque, choisis-en un autre.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "vram",
        motif: r"out of memory|OutOfMemory|Allocation on device|CUBLAS_STATUS_ALLOC_FAILED|failed to allocate|not enough memory",
        cause: "La mémoire vidéo (VRAM) a manqué : le modèle, la taille ou la durée demandés ne tenaient pas sur la \
                carte à ce moment-là.",
        certitude: Certitude::Probable,
        remedes: &[
            "Réglages → Moteurs → « Libérer la carte », ferme les autres applis qui utilisent la carte, puis relance.",
            "Demande moins grand : moins de mégapixels, un format plus petit, ou au Montage la résolution rapide et \
             une durée plus courte.",
            "Choisis un checkpoint plus léger (Modèles → Ma collection montre leur poids).",
            "Pour un LLM : Réglages → LLM → « Adapter à ma machine » propose un contexte qui tient sur ta carte.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "pilote",
        motif: r"Pilote .* trop ancien|driver version is insufficient|CUDA driver version",
        cause: "Le pilote NVIDIA est trop ancien pour les moteurs d'Aria (il faut CUDA 13).",
        certitude: Certitude::Sure,
        remedes: &[
            "Installe le dernier pilote NVIDIA (l'application NVIDIA ou nvidia.com/drivers), puis redémarre le PC.",
            "Réglages → Moteurs montre le pilote lu et le minimum demandé.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "sans_nvidia",
        motif: r"il faut une carte NVIDIA|VRAM illisible|no CUDA|CUDA is not available|Torch not compiled with CUDA",
        cause: "Aria ne trouve pas de carte NVIDIA utilisable, ou son pilote ne répond pas.",
        certitude: Certitude::Probable,
        remedes: &[
            "Vérifie que le PC a une carte NVIDIA et que son pilote est installé : Réglages → Moteurs la montre.",
            "Sur un portable, branche-le et choisis le mode performances élevées.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "noeud_manquant",
        motif: r"missing_node_type|node[^.]{0,60}(?:does not exist|not found)|Cannot execute because|n[œo]euds? manquants?",
        cause: "ComfyUI ne connaît pas un nœud du workflow : il lui manque une extension (custom node), ou ce ComfyUI \
                est trop ancien.",
        certitude: Certitude::Probable,
        remedes: &[
            "Avec le ComfyUI d'Aria : Réglages → Moteurs, mets à jour ou réinstalle ComfyUI.",
            "Avec ton ComfyUI (« J'ai déjà ComfyUI ») : installe le nœud manquant (ComfyUI Manager) ou mets ComfyUI à \
             jour ; Réglages → Moteurs liste les nœuds qui manquent.",
            "Sinon, choisis un autre workflow.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "port_occupe",
        motif: r"address already in use|Only one usage of each socket address|os error 10048|port[^.]{0,30}(?:occupé|déjà utilisé)",
        cause: "Le port du moteur est déjà pris : une autre copie d'Aria, la v2 ou un autre ComfyUI tourne déjà.",
        certitude: Certitude::Probable,
        remedes: &[
            "Ferme l'autre copie d'Aria ou la v2 (son icône près de l'horloge), puis relance.",
            "Si c'est ton propre ComfyUI qui tourne, arrête-le, ou déclare-le dans Réglages → Moteurs (« J'ai déjà \
             ComfyUI »).",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "espace_disque",
        motif: r"Espace disque insuffisant|Place insuffisante|No space left|not enough space|disque plein",
        cause: "Le disque n'a plus assez de place pour ce fichier.",
        certitude: Certitude::Sure,
        remedes: &[
            "Libère de la place (corbeille des Rushes et des Bobines, anciens modèles), puis relance.",
            "Ou range les modèles sur un disque plus grand : Réglages → Emplacements et clés.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "empreinte",
        motif: r"Empreinte SHA-256 incorrecte|ne correspond pas à l'empreinte|SHA-256 de .* invalide|Empreinte invalide",
        cause: "Le fichier téléchargé ne correspond pas à son empreinte : il est incomplet ou abîmé, ou la source a \
                changé de fichier. Il a été rejeté.",
        certitude: Certitude::Sure,
        remedes: &[
            "Relance le téléchargement : il repart proprement.",
            "Si l'erreur revient, la source a sans doute changé de fichier : signale-le avec le nom du modèle.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "acces_refuse",
        motif: r"HTTP 401|HTTP 403|\b401\b|\b403\b|Unauthorized|Forbidden|gated",
        cause: "La source refuse le téléchargement sans autorisation : le modèle demande une clé, ou une licence à \
                accepter sur sa page.",
        certitude: Certitude::Probable,
        remedes: &[
            "Ajoute ta clé Civitai ou Hugging Face dans Réglages → Emplacements et clés (« Tester » vérifie qu'elle \
             est acceptée).",
            "Sur Hugging Face, ouvre la page du modèle et accepte sa licence avec ton compte, puis relance.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "sans_7z",
        motif: r"7-Zip|format 7z",
        cause: "Windows ne sait pas ouvrir l'archive 7z de ce moteur.",
        certitude: Certitude::Sure,
        remedes: &["Installe 7-Zip (7-zip.org), puis relance l'installation dans Réglages → Moteurs."],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "fichier_ouvert",
        motif: r"en cours d'utilisation|tourne : arrête-le|being used by another process|Accès refusé|Access is denied",
        cause: "Un fichier est ouvert ou le moteur tourne encore : Windows empêche de le remplacer.",
        certitude: Certitude::Probable,
        remedes: &[
            "Réglages → Moteurs → « Libérer la carte », ferme les fenêtres ouvertes dans ce dossier, puis réessaie.",
            "Si ça persiste, redémarre Aria, puis relance.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "llm",
        motif: r"llama-server|Génération interrompue|Génération refusée|Serveur introuvable|Réponse du LLM vide",
        cause: "Le LLM (llama-server) n'a pas répondu ou s'est arrêté : modèle trop gros pour la carte, contexte trop \
                grand, ou moteur llama.cpp absent ou abîmé.",
        certitude: Certitude::Possible,
        remedes: &[
            "Réglages → LLM → « Adapter à ma machine » propose un contexte qui tient sur ta carte.",
            "Essaie un autre LLM, plus petit.",
            "Réglages → Moteurs : vérifie que llama.cpp est installé, réinstalle-le au besoin.",
            "Le détail est dans le journal du moteur : data\\logs\\llama.log.",
        ],
        llm_en_cause: true,
        large: false,
    },
    CauseConnue {
        cle: "image_refusee",
        motif: r"ComfyUI a refusé l'image|nom d'image refusé",
        cause: "ComfyUI n'a pas accepté l'image de départ (son format ou sa taille).",
        certitude: Certitude::Probable,
        remedes: &[
            "Importe l'image en PNG ou en JPEG, de 4 096 px de côté au plus.",
            "Ou choisis un autre rush comme image de départ.",
        ],
        llm_en_cause: false,
        large: false,
    },
    CauseConnue {
        cle: "comfyui",
        motif: r"ComfyUI (?:ne répond|ne se lance|n'a pas répondu|introuvable|tourne mais|a coupé|s'est arrêté)|liaison avec ComfyUI|Workflow refusé|Exécution interrompue",
        cause: "ComfyUI n'a pas pu faire le travail : il ne s'est pas lancé, s'est arrêté, ou a refusé le workflow.",
        certitude: Certitude::Possible,
        remedes: &[
            "Réglages → Moteurs : vérifie que ComfyUI est installé et à jour (ou le chemin de ton ComfyUI).",
            "Le détail est dans le journal du moteur, data\\logs\\comfyui.log : « Approfondir avec le LLM » le lit \
             pour toi.",
            "Redémarre Aria si ComfyUI reste bloqué.",
        ],
        llm_en_cause: false,
        large: true,
    },
    CauseConnue {
        cle: "reseau",
        motif: r"Réseau :|GitHub a répondu|HTTP 5\d\d|timed out|connexion|Téléchargement de .* (?:impossible|interrompu)",
        cause: "La connexion réseau a coupé, ou la source n'a pas répondu.",
        certitude: Certitude::Probable,
        remedes: &[
            "Vérifie ta connexion, puis relance : un téléchargement reprend là où il s'était arrêté.",
            "Si la source est en panne, réessaie plus tard.",
        ],
        llm_en_cause: false,
        large: false,
    },
];

/// La cause connue qui correspond à ce message, s'il y en a une.
pub fn trouver(erreur: &str) -> Option<&'static CauseConnue> {
    CAUSES
        .iter()
        .find(|c| Regex::new(&format!("(?i){}", c.motif)).is_ok_and(|r| r.is_match(erreur)))
}

/// Ce que l'interface demande d'expliquer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeExplication {
    /// Le message d'erreur, tel que l'appli l'a montré.
    pub erreur: String,
    /// Où l'erreur s'est produite : voir [`ORIGINES`].
    #[serde(default)]
    pub origine: String,
    /// L'étape, l'objet du Journal, le fichier téléchargé…
    #[serde(default)]
    pub contexte: String,
    /// La commande du Plateau, de la Retouche ou du Montage en erreur.
    #[serde(default)]
    pub commande: Option<String>,
    /// Le moteur en cause (`comfyui`, `llama`), pour lire la fin de son journal.
    #[serde(default)]
    pub moteur: Option<String>,
    /// Le GGUF qui explique ; absent : le LLM par défaut (un spécialiste est remplacé par un généraliste).
    #[serde(default)]
    pub llm: Option<String>,
    /// Faire lire au LLM une cause connue mais large (le journal du moteur en plus).
    #[serde(default)]
    pub approfondir: bool,
}

/// Ce que l'appli sait au moment de l'explication.
#[derive(Debug, Clone, Default)]
pub struct Situation {
    /// L'état de chaque moteur, en une ligne (« ComfyUI : à jour »).
    pub moteurs: Vec<String>,
    /// Le moteur qui tient la carte.
    pub actif: Option<String>,
    pub commande: Option<Commande>,
    /// Les secrets connus (clés, jeton MCP), masqués mot pour mot avant l'envoi au LLM.
    pub secrets: Vec<String>,
}

/// Une explication.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Explication {
    pub cause: String,
    pub certitude: Certitude,
    pub remedes: Vec<String>,
    /// `base` (une cause connue d'Aria), `llm` (l'analyse du LLM) ou `aucune` (rien ne correspond).
    pub source: &'static str,
    /// La cause connue reconnue (ou donnée en repère au LLM).
    pub cle: Option<String>,
    /// Une cause connue mais large, qu'un LLM disponible peut approfondir.
    pub approfondir: bool,
    pub llm: Option<String>,
    pub alertes: Vec<String>,
    pub duree_s: f64,
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

fn libelle_origine(origine: &str) -> &'static str {
    match origine {
        "plateau" => "une création du Plateau",
        "retouche" => "une retouche ou un agrandissement",
        "montage" => "un clip ou une séquence du Montage",
        "moteur" => "un moteur (installation, lancement, mise à jour)",
        "telechargement" => "un téléchargement de modèle",
        "installation" => "l'installation d'Aria",
        "journal" => "une opération du Journal",
        _ => "une opération d'Aria",
    }
}

// --- secrets ------------------------------------------------------------------------------------------------------

/// Les motifs masqués avant l'envoi au LLM, après les secrets connus mot pour mot.
const MASQUES: [(&str, &str); 5] = [
    // Un chemin dans le profil d'un utilisateur de Windows (le nom du compte en dit trop).
    (
        r#"(?i)\b[a-z]:[\\/](?:users|utilisateurs)[\\/][^\\/\s"'<>|]+"#,
        "%USERPROFILE%",
    ),
    (r#"(?:/home/|/Users/)[^/\s"']+"#, "~"),
    // Une clé Hugging Face.
    (r"\bhf_[A-Za-z0-9]{16,}", "[clé masquée]"),
    // Ce qui suit un mot de secret : « Bearer … », « token=… », « clé : … ».
    (
        r#"(?i)\b(bearer|token|jeton|cl[eé]|key|api[_-]?key|authorization|password|mot de passe)(["']?\s*[:=]\s*["']?|\s+)[A-Za-z0-9._~+/=\-]{8,}"#,
        "${1}${2}[masqué]",
    ),
    // Une longue suite hexadécimale (clé Civitai, jeton, empreinte) : jamais utile au diagnostic.
    (r"\b[0-9a-fA-F]{32,}\b", "[masqué]"),
];

/// Le texte sans secret : les secrets connus mot pour mot, le profil de l'utilisateur, puis les formes de clés et de
/// jetons. Ce qui sert au diagnostic (noms de modèles, messages, chemins relatifs) reste.
pub fn masquer(texte: &str, profil: Option<&str>, secrets: &[String]) -> String {
    let mut t = texte.to_owned();
    for s in secrets
        .iter()
        .map(|s| s.trim())
        .filter(|s| s.chars().count() >= 8)
    {
        t = t.replace(s, "[secret masqué]");
    }
    if let Some(p) = profil.map(str::trim).filter(|p| p.chars().count() > 3) {
        for variante in [p.to_owned(), p.replace('\\', "/")] {
            if let Ok(r) = Regex::new(&format!("(?i){}", regex::escape(&variante))) {
                t = r.replace_all(&t, "%USERPROFILE%").into_owned();
            }
        }
    }
    for (motif, par) in MASQUES {
        if let Ok(r) = Regex::new(motif) {
            t = r.replace_all(&t, par).into_owned();
        }
    }
    t
}

fn profil() -> Option<String> {
    std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
        .filter(|p| !p.trim().is_empty())
}

// --- contexte -----------------------------------------------------------------------------------------------------

fn gio(octets: u64) -> String {
    let mio = u32::try_from(octets >> 20).unwrap_or(u32::MAX);
    format!("{:.1}", f64::from(mio) / 1024.0).replace('.', ",")
}

fn ligne_carte() -> String {
    let c = carte::lire(0);
    if !c.nvidia {
        return "Carte : aucune carte NVIDIA lue".to_owned();
    }
    let mut morceaux = vec![c.nom.unwrap_or_else(|| "carte NVIDIA".to_owned())];
    if let Some(p) = c.pilote {
        morceaux.push(format!("pilote {p}"));
    }
    if let Some(t) = carte::vram_totale() {
        morceaux.push(format!("{} Gio de VRAM", gio(t)));
    }
    if let Some(l) = carte::vram_libre() {
        morceaux.push(format!("{} Gio libres pour Aria", gio(l)));
    }
    format!("Carte : {}", morceaux.join(", "))
}

fn ligne_commande(c: &Commande) -> String {
    let mut morceaux = vec![format!("type {}", c.genre), format!("workflow {}", c.workflow)];
    if let Some(m) = &c.modele {
        morceaux.push(format!("checkpoint {m}"));
    }
    if !c.format.is_empty() {
        morceaux.push(format!("format {}", c.format));
    }
    if let Some(q) = &c.qualite {
        morceaux.push(format!("résolution {q}"));
    }
    if let Some(d) = c.duree {
        morceaux.push(format!("durée {d} s"));
    }
    if !c.etape.is_empty() {
        morceaux.push(format!("étape {}", c.etape));
    }
    morceaux.push(format!("{} demandée(s)", c.nombre));
    format!("Commande : {}", morceaux.join(", "))
}

/// Le moteur dont la fin du journal aide : celui donné, sinon celui que le message nomme, sinon ComfyUI pour une
/// commande.
fn moteur_de(d: &DemandeExplication) -> Option<&'static str> {
    match d.moteur.as_deref() {
        Some("comfyui") => Some("comfyui"),
        Some(m) if m.starts_with("llama") || m == "llm" => Some("llama"),
        Some(_) => None,
        None => {
            let e = d.erreur.to_lowercase();
            if e.contains("llama") {
                Some("llama")
            } else if e.contains("comfyui")
                || ["plateau", "retouche", "montage"].contains(&d.origine.as_str())
            {
                Some("comfyui")
            } else {
                None
            }
        }
    }
}

/// Les dernières lignes du journal d'un moteur (`data\logs\<moteur>.log`), au plus [`LONGUEUR_JOURNAL`] caractères.
fn fin_du_journal(data: &Path, moteur: &str) -> Option<String> {
    let octets = std::fs::read(data.join("logs").join(format!("{moteur}.log"))).ok()?;
    let texte = String::from_utf8_lossy(&octets);
    let lignes: Vec<&str> = texte.lines().filter(|l| !l.trim().is_empty()).collect();
    let fin = lignes
        .get(lignes.len().saturating_sub(LIGNES_JOURNAL)..)?
        .join("\n");
    let n = fin.chars().count();
    let fin = if n > LONGUEUR_JOURNAL {
        fin.chars().skip(n - LONGUEUR_JOURNAL).collect()
    } else {
        fin
    };
    Some(fin).filter(|f| !f.trim().is_empty())
}

/// Le message envoyé au LLM, avant masquage.
fn message(
    contexte: &Contexte,
    d: &DemandeExplication,
    s: &Situation,
    erreur: &str,
    connue: Option<&CauseConnue>,
) -> String {
    let mut lignes = vec![
        format!("Message d'erreur :\n{erreur}"),
        String::new(),
        format!("Où : {}", libelle_origine(&d.origine)),
    ];
    if !d.contexte.trim().is_empty() {
        lignes.push(format!("Contexte : {}", borne(&d.contexte, LONGUEUR_CONTEXTE)));
    }
    if let Some(c) = &s.commande {
        lignes.push(ligne_commande(c));
    }
    lignes.push(ligne_carte());
    if !s.moteurs.is_empty() {
        lignes.push(format!("Moteurs : {}", s.moteurs.join(" ; ")));
    }
    if let Some(a) = &s.actif {
        lignes.push(format!("Moteur qui tient la carte : {a}"));
    }
    if let Some(c) = connue {
        lignes.push(format!(
            "Cause connue qui semble correspondre, à confirmer ou à préciser : {}",
            c.cause
        ));
    }
    if let Some(m) = moteur_de(d)
        && let Some(fin) = fin_du_journal(&contexte.data, m)
    {
        lignes.push(String::new());
        lignes.push(format!("Fin du journal de {m} (data\\logs\\{m}.log) :\n{fin}"));
    }
    lignes.join("\n")
}

// --- consigne et lecture ------------------------------------------------------------------------------------------

/// La consigne : celle de Réglages pour ce GGUF, sinon celle d'origine ; puis les repères d'Aria et la clause
/// verrouillée sur les mineurs.
pub fn consigne(reecrite: Option<&str>) -> String {
    let base = reecrite
        .filter(|c| !c.trim().is_empty())
        .unwrap_or(CONSIGNE_EXPLICATION)
        .trim_end();
    let reperes: Vec<String> = CAUSES
        .iter()
        .map(|c| format!("- {} → {}", c.cause, c.remedes.first().copied().unwrap_or("")))
        .collect();
    format!(
        "{base}\n\nRepères d'Aria (les causes déjà connues, avec leur premier remède) :\n{}\n\n{CLAUSE_MINEURS}",
        reperes.join("\n")
    )
}

/// Une explication telle que le LLM l'a rendue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lue {
    pub cause: String,
    pub certitude: Certitude,
    pub remedes: Vec<String>,
}

fn objet(texte: &str) -> Result<Map<String, Value>, ErreurLecture> {
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
    if Regex::new(crate::conversation::REFUS).is_ok_and(|r| r.is_match(&debut)) {
        return Err(ErreurLecture {
            message: format!("Le LLM refuse d'expliquer : « {} »", borne(texte, 160)),
            refus: true,
        });
    }
    Err(ErreurLecture {
        message: format!(
            "Réponse du LLM illisible (pas d'explication en JSON) : « {} »",
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

fn certitude_de(texte: &str) -> Certitude {
    let t = texte.to_lowercase();
    if ["sûr", "sur", "certain", "high", "haute", "élevée"]
        .iter()
        .any(|m| t.starts_with(m))
    {
        Certitude::Sure
    } else if ["probable", "likely", "medium", "moyenne"]
        .iter()
        .any(|m| t.starts_with(m))
    {
        Certitude::Probable
    } else {
        Certitude::Possible
    }
}

/// L'explication d'une réponse : `{"cause", "certitude", "remedes"}` (quelques variantes de noms tolérées).
///
/// # Errors
/// Réponse vide, sans JSON lisible, ou refus du LLM.
pub fn lire_explication(texte: &str) -> Result<Lue, ErreurLecture> {
    let o = objet(texte)?;
    let champ = |cles: &[&str]| cles.iter().find_map(|c| o.get(*c));
    let cause = borne(
        &texte_de(champ(&["cause", "explication", "explanation"])),
        LONGUEUR_CAUSE,
    );
    let certitude = certitude_de(&texte_de(champ(&["certitude", "confidence", "certainty"])));
    let remedes: Vec<String> = match champ(&["remedes", "remèdes", "remedies", "solutions", "remede"]) {
        Some(Value::Array(a)) => a
            .iter()
            .map(|r| match r {
                Value::Object(m) => texte_de(
                    m.get("geste")
                        .or_else(|| m.get("texte"))
                        .or_else(|| m.get("text")),
                ),
                autre => texte_de(Some(autre)),
            })
            .collect(),
        Some(autre) => vec![texte_de(Some(autre))],
        None => Vec::new(),
    };
    let remedes = remedes
        .iter()
        .map(|r| borne(r, LONGUEUR_REMEDE))
        .filter(|r| !r.is_empty())
        .take(REMEDES_MAX)
        .collect();
    Ok(Lue {
        cause,
        certitude,
        remedes,
    })
}

/// Ce qui manque à une explication lue.
pub fn defauts(lue: &Lue) -> Vec<String> {
    let mut sortie = Vec::new();
    if lue.cause.is_empty() {
        sortie.push("la cause manque".to_owned());
    }
    if lue.remedes.is_empty() {
        sortie.push("aucun remède n'est proposé".to_owned());
    }
    sortie
}

// --- l'explication ------------------------------------------------------------------------------------------------

fn depuis_base(c: &CauseConnue) -> Explication {
    Explication {
        cause: c.cause.to_owned(),
        certitude: c.certitude,
        remedes: c.remedes.iter().map(|r| (*r).to_owned()).collect(),
        source: "base",
        cle: Some(c.cle.to_owned()),
        approfondir: false,
        llm: None,
        alertes: Vec::new(),
        duree_s: 0.0,
    }
}

/// Sans le LLM : la cause connue s'il y en a une, sinon un aveu honnête. `motif` dit pourquoi le LLM n'a pas servi.
fn sans_llm(connue: Option<&CauseConnue>, motif: Option<String>, llm_installe: bool) -> Explication {
    let mut e = connue.map_or_else(
        || Explication {
            cause: "Aucune cause connue d'Aria ne correspond à ce message, et le LLM n'a pas pu l'analyser.".to_owned(),
            certitude: Certitude::Inconnue,
            remedes: {
                let mut r = vec![
                    "Relis le message : il nomme souvent le fichier, le modèle ou le réglage en cause.".to_owned(),
                    "Le journal du moteur donne le détail : data\\logs\\comfyui.log ou data\\logs\\llama.log.".to_owned(),
                ];
                if !llm_installe {
                    r.push(
                        "Installe un LLM (Modèles → Workflows et LLM) pour qu'Aria analyse les erreurs qu'elle ne \
                         connaît pas."
                            .to_owned(),
                    );
                }
                r
            },
            source: "aucune",
            cle: None,
            approfondir: false,
            llm: None,
            alertes: Vec::new(),
            duree_s: 0.0,
        },
        depuis_base,
    );
    e.alertes.extend(motif);
    e
}

/// Le GGUF qui explique : celui choisi, sinon le LLM par défaut ; un généraliste (un spécialiste n'écrit que des
/// prompts).
fn llm_de(contexte: &Contexte, d: &DemandeExplication) -> String {
    let nom = llm::nom(
        &contexte.dossier,
        &contexte.modeles,
        d.llm.as_deref().filter(|l| !l.is_empty()),
    );
    if fiches::specialiste(&nom).is_some() {
        llm::defaut(&contexte.dossier, &contexte.modeles)
    } else {
        nom
    }
}

fn nom_de(lancement: &Lancement) -> String {
    lancement
        .modele
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// L'analyse du LLM, quand la base ne suffit pas.
#[allow(clippy::too_many_lines)] // la mécanique d'écriture du Scénario, gardée d'un tenant
fn par_le_llm(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    d: &DemandeExplication,
    s: &Situation,
    erreur: &str,
    connue: Option<&'static CauseConnue>,
) -> Result<Explication, String> {
    let nom = llm_de(contexte, d);
    let lancement = match lancement_pret(contexte, &nom) {
        Ok(l) => l,
        Err(e) => {
            return Ok(sans_llm(
                connue,
                Some(format!("Pas de LLM pour analyser : {e}")),
                false,
            ));
        }
    };
    let profil = profil();
    let texte = masquer(
        &message(contexte, d, s, erreur, connue),
        profil.as_deref(),
        &s.secrets,
    );
    // AS-18 : ce qui part au LLM.
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    if filtre.verifier(&[texte.as_str()]).is_err() {
        return Ok(sans_llm(
            connue,
            Some(
                "Le message contient un texte que le filtre de protection des mineurs bloque : il n'est pas envoyé \
                 au LLM."
                    .to_owned(),
            ),
            true,
        ));
    }
    let regle = llm::valeurs(&contexte.dossier, &nom);
    let reecrite = llm::consigne(&contexte.dossier, &nom, "explication");
    let (temperature, echantillonnage) = regle.ecriture(false);
    let mut messages = vec![
        json!({"role": "system", "content": consigne(reecrite.as_deref())}),
        json!({"role": "user", "content": texte}),
    ];

    let mut meilleur: Option<(Lue, Vec<String>)> = None;
    let mut redemandee: Option<String> = None;
    let mut derniere_erreur = "Réponse du LLM illisible".to_owned();
    for essai in 0..2 {
        let demande = llama::Demande {
            messages: Value::Array(messages.clone()),
            temperature,
            max_tokens: regle.max_tokens.max(700),
            reflechir: false,
            echantillonnage: echantillonnage.clone(),
            ..llama::Demande::default()
        };
        let reponse = match generateur.ecrire(lancement.clone(), &demande) {
            Ok(r) => r,
            Err(e) if e == crate::comfy_serveur::ANNULEE => return Err(e),
            // Le LLM ne répond pas : la base, honnêtement ; une panne au second essai garde le premier.
            Err(e) if essai == 0 => {
                return Ok(sans_llm(
                    connue,
                    Some(format!("Le LLM n'a pas répondu : {e}")),
                    true,
                ));
            }
            Err(_) => break,
        };
        let problemes = match lire_explication(&reponse.texte) {
            Err(e) if e.refus => return Ok(sans_llm(connue, Some(e.message), true)),
            Err(e) => {
                derniere_erreur = e.message;
                vec!["la réponse n'est pas l'objet JSON demandé".to_owned()]
            }
            Ok(lue) => {
                let manques = defauts(&lue);
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
            "Ta réponse a ces défauts : {}. Recommence en les corrigeant. Réponds UNIQUEMENT par l'objet JSON demandé, \
             en français.",
            problemes.join(" ; ")
        )}));
    }
    let Some((lue, restants)) = meilleur else {
        return Ok(sans_llm(connue, Some(derniere_erreur), true));
    };
    if lue.cause.is_empty() && lue.remedes.is_empty() {
        return Ok(sans_llm(
            connue,
            Some("Le LLM n'a rien expliqué".to_owned()),
            true,
        ));
    }
    // AS-18 : ce que le LLM a écrit, relu avec ce qui l'a nourri.
    let mut controles = vec![texte.as_str(), lue.cause.as_str()];
    controles.extend(lue.remedes.iter().map(String::as_str));
    if filtre.verifier(&controles).is_err() {
        return Ok(sans_llm(
            connue,
            Some("La réponse du LLM a été bloquée par le filtre de protection des mineurs.".to_owned()),
            true,
        ));
    }
    let mut alertes = Vec::new();
    if let Some(m) = redemandee {
        alertes.push(format!("Contrôle qualité : réponse redemandée au LLM ({m})"));
    }
    alertes.extend(restants.iter().map(|r| format!("Contrôle qualité : {r}")));
    Ok(Explication {
        cause: lue.cause,
        certitude: lue.certitude,
        remedes: lue.remedes,
        source: "llm",
        cle: connue.map(|c| c.cle.to_owned()),
        approfondir: false,
        llm: Some(nom_de(&lancement)),
        alertes,
        duree_s: 0.0,
    })
}

/// Un LLM généraliste est-il prêt à expliquer ?
fn llm_disponible(contexte: &Contexte, d: &DemandeExplication) -> bool {
    lancement_pret(contexte, &llm_de(contexte, d)).is_ok()
}

/// Le lancement de ce GGUF, s'il est bien sur le disque (`llm::lancement` ne vérifie ses fichiers qu'au démarrage du
/// serveur). Un llama-server absent se dira au premier appel.
fn lancement_pret(contexte: &Contexte, nom: &str) -> Result<Lancement, String> {
    let lancement = llm::lancement(&contexte.lieux(), Some(nom))?;
    if lancement.modele.is_file() {
        Ok(lancement)
    } else {
        Err(format!("GGUF absent de modeles/llm : {nom}"))
    }
}

/// Explique une erreur : la cause connue si elle correspond (aussitôt, sans LLM), sinon l'analyse du LLM ; notée au
/// Journal avec l'erreur d'origine.
///
/// # Errors
/// Message vide, filtre AS-18 illisible, ou annulation.
pub fn expliquer(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    d: &DemandeExplication,
    s: &Situation,
) -> Result<Explication, String> {
    let debut = Instant::now();
    let erreur = borne(&d.erreur, LONGUEUR_ERREUR);
    if erreur.is_empty() {
        return Err("Rien à expliquer : le message d'erreur est vide".to_owned());
    }
    let origine = if ORIGINES.contains(&d.origine.as_str()) {
        d.origine.clone()
    } else {
        "autre".to_owned()
    };
    let d = DemandeExplication { origine, ..d.clone() };
    let connue = trouver(&erreur);
    let mut explication = match connue {
        Some(c) if !d.approfondir || c.llm_en_cause => {
            let mut e = depuis_base(c);
            if d.approfondir {
                e.alertes.push(
                    "Le LLM est lui-même en cause : il ne peut pas expliquer sa propre panne.".to_owned(),
                );
            }
            e.approfondir = c.large && llm_disponible(contexte, &d);
            e
        }
        _ => par_le_llm(contexte, generateur, &d, s, &erreur, connue)?,
    };
    explication.duree_s = (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0;
    let detail = masquer(
        &format!(
            "Erreur : {} · Cause ({}) : {}",
            borne(&erreur, 300),
            match explication.source {
                "base" => "connue d'Aria",
                "llm" => "analyse du LLM",
                _ => "inconnue",
            },
            borne(&explication.cause, 300)
        ),
        profil().as_deref(),
        &s.secrets,
    );
    journal::noter(
        &contexte.dossier,
        "Erreur expliquée",
        libelle_origine(&d.origine),
        journal::Resultat::Ok,
        Some(&detail),
    );
    Ok(explication)
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

    fn faux(reponses: Vec<Result<String, String>>) -> Faux {
        Faux {
            reponses,
            demandes: Vec::new(),
        }
    }

    fn contexte(temp: &tempfile::TempDir, avec_llm: bool) -> Contexte {
        let modeles = temp.path().join("modeles");
        fs::create_dir_all(modeles.join("llm")).unwrap();
        if avec_llm {
            fs::write(modeles.join("llm/mistral.gguf"), crate::gguf::essai::gguf(&[12])).unwrap();
        }
        Contexte {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles,
            moteurs: temp.path().join("moteurs"),
            port_llama: 8095,
        }
    }

    fn demande(erreur: &str) -> DemandeExplication {
        DemandeExplication {
            erreur: erreur.to_owned(),
            origine: "plateau".to_owned(),
            ..DemandeExplication::default()
        }
    }

    #[test]
    fn la_base_reconnait_les_messages_d_aria() {
        for c in CAUSES {
            assert!(
                Regex::new(&format!("(?i){}", c.motif)).is_ok(),
                "motif invalide : {}",
                c.cle
            );
            assert!(!c.remedes.is_empty(), "sans remède : {}", c.cle);
        }
        let cas = [
            (
                "Annulé : la carte graphique était occupée par une autre appli",
                "carte_occupee",
            ),
            ("Annulé", "annule"),
            ("ERREUR AS-18 · Génération bloquée : …", "as18"),
            (
                "Checkpoint à valider : ses steps ne sont pas connus ; valide-les dans Modèles → Ma collection",
                "checkpoint_a_valider",
            ),
            (
                "Modèle absent de modeles/image/krea : x.safetensors",
                "modele_absent",
            ),
            ("Workflow vidéo incomplet : minimax_h3_i2v", "modele_absent"),
            (
                "KSampler : CUDA out of memory. Tried to allocate 2.00 GiB",
                "vram",
            ),
            (
                "Pilote 551.23 trop ancien : installe un pilote NVIDIA 580 ou plus récent.",
                "pilote",
            ),
            (
                r#"Workflow refusé : {"error": {"type": "missing_node_type"}}"#,
                "noeud_manquant",
            ),
            ("Espace disque insuffisant", "espace_disque"),
            ("Empreinte SHA-256 incorrecte : fichier rejeté", "empreinte"),
            ("Réponse HTTP 401", "acces_refuse"),
            ("llama-server ne répond pas : connexion refusée", "llm"),
            ("Réseau : la source s'arrête avant la fin du fichier", "reseau"),
            ("ComfyUI ne répond pas : délai dépassé", "comfyui"),
        ];
        for (message, cle) in cas {
            assert_eq!(trouver(message).map(|c| c.cle), Some(cle), "{message}");
        }
        assert!(trouver("Une erreur que personne ne connaît").is_none());
    }

    #[test]
    fn aucun_secret_ne_part() {
        let secrets = vec!["jeton-mcp-123456789".to_owned()];
        let texte = r"Échec : C:\Users\jeremy\.Aria-Studio\modeles\image\qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors \
             (C:/Users/jeremy/AppData) Authorization: Bearer abcdefghijkl0123 token=xyz12345678 hf_AbCdEfGhIjKlMnOpQrSt \
             cle 0123456789abcdef0123456789abcdef jeton-mcp-123456789 /home/bob/x";
        let m = masquer(texte, Some(r"C:\Users\jeremy"), &secrets);
        for interdit in [
            "jeremy",
            "abcdefghijkl0123",
            "xyz12345678",
            "hf_AbCdEf",
            "0123456789abcdef0123456789abcdef",
            "jeton-mcp-123456789",
            "bob",
        ] {
            assert!(!m.contains(interdit), "{interdit} reste dans : {m}");
        }
        // Ce qui sert au diagnostic reste : le nom du modèle et le chemin relatif.
        assert!(m.contains(
            r"%USERPROFILE%\.Aria-Studio\modeles\image\qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors"
        ));
        // Un autre compte que le sien est masqué aussi.
        assert!(!masquer(r"D:\Users\alice\x", None, &[]).contains("alice"));
    }

    #[test]
    fn lecture_d_une_explication() {
        let l = lire_explication(
            r#"Voici : {"cause": "Le **modèle** est trop gros.", "certitude": "probable", "remedes": ["Libère la carte.", "Choisis plus léger."]}"#,
        )
        .unwrap();
        assert_eq!(l.cause, "Le modèle est trop gros.");
        assert_eq!(l.certitude, Certitude::Probable);
        assert_eq!(l.remedes.len(), 2);
        let en =
            lire_explication(r#"{"explanation": "x", "confidence": "high", "solutions": [{"text": "y"}]}"#)
                .unwrap();
        assert_eq!((en.certitude, en.remedes[0].as_str()), (Certitude::Sure, "y"));
        assert!(defauts(&lire_explication(r#"{"cause": "x"}"#).unwrap())[0].contains("remède"));
        assert!(lire_explication("").unwrap_err().message.contains("vide"));
        assert!(
            lire_explication("I'm sorry, I can't help with that.")
                .unwrap_err()
                .refus
        );
    }

    #[test]
    fn une_cause_connue_sans_llm_et_au_journal() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, true);
        let mut f = faux(Vec::new());
        let e = expliquer(
            &ctx,
            &mut f,
            &demande("Checkpoint à valider : ses steps ne sont pas connus"),
            &Situation::default(),
        )
        .unwrap();
        assert_eq!(
            (e.source, e.cle.as_deref()),
            ("base", Some("checkpoint_a_valider"))
        );
        assert!(e.remedes[0].contains("Ma collection"));
        assert!(f.demandes.is_empty(), "le LLM n'est pas appelé");
        let journal = fs::read_to_string(temp.path().join("journal.json")).unwrap();
        assert!(journal.contains("Erreur expliquée") && journal.contains("Checkpoint à valider"));
    }

    #[test]
    fn une_cause_large_s_approfondit_avec_le_journal_du_moteur() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, true);
        fs::create_dir_all(ctx.data.join("logs")).unwrap();
        fs::write(
            ctx.data.join("logs/comfyui.log"),
            "démarrage\nImportError: cannot import name 'x' from torch\n",
        )
        .unwrap();
        let mut f = faux(Vec::new());
        let base = expliquer(
            &ctx,
            &mut f,
            &demande("ComfyUI ne répond pas : délai"),
            &Situation::default(),
        )
        .unwrap();
        assert!(base.approfondir && f.demandes.is_empty());
        let mut f = faux(vec![Ok(
            r#"{"cause": "torch est cassé.", "certitude": "probable", "remedes": ["Réinstalle ComfyUI."]}"#
                .to_owned(),
        )]);
        let d = DemandeExplication {
            approfondir: true,
            ..demande("ComfyUI ne répond pas : délai")
        };
        let e = expliquer(&ctx, &mut f, &d, &Situation::default()).unwrap();
        assert_eq!((e.source, e.cle.as_deref()), ("llm", Some("comfyui")));
        assert_eq!(e.llm.as_deref(), Some("mistral.gguf"));
        let message = f.demandes[0].messages[1]["content"].as_str().unwrap();
        assert!(message.contains("ImportError") && message.contains("Cause connue qui semble correspondre"));
        let systeme = f.demandes[0].messages[0]["content"].as_str().unwrap();
        assert!(systeme.contains("Repères d'Aria") && systeme.contains("Non-negotiable rule"));
    }

    #[test]
    fn une_erreur_inconnue_va_au_llm_redemandee_une_fois() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, true);
        let mut f = faux(vec![
            Ok("Je pense que c'est un problème.".to_owned()),
            Ok(r#"{"cause": "Le fichier est verrouillé.", "certitude": "possible", "remedes": ["Ferme l'autre appli."]}"#
                .to_owned()),
        ]);
        let s = Situation {
            secrets: vec!["secret-tres-long-42".to_owned()],
            ..Situation::default()
        };
        let e = expliquer(&ctx, &mut f, &demande("Erreur étrange secret-tres-long-42"), &s).unwrap();
        assert_eq!((e.source, e.certitude), ("llm", Certitude::Possible));
        assert_eq!(f.demandes.len(), 2);
        assert!(e.alertes[0].contains("redemandée"));
        let message = f.demandes[0].messages[1]["content"].as_str().unwrap();
        assert!(!message.contains("secret-tres-long-42"), "le secret ne part pas");
    }

    #[test]
    fn sans_llm_ou_llm_en_cause_la_base_suffit() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp, false);
        let mut f = faux(Vec::new());
        let e = expliquer(
            &ctx,
            &mut f,
            &demande("Erreur que personne ne connaît"),
            &Situation::default(),
        )
        .unwrap();
        assert_eq!((e.source, e.certitude), ("aucune", Certitude::Inconnue));
        assert!(e.remedes.iter().any(|r| r.contains("Installe un LLM")));
        assert!(f.demandes.is_empty());
        // Le LLM en cause : jamais d'appel, même pour approfondir.
        let ctx = contexte(&temp, true);
        let d = DemandeExplication {
            approfondir: true,
            ..demande("llama-server ne répond pas : x")
        };
        let e = expliquer(&ctx, &mut f, &d, &Situation::default()).unwrap();
        assert_eq!(e.cle.as_deref(), Some("llm"));
        assert!(!e.approfondir && f.demandes.is_empty());
        // Un LLM qui ne répond pas : la base, honnêtement ; une annulation reste une annulation.
        let mut panne = faux(vec![Err("connexion refusée".to_owned())]);
        let e = expliquer(
            &ctx,
            &mut panne,
            &demande("Erreur inconnue"),
            &Situation::default(),
        )
        .unwrap();
        assert_eq!(e.source, "aucune");
        assert!(e.alertes[0].contains("n'a pas répondu"));
        let mut annule = faux(vec![Err(crate::comfy_serveur::ANNULEE.to_owned())]);
        assert_eq!(
            expliquer(
                &ctx,
                &mut annule,
                &demande("Erreur inconnue"),
                &Situation::default()
            )
            .unwrap_err(),
            crate::comfy_serveur::ANNULEE
        );
        assert!(expliquer(&ctx, &mut f, &demande("   "), &Situation::default()).is_err());
    }
}
