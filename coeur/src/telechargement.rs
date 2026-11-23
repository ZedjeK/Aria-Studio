//! La file de téléchargement des modèles, comme en v2 (`app/telechargement.py`) : un fichier à la fois, reprise du
//! `.part` (`Range`), cinq essais en cas de coupure, vérification de la taille et du SHA-256, puis inscription au
//! registre `data\modeles.json` (le même que la v2). Une source qui refuse l'accès le dit avec ce qu'il faut faire.
//!
//! Les fichiers viennent des fiches des workflows et du catalogue des LLM (adresses, tailles et empreintes figées), ou
//! de « Rechercher » et d'un lien collé (`decouvrir`), lus sur Hugging Face et Civitai.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::{Dossier, journal, modeles};

/// Taille d'un bloc lu.
const BLOC: usize = 1 << 20;
/// La place gardée libre sur le disque des modèles : 1 Gio.
pub const MARGE_DISQUE: u64 = 1 << 30;
/// Les essais après une coupure du réseau, avec une attente qui double.
pub const ESSAIS_RESEAU: u32 = 5;

/// Ce qu'il faut faire quand une source refuse l'accès (la clé elle-même n'est jamais citée).
fn acces_refuse(source: &str) -> String {
    match source {
        "civitai" => {
            "Civitai demande une clé d'API pour ce modèle : crée-la sur civitai.com (Account settings, API \
                      Keys), puis colle-la dans Réglages → Emplacements et clés"
                .to_owned()
        }
        "huggingface" => {
            "Hugging Face refuse l'accès : accepte les conditions sur la page du modèle, et colle ta clé \
                          (Settings, Access Tokens) dans Réglages → Emplacements et clés"
                .to_owned()
        }
        _ => "Accès refusé par la source".to_owned(),
    }
}

/// Un fichier distant à télécharger.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Fiche {
    /// `huggingface` ou `civitai`.
    pub source: String,
    pub nom: String,
    pub taille: u64,
    pub sha256: Option<String>,
    pub url: String,
    pub page: String,
    pub licence: Option<String>,
    /// LoRA de Civitai : les mots qui la déclenchent.
    #[serde(default)]
    pub mots_declencheurs: Vec<String>,
    /// Le modèle de base annoncé par la source (Civitai « baseModel »).
    #[serde(default)]
    pub base: Option<String>,
    /// LoRA : la force médiane de ses images d'exemple, combien la donnent, et un de leurs prompts.
    #[serde(default)]
    pub force_auteur: Option<f64>,
    #[serde(default)]
    pub images_force: u32,
    #[serde(default)]
    pub prompt_exemple: Option<String>,
    /// Checkpoint : les steps et le CFG lus sur sa page (`suggestions`), jamais appliqués seuls.
    #[serde(default)]
    pub steps_suggeres: Option<Value>,
    #[serde(default)]
    pub cfg_suggere: Option<Value>,
}

/// Où en est un téléchargement : « attente », « en_cours », « verification », « termine », « erreur », « annule ».
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tache {
    pub id: String,
    pub nom: String,
    pub dossier: String,
    pub taille: u64,
    pub etat: String,
    pub recu: u64,
    /// Octets par seconde.
    pub debit: f64,
    pub erreur: Option<String>,
    #[serde(skip)]
    fiche: Option<Fiche>,
}

/// Ce qui ouvre une adresse : HTTPS avec les certificats de Windows dans l'appli, un faux dans les tests.
pub trait Reseau: Send + Sync + 'static {
    /// Ouvre l'adresse en GET avec ces en-têtes, à partir de l'octet `depuis` (`Range`) s'il n'est pas nul. Rend le
    /// statut HTTP et le corps.
    ///
    /// # Errors
    /// Une coupure ou une connexion impossible (réessayée).
    fn ouvrir(
        &self,
        adresse: &str,
        entetes: &[(String, String)],
        depuis: u64,
    ) -> io::Result<(u16, Box<dyn Read>)>;
}

