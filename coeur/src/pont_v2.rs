//! Le pont de mise à jour depuis la version 2. La mise à jour intégrée de la v2 (`app/installation_maj.py`) télécharge
//! le zip de la dernière release, le vérifie, le décompresse dans `cache\mise-a-jour\Aria-Studio-X.Y.Z\`, puis lance
//! le nouvel exe avec `--mise-a-jour <pid du serveur v2> <ancien lanceur>` et s'arrête. Ce module est la suite, côté
//! v3, comme `tools/launcher/Launcher.cs` l'était côté v2 :
//!
//! 1. attendre la fin du processus `<pid>` (120 s au plus) ;
//! 2. attendre que l'icône de la v2 se ferme (elle tient `programme\` ouvert, environ 12 s), puis que `programme\` se
//!    libère (15 s d'attente) ;
//! 3. installer dans `programme\` ce que liste `fichiers.txt` du zip, sans la marque « téléchargé depuis Internet » ;
//!    la v2, qui n'a ni `fichiers.txt` ni `BUILDINFO.txt`, est gardée en bloc dans `programme.precedent\` ;
//! 4. relancer l'exe installé, ou, si rien n'a pu être installé, l'ancien lanceur (la v2 note elle-même l'échec) ;
//! 5. au démarrage suivant, lire `cache\mise-a-jour\relais.json` et noter le bilan au journal.
//!
//! Le zip porte aussi un témoin `app/__init__.py` (`VERSION = "X.Y.Z"`) que la v2 contrôle avant de s'effacer : il
//! n'est pas dans `fichiers.txt`, donc il n'est jamais copié dans `programme\`.
//!
//! Ce qui touche le système (le temps, les processus, l'icône de la v2) passe par [`Systeme`] : le vrai est dans la
//! crate `aria-appli`, les essais en donnent un simulé.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::installation::{self, Chemins, Echec, LISTE};
use crate::journal::{self, Resultat};
use crate::{Dossier, identite, mises_a_jour};

/// L'argument que la v2 passe au nouvel exe : `--mise-a-jour <pid> <ancien lanceur>`.
pub const ARGUMENT: &str = "--mise-a-jour";

/// Le fichier que la v2 écrit avant le relais : `{"de": …, "vers": …, "date": …}`.
pub const FICHIER_RELAIS: &str = "relais.json";

/// Combien attendre la fin du serveur de la v2.
pub const ATTENTE_PROCESSUS: Duration = Duration::from_secs(120);

/// Combien attendre la fermeture de l'icône de la v2.
pub const ATTENTE_ICONE: Duration = Duration::from_secs(30);

/// Combien attendre que `programme\` se libère.
pub const ATTENTE_DOSSIER: Duration = Duration::from_secs(15);

const PAS: Duration = Duration::from_millis(250);
const SUFFIXE_PRECEDENT: &str = ".precedent";
const SUFFIXE_NOUVEAU: &str = ".nouveau";
const SUFFIXE_AVANT: &str = ".avant";
const SUFFIXE_VERIF: &str = ".verif";

const SANS_NUMERO: &str = "Mise à jour : il manque le numéro du serveur d'Aria à attendre.";
const PAS_ARRETEE: &str = "Aria ne s'est pas arrêtée : la mise à jour n'est pas installée.";
const DEJA_INSTALLE: &str = "Ce lanceur est déjà celui du dossier d'installation : rien à installer.";
const SANS_LANCEUR: &str = "Le lanceur de la mise à jour est introuvable.";
const SANS_LISTE: &str = "fichiers.txt manque ou ne convient pas dans la mise à jour.";

/// Ce que demande la v2 : le serveur à attendre et le lanceur à relancer si rien ne se fait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Demande {
    /// Le processus du serveur de la v2.
    pub pid: u32,
    /// Le lanceur de la v2, relancé si l'installation échoue.
    pub ancien_lanceur: Option<PathBuf>,
}

/// Lit `--mise-a-jour <pid> <ancien lanceur>` ; rien si l'argument n'y est pas, une erreur s'il manque le numéro.
pub fn lire_demande(arguments: &[String]) -> Option<Result<Demande, String>> {
    let place = arguments.iter().position(|a| a == ARGUMENT)?;
    let pid = arguments.get(place + 1).and_then(|a| a.parse::<u32>().ok());
    let ancien = arguments.get(place + 2).map(PathBuf::from);
    let Some(pid) = pid else {
        return Some(Err(SANS_NUMERO.to_owned()));
    };
    Some(Ok(Demande {
        pid,
        ancien_lanceur: ancien,
    }))
}

/// Ce que le pont demande au système.
pub trait Systeme {
    /// Le temps écoulé depuis un point fixe.
    fn ecoule(&self) -> Duration;

