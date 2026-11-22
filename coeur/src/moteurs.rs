//! Installation des moteurs (ComfyUI portable, llama.cpp) d'après le catalogue, comme en v2 (`app/moteurs.py`).
//!
//! - Les archives vont dans `<moteurs>\_archives\`, téléchargées depuis la release GitHub avec reprise
//!   (`<fichier>.part`), et ne servent qu'une fois leur SHA-256 vérifié. Une empreinte fausse supprime le `.part` ;
//!   une annulation le garde, pour reprendre plus tard.
//! - Extraction dans `<dossier>.extrait\` : les zip par la crate `zip` (aucun chemin ne sort du dossier), les 7z par
//!   le `tar` de Windows s'il lit le LZMA (Windows 11), sinon par 7-Zip trouvé dans le registre ou Program Files.
//! - Le contenu (ou le sous-dossier `racine` de l'archive) est versé dans `<dossier>.new\`, avec la fiche
//!   `.aria-moteur.json`. Puis, sous le verrou de l'appelant, `<dossier>` devient `<dossier>.prev` et `.new`
//!   devient `<dossier>` : on peut revenir à la version d'avant ([`revenir`]).
//!
//! Le réseau ([`Reseau`]), les outils d'extraction ([`Extracteurs`], [`Lanceur`]) et la place libre viennent de
//! l'appelant : tout ici se teste sans réseau ni 7-Zip.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::catalogue::{Archive, Catalogue, Moteur};
use crate::mises_a_jour;

/// La fiche d'un moteur installé par Aria, dans son dossier.
pub const FICHE: &str = ".aria-moteur.json";
/// Le dossier des archives téléchargées, dans le dossier des moteurs.
pub const DOSSIER_ARCHIVES: &str = "_archives";
/// Place à prévoir pour l'extraction et l'échange, en multiple de la taille des archives.
pub const EXPANSION: u64 = 3;
/// Ce qu'il faut faire quand aucun outil ne lit un 7z.
pub const SANS_EXTRACTEUR_7Z: &str =
    "le tar de Windows de ce PC ne lit pas le format 7z (LZMA) : installe 7-Zip (7-zip.org), puis réessaie";
/// Le message d'une annulation pendant un téléchargement.
pub const ANNULE: &str = "Annulé : le téléchargement reprendra où il s'est arrêté";

const SUFFIXE_PART: &str = ".part";
const SUFFIXE_NOUVEAU: &str = ".new";
const SUFFIXE_EXTRAIT: &str = ".extrait";
const SUFFIXE_PRECEDENT: &str = ".prev";
const SUFFIXE_ECHANGE: &str = ".echange";
const BLOC: usize = 1024 * 1024;

/// La fiche `.aria-moteur.json` : quelle version Aria a installée, et depuis quelles archives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fiche {
    pub id: String,
    pub version: String,
    /// Date et heure locales à la seconde, sans décalage (comme en v2).
    pub installe_le: String,
    /// Nom de chaque archive et son SHA-256.
    #[serde(default)]
    pub archives: BTreeMap<String, String>,
}

/// L'état d'un moteur sur le disque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Statut {
    /// Pas installé.
    Absent,
    /// Installé à la main (pas de fiche).
    Manuel,
    /// Installé par Aria, dans une autre version que celle du catalogue.
    Different,
    /// Installé par Aria, dans la version du catalogue.
    AJour,
    /// ComfyUI de l'utilisateur, choisi à la place de celui d'Aria.
    Externe,
}

/// Ce que l'onglet Moteurs montre d'un moteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatMoteur {
    pub id: String,
    pub nom: String,
    pub role: String,
    pub facultatif: bool,
    pub dossier: String,
    /// La version du catalogue.
    pub version: String,
    pub version_installee: Option<String>,
    pub statut: Statut,
    pub chemin: String,
    pub externe: bool,
    /// Une version précédente attend dans `<dossier>.prev`.
    pub precedente: bool,
    /// Octets encore à télécharger.
    pub a_telecharger: u64,
    /// Un programme écoute sur son port.
    pub occupe: bool,
    /// Ce qui manque pour l'installer (un outil pour les 7z) ; rien s'il est à jour ou externe.
    pub prerequis: Option<String>,
}

/// Le réseau, fourni par l'appelant.
pub trait Reseau {
    /// Ouvre l'adresse en GET, en demandant la suite à partir de l'octet `depuis` (`Range`) s'il n'est pas nul.
    /// Rend le statut HTTP et le corps.
    ///
    /// # Errors
    /// Une erreur de connexion.
    fn ouvrir(&self, adresse: &str, depuis: u64) -> io::Result<(u16, Box<dyn Read>)>;
}

/// Ce qu'un outil a rendu.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sortie {
    pub reussi: bool,
    pub code: Option<i32>,
    pub sortie: String,
    pub erreurs: String,
}

/// Lance un outil (tar, 7-Zip) et attend sa fin.
pub trait Lanceur {
    /// # Errors
    /// L'outil ne se lance pas.
    fn lancer(&self, commande: &[OsString]) -> io::Result<Sortie>;
}

/// Les outils qui savent lire un 7z sur ce PC.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extracteurs {
    /// Le `tar` de Windows, s'il lit le LZMA.
    pub tar: Option<PathBuf>,
    /// `7z.exe`, s'il est installé.
    pub sept_zip: Option<PathBuf>,
}

impl Extracteurs {
    /// Les commandes à essayer pour extraire un 7z, dans l'ordre : tar, puis 7-Zip.
    fn commandes(&self, archive: &Path, vers: &Path) -> Vec<Vec<OsString>> {
        let mut commandes = Vec::new();
        if let Some(tar) = &self.tar {
            commandes.push(vec![
                tar.clone().into_os_string(),
                "-xf".into(),
                archive.as_os_str().to_owned(),
                "-C".into(),
                vers.as_os_str().to_owned(),
            ]);
        }
        if let Some(sept) = &self.sept_zip {
            let mut sortie = OsString::from("-o");
            sortie.push(vers.as_os_str());
            commandes.push(vec![
                sept.clone().into_os_string(),
                "x".into(),
                archive.as_os_str().to_owned(),
                sortie,
                "-y".into(),
            ]);
        }
        commandes
    }

