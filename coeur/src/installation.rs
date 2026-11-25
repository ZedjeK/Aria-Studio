//! Installation par l'exe lui-même : lancé depuis le zip décompressé, il propose de se ranger dans `programme\` du
//! dossier de données avec tout ce que liste `fichiers.txt`, en gardant la version qui s'y trouvait. Ce n'est pas
//! un installeur : rien n'est écrit hors du dossier de données, sauf le raccourci demandé, et l'exe reste
//! utilisable là où il est (« Lancer sans installer »).
//!
//! Les copies ne reprennent que le contenu des fichiers : la marque « téléchargé d'Internet » (flux
//! `Zone.Identifier`) reste sur le zip et ne suit pas l'appli installée.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::{Dossier, Reglages, identite};

/// Le fichier qui liste ce que livre une version, un nom par ligne (écrit par `tools\empaqueter.ps1`).
pub const LISTE: &str = "fichiers.txt";

/// Le fichier qui dit d'où vient le zip ; sa première ligne finit par le numéro de version.
pub const INFOS_BUILD: &str = "BUILDINFO.txt";

/// Le dossier des versions gardées, dans les sauvegardes.
pub const DOSSIER_VERSIONS: &str = "versions";

/// Argument qui dispense l'appli de proposer l'installation, pour ce lancement.
pub const ARGUMENT_PORTABLE: &str = "--portable";

const SUFFIXE_NOUVEAU: &str = ".nouveau";
const SUFFIXE_ANCIEN: &str = ".ancien";
const SUFFIXE_TEMPORAIRE: &str = ".tmp";
/// Le dossier où la v2 est gardée entière quand la v3 prend sa place, comme le faisait la v2 pour sa version d'avant.
const SUFFIXE_PRECEDENT: &str = ".precedent";
const SUFFIXE_AVANT: &str = ".avant";

/// Le nom de l'exe livré, sous lequel il est installé.
pub fn nom_exe() -> String {
    format!("{}.exe", identite::NOM_EXE)
}

/// Où tourne l'exe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Dans `programme\` : installée.
    Installee,
    /// Téléchargements, Bureau, clé USB… : l'exe peut s'installer.
    Ailleurs,
}

/// Les deux dossiers sont-ils le même ? Sans tenir compte de la casse, comme Windows.
pub(crate) fn meme_dossier(a: &Path, b: &Path) -> bool {
    let normal = |p: &Path| {
        let complet = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        complet
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    normal(a) == normal(b)
}

/// Où tourne cet exe, par rapport à `programme\`.
pub fn place_de(exe: &Path, programme: &Path) -> Place {
    match exe.parent() {
        Some(dossier) if meme_dossier(dossier, programme) => Place::Installee,
        _ => Place::Ailleurs,
    }
}

/// La liste des fichiers livrés, lue à côté de l'exe ; rien si elle manque ou ne convient pas (un nom qui sortirait
/// du dossier, ou pas d'exe de l'appli dedans).
///
/// # Errors
/// Une lecture qui échoue.
pub fn lire_liste(dossier: &Path) -> io::Result<Option<Vec<String>>> {
    let chemin = dossier.join(LISTE);
    let texte = match fs::read_to_string(&chemin) {
        Ok(texte) => texte,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let noms: Vec<String> = texte
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    let simple = |n: &String| {
        Path::new(n).file_name().is_some_and(|f| f == n.as_str())
            && !n.contains(['\\', '/', ':'])
            && !n.eq_ignore_ascii_case(LISTE)
    };
    let exe = nom_exe();
    if noms.is_empty() || !noms.iter().all(simple) || !noms.iter().any(|n| n.eq_ignore_ascii_case(&exe)) {
        return Ok(None);
    }
    Ok(Some(noms))
}

/// Les noms de la liste absents du dossier.
pub fn manquants<'a>(dossier: &Path, noms: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    noms.into_iter()
        .filter(|n| !dossier.join(n).is_file())
        .cloned()
        .collect()
}

/// Copie le contenu seul (sans flux de données secondaire, donc sans la marque « téléchargé d'Internet ») et
/// garde la date de modification.
///
/// # Errors
/// Une lecture ou une écriture qui échoue.
pub fn copier_contenu(de: &Path, vers: &Path) -> io::Result<()> {
    let mut entree = File::open(de)?;
    let mut sortie = File::create(vers)?;
    io::copy(&mut entree, &mut sortie)?;
    if let Ok(date) = entree.metadata().and_then(|m| m.modified()) {
        sortie.set_modified(date)?;
    }
    sortie.sync_all()
}

/// Proposer l'installation : appli installable, proposition non coupée, pas de `--portable`, exe lancé hors de
/// `programme\` et accompagné de son `fichiers.txt` (un exe compilé à la main, sans zip, ne propose rien).
pub fn doit_proposer<I, S>(exe: &Path, dossier: &Dossier, reglages: &Reglages, arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    identite::INSTALLABLE
        && reglages.proposer_installation
        && !arguments
            .into_iter()
            .any(|a| a.as_ref().eq_ignore_ascii_case(ARGUMENT_PORTABLE))
        && place_de(exe, &dossier.programme()) == Place::Ailleurs
        && exe
            .parent()
            .is_some_and(|d| lire_liste(d).ok().flatten().is_some())
}

/// La version installée dans `programme\`, lue dans son `BUILDINFO.txt` ; rien s'il n'y a pas d'appli installée.
pub fn version_installee(programme: &Path) -> Option<String> {
    if !programme.join(nom_exe()).is_file() {
        return None;
    }
    let infos = fs::read_to_string(programme.join(INFOS_BUILD)).ok();
    let version = infos
        .as_deref()
        .and_then(|t| t.lines().next())
        .and_then(|l| l.split_whitespace().last())
        .filter(|v| crate::mises_a_jour::lire_actuelle(v).is_some());
    Some(version.unwrap_or("precedente").to_owned())
}

/// Ce qui occupe `programme\`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installee {
    /// Rien : pas d'exe de l'appli.
    Rien,
    /// Une v3, avec sa version (« precedente » si BUILDINFO.txt ne la dit pas).
    Version(String),
    /// Une version 2 : l'exe du lanceur, avec `python\`, `app\` et `web\` autour, mais pas de `fichiers.txt`.
    Ancienne,
}