    /// Laisse passer `duree`.
    fn dormir(&self, duree: Duration);

    /// Le processus `pid` tourne-t-il encore ?
    fn processus_vivant(&self, pid: u32) -> bool;

    /// Attend au plus `delai` que l'icône de la v2 se ferme ; vrai si elle l'est ou n'existe pas.
    fn attendre_icone(&self, delai: Duration) -> bool;

    /// Le dossier peut-il être remplacé ? Sous Windows, un dossier dont un fichier est ouvert ne se renomme pas.
    fn dossier_libre(&self, dossier: &Path) -> bool {
        dossier_renommable(dossier)
    }
}

/// Le dossier se renomme-t-il, puis reprend-il son nom ?
pub fn dossier_renommable(dossier: &Path) -> bool {
    let verif = installation::suffixe(dossier, SUFFIXE_VERIF);
    if fs::rename(dossier, &verif).is_err() {
        return false;
    }
    fs::rename(&verif, dossier).is_ok()
}

/// Réessaie `pret` toutes les 250 ms jusqu'à ce qu'il soit vrai ou que `delai` soit passé ; le dernier essai est
/// fait à l'échéance. Vrai si `pret` l'a été.
pub fn attendre(systeme: &impl Systeme, delai: Duration, mut pret: impl FnMut() -> bool) -> bool {
    let debut = systeme.ecoule();
    loop {
        if pret() {
            return true;
        }
        if systeme.ecoule().saturating_sub(debut) >= delai {
            return false;
        }
        systeme.dormir(PAS);
    }
}

/// Le nom du verrou que la v2 tient tant que son icône est affichée (`Icone.cs`, `NomVerrou`) : un hachage FNV-1a
/// du dossier de données, en minuscules, lu en unités UTF-16 comme en C#.
pub fn nom_verrou_icone(maison: &Path) -> String {
    let mut h: u32 = 2_166_136_261;
    for unite in maison.to_string_lossy().to_lowercase().encode_utf16() {
        h ^= u32::from(unite);
        h = h.wrapping_mul(16_777_619);
    }
    format!("Local\\{}-icone-{h:08x}", identite::NOM_EXE)
}

/// Une mise à jour qui n'a rien changé : pourquoi, et quoi relancer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refus {
    /// Le message, en français, tel que l'utilisateur le lit.
    pub message: String,
    /// L'ancien lanceur, à relancer ; rien si la v2 tourne encore ou si son exe n'existe plus.
    pub relancer: Option<PathBuf>,
}

/// L'issue : l'exe installé à relancer, ou le refus.
pub type Issue = Result<PathBuf, Refus>;

fn refus(message: String, relancer: Option<PathBuf>) -> Refus {
    Refus { message, relancer }
}

fn echec(message: &str) -> Echec {
    Echec(message.to_owned())
}

/// Toute la suite d'une mise à jour lancée par la v2. `exe` est le nouvel exe, dans le zip décompressé.
///
/// # Errors
/// [`Refus`] : rien n'a changé dans `programme\`, et le refus dit quoi relancer.
pub fn mettre_a_jour(demande: &Demande, dossier: &Dossier, exe: &Path, systeme: &impl Systeme) -> Issue {
    let ancien = demande.ancien_lanceur.clone().filter(|p| p.is_file());
    let arretee = || !systeme.processus_vivant(demande.pid);
    if !attendre(systeme, ATTENTE_PROCESSUS, arretee) {
        // La v2 tourne encore : il n'y a rien à relancer.
        return Err(refus(PAS_ARRETEE.to_owned(), None));
    }
    // L'icône de la v2 tient `programme\` jusqu'à sa fermeture ; si elle tarde, le dossier dira s'il est libre.
    let _ = systeme.attendre_icone(ATTENTE_ICONE);
    let chemins = Chemins::du_dossier(dossier);
    let resultat = installer(&chemins, exe, systeme);
    resultat.map_err(|e| refus(e.0, ancien))
}