/// Les en-têtes HTTP d'une source.
pub type Entetes = Box<dyn Fn(&str) -> Vec<(String, String)> + Send + Sync>;
/// La place libre sur le disque d'un dossier, en octets.
pub type EspaceLibre = Box<dyn Fn(&Path) -> Option<u64> + Send + Sync>;

/// Ce dont la file a besoin autour d'elle.
pub struct Contexte {
    pub dossier: Dossier,
    pub data: PathBuf,
    pub modeles: PathBuf,
    /// Les en-têtes d'une source (`Authorization` avec sa clé, s'il y en a une).
    pub entetes: Entetes,
    /// La place libre sur le disque d'un dossier, en octets (rien : inconnue).
    pub espace_libre: EspaceLibre,
}

/// Le registre des modèles téléchargés par Aria (`data\modeles.json`, au format de la v2).
pub fn registre(data: &Path) -> Map<String, Value> {
    fs::read_to_string(data.join("modeles.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Un seul fil à la fois modifie le registre : la file, la corbeille, Retrouver les sources.
static VERROU_REGISTRE: Mutex<()> = Mutex::new(());

/// Modifie le registre sous verrou : relu, modifié, réécrit d'un coup.
///
/// # Errors
/// Le registre ne s'écrit pas.
pub fn modifier_registre<T>(
    data: &Path,
    modifier: impl FnOnce(&mut Map<String, Value>) -> T,
) -> io::Result<T> {
    let _verrou = VERROU_REGISTRE.lock().unwrap_or_else(PoisonError::into_inner);
    let mut r = registre(data);
    let sortie = modifier(&mut r);
    ecrire_registre(data, &r)?;
    Ok(sortie)
}

fn ecrire_registre(data: &Path, registre: &Map<String, Value>) -> io::Result<()> {
    fs::create_dir_all(data)?;
    let chemin = data.join("modeles.json");
    let temporaire = data.join("modeles.json.tmp");
    let mut octets = Vec::new();
    let formateur = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serialiseur = serde_json::Serializer::with_formatter(&mut octets, formateur);
    Value::Object(registre.clone())
        .serialize(&mut serialiseur)
        .map_err(io::Error::other)?;
    fs::write(&temporaire, octets)?;
    fs::rename(&temporaire, &chemin)
}

/// Le fichier installé qui a cette empreinte (registre, ou cache des empreintes calculées de la v2), s'il existe.
pub fn cle_par_empreinte(contexte: &Contexte, sha256: &str) -> Option<String> {
    let voulu = sha256.to_lowercase();
    let du_registre = registre(&contexte.data).into_iter().filter_map(|(cle, i)| {
        let s = i.get("sha256")?.as_str()?.to_lowercase();
        Some((s, cle))
    });
    let du_cache = fs::read_to_string(contexte.dossier.cache().join("empreintes.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(cle, e)| Some((e.get("sha256")?.as_str()?.to_lowercase(), cle)));
    du_registre
        .chain(du_cache)
        .find(|(s, cle)| *s == voulu && contexte.modeles.join(cle).is_file())
        .map(|(_, cle)| cle)
}

/// Inscrit un fichier téléchargé au registre, comme la v2. Un checkpoint de famille neuf attend la validation de ses
/// steps : ses suggestions (aucune pour un fichier de fiche) sont gardées pour « Ma collection ».
fn inscrire(
    contexte: &Contexte,
    cle: &str,
    dossier: &str,
    fiche: &Fiche,
    taille: u64,
    sha256: &str,
) -> io::Result<()> {
    let mut entree = json!({
        "source": fiche.source,
        "page": fiche.page,
        "url": fiche.url,
        "licence": fiche.licence,
        "mots_declencheurs": fiche.mots_declencheurs,
        "base": fiche.base,
        "taille": taille,
        "sha256": sha256,
        "date": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    });
    let Some(objet) = entree.as_object_mut() else {
        return Err(io::Error::other("entrée du registre"));
    };
    if let Some(force) = fiche.force_auteur {
        objet.insert("force_auteur".to_owned(), json!(force));
        objet.insert("images_force".to_owned(), json!(fiche.images_force));
    }
    if let Some(prompt) = &fiche.prompt_exemple {
        objet.insert("prompt_exemple".to_owned(), json!(prompt));
    }
    if modeles::est_checkpoint(dossier) {
        let vide = json!({"statut": "aucun", "valeurs": [], "extraits": []});
        let steps = fiche.steps_suggeres.clone().unwrap_or_else(|| vide.clone());
        let cfg = fiche.cfg_suggere.clone().unwrap_or_else(|| vide.clone());
        objet.insert("a_valider".to_owned(), json!({"steps": steps}));
        objet.insert("conseils".to_owned(), json!({"steps": steps, "cfg": cfg}));
    }
    modifier_registre(&contexte.data, |r| {
        r.insert(cle.to_owned(), entree);
    })
}

fn nouvel_id() -> Result<String, String> {
    let mut octets = [0u8; 6];
    getrandom::fill(&mut octets).map_err(|e| format!("aléa du système indisponible : {e}"))?;
    Ok(octets.iter().map(|o| format!("{o:02x}")).collect())
}

#[derive(Default)]
struct Partage {
    taches: Mutex<Vec<Tache>>,
    annulations: Mutex<HashSet<String>>,
    /// Le fil est occupé (pour les tests).
    occupe: AtomicBool,
}

impl Partage {
    fn taches(&self) -> MutexGuard<'_, Vec<Tache>> {
        self.taches.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn modifier(&self, id: &str, modification: impl FnOnce(&mut Tache)) {
        if let Some(t) = self.taches().iter_mut().find(|t| t.id == id) {
            modification(t);
        }
    }

    fn annulee(&self, id: &str) -> bool {
        self.annulations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(id)
    }
}

/// La file de téléchargement et son fil.
pub struct Telechargeur {
    contexte: Arc<Contexte>,
    partage: Arc<Partage>,
    envoi: Sender<String>,
}

impl Telechargeur {
    /// La file, son fil lancé avec ce réseau.
    pub fn demarrer(contexte: Contexte, reseau: impl Reseau) -> Self {
        let contexte = Arc::new(contexte);
        let partage = Arc::new(Partage::default());
        let (envoi, reception) = mpsc::channel::<String>();
        let fil_contexte = Arc::clone(&contexte);
        let fil_partage = Arc::clone(&partage);
        std::thread::spawn(move || {
            for id in &reception {
                boucle_une(&fil_contexte, &reseau, &fil_partage, &id);
            }
        });
        Self {
            contexte,
            partage,
            envoi,
        }
    }

    /// Les téléchargements de ce lancement.
    pub fn taches(&self) -> Vec<Tache> {
        self.partage.taches().clone()
    }

    /// Ce qui entoure la file (dossiers, clés, place libre).
    pub fn contexte(&self) -> &Contexte {
        &self.contexte
    }

    /// Met un fichier en file, comme la v2 : refusé s'il est déjà là (même nom, ou même empreinte sous un autre nom),
    /// déjà dans la file, ou si la place manque (1 Gio gardé libre).
    ///
    /// # Errors
    /// Le message de la v2.
    pub fn ajouter(&self, fiche: Fiche, dossier: &str) -> Result<Tache, String> {
        let destination = modeles::destination(&self.contexte.modeles, dossier, &fiche.nom)?;
        if destination.exists() {
            return Err(format!("Déjà présent : {dossier}/{}", fiche.nom));
        }
        if let Some(deja) = fiche
            .sha256
            .as_deref()
            .and_then(|s| cle_par_empreinte(&self.contexte, s))
        {
            return Err(format!("Déjà installé sous un autre nom : {deja}"));
        }
        let actifs = ["attente", "en_cours", "verification"];
        if self.partage.taches().iter().any(|t| {
            actifs.contains(&t.etat.as_str()) && t.fiche.as_ref().is_some_and(|f| f.url == fiche.url)
        }) {
            return Err("Ce fichier est déjà dans la file".to_owned());
        }
        let part = part_de(&destination);
        let deja = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        let libre = (self.contexte.espace_libre)(&premier_existant(&self.contexte.modeles)).unwrap_or(0);
        if libre.saturating_sub(fiche.taille.saturating_sub(deja)) < MARGE_DISQUE {
            return Err("Espace disque insuffisant".to_owned());
        }
        let tache = Tache {
            id: nouvel_id()?,
            nom: fiche.nom.clone(),
            dossier: dossier.to_owned(),
            taille: fiche.taille,
            etat: "attente".to_owned(),
            recu: 0,
            debit: 0.0,
            erreur: None,
            fiche: Some(fiche),
        };
        self.partage.taches().push(tache.clone());
        self.envoi
            .send(tache.id.clone())
            .map_err(|_| "la file de téléchargement est arrêtée".to_owned())?;
        Ok(tache)
    }

    /// Annule un téléchargement : en attente, il ne partira pas ; en cours, il s'arrête et son `.part` est effacé.
    ///
    /// # Errors
    /// Téléchargement inconnu.
    pub fn annuler(&self, id: &str) -> Result<Tache, String> {
        let mut taches = self.partage.taches();
        let tache = taches
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| "Téléchargement inconnu".to_owned())?;
        match tache.etat.as_str() {
            "attente" => "annule".clone_into(&mut tache.etat),
            "en_cours" | "verification" => {
                self.partage
                    .annulations
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(id.to_owned());
            }
            _ => {}
        }
        Ok(tache.clone())
    }

    /// Vrai tant qu'un fichier est en cours de téléchargement.
    pub fn occupe(&self) -> bool {
        self.partage.occupe.load(Ordering::Relaxed)
    }
}

/// Le premier dossier qui existe, en remontant (un dossier des modèles neuf n'existe pas encore : même disque).
/// Le dossier lui-même, ou son premier parent qui existe (pour lire la place libre d'un dossier à venir).
pub fn premier_existant(chemin: &Path) -> PathBuf {
    chemin
        .ancestors()
        .find(|p| p.exists())
        .map_or_else(|| chemin.to_path_buf(), Path::to_path_buf)
}

fn part_de(destination: &Path) -> PathBuf {
    let mut nom = destination
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    nom.push(".part");
    destination.with_file_name(nom)
}

/// Une coupure du réseau : réessayée.
struct Coupure(String);

fn boucle_une(contexte: &Contexte, reseau: &impl Reseau, partage: &Partage, id: &str) {
    let fiche = {
        let mut taches = partage.taches();
        let Some(t) = taches.iter_mut().find(|t| t.id == id && t.etat == "attente") else {
            return;
        };
        t.fiche.clone()
    };
    let Some(fiche) = fiche else { return };
    let dossier = partage
        .taches()
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.dossier.clone())
        .unwrap_or_default();
    partage.occupe.store(true, Ordering::Relaxed);
    let objet = format!("{dossier}/{}", fiche.nom);
    let resultat = telecharger(contexte, reseau, partage, id, &fiche, &dossier);
    match resultat {
        Ok(()) => journal::noter(
            &contexte.dossier,
            "Modèle téléchargé",
            &objet,
            journal::Resultat::Ok,
            Some(&fiche.source),
        ),
        Err(_) if partage.annulee(id) => {
            partage.modifier(id, |t| "annule".clone_into(&mut t.etat));
            journal::noter(
                &contexte.dossier,
                "Téléchargement annulé",
                &objet,
                journal::Resultat::Annule,
                None,
            );
        }
        Err(e) => {
            partage.modifier(id, |t| {
                "erreur".clone_into(&mut t.etat);
                t.erreur = Some(e.clone());
            });
            journal::noter(
                &contexte.dossier,
                "Téléchargement en échec",
                &objet,
                journal::Resultat::Erreur,
                Some(&e),
            );
        }
    }
    partage.modifier(id, |t| t.debit = 0.0);
    partage
        .annulations
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(id);
    partage.occupe.store(false, Ordering::Relaxed);
}

fn telecharger(
    contexte: &Contexte,
    reseau: &impl Reseau,
    partage: &Partage,
    id: &str,
    fiche: &Fiche,
    dossier: &str,
) -> Result<(), String> {
    let destination = modeles::destination(&contexte.modeles, dossier, &fiche.nom)?;
    let parent = destination.parent().unwrap_or(&contexte.modeles).to_path_buf();
    fs::create_dir_all(&parent).map_err(|e| format!("{} : {e}", parent.display()))?;
    let part = part_de(&destination);
    let mut recu = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if recu > fiche.taille {
        let _ = fs::remove_file(&part);
        recu = 0;
    }
    partage.modifier(id, |t| t.recu = recu);
    let mut essais = 0;
    while recu < fiche.taille {
        match recevoir(contexte, reseau, partage, id, fiche, &part, recu) {
            Ok(n) if n > recu => {
                recu = n;
                essais = 0;
            }
            Ok(_) => {
                // La source a fini sa réponse sans rien envoyer de plus : compté comme une coupure.
                essais += 1;
                if essais > ESSAIS_RESEAU {
                    return Err("Réseau : la source s'arrête avant la fin du fichier".to_owned());
                }
            }
            Err(Err(coupure)) => {
                essais += 1;
                if essais > ESSAIS_RESEAU {
                    return Err(format!("Réseau : {}", coupure.0));
                }
                partage.modifier(id, |t| t.debit = 0.0);
                std::thread::sleep(Duration::from_secs(1 << essais.min(5)));
                recu = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            }
            Err(Ok(arret)) => {
                if partage.annulee(id) {
                    let _ = fs::remove_file(&part);
                }
                return Err(arret);
            }
        }
    }
    partage.modifier(id, |t| "verification".clone_into(&mut t.etat));
    if recu != fiche.taille {
        return Err(format!(
            "Taille incorrecte : {recu} octets au lieu de {}",
            fiche.taille
        ));
    }
    let sha256 = empreinte(&part).map_err(|e| format!("{} : {e}", part.display()))?;
    if let Some(attendu) = &fiche.sha256
        && sha256 != attendu.to_lowercase()
    {
        let _ = fs::remove_file(&part);
        return Err("Empreinte SHA-256 incorrecte : fichier rejeté".to_owned());
    }
    fs::rename(&part, &destination).map_err(|e| format!("{} : {e}", destination.display()))?;
    let cle = format!("{dossier}/{}", fiche.nom);
    inscrire(contexte, &cle, dossier, fiche, recu, &sha256)
        .map_err(|e| format!("registre des modèles : {e}"))?;
    partage.modifier(id, |t| "termine".clone_into(&mut t.etat));
    Ok(())
}

/// Le SHA-256 d'un fichier, en hexadécimal minuscule.
///
/// # Errors
/// Le fichier ne se lit pas.
pub fn empreinte(fichier: &Path) -> io::Result<String> {
    let mut entree = fs::File::open(fichier)?;
    let mut hacheur = Sha256::new();
    let mut tampon = vec![0u8; 16 * BLOC];
    loop {
        let n = entree.read(&mut tampon)?;
        if n == 0 {
            break;
        }
        hacheur.update(tampon.get(..n).unwrap_or_default());
    }
    Ok(hacheur.finalize().iter().map(|o| format!("{o:02x}")).collect())
}

/// Reçoit la suite du fichier. `Err(Ok(message))` : arrêt (refus, annulation) ; `Err(Err(coupure))` : à réessayer.
fn recevoir(
    contexte: &Contexte,
    reseau: &impl Reseau,
    partage: &Partage,
    id: &str,
    fiche: &Fiche,
    part: &Path,
    deja: u64,
) -> Result<u64, Result<String, Coupure>> {
    let entetes = (contexte.entetes)(&fiche.source);
    let (statut, mut corps) = reseau
        .ouvrir(&fiche.url, &entetes, deja)
        .map_err(|e| Err(Coupure(e.to_string())))?;
    if statut == 401 || statut == 403 {
        return Err(Ok(acces_refuse(&fiche.source)));
    }
    let mut recu = deja;
    let reprise = match statut {
        206 if deja > 0 => true,
        200 => {
            // Le serveur ignore la reprise : on repart de zéro.
            recu = 0;
            false
        }
        _ => return Err(Ok(format!("Réponse HTTP {statut}"))),
    };
    partage.modifier(id, |t| {
        "en_cours".clone_into(&mut t.etat);
        t.recu = recu;
    });
    let mut sortie = OpenOptions::new()
        .create(true)
        .write(true)
        .append(reprise)
        .truncate(!reprise)
        .open(part)
        .map_err(|e| Ok(format!("{} : {e}", part.display())))?;
    let mut tampon = vec![0u8; BLOC];
    let mut fenetre = (Instant::now(), recu);
    loop {
        if partage.annulee(id) {
            return Err(Ok("Annulé".to_owned()));
        }
        let n = corps.read(&mut tampon).map_err(|e| Err(Coupure(e.to_string())))?;
        if n == 0 {
            break;
        }
        sortie
            .write_all(tampon.get(..n).unwrap_or_default())
            .map_err(|e| Ok(format!("{} : {e}", part.display())))?;
        recu += n as u64;
        let ecoule = fenetre.0.elapsed().as_secs_f64();
        if ecoule >= 1.0 {
            #[allow(clippy::cast_precision_loss)] // un débit affiché
            let debit = (recu - fenetre.1) as f64 / ecoule;
            fenetre = (Instant::now(), recu);
            partage.modifier(id, |t| t.debit = debit);
        }
        partage.modifier(id, |t| t.recu = recu);
    }
    sortie
        .flush()
        .map_err(|e| Ok(format!("{} : {e}", part.display())))?;
    Ok(recu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Cursor;

    /// Une demande reçue par le faux serveur : adresse, octet de départ, en-têtes.
    type Demande = (String, u64, Vec<(String, String)>);

    /// Un serveur de test : des fichiers par adresse, qui sait reprendre, couper ou refuser.
    #[derive(Default, Clone)]
    struct Faux {
        fichiers: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        refuses: Arc<Mutex<HashSet<String>>>,
        coupures: Arc<Mutex<u32>>,
        demandes: Arc<Mutex<Vec<Demande>>>,
    }

    impl Reseau for Faux {
        fn ouvrir(
            &self,
            adresse: &str,
            entetes: &[(String, String)],
            depuis: u64,
        ) -> io::Result<(u16, Box<dyn Read>)> {
            self.demandes
                .lock()
                .unwrap()
                .push((adresse.to_owned(), depuis, entetes.to_vec()));
            {
                let mut coupures = self.coupures.lock().unwrap();
                if *coupures > 0 {
                    *coupures -= 1;
                    return Err(io::Error::other("coupure"));
                }
            }
            if self.refuses.lock().unwrap().contains(adresse) {
                return Ok((403, Box::new(Cursor::new(Vec::new()))));
            }
            let Some(octets) = self.fichiers.lock().unwrap().get(adresse).cloned() else {
                return Ok((404, Box::new(Cursor::new(Vec::new()))));
            };
            if depuis > 0 {
                let reste = octets
                    .get(usize::try_from(depuis).unwrap()..)
                    .unwrap_or_default()
                    .to_vec();
                return Ok((206, Box::new(Cursor::new(reste))));
            }
            Ok((200, Box::new(Cursor::new(octets))))
        }
    }

    fn sha(octets: &[u8]) -> String {
        Sha256::digest(octets)
            .iter()
            .map(|o| format!("{o:02x}"))
            .collect()
    }

    fn contexte(temp: &tempfile::TempDir, libre: u64) -> Contexte {
        Contexte {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles: temp.path().join("modeles"),
            entetes: Box::new(|source| {
                if source == "huggingface" {
                    vec![("Authorization".to_owned(), "Bearer hf_x".to_owned())]
                } else {
                    Vec::new()
                }
            }),
            espace_libre: Box::new(move |_| Some(libre)),
        }
    }

    fn fiche(nom: &str, octets: &[u8]) -> Fiche {
        Fiche {
            source: "huggingface".to_owned(),
            nom: nom.to_owned(),
            taille: octets.len() as u64,
            sha256: Some(sha(octets)),
            url: format!("https://huggingface.co/x/resolve/main/{nom}"),
            page: "https://huggingface.co/x".to_owned(),
            licence: Some("Apache 2.0".to_owned()),
            ..Fiche::default()
        }
    }

    fn attendre(t: &Telechargeur, id: &str) -> Tache {
        let fin = Instant::now() + Duration::from_secs(60);
        loop {
            let tache = t.taches().into_iter().find(|x| x.id == id).unwrap();
            if !["attente", "en_cours", "verification"].contains(&tache.etat.as_str()) || Instant::now() > fin
            {
                return tache;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    const GRAND: u64 = 1 << 40;

    #[test]
    fn telecharge_verifie_et_inscrit_au_registre() {
        let temp = tempfile::tempdir().unwrap();
        let faux = Faux::default();
        let octets = vec![7u8; 3 * BLOC + 5];
        let f = fiche("vae.safetensors", &octets);
        faux.fichiers
            .lock()
            .unwrap()
            .insert(f.url.clone(), octets.clone());
        let t = Telechargeur::demarrer(contexte(&temp, GRAND), faux.clone());
        let tache = t.ajouter(f.clone(), "image/_base/vae").unwrap();
        let fin = attendre(&t, &tache.id);
        assert_eq!(fin.etat, "termine", "{:?}", fin.erreur);
        assert_eq!(fin.recu, octets.len() as u64);
        let posee = temp.path().join("modeles/image/_base/vae/vae.safetensors");
        assert_eq!(fs::read(&posee).unwrap(), octets);
        assert!(!part_de(&posee).exists());
        let r = registre(&temp.path().join("data"));
        let entree = &r["image/_base/vae/vae.safetensors"];
        assert_eq!(entree["sha256"], json!(sha(&octets)));
        assert_eq!(entree["licence"], json!("Apache 2.0"));
        assert!(entree.get("a_valider").is_none());
        // La clé de la source part dans les en-têtes.
        assert_eq!(faux.demandes.lock().unwrap()[0].2[0].1, "Bearer hf_x");
        // Déjà là : sous ce nom, ou sous un autre nom avec la même empreinte.
        assert!(
            t.ajouter(f.clone(), "image/_base/vae")
                .unwrap_err()
                .starts_with("Déjà présent")
        );
        let mut autre = f.clone();
        autre.nom = "copie.safetensors".to_owned();
        assert_eq!(
            t.ajouter(autre, "image/_base/vae").unwrap_err(),
            "Déjà installé sous un autre nom : image/_base/vae/vae.safetensors"
        );
        let journal = journal::lire(&Dossier::a(temp.path()));
        assert_eq!(journal[0].action, "Modèle téléchargé");
    }

    #[test]
    fn reprend_le_part_et_resiste_aux_coupures() {
        let temp = tempfile::tempdir().unwrap();
        let faux = Faux::default();
        let octets: Vec<u8> = (0..(2 * BLOC + 10))
            .map(|i| u8::try_from(i % 251).unwrap())
            .collect();
        let f = fiche("modele.gguf", &octets);
        faux.fichiers
            .lock()
            .unwrap()
            .insert(f.url.clone(), octets.clone());
        // Un .part laissé par une session précédente, et deux coupures.
        let dossier = temp.path().join("modeles/llm");
        fs::create_dir_all(&dossier).unwrap();
        fs::write(dossier.join("modele.gguf.part"), &octets[..BLOC]).unwrap();
        *faux.coupures.lock().unwrap() = 2;
        let t = Telechargeur::demarrer(contexte(&temp, GRAND), faux.clone());
        let tache = t.ajouter(f, "llm").unwrap();
        let fin = attendre(&t, &tache.id);
        assert_eq!(fin.etat, "termine", "{:?}", fin.erreur);
        assert_eq!(fs::read(dossier.join("modele.gguf")).unwrap(), octets);
        assert!(
            faux.demandes
                .lock()
                .unwrap()
                .iter()
                .any(|(_, depuis, _)| *depuis == BLOC as u64)
        );
    }

    #[test]
    fn refus_empreinte_et_place() {
        let temp = tempfile::tempdir().unwrap();
        let faux = Faux::default();
        let t = Telechargeur::demarrer(contexte(&temp, GRAND), faux.clone());
        // La source refuse.
        let protege = fiche("protege.safetensors", b"secret");
        faux.refuses.lock().unwrap().insert(protege.url.clone());
        let fin = attendre(&t, &t.ajouter(protege, "llm").unwrap().id);
        assert_eq!(fin.etat, "erreur");
        assert!(fin.erreur.unwrap().starts_with("Hugging Face refuse l'accès"));
        // Une empreinte qui ne correspond pas : le fichier est rejeté.
        let mut faussee = fiche("faux.gguf", b"bon contenu");
        faux.fichiers
            .lock()
            .unwrap()
            .insert(faussee.url.clone(), b"bon contenu".to_vec());
        faussee.sha256 = Some(sha(b"autre"));
        let fin = attendre(&t, &t.ajouter(faussee, "llm").unwrap().id);
        assert_eq!(
            fin.erreur.as_deref(),
            Some("Empreinte SHA-256 incorrecte : fichier rejeté")
        );
        assert!(!temp.path().join("modeles/llm/faux.gguf").exists());
        assert!(!temp.path().join("modeles/llm/faux.gguf.part").exists());
        // Hors des dossiers connus, ou sans la place.
        assert!(t.ajouter(fiche("x.gguf", b"x"), "../ailleurs").is_err());
        let petit = Telechargeur::demarrer(contexte(&temp, MARGE_DISQUE), Faux::default());
        assert_eq!(
            petit.ajouter(fiche("y.gguf", b"yy"), "llm").unwrap_err(),
            "Espace disque insuffisant"
        );
    }

    #[test]
    fn un_checkpoint_neuf_attend_ses_steps() {
        let temp = tempfile::tempdir().unwrap();
        let faux = Faux::default();
        let wf = crate::workflows::obtenir("krea2_turbo").unwrap();
        let dossier = wf.dossier_modeles();
        let f = fiche("krea.safetensors", b"poids");
        faux.fichiers
            .lock()
            .unwrap()
            .insert(f.url.clone(), b"poids".to_vec());
        let t = Telechargeur::demarrer(contexte(&temp, GRAND), faux);
        let fin = attendre(&t, &t.ajouter(f, &dossier).unwrap().id);
        assert_eq!(fin.etat, "termine", "{:?}", fin.erreur);
        let r = registre(&temp.path().join("data"));
        assert_eq!(
            r[&format!("{dossier}/krea.safetensors")]["a_valider"]["steps"]["statut"],
            json!("aucun")
        );
    }
}