/// Ce qui est installé dans `programme\` : rien, une v3 (elle a toujours son `fichiers.txt`), ou une v2.
pub fn installee(programme: &Path) -> Installee {
    match version_installee(programme) {
        None => Installee::Rien,
        Some(_) if !programme.join(LISTE).is_file() => Installee::Ancienne,
        Some(version) => Installee::Version(version),
    }
}

/// Où la v2 est gardée entière quand la v3 prend sa place : `programme.precedent`, à côté de `programme`.
pub fn dossier_v2_gardee(programme: &Path) -> PathBuf {
    suffixe(programme, SUFFIXE_PRECEDENT)
}

/// Les dossiers que touche l'installation.
#[derive(Debug, Clone)]
pub struct Chemins {
    /// `programme\`.
    pub programme: PathBuf,
    /// `sauvegardes\versions\`.
    pub versions: PathBuf,
    /// `cache\installation\` : dossier de travail, vidé à la fin.
    pub travail: PathBuf,
}

impl Chemins {
    /// Ceux du dossier de données.
    pub fn du_dossier(dossier: &Dossier) -> Self {
        Self {
            programme: dossier.programme(),
            versions: dossier.sauvegardes().join(DOSSIER_VERSIONS),
            travail: dossier.cache().join("installation"),
        }
    }
}

/// Une installation refusée ou ratée : rien n'a changé, le message dit pourquoi.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Echec(pub String);

const DECOMPRESSER: &str = "Décompresse d'abord le zip en entier (clic droit, « Extraire tout »).";

