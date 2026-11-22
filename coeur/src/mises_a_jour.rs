//! Mise à jour intégrée : la dernière release du dépôt de l'appli sur GitHub, son zip vérifié (taille et empreinte
//! SHA-256 annoncées par GitHub), puis le remplacement des fichiers là où l'exe tourne, en gardant la version en
//! cours. Rien n'est installé sans l'accord de l'utilisateur : c'est l'interface qui déclenche l'installation.
//!
//! L'empreinte prouve que le téléchargement est intact, pas d'où il vient : la provenance repose sur le compte de
//! l'auteur, les releases immuables et l'attestation jointe.
//!
//! Ce module analyse, vérifie et installe ; la requête HTTPS elle-même est dans la crate `aria-appli`.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::identite;
use crate::installation::{self, Echec, LISTE};

/// Le compte GitHub des applis Aria.
pub const PROPRIETAIRE: &str = "ZedjeK";

/// Le dépôt de l'appli.
pub fn depot() -> String {
    format!("https://github.com/{PROPRIETAIRE}/{}", identite::NOM_EXE)
}

/// Le seul début d'adresse accepté pour un téléchargement : une release de ce dépôt.
pub fn prefixe_telechargement() -> String {
    format!("{}/releases/download/", depot())
}

/// La dernière release publiée, par l'API de GitHub.
pub fn adresse_derniere_release() -> String {
    format!(
        "https://api.github.com/repos/{PROPRIETAIRE}/{}/releases/latest",
        identite::NOM_EXE
    )
}

/// Une version X.Y.Z.
pub type Version = (u32, u32, u32);

/// Lit « X.Y.Z » ; rien pour tout autre texte.
pub fn lire_version(texte: &str) -> Option<Version> {
    let mut parties = texte.split('.');
    let mut nombre = || {
        parties
            .next()
            .filter(|p| !p.is_empty() && p.bytes().all(|o| o.is_ascii_digit()))
            .and_then(|p| p.parse::<u32>().ok())
    };
    let version = (nombre()?, nombre()?, nombre()?);
    parties.next().is_none().then_some(version)
}

/// Écrit une version en « X.Y.Z ».
pub fn ecrire_version((a, b, c): Version) -> String {
    format!("{a}.{b}.{c}")
}

/// Le zip d'une version, tel que GitHub l'annonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paquet {
    pub nom: String,
    pub adresse: String,
    pub taille: u64,
    /// En hexadécimal minuscule.
    pub sha256: String,
}

/// Une version plus récente que celle en cours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiseAJour {
    pub version: Version,
    /// La page de la release, à ouvrir pour lire ses notes.
    pub page: String,
    pub paquet: Paquet,
}

/// L'empreinte d'un champ `digest` de GitHub (« sha256:… ») ; rien si ce n'est pas une empreinte SHA-256.
pub fn lire_sha256(digest: &str) -> Option<String> {
    let hex = digest
        .get(..7)
        .filter(|debut| debut.eq_ignore_ascii_case("sha256:"))
        .and_then(|_| digest.get(7..))?
        .trim()
        .to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|o| o.is_ascii_hexdigit())).then_some(hex)
}

fn texte<'a>(valeur: &'a Value, cle: &str) -> Option<&'a str> {
    valeur.get(cle).and_then(Value::as_str)
}

/// Analyse la réponse de l'API : une mise à jour seulement si la release est publiée, plus récente, et porte un
/// zip `Aria-Nom-X.Y.Z.zip` de ce dépôt avec sa taille et son empreinte. Tout le reste : rien.
pub fn analyser(json: &str, actuelle: Version) -> Option<MiseAJour> {
    let racine: Value = serde_json::from_str(json).ok()?;
    if racine.get("draft").and_then(Value::as_bool) == Some(true)
        || racine.get("prerelease").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let version = texte(&racine, "tag_name")?
        .strip_prefix('v')
        .and_then(lire_version)?;
    if version <= actuelle {
        return None;
    }
    let attendu = format!("{}-{}.zip", identite::NOM_EXE, ecrire_version(version));
    let paquet = racine
        .get("assets")?
        .as_array()?
        .iter()
        .find(|a| texte(a, "name") == Some(attendu.as_str()))?;
    let adresse =
        texte(paquet, "browser_download_url").filter(|u| u.starts_with(&prefixe_telechargement()))?;
    let taille = paquet.get("size").and_then(Value::as_u64).filter(|t| *t > 0)?;
    let sha256 = texte(paquet, "digest").and_then(lire_sha256)?;
    // Seule une page de ce dépôt est proposée à l'ouverture.
    let depot = depot();
    let page = texte(&racine, "html_url")
        .filter(|p| p.starts_with(&format!("{depot}/")))
        .map_or_else(|| format!("{depot}/releases/latest"), str::to_owned);
    Some(MiseAJour {
        version,
        page,
        paquet: Paquet {
            nom: attendu,
            adresse: adresse.to_owned(),
            taille,
            sha256,
        },
    })
}