    /// Ce qui manque pour installer ce moteur : un outil, s'il a un 7z et qu'aucun ne le lit.
    pub fn prerequis(&self, moteur: &Moteur) -> Option<String> {
        let a_un_7z = moteur
            .archives
            .iter()
            .any(|a| a.fichier.to_lowercase().ends_with(".7z"));
        (a_un_7z && self.tar.is_none() && self.sept_zip.is_none())
            .then(|| format!("Pour installer {}, {SANS_EXTRACTEUR_7Z}", moteur.nom))
    }
}

/// La fiche d'un dossier de moteur ; rien si elle manque ou ne se lit pas.
pub fn fiche(dossier: &Path) -> Option<Fiche> {
    let texte = fs::read_to_string(dossier.join(FICHE)).ok()?;
    serde_json::from_str(&texte).ok()
}

/// L'archive est-elle déjà là, à sa taille ? (Son empreinte est vérifiée au moment de s'en servir.)
fn deja_la(archive: &Archive, archives: &Path) -> bool {
    fs::metadata(archives.join(&archive.fichier)).is_ok_and(|m| m.is_file() && m.len() == archive.taille)
}

/// Octets à télécharger pour ce moteur.
pub fn a_telecharger(moteur: &Moteur, archives: &Path) -> u64 {
    moteur
        .archives
        .iter()
        .filter(|a| !deja_la(a, archives))
        .map(|a| a.taille)
        .sum()
}

/// La place disque qu'il faut pour installer ce moteur : ce qui reste à télécharger, plus trois fois ses archives.
pub fn place_necessaire(moteur: &Moteur, archives: &Path) -> u64 {
    let total: u64 = moteur.archives.iter().map(|a| a.taille).sum();
    a_telecharger(moteur, archives).saturating_add(total.saturating_mul(EXPANSION))
}

/// La version d'un ComfyUI portable, lue dans `ComfyUI\comfyui_version.py` (`__version__ = "0.42.1"`).
pub fn version_comfyui(dossier: &Path) -> Option<String> {
    let texte = fs::read_to_string(dossier.join("ComfyUI").join("comfyui_version.py")).ok()?;
    texte.lines().find_map(|ligne| {
        let reste = ligne.trim_start().strip_prefix("__version__")?.trim_start();
        let reste = reste.strip_prefix('=')?.trim_start().strip_prefix('"')?;
        let fin = reste.find('"')?;
        reste.get(..fin).map(str::to_owned)
    })
}

/// L'état de chaque moteur du catalogue, dans l'ordre du catalogue.
///
/// `externe` : le ComfyUI de l'utilisateur, s'il en a choisi un. `occupe` dit si un programme écoute sur un port
/// (`comfyui`, `llama`), une seule fois par port.
pub fn etat(
    catalogue: &Catalogue,
    moteurs: &Path,
    externe: Option<&Path>,
    extracteurs: &Extracteurs,
    occupe: impl Fn(&str) -> bool,
) -> Vec<EtatMoteur> {
    let archives = moteurs.join(DOSSIER_ARCHIVES);
    let mut ports: BTreeMap<&str, bool> = BTreeMap::new();
    catalogue
        .moteurs
        .iter()
        .map(|(id, m)| {
            let dossier = moteurs.join(&m.dossier);
            let pris = *ports.entry(m.port.as_str()).or_insert_with(|| occupe(&m.port));
            let externe = if id == "comfyui" { externe } else { None };
            let (statut, version_installee) = match externe {
                Some(chemin) => (Statut::Externe, version_comfyui(chemin)),
                None if !dossier.join(&m.preuve).is_file() => (Statut::Absent, None),
                None => match fiche(&dossier) {
                    None => (Statut::Manuel, None),
                    Some(f) if f.version == m.version => (Statut::AJour, Some(f.version)),
                    Some(f) => (Statut::Different, Some(f.version)),
                },
            };
            let prerequis = match statut {
                Statut::AJour | Statut::Externe => None,
                _ => extracteurs.prerequis(m),
            };
            EtatMoteur {
                id: id.clone(),
                nom: m.nom.clone(),
                role: m.role.clone(),
                facultatif: m.facultatif,
                dossier: m.dossier.clone(),
                version: m.version.clone(),
                version_installee,
                statut,
                chemin: externe.unwrap_or(dossier.as_path()).display().to_string(),
                externe: externe.is_some(),
                precedente: suffixe(&dossier, SUFFIXE_PRECEDENT).is_dir(),
                a_telecharger: a_telecharger(m, &archives),
                occupe: pris,
                prerequis,
            }
        })
        .collect()
}

/// Le SHA-256 d'un fichier, en hexadécimal minuscule.
fn empreinte(chemin: &Path) -> io::Result<String> {
    let mut fichier = File::open(chemin)?;
    let mut hacheur = Sha256::new();
    let mut tampon = vec![0u8; BLOC];
    loop {
        let lus = fichier.read(&mut tampon)?;
        if lus == 0 {
            break;
        }
        hacheur.update(tampon.get(..lus).unwrap_or_default());
    }
    Ok(hacheur.finalize().iter().map(|o| format!("{o:02x}")).collect())
}