/// Range dans `programme\` l'exe et tout ce que liste son `fichiers.txt`, en gardant la version qui s'y trouvait,
/// et renvoie le chemin de l'exe installé, à relancer. À la moindre erreur, rien n'a changé.
///
/// # Errors
/// [`Echec`], avec la raison en français.
pub fn installer(exe: &Path, chemins: &Chemins) -> Result<PathBuf, Echec> {
    let source = fs::canonicalize(exe).map_err(|_| Echec("L'exe de l'appli est introuvable.".to_owned()))?;
    let dossier_source = source
        .parent()
        .ok_or_else(|| Echec("Le dossier de l'exe est introuvable.".to_owned()))?;
    let noms = lire_liste(dossier_source)
        .map_err(|e| Echec(format!("La liste des fichiers n'a pas pu être lue : {e}")))?
        .ok_or_else(|| Echec(format!("{LISTE} manque à côté de l'exe. {DECOMPRESSER}")))?;
    let exe_livre = nom_exe();
    // L'exe en cours peut porter un autre nom (« Aria-X (1).exe ») : il est copié sous le bon.
    let absents = manquants(
        dossier_source,
        noms.iter().filter(|n| !n.eq_ignore_ascii_case(&exe_livre)),
    );
    if !absents.is_empty() {
        return Err(Echec(format!(
            "Il manque à côté de l'exe : {}. {DECOMPRESSER}",
            absents.join(", ")
        )));
    }

    let resultat = preparer_et_echanger(&source, dossier_source, &noms, chemins);
    supprimer_dossier(&chemins.travail);
    resultat?;
    Ok(chemins.programme.join(exe_livre))
}

fn preparer_et_echanger(
    source: &Path,
    dossier_source: &Path,
    noms: &[String],
    chemins: &Chemins,
) -> Result<(), Echec> {
    let copie = |e: io::Error| Echec(format!("Les fichiers n'ont pas pu être copiés : {e}"));
    supprimer_dossier(&chemins.travail);
    fs::create_dir_all(&chemins.travail).map_err(copie)?;
    let exe_livre = nom_exe();
    for nom in noms {
        let de = if nom.eq_ignore_ascii_case(&exe_livre) {
            source.to_path_buf()
        } else {
            dossier_source.join(nom)
        };
        copier_contenu(&de, &chemins.travail.join(nom)).map_err(copie)?;
    }
    copier_contenu(&dossier_source.join(LISTE), &chemins.travail.join(LISTE)).map_err(copie)?;
    let mut tous = noms.to_vec();
    tous.push(LISTE.to_owned());
    match installee(&chemins.programme) {
        // Une v2 : elle est gardée entière à côté, la v3 prend `programme\` à neuf.
        Installee::Ancienne => return remplacer_la_v2(&tous, chemins),
        Installee::Version(version) => {
            garder(&chemins.programme, &version, &chemins.versions)
                .map_err(|e| Echec(format!("La version installée n'a pas pu être mise de côté : {e}")))?;
        }
        Installee::Rien => {}
    }
    fs::create_dir_all(&chemins.programme).map_err(copie)?;
    echanger(&chemins.travail, &chemins.programme, &tous)
}

/// La v2 installée devient `programme.precedent\`, entière (son `python\`, son `app\`, son `web\` : on peut la
/// remettre), et la v3 s'installe dans un `programme\` neuf. Le `programme.precedent\` d'avant ne disparaît
/// qu'une fois la v3 en place. À la moindre erreur, tout reprend sa place.
fn remplacer_la_v2(noms: &[String], chemins: &Chemins) -> Result<(), Echec> {
    let programme = &chemins.programme;
    let precedent = dossier_v2_gardee(programme);
    let avant = suffixe(&precedent, SUFFIXE_AVANT);
    supprimer_dossier(&avant);
    let avait_precedent = precedent.exists();
    let remettre_l_avant = || {
        if avait_precedent {
            let _ = fs::rename(&avant, &precedent);
        }
    };
    if avait_precedent && let Err(e) = fs::rename(&precedent, &avant) {
        return Err(Echec(format!(
            "Le dossier « {} » n'a pas pu être mis de côté : {e}. Rien n'a été changé.",
            precedent.display()
        )));
    }
    if let Err(e) = fs::rename(programme, &precedent) {
        remettre_l_avant();
        return Err(Echec(format!(
            "La version 2 n'a pas pu être mise de côté : {e}. Ferme-la (icône de la barre des tâches, \
             « Quitter »), puis recommence. Rien n'a été changé."
        )));
    }
    let resultat = fs::create_dir_all(programme)
        .map_err(|e| Echec(format!("Les fichiers n'ont pas pu être copiés : {e}")))
        .and_then(|()| echanger(&chemins.travail, programme, noms));
    match resultat {
        Ok(()) => {
            supprimer_dossier(&avant);
            Ok(())
        }
        Err(e) => {
            supprimer_dossier(programme);
            // La v2 reprend sa place.
            let _ = fs::rename(&precedent, programme);
            remettre_l_avant();
            Err(e)
        }
    }
}