/// Le dossier de travail des mises à jour, dans le cache.
pub fn dossier_travail(dossier: &crate::Dossier) -> PathBuf {
    dossier.cache().join("mise-a-jour")
}

/// Vide le dossier de travail (zip ou extraction laissés par une mise à jour interrompue). Ne lève jamais.
pub fn nettoyer(travail: &Path) {
    if travail.exists() {
        let _ = fs::remove_dir_all(travail);
    }
}

fn invalide(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Écrit le flux téléchargé dans `cible`, en vérifiant au fil de l'eau la taille annoncée, puis l'empreinte. Le
/// fichier n'apparaît sous son nom que s'il est le bon ; sinon rien ne reste.
///
/// # Errors
/// Un fichier plus gros ou plus petit qu'annoncé, une empreinte différente, ou une erreur de lecture ou
/// d'écriture.
pub fn recevoir(mut flux: impl Read, paquet: &Paquet, cible: &Path) -> io::Result<()> {
    let partiel = cible.with_extension("part");
    let resultat = (|| {
        let mut sortie = File::create(&partiel)?;
        let mut empreinte = Sha256::new();
        let mut total: u64 = 0;
        let mut tampon = vec![0u8; 64 * 1024];
        loop {
            let lus = flux.read(&mut tampon)?;
            if lus == 0 {
                break;
            }
            let morceau = tampon.get(..lus).unwrap_or_default();
            total += u64::try_from(lus).unwrap_or(u64::MAX);
            if total > paquet.taille {
                return Err(invalide("le fichier téléchargé est plus gros qu'annoncé"));
            }
            empreinte.update(morceau);
            sortie.write_all(morceau)?;
        }
        sortie.sync_all()?;
        let obtenue: String = empreinte.finalize().iter().map(|o| format!("{o:02x}")).collect();
        if total != paquet.taille || obtenue != paquet.sha256 {
            return Err(invalide(
                "le fichier téléchargé ne correspond pas à l'empreinte annoncée par GitHub : rien n'est installé",
            ));
        }
        fs::rename(&partiel, cible)
    })();
    if partiel.exists() {
        let _ = fs::remove_file(&partiel);
    }
    resultat
}

/// Extrait le zip dans `vers`, sans rien laisser sortir de ce dossier.
fn extraire(zip: &Path, vers: &Path) -> io::Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(zip)?).map_err(io::Error::other)?;
    for i in 0..archive.len() {
        let mut entree = archive.by_index(i).map_err(io::Error::other)?;
        let relatif = entree
            .enclosed_name()
            .ok_or_else(|| invalide("le zip contient un chemin qui sortirait du dossier"))?;
        let chemin = vers.join(relatif);
        if entree.is_dir() {
            fs::create_dir_all(&chemin)?;
            continue;
        }
        if let Some(parent) = chemin.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entree, &mut File::create(&chemin)?)?;
    }
    Ok(())
}