/// L'archive vérifiée dans `archives`, téléchargée au besoin (avec reprise).
///
/// `progression(étape, fait, total)` est appelée au fil de l'eau ; si elle rend faux pendant le téléchargement,
/// celui-ci s'arrête, le `.part` est gardé et l'erreur est [`ANNULE`].
///
/// # Errors
/// Le message à montrer : réponse de GitHub inattendue, empreinte fausse, disque.
pub fn obtenir(
    archive: &Archive,
    archives: &Path,
    reseau: &impl Reseau,
    progression: &mut impl FnMut(&str, u64, u64) -> bool,
) -> Result<PathBuf, String> {
    let cible = archives.join(&archive.fichier);
    if deja_la(archive, archives) {
        let _ = progression(&format!("Vérification de {}", archive.fichier), 0, archive.taille);
        if empreinte(&cible).map_err(|e| disque(&cible, &e))? == archive.sha256 {
            return Ok(cible);
        }
        fs::remove_file(&cible).map_err(|e| disque(&cible, &e))?;
    }
    fs::create_dir_all(archives).map_err(|e| disque(archives, &e))?;
    let part = suffixe(&cible, SUFFIXE_PART);
    telecharger(archive, &part, reseau, progression)?;
    let obtenue = empreinte(&part).map_err(|e| disque(&part, &e))?;
    if obtenue != archive.sha256 {
        let _ = fs::remove_file(&part);
        return Err(format!(
            "SHA-256 de {} invalide ({obtenue}) : fichier supprimé, rien n'est installé",
            archive.fichier
        ));
    }
    fs::rename(&part, &cible).map_err(|e| disque(&cible, &e))?;
    Ok(cible)
}

/// Télécharge l'archive dans `part`, à la suite de ce qu'il contient déjà.
fn telecharger(
    archive: &Archive,
    part: &Path,
    reseau: &impl Reseau,
    progression: &mut impl FnMut(&str, u64, u64) -> bool,
) -> Result<(), String> {
    let etape = format!("Téléchargement de {}", archive.fichier);
    let mut deja = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if deja > archive.taille {
        fs::remove_file(part).map_err(|e| disque(part, &e))?;
        deja = 0;
    }
    let (statut, mut corps) = reseau
        .ouvrir(&archive.adresse(), deja)
        .map_err(|e| format!("Téléchargement de {} impossible : {e}", archive.fichier))?;
    match statut {
        200 => deja = 0,
        206 if deja > 0 => {}
        _ => return Err(format!("GitHub a répondu {statut} pour {}", archive.fichier)),
    }
    let mut sortie = OpenOptions::new()
        .create(true)
        .write(true)
        .append(deja > 0)
        .truncate(deja == 0)
        .open(part)
        .map_err(|e| disque(part, &e))?;
    let mut tampon = vec![0u8; BLOC];
    loop {
        if !progression(&etape, deja, archive.taille) {
            return Err(ANNULE.to_owned());
        }
        let lus = corps
            .read(&mut tampon)
            .map_err(|e| format!("Téléchargement de {} interrompu : {e}", archive.fichier))?;
        if lus == 0 {
            break;
        }
        sortie
            .write_all(tampon.get(..lus).unwrap_or_default())
            .map_err(|e| disque(part, &e))?;
        deja = deja.saturating_add(u64::try_from(lus).unwrap_or(u64::MAX));
        if deja > archive.taille {
            break;
        }
    }
    sortie.sync_all().map_err(|e| disque(part, &e))
}

/// La dernière ligne utile de la sortie d'un outil (7-Zip ajoute « Error exit delayed… » à la fin).
fn erreur_utile(texte: &str) -> Option<String> {
    texte
        .lines()
        .map(str::trim)
        .rev()
        .find(|l| !l.is_empty() && !l.contains("Error exit delayed"))
        .map(str::to_owned)
}

/// Vide un dossier en le recréant.
fn recreer(dossier: &Path) -> io::Result<()> {
    if dossier.exists() {
        fs::remove_dir_all(dossier)?;
    }
    fs::create_dir_all(dossier)
}