/// Copie la version installée dans `versions`, sous son numéro, et retire les versions gardées avant elle. La copie
/// se fait à côté, puis prend sa place : une copie interrompue ne remplace jamais une version déjà gardée.
///
/// # Errors
/// Une copie qui échoue, ou pas d'exe dans `programme`.
pub fn garder(programme: &Path, version: &str, versions: &Path) -> io::Result<()> {
    let noms = lire_liste(programme)
        .ok()
        .flatten()
        .unwrap_or_else(|| vec![nom_exe()]);
    let temporaire = versions.join(format!("{version}{SUFFIXE_TEMPORAIRE}"));
    supprimer_dossier(&temporaire);
    fs::create_dir_all(&temporaire)?;
    for nom in noms.iter().map(String::as_str).chain([LISTE]) {
        let de = programme.join(nom);
        if de.is_file() {
            copier_contenu(&de, &temporaire.join(nom))?;
        }
    }
    if !temporaire.join(nom_exe()).is_file() {
        supprimer_dossier(&temporaire);
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} introuvable dans « {} »", nom_exe(), programme.display()),
        ));
    }
    for entree in fs::read_dir(versions)? {
        let autre = entree?.path();
        if autre.is_dir() && autre != temporaire {
            fs::remove_dir_all(&autre)?;
        }
    }
    fs::rename(&temporaire, versions.join(version))
}

/// Remplace, dans `cible`, chaque fichier de la liste par celui de `source`. Le fichier en place est renommé en
/// « .ancien » (un exe qui tourne se renomme, il ne s'écrase pas). À la première erreur, tout est défait.
///
/// # Errors
/// [`Echec`] : rien n'a été changé.
pub fn echanger(source: &Path, cible: &Path, noms: &[String]) -> Result<(), Echec> {
    let mut faits: Vec<(PathBuf, bool)> = Vec::new();
    for nom in noms {
        let de = source.join(nom);
        if !de.is_file() {
            continue;
        }
        let place = cible.join(nom);
        let nouveau = suffixe(&place, SUFFIXE_NOUVEAU);
        let ancien = suffixe(&place, SUFFIXE_ANCIEN);
        let mut avait_ancien = false;
        let etape = (|| -> io::Result<()> {
            copier_contenu(&de, &nouveau)?;
            if place.exists() {
                let _ = fs::remove_file(&ancien);
                fs::rename(&place, &ancien)?;
                avait_ancien = true;
            }
            fs::rename(&nouveau, &place)
        })();
        if let Err(e) = etape {
            let _ = fs::remove_file(&nouveau);
            // Le fichier en place avait déjà été mis de côté : il reprend sa place.
            if avait_ancien && !place.exists() {
                let _ = fs::rename(&ancien, &place);
            }
            for (fait, avait) in faits.iter().rev() {
                let _ = fs::remove_file(fait);
                if *avait {
                    let _ = fs::rename(suffixe(fait, SUFFIXE_ANCIEN), fait);
                }
            }
            return Err(Echec(format!(
                "« {nom} » n'a pas pu être remplacé : {e}. Rien n'a été changé."
            )));
        }
        faits.push((place, avait_ancien));
    }
    if faits.is_empty() {
        return Err(Echec("Aucun fichier à installer.".to_owned()));
    }
    Ok(())
}

/// Retire les fichiers « .ancien » laissés par un remplacement, une fois l'ancienne version arrêtée.
pub fn nettoyer(programme: &Path) {
    let Ok(entrees) = fs::read_dir(programme) else {
        return;
    };
    for entree in entrees.flatten() {
        if entree.file_name().to_string_lossy().ends_with(SUFFIXE_ANCIEN) {
            // Encore utilisé : le prochain ménage le retirera.
            let _ = fs::remove_file(entree.path());
        }
    }
}

pub(crate) fn suffixe(chemin: &Path, ajout: &str) -> PathBuf {
    let mut nom = chemin.as_os_str().to_owned();
    nom.push(ajout);
    PathBuf::from(nom)
}

/// Retire la marque « téléchargé depuis Internet » d'un fichier : le flux `Zone.Identifier`, que Windows pose sur
/// ce qui vient d'un navigateur et que `fs::copy` recopierait. Sans marque, il n'y a rien à retirer.
#[cfg(windows)]
pub fn retirer_marque(fichier: &Path) {
    let flux = suffixe(fichier, ":Zone.Identifier");
    // Pas de marque : rien à retirer, et une marque tenace n'empêche pas d'installer.
    let _ = fs::remove_file(flux);
}