/// Installe le zip vérifié dans `dossier_exe` (là où tourne l'exe), en gardant la version en cours dans
/// `versions`, et renvoie l'exe à relancer. À la moindre erreur, rien n'a changé.
///
/// # Errors
/// [`Echec`], avec la raison en français.
pub fn installer_zip(
    zip: &Path,
    dossier_exe: &Path,
    versions: &Path,
    travail: &Path,
) -> Result<PathBuf, Echec> {
    let extrait = travail.join("extrait");
    let resultat = (|| {
        let echec = |e: io::Error| Echec(format!("La mise à jour n'a pas pu être installée : {e}"));
        if extrait.exists() {
            fs::remove_dir_all(&extrait).map_err(echec)?;
        }
        extraire(zip, &extrait).map_err(echec)?;
        // Le zip peut tout ranger dans un seul dossier : c'est alors lui qui compte.
        let racine = un_seul_dossier(&extrait).unwrap_or_else(|| extrait.clone());
        let noms = installation::lire_liste(&racine).ok().flatten().ok_or_else(|| {
            Echec(format!(
                "Le zip ne contient pas de {LISTE} lisible : rien n'est installé."
            ))
        })?;
        let absents = installation::manquants(&racine, &noms);
        if !absents.is_empty() {
            return Err(Echec(format!(
                "Il manque dans le zip : {}. Rien n'est installé.",
                absents.join(", ")
            )));
        }
        if let Some(version) = installation::version_installee(dossier_exe) {
            installation::garder(dossier_exe, &version, versions)
                .map_err(|e| Echec(format!("La version en cours n'a pas pu être mise de côté : {e}")))?;
        }
        let mut tous = noms;
        tous.push(LISTE.to_owned());
        installation::echanger(&racine, dossier_exe, &tous)?;
        Ok(dossier_exe.join(installation::nom_exe()))
    })();
    if extrait.exists() {
        let _ = fs::remove_dir_all(&extrait);
    }
    if zip.starts_with(travail) {
        let _ = fs::remove_file(zip);
    }
    resultat
}