/// Extrait l'archive dans `vers` : un zip directement, un 7z avec le premier outil qui y arrive.
///
/// # Errors
/// Le message à montrer, avec la vraie erreur de chaque outil essayé.
pub fn extraire(
    archive: &Path,
    vers: &Path,
    extracteurs: &Extracteurs,
    lanceur: &impl Lanceur,
) -> Result<(), String> {
    let nom = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    fs::create_dir_all(vers).map_err(|e| disque(vers, &e))?;
    if nom.to_lowercase().ends_with(".zip") {
        return mises_a_jour::extraire(archive, vers)
            .map_err(|e| format!("Extraction de {nom} impossible : {e}"));
    }
    let commandes = extracteurs.commandes(archive, vers);
    if commandes.is_empty() {
        return Err(format!("Pour extraire {nom}, {SANS_EXTRACTEUR_7Z}"));
    }
    let mut erreurs = Vec::new();
    for commande in &commandes {
        let outil = commande
            .first()
            .and_then(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let cause = match lanceur.lancer(commande) {
            Ok(s) if s.reussi => return Ok(()),
            Ok(s) => erreur_utile(&s.erreurs)
                .or_else(|| erreur_utile(&s.sortie))
                .unwrap_or_else(|| {
                    format!(
                        "code {}",
                        s.code.map_or_else(|| "inconnu".to_owned(), |c| c.to_string())
                    )
                }),
            Err(e) => e.to_string(),
        };
        erreurs.push(format!("{outil} : {cause}"));
        // L'outil suivant repart d'un dossier vide.
        recreer(vers).map_err(|e| disque(vers, &e))?;
    }
    Err(format!(
        "Extraction de {nom} impossible ({})",
        erreurs.join(" ; ")
    ))
}

/// Déplace le contenu de `source` dans `cible` : les dossiers se fusionnent, un fichier remplace celui qui était là.
fn verser(source: &Path, cible: &Path) -> io::Result<()> {
    fs::create_dir_all(cible)?;
    for entree in fs::read_dir(source)? {
        let entree = entree?;
        let de = entree.path();
        let vers = cible.join(entree.file_name());
        if entree.file_type()?.is_dir() && vers.is_dir() {
            verser(&de, &vers)?;
        } else {
            if vers.is_file() {
                fs::remove_file(&vers)?;
            }
            fs::rename(&de, &vers)?;
        }
    }
    Ok(())
}

/// Un moteur prêt à être mis en place : son dossier `.new` et sa fiche.
#[derive(Debug)]
pub struct Prepare {
    dossier: PathBuf,
    nouveau: PathBuf,
    nom: String,
    fiche: Fiche,
}

/// Télécharge (avec reprise), vérifie et extrait les archives du moteur dans `<dossier>.new`, sans toucher au
/// moteur en place. `libre` : la place libre du disque des moteurs, si l'appelant la connaît. `occupe` : un
/// programme écoute-t-il sur le port du moteur ?
///
/// # Errors
/// Le message à montrer ; tout reste d'extraction est effacé, le moteur en place n'a pas changé.
// Neuf paramètres : le réseau, les outils, la place libre et l'état du port viennent de l'appelant pour que la
// préparation se teste sans réseau ni 7-Zip ; les regrouper dans un type n'apporterait rien d'autre.
#[allow(clippy::too_many_arguments)]
pub fn preparer(
    catalogue: &Catalogue,
    id: &str,
    moteurs: &Path,
    libre: Option<u64>,
    occupe: bool,
    reseau: &impl Reseau,
    extracteurs: &Extracteurs,
    lanceur: &impl Lanceur,
    progression: &mut impl FnMut(&str, u64, u64) -> bool,
) -> Result<Prepare, String> {
    let moteur = moteur(catalogue, id)?;
    if occupe {
        return Err(format!("{} tourne : arrête-le avant de l'installer", moteur.nom));
    }
    if let Some(manque) = extracteurs.prerequis(moteur) {
        return Err(manque);
    }
    fs::create_dir_all(moteurs).map_err(|e| disque(moteurs, &e))?;
    let archives = moteurs.join(DOSSIER_ARCHIVES);
    let besoin = place_necessaire(moteur, &archives);
    if let Some(libre) = libre.filter(|&l| l < besoin) {
        return Err(format!(
            "Place insuffisante : {:.1} Go nécessaires, {:.1} Go libres",
            go(besoin),
            go(libre)
        ));
    }
    let mut fichiers = Vec::new();
    for archive in &moteur.archives {
        fichiers.push((archive, obtenir(archive, &archives, reseau, progression)?));
    }
    let dossier = moteurs.join(&moteur.dossier);
    let nouveau = suffixe(&dossier, SUFFIXE_NOUVEAU);
    let extrait = suffixe(&dossier, SUFFIXE_EXTRAIT);
    let resultat = (|| {
        for reste in [&nouveau, &extrait] {
            if reste.exists() {
                fs::remove_dir_all(reste).map_err(|e| disque(reste, &e))?;
            }
        }
        for (archive, fichier) in &fichiers {
            let _ = progression(&format!("Extraction de {}", archive.fichier), 0, 0);
            extraire(fichier, &extrait, extracteurs, lanceur)?;
            let source = match &archive.racine {
                Some(racine) => extrait.join(racine),
                None => extrait.clone(),
            };
            if !source.is_dir() {
                return Err(format!(
                    "{} ne contient pas le dossier {}",
                    archive.fichier,
                    archive.racine.as_deref().unwrap_or_default()
                ));
            }
            verser(&source, &nouveau).map_err(|e| disque(&nouveau, &e))?;
            fs::remove_dir_all(&extrait).map_err(|e| disque(&extrait, &e))?;
        }
        if !nouveau.join(&moteur.preuve).is_file() {
            return Err(format!(
                "{} absent après extraction : l'archive ne ressemble pas à {}",
                moteur.preuve, moteur.nom
            ));
        }
        let fiche = Fiche {
            id: id.to_owned(),
            version: moteur.version.clone(),
            installe_le: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            archives: moteur
                .archives
                .iter()
                .map(|a| (a.fichier.clone(), a.sha256.clone()))
                .collect(),
        };
        let texte = serde_json::to_string_pretty(&fiche).map_err(|e| e.to_string())?;
        fs::write(nouveau.join(FICHE), texte).map_err(|e| disque(&nouveau, &e))?;
        Ok(fiche)
    })();
    if extrait.exists() {
        let _ = fs::remove_dir_all(&extrait);
    }
    match resultat {
        Ok(fiche) => Ok(Prepare {
            dossier,
            nouveau,
            nom: moteur.nom.clone(),
            fiche,
        }),
        Err(e) => {
            if nouveau.exists() {
                let _ = fs::remove_dir_all(&nouveau);
            }
            Err(e)
        }
    }
}

/// Met en place le moteur préparé : l'ancien dossier devient `<dossier>.prev`, le nouveau prend sa place. À appeler
/// sous le verrou de l'orchestrateur, moteur arrêté.
///
/// # Errors
/// Un programme écoute encore sur le port du moteur, ou un fichier de l'ancien dossier est ouvert : rien n'a changé
/// et `.new` est effacé.
pub fn mettre_en_place(prepare: Prepare, occupe: bool) -> Result<Fiche, String> {
    let resultat = if occupe {
        Err(format!(
            "{} tourne encore (lancé par une autre appli ?) : rien n'a changé",
            prepare.nom
        ))
    } else {
        echanger(&prepare.dossier, &prepare.nouveau)
    };
    if prepare.nouveau.exists() {
        let _ = fs::remove_dir_all(&prepare.nouveau);
    }
    resultat.map(|()| prepare.fiche)
}

fn echanger(dossier: &Path, nouveau: &Path) -> Result<(), String> {
    let precedent = suffixe(dossier, SUFFIXE_PRECEDENT);
    let nom = dossier
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if dossier.exists() {
        if precedent.exists() {
            fs::remove_dir_all(&precedent).map_err(|e| disque(&precedent, &e))?;
        }
        fs::rename(dossier, &precedent).map_err(|_| {
            format!("{nom} est en cours d'utilisation (un fichier y est ouvert) : rien n'a changé")
        })?;
    }
    fs::rename(nouveau, dossier).map_err(|e| {
        // Le nouveau n'a pas pu prendre la place : l'ancien revient.
        if !dossier.exists() && precedent.exists() {
            let _ = fs::rename(&precedent, dossier);
        }
        disque(dossier, &e)
    })
}

/// Revient à la version précédente du moteur (échange avec `<dossier>.prev`) et rend la fiche de celle qui revient
/// (rien pour une version posée à la main). Un second appel revient à la version d'après.
///
/// # Errors
/// Pas de version précédente, moteur en marche, ou un fichier ouvert : rien n'a changé.
pub fn revenir(
    catalogue: &Catalogue,
    id: &str,
    moteurs: &Path,
    occupe: bool,
) -> Result<Option<Fiche>, String> {
    let moteur = moteur(catalogue, id)?;
    let dossier = moteurs.join(&moteur.dossier);
    let precedent = suffixe(&dossier, SUFFIXE_PRECEDENT);
    let echange = suffixe(&dossier, SUFFIXE_ECHANGE);
    if !precedent.is_dir() {
        return Err(format!("Pas de version précédente de {}", moteur.nom));
    }
    if occupe {
        return Err(format!("{} tourne : arrête-le avant", moteur.nom));
    }
    let resultat = (|| {
        fs::rename(&dossier, &echange)?;
        fs::rename(&precedent, &dossier)?;
        fs::rename(&echange, &precedent)
    })();
    if resultat.is_err() {
        if echange.exists() && !dossier.exists() {
            let _ = fs::rename(&echange, &dossier);
        }
        return Err(format!(
            "{} est en cours d'utilisation : rien n'a changé",
            moteur.nom
        ));
    }
    Ok(fiche(&dossier))
}

fn moteur<'c>(catalogue: &'c Catalogue, id: &str) -> Result<&'c Moteur, String> {
    catalogue.moteur(id).ok_or_else(|| {
        let connus: Vec<&str> = catalogue.moteurs.keys().map(String::as_str).collect();
        format!("Moteur inconnu : {id} (connus : {})", connus.join(", "))
    })
}