/// Hors de Windows, il n'y a pas de marque.
#[cfg(not(windows))]
pub fn retirer_marque(_fichier: &Path) {}

/// Dossier de travail resté : il sera retiré au prochain passage.
pub(crate) fn supprimer_dossier(dossier: &Path) {
    if dossier.exists() {
        let _ = fs::remove_dir_all(dossier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un zip décompressé : l'exe, une bibliothèque, BUILDINFO.txt et fichiers.txt.
    fn zip(dossier: &Path, version: &str, contenu_exe: &str) {
        fs::create_dir_all(dossier).unwrap();
        fs::write(dossier.join(nom_exe()), contenu_exe).unwrap();
        fs::write(dossier.join("LICENCES-TIERS.txt"), "licences").unwrap();
        fs::write(
            dossier.join(INFOS_BUILD),
            format!("{} {version}\nCommit : abc\n", identite::NOM),
        )
        .unwrap();
        let liste = [INFOS_BUILD.to_owned(), "LICENCES-TIERS.txt".to_owned(), nom_exe()].join("\n");
        fs::write(dossier.join(LISTE), liste).unwrap();
    }

    fn chemins(racine: &Path) -> Chemins {
        Chemins::du_dossier(&Dossier::a(racine))
    }

    #[test]
    fn la_liste_refuse_un_nom_qui_sortirait_du_dossier() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(lire_liste(temp.path()).unwrap(), None);
        for liste in [
            format!("..\\{}\n", nom_exe()),
            format!("{}\nsous/dossier.dll\n", nom_exe()),
            "autre.exe\n".to_owned(),
            format!("{}\n{LISTE}\n", nom_exe()),
        ] {
            fs::write(temp.path().join(LISTE), liste).unwrap();
            assert_eq!(lire_liste(temp.path()).unwrap(), None);
        }
        fs::write(
            temp.path().join(LISTE),
            format!("\n {} \nBUILDINFO.txt\n", nom_exe()),
        )
        .unwrap();
        assert_eq!(
            lire_liste(temp.path()).unwrap(),
            Some(vec![nom_exe(), "BUILDINFO.txt".to_owned()])
        );
    }

    #[test]
    fn installer_range_les_fichiers_et_garde_la_version_precedente() {
        let temp = tempfile::tempdir().unwrap();
        let chemins = chemins(&temp.path().join("donnees"));

        let premier = temp.path().join("telechargements").join("v1");
        zip(&premier, "1.0.0", "exe 1");
        let installe = installer(&premier.join(nom_exe()), &chemins).unwrap();
        assert_eq!(installe, chemins.programme.join(nom_exe()));
        assert_eq!(fs::read_to_string(&installe).unwrap(), "exe 1");
        assert!(chemins.programme.join(LISTE).is_file());
        assert!(!chemins.travail.exists(), "le dossier de travail est vidé");
        assert_eq!(version_installee(&chemins.programme).as_deref(), Some("1.0.0"));

        let second = temp.path().join("telechargements").join("v2");
        zip(&second, "1.1.0", "exe 2");
        // Lancé sous un autre nom (téléchargé deux fois) : installé sous le bon.
        fs::rename(second.join(nom_exe()), second.join("copie (1).exe")).unwrap();
        installer(&second.join("copie (1).exe"), &chemins).unwrap();
        assert_eq!(fs::read_to_string(&installe).unwrap(), "exe 2");
        let gardee = chemins.versions.join("1.0.0");
        assert_eq!(fs::read_to_string(gardee.join(nom_exe())).unwrap(), "exe 1");
        assert!(chemins.programme.join(format!("{}.ancien", nom_exe())).is_file());
        nettoyer(&chemins.programme);
        assert!(!chemins.programme.join(format!("{}.ancien", nom_exe())).exists());
    }

    #[test]
    fn un_zip_incomplet_ne_change_rien() {
        let temp = tempfile::tempdir().unwrap();
        let chemins = chemins(&temp.path().join("donnees"));
        let source = temp.path().join("zip");
        zip(&source, "1.0.0", "exe");
        fs::remove_file(source.join("LICENCES-TIERS.txt")).unwrap();
        let echec = installer(&source.join(nom_exe()), &chemins).unwrap_err();
        assert!(echec.0.contains("LICENCES-TIERS.txt"), "{echec}");
        assert!(!chemins.programme.exists());

        fs::remove_file(source.join(LISTE)).unwrap();
        assert!(
            installer(&source.join(nom_exe()), &chemins)
                .unwrap_err()
                .0
                .contains(LISTE)
        );
    }

    #[test]
    fn echanger_defait_tout_a_la_premiere_erreur() {
        let temp = tempfile::tempdir().unwrap();
        let (source, cible) = (temp.path().join("source"), temp.path().join("cible"));
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&cible).unwrap();
        fs::write(source.join("a.txt"), "neuf a").unwrap();
        fs::write(source.join("b.txt"), "neuf b").unwrap();
        fs::write(cible.join("a.txt"), "vieux a").unwrap();
        // La copie de b.txt échoue : un dossier occupe déjà le nom de sa copie de travail.
        fs::create_dir_all(cible.join("b.txt.nouveau").join("plein")).unwrap();
        let noms = vec!["a.txt".to_owned(), "b.txt".to_owned()];
        assert!(echanger(&source, &cible, &noms).is_err());
        assert_eq!(fs::read_to_string(cible.join("a.txt")).unwrap(), "vieux a");
        assert!(!cible.join("a.txt.ancien").exists());
        assert!(!cible.join("a.txt.nouveau").exists());
    }

    #[test]
    fn la_place_de_l_exe() {
        let temp = tempfile::tempdir().unwrap();
        let programme = temp.path().join("programme");
        fs::create_dir_all(&programme).unwrap();
        assert_eq!(place_de(&programme.join(nom_exe()), &programme), Place::Installee);
        assert_eq!(
            place_de(&temp.path().join(nom_exe()), &programme),
            Place::Ailleurs
        );
    }

    #[test]
    fn la_proposition_suit_les_reglages_et_portable() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path().join("donnees"));
        let source = temp.path().join("zip");
        zip(&source, "1.0.0", "exe");
        let exe = source.join(nom_exe());
        let reglages = Reglages::default();
        assert_eq!(
            doit_proposer(&exe, &dossier, &reglages, ["x"]),
            identite::INSTALLABLE
        );
        assert!(!doit_proposer(&exe, &dossier, &reglages, ["--PORTABLE"]));
        let coupe = Reglages {
            proposer_installation: false,
            ..Reglages::default()
        };
        assert!(!doit_proposer(&exe, &dossier, &coupe, ["x"]));
        fs::remove_file(source.join(LISTE)).unwrap();
        assert!(!doit_proposer(&exe, &dossier, &reglages, ["x"]));
    }

    #[test]
    fn version_illisible_garde_quand_meme() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(nom_exe()), "exe").unwrap();
        assert_eq!(version_installee(temp.path()).as_deref(), Some("precedente"));
        assert_eq!(version_installee(&temp.path().join("vide")), None);
        // Une version d'essai se lit telle quelle dans BUILDINFO.txt.
        fs::write(temp.path().join(INFOS_BUILD), "Aria-Studio 3.0.0-essai.1\n").unwrap();
        assert_eq!(version_installee(temp.path()).as_deref(), Some("3.0.0-essai.1"));
    }

    #[cfg(windows)]
    #[test]
    fn la_marque_internet_ne_suit_pas_la_copie_et_se_retire() {
        let marque = "[ZoneTransfer]\r\nZoneId=3\r\n";
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("a.exe");
        fs::write(&source, "exe").unwrap();
        fs::write(suffixe(&source, ":Zone.Identifier"), marque).unwrap();
        let copie = temp.path().join("b.exe");
        copier_contenu(&source, &copie).unwrap();
        assert!(!suffixe(&copie, ":Zone.Identifier").exists());
        // Une marque posée sur la copie se retire, le fichier reste.
        fs::write(suffixe(&copie, ":Zone.Identifier"), marque).unwrap();
        assert!(suffixe(&copie, ":Zone.Identifier").exists());
        retirer_marque(&copie);
        assert!(!suffixe(&copie, ":Zone.Identifier").exists());
        assert_eq!(fs::read_to_string(&copie).unwrap(), "exe");
    }

    /// Une v2 installée : le lanceur, son `python\`, son `app\`, sans `fichiers.txt` ni BUILDINFO.txt.
    fn v2(programme: &Path) {
        fs::create_dir_all(programme.join("python")).unwrap();
        fs::create_dir_all(programme.join("app")).unwrap();
        fs::write(programme.join(nom_exe()), "lanceur v2").unwrap();
        fs::write(programme.join("python").join("python.exe"), "python").unwrap();
        fs::write(programme.join("app").join("main.py"), "app").unwrap();
    }

    #[test]
    fn programme_dit_ce_qui_y_est_installe() {
        let temp = tempfile::tempdir().unwrap();
        let programme = temp.path().join("programme");
        assert_eq!(installee(&programme), Installee::Rien);
        v2(&programme);
        assert_eq!(installee(&programme), Installee::Ancienne);
        let chemins = chemins(&temp.path().join("donnees"));
        let source = temp.path().join("zip");
        zip(&source, "3.0.0", "exe 3");
        installer(&source.join(nom_exe()), &chemins).unwrap();
        assert_eq!(
            installee(&chemins.programme),
            Installee::Version("3.0.0".to_owned())
        );
    }

    #[test]
    fn une_v2_deja_installee_est_gardee_entiere() {
        let temp = tempfile::tempdir().unwrap();
        let chemins = chemins(&temp.path().join("donnees"));
        v2(&chemins.programme);
        let precedent = suffixe(&chemins.programme, SUFFIXE_PRECEDENT);
        // Un `programme.precedent\` plus ancien : il disparaît, la v2 le remplace.
        fs::create_dir_all(&precedent).unwrap();
        fs::write(precedent.join("tres-ancien.txt"), "x").unwrap();
        let source = temp.path().join("telechargements").join("v3");
        zip(&source, "3.0.0", "exe 3");

        let installe = installer(&source.join(nom_exe()), &chemins).unwrap();

        assert_eq!(fs::read_to_string(&installe).unwrap(), "exe 3");
        assert!(chemins.programme.join(LISTE).is_file());
        assert!(
            !chemins.programme.join("python").exists(),
            "rien de la v2 ne reste dans programme"
        );
        assert_eq!(
            fs::read_to_string(precedent.join(nom_exe())).unwrap(),
            "lanceur v2"
        );
        assert!(precedent.join("python").join("python.exe").is_file());
        assert!(precedent.join("app").join("main.py").is_file());
        assert!(!precedent.join("tres-ancien.txt").exists());
        assert!(!suffixe(&precedent, SUFFIXE_AVANT).exists());
        assert!(!chemins.travail.exists());
        // La v3 suivante se garde comme d'habitude, dans les sauvegardes : la v2 ne bouge plus.
        let suivante = temp.path().join("telechargements").join("v3-1");
        zip(&suivante, "3.1.0", "exe 3.1");
        installer(&suivante.join(nom_exe()), &chemins).unwrap();
        assert_eq!(
            fs::read_to_string(chemins.versions.join("3.0.0").join(nom_exe())).unwrap(),
            "exe 3"
        );
        assert!(precedent.join("python").join("python.exe").is_file());
    }

    #[test]
    fn une_installation_ratee_remet_la_v2_et_son_precedent_en_place() {
        let temp = tempfile::tempdir().unwrap();
        let chemins = chemins(&temp.path().join("donnees"));
        v2(&chemins.programme);
        let precedent = suffixe(&chemins.programme, SUFFIXE_PRECEDENT);
        fs::create_dir_all(&precedent).unwrap();
        fs::write(precedent.join("avant.txt"), "x").unwrap();
        // Rien à copier depuis le dossier de travail : l'échange échoue après la mise de côté de la v2.
        fs::create_dir_all(&chemins.travail).unwrap();
        let echec = remplacer_la_v2(&["absent.txt".to_owned()], &chemins).unwrap_err();
        assert!(echec.0.contains("Aucun fichier"), "{echec}");
        assert_eq!(
            fs::read_to_string(chemins.programme.join(nom_exe())).unwrap(),
            "lanceur v2"
        );
        assert!(chemins.programme.join("python").join("python.exe").is_file());
        assert_eq!(fs::read_to_string(precedent.join("avant.txt")).unwrap(), "x");
        assert!(!suffixe(&precedent, SUFFIXE_AVANT).exists());
    }
}