fn installer(chemins: &Chemins, exe: &Path, systeme: &impl Systeme) -> Result<PathBuf, Echec> {
    let source = fs::canonicalize(exe).map_err(|_| echec(SANS_LANCEUR))?;
    let racine = source.parent().ok_or_else(|| echec(SANS_LANCEUR))?;
    let programme = &chemins.programme;
    let precedent = installation::suffixe(programme, SUFFIXE_PRECEDENT);
    let deja_la = installation::meme_dossier(racine, programme);
    let ancienne = installation::meme_dossier(racine, &precedent);
    if deja_la || ancienne {
        return Err(echec(DEJA_INSTALLE));
    }
    let noms = match installation::lire_liste(racine) {
        Ok(Some(noms)) => noms,
        Ok(None) => return Err(echec(SANS_LISTE)),
        Err(e) => {
            let message = format!("La liste des fichiers n'a pas pu être lue : {e}");
            return Err(Echec(message));
        }
    };
    let absents = installation::manquants(racine, &noms);
    if !absents.is_empty() {
        let liste = absents.join(", ");
        let message = format!("Il manque dans la mise à jour : {liste}.");
        return Err(Echec(message));
    }
    for dossier in [programme, &precedent] {
        if !dossier.is_dir() {
            continue;
        }
        if !attendre(systeme, ATTENTE_DOSSIER, || systeme.dossier_libre(dossier)) {
            let message = format!("Aria Studio tourne encore depuis {}.", dossier.display());
            return Err(Echec(message));
        }
    }
    if installation::lire_liste(programme).ok().flatten().is_some() {
        // Déjà une v3 dans `programme\` : la mise à jour habituelle, qui garde l'ancienne dans les sauvegardes.
        return installation::installer(&source, chemins);
    }
    let nouveau = installation::suffixe(programme, SUFFIXE_NOUVEAU);
    let resultat = copier_et_echanger(racine, &noms, chemins);
    // Un reste de copie ne doit jamais traîner à côté de `programme\`.
    installation::supprimer_dossier(&nouveau);
    resultat?;
    Ok(programme.join(installation::nom_exe()))
}

/// Copie ce que liste `fichiers.txt` dans `programme.nouveau\`, puis remplace `programme\` en bloc : la v2 devient
/// `programme.precedent\`, entière. À la moindre erreur, `programme\` est comme avant.
fn copier_et_echanger(racine: &Path, noms: &[String], chemins: &Chemins) -> Result<(), Echec> {
    let programme = &chemins.programme;
    let nouveau = installation::suffixe(programme, SUFFIXE_NOUVEAU);
    let precedent = installation::suffixe(programme, SUFFIXE_PRECEDENT);
    installation::supprimer_dossier(&nouveau);
    if let Err(e) = copier_dans(racine, noms, &nouveau) {
        let message = format!("Les fichiers n'ont pas pu être copiés : {e}. Rien n'a changé.");
        return Err(Echec(message));
    }
    if let Err(e) = remplacer_en_bloc(programme, &nouveau, &precedent, renommer) {
        let message = format!("Le remplacement n'a pas pu se faire : {e}. Rien n'a changé.");
        return Err(Echec(message));
    }
    Ok(())
}

fn renommer(de: &Path, vers: &Path) -> io::Result<()> {
    fs::rename(de, vers)
}

/// Les fichiers listés, et la liste elle-même, copiés par leur contenu seul : la marque « téléchargé depuis
/// Internet » ne suit pas.
fn copier_dans(racine: &Path, noms: &[String], cible: &Path) -> io::Result<()> {
    fs::create_dir_all(cible)?;
    for nom in noms.iter().map(String::as_str).chain([LISTE]) {
        let copie = cible.join(nom);
        installation::copier_contenu(&racine.join(nom), &copie)?;
        installation::retirer_marque(&copie);
    }
    Ok(())
}

/// `programme\` devient `precedent\` (celui d'avant passe d'abord de côté, et n'est supprimé qu'une fois la nouvelle
/// version en place), puis `nouveau\` devient `programme\`. Sans `programme\`, il n'y a rien à garder. À la moindre
/// erreur, tout reprend sa place.
fn remplacer_en_bloc(
    programme: &Path,
    nouveau: &Path,
    precedent: &Path,
    renommer: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let avant = installation::suffixe(precedent, SUFFIXE_AVANT);
    installation::supprimer_dossier(&avant);
    let etait_la = programme.exists();
    let avait_precedent = etait_la && precedent.exists();
    if avait_precedent {
        renommer(precedent, &avant)?;
    }
    let mut resultat: io::Result<()> = Ok(());
    if etait_la {
        resultat = renommer(programme, precedent);
    }
    if resultat.is_ok() {
        resultat = renommer(nouveau, programme);
        if resultat.is_err() && etait_la {
            // La version en place reprend sa place.
            let _ = renommer(precedent, programme);
        }
    }
    if resultat.is_err() && avait_precedent {
        // Le précédent d'avant reprend sa place.
        let _ = renommer(&avant, precedent);
    }
    if resultat.is_ok() {
        installation::supprimer_dossier(&avant);
    }
    resultat
}

/// Ce que dit `relais.json`, écrit par la v2 avant de passer la main.
#[derive(Debug, Deserialize)]
struct Relais {
    #[serde(default)]
    de: String,
    #[serde(default)]
    vers: String,
}