fn un_seul_dossier(dossier: &Path) -> Option<PathBuf> {
    let entrees: Vec<PathBuf> = fs::read_dir(dossier).ok()?.flatten().map(|e| e.path()).collect();
    match entrees.as_slice() {
        [seul] if seul.is_dir() => Some(seul.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn sha(octets: &[u8]) -> String {
        Sha256::digest(octets)
            .iter()
            .map(|o| format!("{o:02x}"))
            .collect()
    }

    fn reponse(tag: &str, nom: &str, adresse: &str, digest: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": false,
            "html_url": format!("{}/releases/tag/{tag}", depot()),
            "assets": [
                { "name": "autre.zip", "browser_download_url": adresse, "size": 1, "digest": digest },
                { "name": nom, "browser_download_url": adresse, "size": 1234, "digest": digest }
            ]
        })
        .to_string()
    }

    fn zip_attendu(version: &str) -> String {
        format!("{}-{version}.zip", identite::NOM_EXE)
    }

    #[test]
    fn une_release_plus_recente_est_proposee() {
        let nom = zip_attendu("1.2.0");
        let adresse = format!("{}v1.2.0/{nom}", prefixe_telechargement());
        let digest = format!("sha256:{}", "AB".repeat(32));
        let maj = analyser(&reponse("v1.2.0", &nom, &adresse, &digest), (1, 1, 9)).unwrap();
        assert_eq!(maj.version, (1, 2, 0));
        assert_eq!(maj.paquet.taille, 1234);
        assert_eq!(maj.paquet.sha256, "ab".repeat(32));
        assert!(maj.page.starts_with(&depot()));
    }

    #[test]
    fn tout_ce_qui_cloche_ne_propose_rien() {
        let nom = zip_attendu("1.2.0");
        let bonne = format!("{}v1.2.0/{nom}", prefixe_telechargement());
        let digest = format!("sha256:{}", "0".repeat(64));
        // Même version, ou plus ancienne.
        assert!(analyser(&reponse("v1.2.0", &nom, &bonne, &digest), (1, 2, 0)).is_none());
        assert!(analyser(&reponse("v1.2.0", &nom, &bonne, &digest), (2, 0, 0)).is_none());
        // Tag sans « v », ou pas en X.Y.Z.
        assert!(analyser(&reponse("1.2.0", &nom, &bonne, &digest), (1, 0, 0)).is_none());
        assert!(analyser(&reponse("v1.2", &nom, &bonne, &digest), (1, 0, 0)).is_none());
        // Adresse hors du dépôt.
        let ailleurs = format!("https://exemple.com/{nom}");
        assert!(analyser(&reponse("v1.2.0", &nom, &ailleurs, &digest), (1, 0, 0)).is_none());
        // Pas d'empreinte SHA-256.
        assert!(analyser(&reponse("v1.2.0", &nom, &bonne, "md5:abc"), (1, 0, 0)).is_none());
        // Pas de zip au bon nom.
        assert!(analyser(&reponse("v1.2.0", "autre-nom.zip", &bonne, &digest), (1, 0, 0)).is_none());
        // Brouillon, préversion, ou réponse illisible.
        let brouillon = reponse("v1.2.0", &nom, &bonne, &digest).replace("\"draft\":false", "\"draft\":true");
        assert!(analyser(&brouillon, (1, 0, 0)).is_none());
        assert!(analyser("pas du json", (1, 0, 0)).is_none());
    }

    #[test]
    fn les_versions_se_lisent_strictement() {
        assert_eq!(lire_version("0.10.2"), Some((0, 10, 2)));
        for texte in ["1.2", "1.2.3.4", "1..3", "a.b.c", "1.2.-3", ""] {
            assert_eq!(lire_version(texte), None, "{texte}");
        }
        assert_eq!(lire_sha256("SHA256: ABCD"), None);
        assert_eq!(
            lire_sha256(&format!("SHA256:{}", "F".repeat(64))),
            Some("f".repeat(64))
        );
    }

    fn paquet(contenu: &[u8]) -> Paquet {
        Paquet {
            nom: "a.zip".into(),
            adresse: String::new(),
            taille: u64::try_from(contenu.len()).unwrap(),
            sha256: sha(contenu),
        }
    }

    #[test]
    fn recevoir_garde_le_bon_fichier_et_refuse_le_reste() {
        let temp = tempfile::tempdir().unwrap();
        let cible = temp.path().join("a.zip");
        let contenu = b"le bon contenu".to_vec();
        recevoir(Cursor::new(contenu.clone()), &paquet(&contenu), &cible).unwrap();
        assert_eq!(fs::read(&cible).unwrap(), contenu);

        let autre = temp.path().join("b.zip");
        assert!(
            recevoir(
                Cursor::new(b"un autre contenu".to_vec()),
                &paquet(&contenu),
                &autre
            )
            .is_err()
        );
        let mut trop_long = contenu.clone();
        trop_long.push(b'!');
        assert!(recevoir(Cursor::new(trop_long), &paquet(&contenu), &autre).is_err());
        assert!(!autre.exists());
        assert!(!autre.with_extension("part").exists());
    }

    fn ecrire_zip(chemin: &Path, fichiers: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(File::create(chemin).unwrap());
        for (nom, contenu) in fichiers {
            zip.start_file(*nom, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(contenu.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn installer_le_zip_remplace_et_garde_la_version_en_cours() {
        let temp = tempfile::tempdir().unwrap();
        let (programme, versions, travail) = (
            temp.path().join("programme"),
            temp.path().join("versions"),
            temp.path().join("travail"),
        );
        fs::create_dir_all(&programme).unwrap();
        fs::create_dir_all(&travail).unwrap();
        let exe = installation::nom_exe();
        fs::write(programme.join(&exe), "exe 1").unwrap();
        fs::write(
            programme.join("BUILDINFO.txt"),
            format!("{} 1.0.0", identite::NOM),
        )
        .unwrap();
        fs::write(programme.join(LISTE), format!("{exe}\nBUILDINFO.txt")).unwrap();

        let zip = travail.join("maj.zip");
        let liste = format!("{exe}\nBUILDINFO.txt");
        let infos = format!("{} 1.1.0", identite::NOM);
        let dans = |nom: &str| format!("Aria-Essai-1.1.0/{nom}");
        ecrire_zip(
            &zip,
            &[
                (&dans(&exe), "exe 2"),
                (&dans("BUILDINFO.txt"), &infos),
                (&dans(LISTE), &liste),
            ],
        );
        let relance = installer_zip(&zip, &programme, &versions, &travail).unwrap();
        assert_eq!(fs::read_to_string(relance).unwrap(), "exe 2");
        assert_eq!(
            fs::read_to_string(versions.join("1.0.0").join(&exe)).unwrap(),
            "exe 1"
        );
        assert!(!zip.exists(), "le zip du dossier de travail est retiré");
        assert!(!travail.join("extrait").exists());
    }

    #[test]
    fn un_zip_sans_liste_ne_change_rien() {
        let temp = tempfile::tempdir().unwrap();
        let (programme, travail) = (temp.path().join("programme"), temp.path().join("travail"));
        fs::create_dir_all(&programme).unwrap();
        fs::create_dir_all(&travail).unwrap();
        fs::write(programme.join(installation::nom_exe()), "exe 1").unwrap();
        let zip = travail.join("maj.zip");
        ecrire_zip(&zip, &[("lisez-moi.txt", "rien")]);
        let echec = installer_zip(&zip, &programme, &temp.path().join("versions"), &travail).unwrap_err();
        assert!(echec.0.contains(LISTE), "{echec}");
        assert_eq!(
            fs::read_to_string(programme.join(installation::nom_exe())).unwrap(),
            "exe 1"
        );
    }
}