/// `chemin` suivi d'un suffixe (`comfyui` → `comfyui.prev`).
fn suffixe(chemin: &Path, ajout: &str) -> PathBuf {
    let mut nom = chemin.as_os_str().to_owned();
    nom.push(ajout);
    PathBuf::from(nom)
}

fn disque(chemin: &Path, erreur: &io::Error) -> String {
    format!("{} : {erreur}", chemin.display())
}

/// Des octets en gigaoctets (Gio), pour un message.
fn go(octets: u64) -> f64 {
    octets as f64 / 1_073_741_824.0
}

/// Les outils d'extraction de ce PC, et leur lancement sans fenêtre de console.
#[cfg(windows)]
pub mod systeme {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::OnceLock;

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

    use super::{Extracteurs, Lanceur, Sortie};

    /// Pas de fenêtre de console pour les outils lancés.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Lance les outils pour de vrai.
    pub struct Systeme;

    impl Lanceur for Systeme {
        fn lancer(&self, commande: &[OsString]) -> io::Result<Sortie> {
            let (programme, arguments) = commande
                .split_first()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "commande vide"))?;
            let sortie = Command::new(programme)
                .args(arguments)
                .creation_flags(CREATE_NO_WINDOW)
                .output()?;
            Ok(Sortie {
                reussi: sortie.status.success(),
                code: sortie.status.code(),
                sortie: String::from_utf8_lossy(&sortie.stdout).into_owned(),
                erreurs: String::from_utf8_lossy(&sortie.stderr).into_owned(),
            })
        }
    }

    /// Le `tar` de Windows, s'il lit le LZMA (celui de Windows 11 ; pas celui de Windows 10). Regardé une fois.
    fn tar() -> Option<PathBuf> {
        static TAR: OnceLock<Option<PathBuf>> = OnceLock::new();
        TAR.get_or_init(|| {
            let windows = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
            let tar = PathBuf::from(windows).join("System32").join("tar.exe");
            let version = Systeme
                .lancer(&[tar.clone().into_os_string(), "--version".into()])
                .ok()?;
            version.sortie.contains("liblzma").then_some(tar)
        })
        .clone()
    }

    /// `7z.exe` : d'après le registre (machine puis utilisateur, vues 64 et 32 bits), puis Program Files. Jamais
    /// d'après le PATH.
    fn sept_zip() -> Option<PathBuf> {
        let mut candidats = Vec::new();
        for racine in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
            for vue in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
                let chemin: Option<String> = RegKey::predef(racine)
                    .open_subkey_with_flags("SOFTWARE\\7-Zip", KEY_READ | vue)
                    .and_then(|cle| cle.get_value("Path"))
                    .ok();
                if let Some(chemin) = chemin {
                    candidats.push(PathBuf::from(chemin).join("7z.exe"));
                }
            }
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(dossier) = std::env::var_os(variable) {
                candidats.push(PathBuf::from(dossier).join("7-Zip").join("7z.exe"));
            }
        }
        candidats.into_iter().find(|c| c.is_file())
    }

    /// Les outils d'extraction de ce PC.
    pub fn extracteurs() -> Extracteurs {
        Extracteurs {
            tar: tar(),
            sept_zip: sept_zip(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::io::Cursor;

    /// Un GitHub d'essai : des fichiers par adresse, qui sait répondre à partir d'un octet.
    #[derive(Default)]
    struct FauxGithub {
        fichiers: HashMap<String, Vec<u8>>,
        demandes: RefCell<Vec<(String, u64)>>,
        /// Coupe la réponse après ce nombre d'octets.
        coupure: Option<usize>,
    }

    impl Reseau for FauxGithub {
        fn ouvrir(&self, adresse: &str, depuis: u64) -> io::Result<(u16, Box<dyn Read>)> {
            self.demandes.borrow_mut().push((adresse.to_owned(), depuis));
            let Some(octets) = self.fichiers.get(adresse) else {
                return Ok((404, Box::new(Cursor::new(Vec::new()))));
            };
            let debut = usize::try_from(depuis).unwrap();
            let mut corps = octets[debut.min(octets.len())..].to_vec();
            if let Some(n) = self.coupure {
                corps.truncate(n);
            }
            Ok((if depuis > 0 { 206 } else { 200 }, Box::new(Cursor::new(corps))))
        }
    }

    /// Un lanceur d'essai, qui rend des sorties préparées et note ce qu'on lui demande.
    #[derive(Default)]
    struct FauxLanceur {
        sorties: RefCell<Vec<io::Result<Sortie>>>,
        lances: RefCell<Vec<Vec<OsString>>>,
    }

    impl Lanceur for FauxLanceur {
        fn lancer(&self, commande: &[OsString]) -> io::Result<Sortie> {
            self.lances.borrow_mut().push(commande.to_vec());
            if self.sorties.borrow().is_empty() {
                return Ok(Sortie::default());
            }
            self.sorties.borrow_mut().remove(0)
        }
    }

    fn sha(octets: &[u8]) -> String {
        Sha256::digest(octets)
            .iter()
            .map(|o| format!("{o:02x}"))
            .collect()
    }

    fn zip(fichiers: &[(&str, &str)]) -> Vec<u8> {
        let mut tampon = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut tampon);
            for (nom, contenu) in fichiers {
                z.start_file(*nom, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(contenu.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        tampon.into_inner()
    }

    /// Un catalogue d'essai : un serveur en deux zip (le serveur, puis son runtime), comme llama.
    fn catalogue(serveur: &[u8], runtime: &[u8], racine: Option<&str>) -> Catalogue {
        let racine = racine
            .map(|r| format!("\n              racine: {r}"))
            .unwrap_or_default();
        Catalogue::depuis_yaml(&format!(
            "format: 1
pilote_min: 580
moteurs:
    llama:
        nom: llama.cpp d'essai
        role: essai
        version: b2
        dossier: llama
        preuve: llama-server.exe
        port: llama
        archives:
            - depot: ggml-org/llama.cpp
              tag: b2
              fichier: serveur.zip
              taille: {}
              sha256: {}{racine}
            - depot: ggml-org/llama.cpp
              tag: b2
              fichier: runtime.zip
              taille: {}
              sha256: {}
",
            serveur.len(),
            sha(serveur),
            runtime.len(),
            sha(runtime)
        ))
        .unwrap()
    }

    fn github(serveur: &[u8], runtime: &[u8]) -> FauxGithub {
        let mut fichiers = HashMap::new();
        fichiers.insert(
            "https://github.com/ggml-org/llama.cpp/releases/download/b2/serveur.zip".to_owned(),
            serveur.to_vec(),
        );
        fichiers.insert(
            "https://github.com/ggml-org/llama.cpp/releases/download/b2/runtime.zip".to_owned(),
            runtime.to_vec(),
        );
        FauxGithub {
            fichiers,
            ..FauxGithub::default()
        }
    }

    fn sans_suite(_: &str, _: u64, _: u64) -> bool {
        true
    }

    fn installer(cat: &Catalogue, moteurs: &Path, reseau: &FauxGithub) -> Result<Fiche, String> {
        let prepare = preparer(
            cat,
            "llama",
            moteurs,
            None,
            false,
            reseau,
            &Extracteurs::default(),
            &FauxLanceur::default(),
            &mut sans_suite,
        )?;
        mettre_en_place(prepare, false)
    }

    fn restes(moteurs: &Path) -> Vec<String> {
        let mut noms: Vec<String> = fs::read_dir(moteurs)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".new") || n.ends_with(".extrait") || n.ends_with(".echange"))
            .collect();
        noms.sort();
        noms
    }

    #[test]
    fn installe_depuis_la_release_et_fusionne_le_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let serveur = zip(&[("llama-server.exe", "serveur"), ("ggml.dll", "ggml")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        let avant = etat(&cat, moteurs, None, &Extracteurs::default(), |_| false);
        assert_eq!(avant[0].statut, Statut::Absent);
        assert_eq!(avant[0].a_telecharger, (serveur.len() + runtime.len()) as u64);

        let fiche = installer(&cat, moteurs, &github(&serveur, &runtime)).unwrap();
        assert_eq!(fiche.version, "b2");
        for nom in ["llama-server.exe", "ggml.dll", "cudart64_13.dll", FICHE] {
            assert!(moteurs.join("llama").join(nom).is_file(), "{nom}");
        }
        let apres = etat(&cat, moteurs, None, &Extracteurs::default(), |_| false);
        assert_eq!(apres[0].statut, Statut::AJour);
        assert_eq!(apres[0].a_telecharger, 0);
        assert!(restes(moteurs).is_empty(), "{:?}", restes(moteurs));
    }

    #[test]
    fn les_archives_deja_la_ne_sont_pas_retelechargees() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        let archives = moteurs.join(DOSSIER_ARCHIVES);
        fs::create_dir_all(&archives).unwrap();
        fs::write(archives.join("serveur.zip"), &serveur).unwrap();
        fs::write(archives.join("runtime.zip"), &runtime).unwrap();
        let reseau = FauxGithub::default();
        installer(&cat, moteurs, &reseau).unwrap();
        assert!(reseau.demandes.borrow().is_empty());
    }

    #[test]
    fn une_archive_locale_abimee_est_retelechargee() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        let archives = moteurs.join(DOSSIER_ARCHIVES);
        fs::create_dir_all(&archives).unwrap();
        fs::write(archives.join("serveur.zip"), vec![0u8; serveur.len()]).unwrap();
        let reseau = github(&serveur, &runtime);
        installer(&cat, moteurs, &reseau).unwrap();
        assert!(
            reseau
                .demandes
                .borrow()
                .iter()
                .any(|(a, _)| a.ends_with("serveur.zip"))
        );
    }

    #[test]
    fn un_telechargement_coupe_reprend_ou_il_s_etait_arrete() {
        let temp = tempfile::tempdir().unwrap();
        let archives = temp.path().join(DOSSIER_ARCHIVES);
        fs::create_dir_all(&archives).unwrap();
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        let archive = &cat.moteur("llama").unwrap().archives[0];
        fs::write(archives.join("serveur.zip.part"), &serveur[..10]).unwrap();
        let reseau = github(&serveur, &runtime);
        let chemin = obtenir(archive, &archives, &reseau, &mut sans_suite).unwrap();
        assert_eq!(fs::read(chemin).unwrap(), serveur);
        assert_eq!(
            reseau.demandes.borrow()[0],
            (
                "https://github.com/ggml-org/llama.cpp/releases/download/b2/serveur.zip".to_owned(),
                10
            )
        );
        assert!(!archives.join("serveur.zip.part").exists());
    }

    #[test]
    fn une_empreinte_fausse_ne_laisse_rien() {
        let temp = tempfile::tempdir().unwrap();
        let archives = temp.path().join(DOSSIER_ARCHIVES);
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        let archive = &cat.moteur("llama").unwrap().archives[0];
        let mut reseau = github(&serveur, &runtime);
        reseau.coupure = Some(10);
        let erreur = obtenir(archive, &archives, &reseau, &mut sans_suite).unwrap_err();
        assert!(erreur.contains("SHA-256 de serveur.zip invalide"), "{erreur}");
        assert_eq!(fs::read_dir(&archives).unwrap().count(), 0);
    }

    #[test]
    fn annuler_garde_le_part_pour_reprendre() {
        let temp = tempfile::tempdir().unwrap();
        let archives = temp.path().join(DOSSIER_ARCHIVES);
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        let archive = &cat.moteur("llama").unwrap().archives[0];
        let reseau = github(&serveur, &runtime);
        let mut appels = 0;
        let mut progression = |_: &str, _: u64, _: u64| {
            appels += 1;
            appels < 2
        };
        let erreur = obtenir(archive, &archives, &reseau, &mut progression).unwrap_err();
        assert_eq!(erreur, ANNULE);
        assert!(archives.join("serveur.zip.part").is_file());
        assert!(!archives.join("serveur.zip").exists());
    }

    #[test]
    fn la_racine_de_l_archive_est_remontee() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let serveur = zip(&[
            ("Portable/llama-server.exe", "serveur"),
            ("Portable/lib/a.dll", "a"),
        ]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, Some("Portable"));
        installer(&cat, moteurs, &github(&serveur, &runtime)).unwrap();
        assert!(moteurs.join("llama").join("llama-server.exe").is_file());
        assert!(moteurs.join("llama").join("lib").join("a.dll").is_file());
    }

    #[test]
    fn une_racine_absente_ne_change_rien() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, Some("Portable"));
        fs::create_dir_all(moteurs.join("llama")).unwrap();
        fs::write(moteurs.join("llama").join("ancien.txt"), "là").unwrap();
        let erreur = installer(&cat, moteurs, &github(&serveur, &runtime)).unwrap_err();
        assert!(erreur.contains("ne contient pas le dossier Portable"), "{erreur}");
        assert!(moteurs.join("llama").join("ancien.txt").is_file());
        assert!(restes(moteurs).is_empty(), "{:?}", restes(moteurs));
    }

    #[test]
    fn un_chemin_dangereux_dans_un_zip_est_refuse() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path().join("moteurs");
        let serveur = zip(&[("../../evade.txt", "dehors"), ("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        assert!(installer(&cat, &moteurs, &github(&serveur, &runtime)).is_err());
        assert!(!temp.path().join("evade.txt").exists());
    }

    #[test]
    fn reinstaller_garde_la_precedente_et_revenir_echange() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        fs::create_dir_all(moteurs.join("llama")).unwrap();
        fs::write(moteurs.join("llama").join("llama-server.exe"), "v1").unwrap();
        assert_eq!(
            etat(&cat, moteurs, None, &Extracteurs::default(), |_| false)[0].statut,
            Statut::Manuel
        );

        installer(&cat, moteurs, &github(&serveur, &runtime)).unwrap();
        assert!(etat(&cat, moteurs, None, &Extracteurs::default(), |_| false)[0].precedente);
        assert_eq!(revenir(&cat, "llama", moteurs, false).unwrap(), None);
        assert_eq!(
            fs::read_to_string(moteurs.join("llama").join("llama-server.exe")).unwrap(),
            "v1"
        );
        let fiche = revenir(&cat, "llama", moteurs, false).unwrap().unwrap();
        assert_eq!(fiche.version, "b2");
    }

    #[test]
    fn une_autre_version_est_dite_differente() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let cat = catalogue(b"s", b"r", None);
        let dossier = moteurs.join("llama");
        fs::create_dir_all(&dossier).unwrap();
        fs::write(dossier.join("llama-server.exe"), "x").unwrap();
        fs::write(
            dossier.join(FICHE),
            r#"{"id": "llama", "version": "b1", "installe_le": "2026-11-01T10:00:00", "archives": {}}"#,
        )
        .unwrap();
        let e = &etat(&cat, moteurs, None, &Extracteurs::default(), |_| false)[0];
        assert_eq!(e.statut, Statut::Different);
        assert_eq!(e.version_installee.as_deref(), Some("b1"));
    }

    #[test]
    fn revenir_sans_precedente_est_refuse() {
        let temp = tempfile::tempdir().unwrap();
        let cat = catalogue(b"s", b"r", None);
        assert!(
            revenir(&cat, "llama", temp.path(), false)
                .unwrap_err()
                .contains("Pas de version précédente")
        );
    }

    #[test]
    fn un_moteur_en_marche_n_est_pas_installe() {
        let temp = tempfile::tempdir().unwrap();
        let cat = catalogue(b"s", b"r", None);
        let reseau = FauxGithub::default();
        let erreur = preparer(
            &cat,
            "llama",
            temp.path(),
            None,
            true,
            &reseau,
            &Extracteurs::default(),
            &FauxLanceur::default(),
            &mut sans_suite,
        )
        .unwrap_err();
        assert!(erreur.contains("tourne"), "{erreur}");
        assert!(reseau.demandes.borrow().is_empty());
    }

    #[test]
    fn un_port_encore_pris_a_la_mise_en_place_ne_change_rien() {
        let temp = tempfile::tempdir().unwrap();
        let moteurs = temp.path();
        let serveur = zip(&[("llama-server.exe", "serveur")]);
        let runtime = zip(&[("cudart64_13.dll", "cuda")]);
        let cat = catalogue(&serveur, &runtime, None);
        let prepare = preparer(
            &cat,
            "llama",
            moteurs,
            None,
            false,
            &github(&serveur, &runtime),
            &Extracteurs::default(),
            &FauxLanceur::default(),
            &mut sans_suite,
        )
        .unwrap();
        let erreur = mettre_en_place(prepare, true).unwrap_err();
        assert!(erreur.contains("tourne encore"), "{erreur}");
        assert!(!moteurs.join("llama").exists());
        assert!(restes(moteurs).is_empty(), "{:?}", restes(moteurs));
    }

    #[test]
    fn la_place_insuffisante_est_dite_avant_tout_telechargement() {
        let temp = tempfile::tempdir().unwrap();
        let cat = catalogue(b"serveur", b"runtime", None);
        let reseau = FauxGithub::default();
        let erreur = preparer(
            &cat,
            "llama",
            temp.path(),
            Some(10),
            false,
            &reseau,
            &Extracteurs::default(),
            &FauxLanceur::default(),
            &mut sans_suite,
        )
        .unwrap_err();
        assert!(erreur.starts_with("Place insuffisante"), "{erreur}");
        assert!(reseau.demandes.borrow().is_empty());
    }

    #[test]
    fn un_7z_essaie_tar_puis_7zip_avec_la_vraie_erreur_de_chacun() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("ComfyUI.7z");
        fs::write(&archive, "7z").unwrap();
        let vers = temp.path().join("extrait");
        let extracteurs = Extracteurs {
            tar: Some(PathBuf::from("C:/Windows/System32/tar.exe")),
            sept_zip: Some(PathBuf::from("C:/Program Files/7-Zip/7z.exe")),
        };
        let lanceur = FauxLanceur::default();
        lanceur.sorties.borrow_mut().push(Ok(Sortie {
            reussi: false,
            code: Some(1),
            sortie: String::new(),
            erreurs: "tar.exe: Unsupported compression\n".to_owned(),
        }));
        lanceur.sorties.borrow_mut().push(Ok(Sortie {
            reussi: false,
            code: Some(2),
            sortie: "ERROR: Data Error : ComfyUI/main.py\n\nSub items Errors: 1\n".to_owned(),
            erreurs: "ERROR: Data Error : ComfyUI/main.py\nError exit delayed\n".to_owned(),
        }));
        let erreur = extraire(&archive, &vers, &extracteurs, &lanceur).unwrap_err();
        assert!(
            erreur.contains("tar.exe : tar.exe: Unsupported compression"),
            "{erreur}"
        );
        assert!(
            erreur.contains("7z.exe : ERROR: Data Error : ComfyUI/main.py"),
            "{erreur}"
        );
        assert!(!erreur.contains("Error exit delayed"), "{erreur}");
        let lances = lanceur.lances.borrow();
        assert_eq!(lances.len(), 2);
        assert!(lances[0][0].to_string_lossy().ends_with("tar.exe"));
        assert!(lances[1][0].to_string_lossy().ends_with("7z.exe"));
    }

    #[test]
    fn un_7z_sans_outil_demande_7zip_avant_tout_telechargement() {
        let mut cat = catalogue(b"serveur", b"runtime", None);
        if let Some(m) = cat.moteurs.get_mut("llama") {
            m.archives[0].fichier = "ComfyUI_portable.7z".to_owned();
        }
        let m = cat.moteur("llama").unwrap();
        let rien = Extracteurs::default();
        assert!(rien.prerequis(m).unwrap().contains("installe 7-Zip"));
        let avec_7z = Extracteurs {
            sept_zip: Some(PathBuf::from("7z.exe")),
            ..Extracteurs::default()
        };
        assert_eq!(avec_7z.prerequis(m), None);
        let temp = tempfile::tempdir().unwrap();
        let reseau = FauxGithub::default();
        let erreur = preparer(
            &cat,
            "llama",
            temp.path(),
            None,
            false,
            &reseau,
            &rien,
            &FauxLanceur::default(),
            &mut sans_suite,
        )
        .unwrap_err();
        assert!(erreur.contains("installe 7-Zip"), "{erreur}");
        assert!(reseau.demandes.borrow().is_empty());
    }

    #[test]
    fn la_version_d_un_comfyui_se_lit() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("ComfyUI")).unwrap();
        fs::write(
            temp.path().join("ComfyUI").join("comfyui_version.py"),
            "# généré\n__version__ = \"0.42.1\"\n",
        )
        .unwrap();
        assert_eq!(version_comfyui(temp.path()).as_deref(), Some("0.42.1"));
        assert_eq!(version_comfyui(&temp.path().join("absent")), None);
    }

    #[test]
    fn le_catalogue_du_depot_donne_un_etat_pour_chaque_moteur() {
        let temp = tempfile::tempdir().unwrap();
        let cat = Catalogue::embarque().unwrap();
        let etats = etat(&cat, temp.path(), None, &Extracteurs::default(), |port| {
            port == "llama"
        });
        assert_eq!(etats.len(), 3);
        assert!(etats.iter().all(|e| e.statut == Statut::Absent));
        let comfyui = etats.iter().find(|e| e.id == "comfyui").unwrap();
        assert!(!comfyui.occupe);
        assert!(comfyui.prerequis.as_deref().unwrap_or_default().contains("7-Zip"));
        assert!(
            etats
                .iter()
                .filter(|e| e.id.starts_with("llama"))
                .all(|e| e.occupe)
        );
    }

    #[test]
    fn le_statut_s_ecrit_comme_en_v2() {
        assert_eq!(serde_json::to_string(&Statut::AJour).unwrap(), "\"a_jour\"");
        assert_eq!(serde_json::to_string(&Statut::Externe).unwrap(), "\"externe\"");
    }
}