/// Le bilan d'une mise à jour lancée par la v2, relu au démarrage suivant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bilan {
    /// La version d'avant, d'après la v2.
    pub de: String,
    /// La version que la v2 a installée, d'après elle.
    pub vers: String,
    /// La version qui tourne.
    pub tourne: String,
    /// Vrai si la version attendue est celle qui tourne.
    pub installee: bool,
}

/// `cache\mise-a-jour\relais.json`.
pub fn fichier_relais(dossier: &Dossier) -> PathBuf {
    mises_a_jour::dossier_travail(dossier).join(FICHIER_RELAIS)
}

fn lisible(version: &str) -> &str {
    match version {
        "" => "?",
        autre => autre,
    }
}

/// Après un relais de la v2 : la version attendue tourne-t-elle ? Noté au journal, et `relais.json` est retiré pour
/// que le bilan ne se note qu'une fois. Rien si le fichier n'existe pas ou ne se lit pas (il reste alors, et le
/// ménage du dossier de travail l'emportera). À appeler avant ce ménage.
pub fn bilan_au_demarrage(dossier: &Dossier, version: &str) -> Option<Bilan> {
    let chemin = fichier_relais(dossier);
    let texte = fs::read_to_string(&chemin).ok()?;
    let relais: Relais = serde_json::from_str(&texte).ok()?;
    // Une seule lecture ; si le fichier ne peut pas être retiré, le ménage du dossier de travail s'en chargera.
    let _ = fs::remove_file(&chemin);
    let objet = format!("{} → {}", lisible(&relais.de), lisible(&relais.vers));
    let installee = relais.vers == version;
    if installee {
        journal::noter(dossier, "Mise à jour installée", &objet, Resultat::Ok, None);
    } else {
        let detail = format!("{} a redémarré en {version}", identite::NOM);
        journal::noter(
            dossier,
            "Mise à jour non installée",
            &objet,
            Resultat::Erreur,
            Some(&detail),
        );
    }
    Some(Bilan {
        de: relais.de,
        vers: relais.vers,
        tourne: version.to_owned(),
        installee,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const PID: u32 = 4242;

    /// Un système simulé : le temps passe quand on dort, le serveur de la v2 s'arrête à `fin_du_processus`, les
    /// dossiers se libèrent à `libre_des`.
    struct Faux {
        temps: Cell<Duration>,
        fin_du_processus: Duration,
        libre_des: Duration,
        icone_attendue: Cell<Option<Duration>>,
    }

    fn faux(fin_du_processus: u64, libre_des: u64) -> Faux {
        Faux {
            temps: Cell::new(Duration::ZERO),
            fin_du_processus: Duration::from_secs(fin_du_processus),
            libre_des: Duration::from_secs(libre_des),
            icone_attendue: Cell::new(None),
        }
    }

    impl Systeme for Faux {
        fn ecoule(&self) -> Duration {
            self.temps.get()
        }

        fn dormir(&self, duree: Duration) {
            self.temps.set(self.temps.get() + duree);
        }

        fn processus_vivant(&self, pid: u32) -> bool {
            pid == PID && self.temps.get() < self.fin_du_processus
        }

        fn attendre_icone(&self, delai: Duration) -> bool {
            self.icone_attendue.set(Some(delai));
            true
        }

        fn dossier_libre(&self, _dossier: &Path) -> bool {
            self.temps.get() >= self.libre_des
        }
    }

    fn ecrire(chemin: &Path, contenu: &str) {
        fs::create_dir_all(chemin.parent().unwrap()).unwrap();
        fs::write(chemin, contenu).unwrap();
    }

    fn lire(chemin: &Path) -> String {
        fs::read_to_string(chemin).unwrap()
    }

    fn nom() -> String {
        installation::nom_exe()
    }

    /// Le témoin que la v2 contrôle : `app/__init__.py`.
    fn temoin(dossier: &Path) -> PathBuf {
        dossier.join("app").join("__init__.py")
    }

    /// Une v2 installée : son lanceur, son Python et son code, sans `fichiers.txt` ni `BUILDINFO.txt`.
    fn version_2(programme: &Path) {
        ecrire(&programme.join(nom()), "lanceur v2");
        ecrire(&programme.join("python").join("pythonw.exe"), "python");
        ecrire(&temoin(programme), "VERSION = \"2.4.1\"");
    }

    /// Le zip de la v3 décompressé par la v2, témoin `app/__init__.py` compris (hors de `fichiers.txt`).
    fn zip_v3(racine: &Path) -> PathBuf {
        let infos = format!("{} 3.0.0\n", identite::NOM);
        ecrire(&racine.join(nom()), "exe v3");
        ecrire(&racine.join("BUILDINFO.txt"), &infos);
        ecrire(&racine.join("LICENCES-TIERS.txt"), "licences");
        ecrire(&temoin(racine), "VERSION = \"3.0.0\"");
        let liste = format!("{}\nBUILDINFO.txt\nLICENCES-TIERS.txt\n", nom());
        ecrire(&racine.join(LISTE), &liste);
        racine.join(nom())
    }

    /// Le dossier de données, le zip décompressé dans `cache\mise-a-jour\` et le nouvel exe.
    fn monde(temp: &Path) -> (Dossier, PathBuf) {
        let dossier = Dossier::a(temp.join("donnees"));
        let racine = mises_a_jour::dossier_travail(&dossier).join("Aria-Studio-3.0.0");
        let exe = zip_v3(&racine);
        (dossier, exe)
    }

    fn demande(ancien: Option<PathBuf>) -> Demande {
        Demande {
            pid: PID,
            ancien_lanceur: ancien,
        }
    }

    /// La mise à jour avec un serveur v2 déjà arrêté et un dossier libre.
    fn essayer(dossier: &Dossier, exe: &Path, ancien: Option<PathBuf>) -> Issue {
        mettre_a_jour(&demande(ancien), dossier, exe, &faux(0, 0))
    }

    fn arguments(texte: &[&str]) -> Vec<String> {
        texte.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn la_demande_se_lit_dans_les_arguments() {
        assert_eq!(lire_demande(&arguments(&["--silencieux"])), None);
        assert_eq!(lire_demande(&[]), None);
        let ancien = "C:\\v2\\Aria-Studio.exe";
        let complete = lire_demande(&arguments(&[ARGUMENT, "4242", ancien]));
        let attendue = Demande {
            pid: 4242,
            ancien_lanceur: Some(PathBuf::from(ancien)),
        };
        assert_eq!(complete, Some(Ok(attendue)));
        // Sans ancien lanceur, le numéro suffit.
        let seule = lire_demande(&arguments(&["--portable", ARGUMENT, "7"]));
        let attendue = Demande {
            pid: 7,
            ancien_lanceur: None,
        };
        assert_eq!(seule, Some(Ok(attendue)));
    }

    #[test]
    fn une_demande_sans_numero_est_refusee() {
        for texte in [vec![ARGUMENT], vec![ARGUMENT, "abc"], vec![ARGUMENT, "-3"]] {
            let lue = lire_demande(&arguments(&texte));
            assert_eq!(lue, Some(Err(SANS_NUMERO.to_owned())), "{texte:?}");
        }
    }

    #[test]
    fn l_attente_s_arrete_des_que_c_est_pret() {
        let systeme = faux(0, 0);
        let mut essais = 0;
        let pret = || {
            essais += 1;
            essais == 3
        };
        assert!(attendre(&systeme, Duration::from_secs(10), pret));
        // Deux pauses de 250 ms entre trois essais.
        assert_eq!(systeme.ecoule(), Duration::from_millis(500));
    }

    #[test]
    fn l_attente_fait_un_dernier_essai_a_l_echeance() {
        let systeme = faux(0, 0);
        let echeance = Duration::from_secs(2);
        let a_l_echeance = || systeme.ecoule() >= echeance;
        assert!(attendre(&systeme, echeance, a_l_echeance));
        let jamais = faux(0, 0);
        assert!(!attendre(&jamais, echeance, || false));
        assert_eq!(jamais.ecoule(), echeance);
    }

    #[test]
    fn le_verrou_de_l_icone_a_le_nom_que_lui_donne_la_v2() {
        // FNV-1a des unités UTF-16 du chemin en minuscules, comme `NomVerrou()` de Icone.cs.
        let maison = Path::new("C:\\Users\\Zeman\\.Aria-Studio");
        let attendu = "Local\\Aria-Studio-icone-d162cc61";
        assert_eq!(nom_verrou_icone(maison), attendu);
        let minuscules = Path::new("c:\\users\\zeman\\.aria-studio");
        assert_eq!(nom_verrou_icone(minuscules), attendu);
        let autre = Path::new("D:\\essai");
        assert_ne!(nom_verrou_icone(autre), attendu);
    }

    #[test]
    fn sans_v2_en_place_la_v3_s_installe_telle_que_la_liste_la_dit() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let systeme = faux(5, 0);
        let issue = mettre_a_jour(&demande(None), &dossier, &exe, &systeme);
        let installe = issue.unwrap();
        let programme = dossier.programme();
        assert_eq!(installe, programme.join(nom()));
        assert_eq!(lire(&installe), "exe v3");
        assert_eq!(lire(&programme.join("LICENCES-TIERS.txt")), "licences");
        assert!(programme.join(LISTE).is_file());
        // Le témoin de la v2 n'est pas dans fichiers.txt : il ne va pas dans programme\.
        assert!(!programme.join("app").exists());
        assert!(!installation::suffixe(&programme, ".nouveau").exists());
        assert!(!installation::suffixe(&programme, ".precedent").exists());
        // Le serveur de la v2 s'arrête à 5 s ; l'icône est attendue 30 s au plus.
        assert!(systeme.ecoule() >= Duration::from_secs(5));
        assert_eq!(systeme.icone_attendue.get(), Some(ATTENTE_ICONE));
    }

    #[test]
    fn la_v2_est_gardee_entiere_dans_programme_precedent() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let programme = dossier.programme();
        version_2(&programme);
        let ancien = programme.join(nom());
        let installe = essayer(&dossier, &exe, Some(ancien)).unwrap();
        assert_eq!(lire(&installe), "exe v3");
        // La v3 ne garde rien de la v2 dans programme\.
        assert!(!programme.join("python").exists());
        assert!(!programme.join("app").exists());
        // La v2 est là, en bloc, comme elle était.
        let precedent = installation::suffixe(&programme, ".precedent");
        assert_eq!(lire(&precedent.join(nom())), "lanceur v2");
        let python = precedent.join("python").join("pythonw.exe");
        assert_eq!(lire(&python), "python");
        assert_eq!(lire(&temoin(&precedent)), "VERSION = \"2.4.1\"");
        assert!(!precedent.join(LISTE).exists());
        assert!(!installation::suffixe(&precedent, ".avant").exists());
    }

    #[test]
    fn un_ancien_precedent_est_remplace_par_la_v2() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let programme = dossier.programme();
        version_2(&programme);
        let precedent = installation::suffixe(&programme, ".precedent");
        ecrire(&precedent.join("reste.txt"), "d'avant la v2");
        essayer(&dossier, &exe, None).unwrap();
        assert!(!precedent.join("reste.txt").exists());
        assert_eq!(lire(&precedent.join(nom())), "lanceur v2");
        assert!(!installation::suffixe(&precedent, ".avant").exists());
    }

    #[test]
    fn un_precedent_sans_programme_reste_en_place() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let programme = dossier.programme();
        let precedent = installation::suffixe(&programme, ".precedent");
        ecrire(&precedent.join("gardee.txt"), "une version gardée");
        essayer(&dossier, &exe, None).unwrap();
        assert_eq!(lire(&precedent.join("gardee.txt")), "une version gardée");
        assert_eq!(lire(&programme.join(nom())), "exe v3");
    }

    #[test]
    fn une_v3_deja_installee_passe_par_la_mise_a_jour_habituelle() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let programme = dossier.programme();
        let infos = format!("{} 2.9.0\n", identite::NOM);
        ecrire(&programme.join(nom()), "exe v3 d'avant");
        ecrire(&programme.join("BUILDINFO.txt"), &infos);
        let liste = format!("{}\nBUILDINFO.txt\n", nom());
        ecrire(&programme.join(LISTE), &liste);
        essayer(&dossier, &exe, None).unwrap();
        assert_eq!(lire(&programme.join(nom())), "exe v3");
        let versions = dossier.sauvegardes().join(installation::DOSSIER_VERSIONS);
        let gardee = versions.join("2.9.0").join(nom());
        assert_eq!(lire(&gardee), "exe v3 d'avant");
        assert!(!installation::suffixe(&programme, ".precedent").exists());
    }

    #[test]
    fn le_serveur_de_la_v2_qui_ne_s_arrete_pas_arrete_la_mise_a_jour() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let programme = dossier.programme();
        version_2(&programme);
        let demande = demande(Some(programme.join(nom())));
        let systeme = faux(100_000, 0);
        let issue = mettre_a_jour(&demande, &dossier, &exe, &systeme);
        let refus = issue.unwrap_err();
        assert_eq!(refus.message, PAS_ARRETEE);
        // La v2 tourne encore : rien à relancer, rien de touché.
        assert_eq!(refus.relancer, None);
        assert!(systeme.ecoule() >= ATTENTE_PROCESSUS);
        assert_eq!(lire(&programme.join(nom())), "lanceur v2");
        assert!(systeme.icone_attendue.get().is_none());
    }

    #[test]
    fn programme_qui_se_libere_a_temps_est_installe() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        version_2(&dossier.programme());
        // L'icône de la v2 le tient environ 12 s ; l'attente va jusqu'à 15 s.
        let systeme = faux(0, 12);
        let issue = mettre_a_jour(&demande(None), &dossier, &exe, &systeme);
        issue.unwrap();
        assert!(systeme.ecoule() >= Duration::from_secs(12));
        assert!(systeme.ecoule() < ATTENTE_DOSSIER);
        assert_eq!(lire(&dossier.programme().join(nom())), "exe v3");
    }

    #[test]
    fn programme_qui_reste_ouvert_renonce_et_relance_l_ancien_lanceur() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let programme = dossier.programme();
        version_2(&programme);
        let ancien = programme.join(nom());
        let demande = demande(Some(ancien.clone()));
        let systeme = faux(0, 100_000);
        let issue = mettre_a_jour(&demande, &dossier, &exe, &systeme);
        let refus = issue.unwrap_err();
        assert!(refus.message.contains("tourne encore"), "{}", refus.message);
        assert_eq!(refus.relancer, Some(ancien));
        assert!(systeme.ecoule() >= ATTENTE_DOSSIER);
        // La v2 est intacte, sans copie ni précédent laissés.
        assert_eq!(lire(&programme.join(nom())), "lanceur v2");
        assert!(!installation::suffixe(&programme, ".nouveau").exists());
        assert!(!installation::suffixe(&programme, ".precedent").exists());
    }

    #[test]
    fn un_zip_incomplet_ne_change_rien_et_relance_l_ancien_lanceur() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        let programme = dossier.programme();
        version_2(&programme);
        let racine = exe.parent().unwrap().to_path_buf();
        fs::remove_file(racine.join("LICENCES-TIERS.txt")).unwrap();
        let ancien = programme.join(nom());
        let refus = essayer(&dossier, &exe, Some(ancien.clone())).unwrap_err();
        assert!(refus.message.contains("LICENCES"), "{}", refus.message);
        assert_eq!(refus.relancer, Some(ancien));
        assert_eq!(lire(&programme.join(nom())), "lanceur v2");
        assert!(!programme.join("BUILDINFO.txt").exists());

        // Sans fichiers.txt, c'est le même refus.
        fs::remove_file(racine.join(LISTE)).unwrap();
        let refus = essayer(&dossier, &exe, None).unwrap_err();
        assert_eq!(refus.message, SANS_LISTE);
        assert_eq!(lire(&programme.join(nom())), "lanceur v2");
    }

    #[test]
    fn un_ancien_lanceur_disparu_n_est_pas_relance() {
        let temp = tempfile::tempdir().unwrap();
        let (dossier, exe) = monde(temp.path());
        fs::remove_file(exe.parent().unwrap().join(LISTE)).unwrap();
        let disparu = temp.path().join("v2").join(nom());
        let refus = essayer(&dossier, &exe, Some(disparu)).unwrap_err();
        assert_eq!(refus.relancer, None);
    }

    #[test]
    fn un_lanceur_deja_dans_programme_n_a_rien_a_installer() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path().join("donnees"));
        let exe = zip_v3(&dossier.programme());
        let refus = essayer(&dossier, &exe, None).unwrap_err();
        assert_eq!(refus.message, DEJA_INSTALLE);
        // Le lanceur de programme.precedent\ non plus.
        let precedent = installation::suffixe(&dossier.programme(), ".precedent");
        let exe = zip_v3(&precedent);
        let refus = essayer(&dossier, &exe, None).unwrap_err();
        assert_eq!(refus.message, DEJA_INSTALLE);
    }

    #[test]
    fn un_lanceur_introuvable_est_refuse() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path().join("donnees"));
        let exe = temp.path().join("absent").join(nom());
        let refus = essayer(&dossier, &exe, None).unwrap_err();
        assert_eq!(refus.message, SANS_LANCEUR);
    }

    #[test]
    fn un_remplacement_qui_echoue_remet_tout_en_place() {
        let temp = tempfile::tempdir().unwrap();
        let programme = temp.path().join("programme");
        let nouveau = temp.path().join("programme.nouveau");
        let precedent = temp.path().join("programme.precedent");
        ecrire(&programme.join("v2.txt"), "la v2");
        ecrire(&nouveau.join("v3.txt"), "la v3");
        ecrire(&precedent.join("avant.txt"), "d'avant la v2");
        // Le dernier pas échoue : la nouvelle version ne peut pas prendre la place de programme\.
        let echouer = |de: &Path, vers: &Path| -> io::Result<()> {
            if de == nouveau && vers == programme {
                return Err(io::Error::other("essai"));
            }
            fs::rename(de, vers)
        };
        let resultat = remplacer_en_bloc(&programme, &nouveau, &precedent, echouer);
        assert!(resultat.is_err());
        assert_eq!(lire(&programme.join("v2.txt")), "la v2");
        assert_eq!(lire(&precedent.join("avant.txt")), "d'avant la v2");
        assert_eq!(lire(&nouveau.join("v3.txt")), "la v3");
        assert!(!installation::suffixe(&precedent, ".avant").exists());
    }

    #[test]
    fn un_premier_pas_qui_echoue_remet_le_precedent_en_place() {
        let temp = tempfile::tempdir().unwrap();
        let programme = temp.path().join("programme");
        let nouveau = temp.path().join("programme.nouveau");
        let precedent = temp.path().join("programme.precedent");
        ecrire(&programme.join("v2.txt"), "la v2");
        ecrire(&nouveau.join("v3.txt"), "la v3");
        ecrire(&precedent.join("avant.txt"), "d'avant la v2");
        // La v2 ne peut pas passer dans programme.precedent\.
        let echouer = |de: &Path, vers: &Path| -> io::Result<()> {
            if de == programme && vers == precedent {
                return Err(io::Error::other("essai"));
            }
            fs::rename(de, vers)
        };
        let resultat = remplacer_en_bloc(&programme, &nouveau, &precedent, echouer);
        assert!(resultat.is_err());
        assert_eq!(lire(&programme.join("v2.txt")), "la v2");
        assert_eq!(lire(&precedent.join("avant.txt")), "d'avant la v2");
        assert!(!installation::suffixe(&precedent, ".avant").exists());
    }

    #[test]
    fn un_dossier_ordinaire_se_renomme_et_reprend_son_nom() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = temp.path().join("programme");
        ecrire(&dossier.join("a.txt"), "a");
        assert!(dossier_renommable(&dossier));
        assert_eq!(lire(&dossier.join("a.txt")), "a");
        assert!(!dossier_renommable(&temp.path().join("absent")));
    }

    fn ecrire_relais(dossier: &Dossier, contenu: &str) -> PathBuf {
        let chemin = fichier_relais(dossier);
        ecrire(&chemin, contenu);
        chemin
    }

    #[test]
    fn le_bilan_note_la_mise_a_jour_installee() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let contenu = r#"{"de": "2.4.1", "vers": "3.0.0", "date": "2026-11-25T10:00:00"}"#;
        let chemin = ecrire_relais(&dossier, contenu);
        let bilan = bilan_au_demarrage(&dossier, "3.0.0").unwrap();
        assert!(bilan.installee);
        assert_eq!(bilan.de, "2.4.1");
        assert_eq!(bilan.vers, "3.0.0");
        assert!(!chemin.exists(), "le relais est lu une fois");
        let operations = journal::lire(&dossier);
        assert_eq!(operations.len(), 1);
        let operation = operations.first().unwrap();
        assert_eq!(operation.action, "Mise à jour installée");
        assert_eq!(operation.objet, "2.4.1 → 3.0.0");
        assert_eq!(operation.resultat, Resultat::Ok);
        // Une seconde lecture ne note rien de plus.
        assert_eq!(bilan_au_demarrage(&dossier, "3.0.0"), None);
        assert_eq!(journal::lire(&dossier).len(), 1);
    }

    #[test]
    fn le_bilan_note_une_mise_a_jour_non_installee() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        ecrire_relais(&dossier, r#"{"de": "2.4.1", "vers": "3.0.0"}"#);
        let bilan = bilan_au_demarrage(&dossier, "2.4.1").unwrap();
        assert!(!bilan.installee);
        assert_eq!(bilan.tourne, "2.4.1");
        let operations = journal::lire(&dossier);
        let operation = operations.first().unwrap();
        assert_eq!(operation.action, "Mise à jour non installée");
        assert_eq!(operation.objet, "2.4.1 → 3.0.0");
        assert_eq!(operation.resultat, Resultat::Erreur);
        let detail = operation.detail.as_deref().unwrap_or_default();
        assert!(detail.contains("2.4.1"), "{detail}");
    }

    #[test]
    fn un_relais_incomplet_ou_illisible_ne_fait_pas_de_faux_bilan() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        assert_eq!(bilan_au_demarrage(&dossier, "3.0.0"), None);
        let chemin = ecrire_relais(&dossier, "pas du json");
        assert_eq!(bilan_au_demarrage(&dossier, "3.0.0"), None);
        assert!(chemin.exists(), "un relais illisible reste");
        assert!(journal::lire(&dossier).is_empty());
        // Sans version annoncée, c'est un échec dit avec des points d'interrogation.
        ecrire_relais(&dossier, "{}");
        let bilan = bilan_au_demarrage(&dossier, "3.0.0").unwrap();
        assert!(!bilan.installee);
        let operations = journal::lire(&dossier);
        assert_eq!(operations.first().unwrap().objet, "? → ?");
    }

    #[test]
    fn le_relais_est_dans_le_dossier_de_travail_des_mises_a_jour() {
        let dossier = Dossier::a("donnees");
        let travail = Path::new("donnees").join("cache").join("mise-a-jour");
        assert_eq!(fichier_relais(&dossier), travail.join(FICHIER_RELAIS));
    }
}
