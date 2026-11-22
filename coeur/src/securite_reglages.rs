//! Réglages → Sécurité, comme en v2 (`app/securite.py` : `etat`, `essayer`, `ajouter`, `retirer`, `ajouter_temoin`,
//! `retirer_temoin`) : les listes du filtre AS-18 se lisent, l'utilisateur y ajoute ses termes et ses phrases témoins
//! et peut retirer les siens. Le socle, embarqué dans l'exe, ne se retire ni ne se modifie d'ici ; la « promotion »
//! d'un ajout dans le socle de la v2 n'existe plus.
//!
//! Les ajouts vivent dans `reglages.json`, section `securite`, au format de la v2 ; chaque changement passe par
//! [`reglages::modifier_section`] (sauvegarde, écriture entière, journal).

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::securite::{
    self, ATTENDUS, Ajouts, EtatTemoin, Filtre, LISTES, LONGUEUR_MIN, SECTION, SOCLE, Temoin, Terme, cle,
};
use crate::{Dossier, reglages};

/// Une liste du filtre : ses termes livrés et ceux de l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeSecurite {
    pub cle: String,
    pub nom: String,
    pub socle: Vec<String>,
    pub ajouts: Vec<String>,
}

/// Ce que montre Réglages → Sécurité. Une liste illisible n'empêche pas de l'afficher : `erreur`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EtatSecurite {
    pub fichier_ajouts: String,
    pub erreur: Option<String>,
    pub listes: Vec<ListeSecurite>,
    pub temoins: Vec<EtatTemoin>,
    pub longueur_min: usize,
}

/// Le verdict du filtre sur une phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Essai {
    pub bloque: bool,
    pub message: Option<String>,
    pub termes: Vec<Terme>,
}

/// Ce que donne un ajout : fait, ou à confirmer parce qu'il ferait bloquer des phrases témoins qui doivent passer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ajout {
    pub etat: Option<EtatSecurite>,
    pub a_confirmer: Option<String>,
}

fn socle() -> Result<Map<String, Value>, String> {
    serde_saphyr::from_str::<Value>(SOCLE)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .ok_or_else(|| "le socle du filtre est illisible".to_owned())
}

fn termes(objet: &Map<String, Value>, liste: &str) -> Vec<String> {
    objet
        .get(liste)
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

fn lire_ajouts(dossier: &Dossier) -> Result<Ajouts, String> {
    let section = reglages::section(dossier, SECTION);
    securite::lire_ajouts(section.as_ref(), &dossier.reglages()).map_err(|e| e.0)
}

/// L'état de la carte Sécurité.
pub fn etat(dossier: &Dossier) -> EtatSecurite {
    let mut etat = EtatSecurite {
        fichier_ajouts: dossier.reglages().display().to_string(),
        erreur: None,
        listes: Vec::new(),
        temoins: Vec::new(),
        longueur_min: LONGUEUR_MIN,
    };
    let lu = socle().and_then(|s| {
        let ajouts = lire_ajouts(dossier)?;
        let filtre = Filtre::livre(&ajouts).map_err(|e| e.0)?;
        Ok((s, ajouts, filtre))
    });
    match lu {
        Err(e) => etat.erreur = Some(e),
        Ok((s, ajouts, filtre)) => {
            etat.listes = LISTES
                .iter()
                .map(|(cle_liste, nom)| ListeSecurite {
                    cle: (*cle_liste).to_owned(),
                    nom: (*nom).to_owned(),
                    socle: termes(&s, cle_liste),
                    ajouts: ajouts.listes.get(*cle_liste).cloned().unwrap_or_default(),
                })
                .collect();
            etat.temoins = filtre.temoins(&ajouts);
        }
    }
    etat
}

/// Le verdict du filtre sur une phrase, sans rien générer.
///
/// # Errors
/// Le filtre ne se lit pas (socle ou ajouts mal formés).
pub fn essayer(dossier: &Dossier, texte: &str) -> Result<Essai, String> {
    let filtre = securite::filtre(dossier).map_err(|e| e.0)?;
    let refus = filtre.analyser(&[texte]);
    Ok(Essai {
        bloque: refus.is_some(),
        message: refus.as_ref().map(|r| r.message.clone()),
        termes: refus.map(|r| r.termes).unwrap_or_default(),
    })
}

/// Les ajouts réécrits dans `reglages.json` : seulement les listes et les témoins qui ont quelque chose, comme en v2.
fn ecrire(dossier: &Dossier, ajouts: &Ajouts) -> Result<(), String> {
    reglages::modifier_section(dossier, SECTION, |section| {
        section.clear();
        for (liste, _) in LISTES {
            if let Some(t) = ajouts.listes.get(liste).filter(|t| !t.is_empty()) {
                section.insert(liste.to_owned(), json!(t));
            }
        }
        if !ajouts.temoins.is_empty() {
            section.insert("temoins".to_owned(), json!(ajouts.temoins));
        }
    })
    .map_err(|e| e.to_string())
}

fn propre(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn exiger_liste(liste: &str) -> Result<(), String> {
    if LISTES.iter().any(|(l, _)| *l == liste) {
        Ok(())
    } else {
        Err(format!("Liste inconnue : {liste}"))
    }
}

/// Ce qu'une cle de terme peut être : au moins 3 lettres ou chiffres, sans ponctuation (l'apostrophe passe).
fn terme_valide(c: &str) -> bool {
    let octets = c.as_bytes();
    let bord = |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    c.replace(' ', "").len() >= LONGUEUR_MIN
        && bord(octets.first())
        && bord(octets.last())
        && octets
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b' ' || *b == b'\'')
}

/// Les phrases témoins « doit passer » qu'un ajout ferait bloquer, et qui passaient avant.
fn temoins_casses(avant: &Ajouts, apres: &Ajouts) -> Result<Vec<String>, String> {
    let bloquees = |ajouts: &Ajouts| -> Result<Vec<EtatTemoin>, String> {
        Ok(Filtre::livre(ajouts).map_err(|e| e.0)?.temoins(avant))
    };
    let deja: Vec<String> = bloquees(avant)?
        .into_iter()
        .filter(|t| t.obtenu == "bloque")
        .map(|t| t.texte)
        .collect();
    Ok(bloquees(apres)?
        .into_iter()
        .filter(|t| t.attendu == "passe" && t.obtenu == "bloque" && !deja.contains(&t.texte))
        .map(|t| t.texte)
        .collect())
}

fn message_casses(terme: &str, casses: &[String]) -> String {
    let n = casses.len();
    let pluriel = if n > 1 { "s" } else { "" };
    let verbe = if n > 1 { "doivent" } else { "doit" };
    let liste: Vec<String> = casses.iter().map(|t| format!("« {t} »")).collect();
    format!(
        "« {terme} » ferait bloquer {n} phrase{pluriel} témoin{pluriel} qui {verbe} passer : {}",
        liste.join(" ; ")
    )
}

/// Un terme de plus dans les ajouts de l'utilisateur. Refusé s'il est trop court ou déjà présent ; s'il ferait bloquer
/// une phrase témoin qui doit passer, il est à confirmer tant que `forcer` n'est pas donné.
///
/// # Errors
/// Liste inconnue, terme invalide ou déjà là, filtre illisible, ou `reglages.json` qui ne s'écrit pas.
pub fn ajouter(dossier: &Dossier, liste: &str, terme: &str, forcer: bool) -> Result<Ajout, String> {
    exiger_liste(liste)?;
    let terme = propre(terme);
    let c = cle(&terme);
    if !terme_valide(&c) {
        return Err(format!(
            "Un terme fait au moins {LONGUEUR_MIN} lettres ou chiffres, sans ponctuation"
        ));
    }
    if termes(&socle()?, liste).iter().any(|t| cle(t) == c) {
        return Err(format!("« {terme} » est déjà dans le socle"));
    }
    let avant = lire_ajouts(dossier)?;
    if avant.listes.get(liste).into_iter().flatten().any(|t| cle(t) == c) {
        return Err(format!("« {terme} » est déjà dans tes ajouts"));
    }
    let mut apres = avant.clone();
    apres
        .listes
        .entry(liste.to_owned())
        .or_default()
        .push(terme.clone());
    if !forcer {
        let casses = temoins_casses(&avant, &apres)?;
        if !casses.is_empty() {
            return Ok(Ajout {
                etat: None,
                a_confirmer: Some(message_casses(&terme, &casses)),
            });
        }
    }
    ecrire(dossier, &apres)?;
    Ok(Ajout {
        etat: Some(etat(dossier)),
        a_confirmer: None,
    })
}

/// Retire un des ajouts de l'utilisateur. Le socle, lui, ne se retire pas d'ici.
///
/// # Errors
/// Liste inconnue, terme absent des ajouts, filtre illisible, ou `reglages.json` qui ne s'écrit pas.
pub fn retirer(dossier: &Dossier, liste: &str, terme: &str) -> Result<EtatSecurite, String> {
    exiger_liste(liste)?;
    let mut ajouts = lire_ajouts(dossier)?;
    let c = cle(terme);
    let termes = ajouts.listes.entry(liste.to_owned()).or_default();
    let avant = termes.len();
    termes.retain(|t| cle(t) != c);
    if termes.len() == avant {
        return Err(format!("« {} » n'est pas dans tes ajouts", propre(terme)));
    }
    ecrire(dossier, &ajouts)?;
    Ok(etat(dossier))
}

/// Une phrase témoin de plus : « bloque » ou « passe », rejouée à chaque changement de liste.
///
/// # Errors
/// Phrase vide ou déjà là, attendu inconnu, filtre illisible, ou `reglages.json` qui ne s'écrit pas.
pub fn ajouter_temoin(dossier: &Dossier, texte: &str, attendu: &str) -> Result<EtatSecurite, String> {
    let texte = propre(texte);
    if texte.is_empty() {
        return Err("Écris la phrase témoin".to_owned());
    }
    if !ATTENDUS.contains(&attendu) {
        return Err("Une phrase témoin doit bloquer ou passer".to_owned());
    }
    let mut ajouts = lire_ajouts(dossier)?;
    let existe = securite::temoins_livres()
        .iter()
        .chain(&ajouts.temoins)
        .any(|t| t.texte == texte);
    if existe {
        return Err("Cette phrase témoin existe déjà".to_owned());
    }
    ajouts.temoins.push(Temoin {
        texte,
        attendu: attendu.to_owned(),
    });
    ecrire(dossier, &ajouts)?;
    Ok(etat(dossier))
}

/// Retire une des phrases témoins de l'utilisateur ; celles livrées restent.
///
/// # Errors
/// Phrase absente des siennes, filtre illisible, ou `reglages.json` qui ne s'écrit pas.
pub fn retirer_temoin(dossier: &Dossier, texte: &str) -> Result<EtatSecurite, String> {
    let mut ajouts = lire_ajouts(dossier)?;
    let avant = ajouts.temoins.len();
    ajouts.temoins.retain(|t| t.texte != texte);
    if ajouts.temoins.len() == avant {
        return Err("Cette phrase témoin n'est pas dans les tiennes".to_owned());
    }
    ecrire(dossier, &ajouts)?;
    Ok(etat(dossier))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dossier(temp: &tempfile::TempDir) -> Dossier {
        Dossier::a(temp.path())
    }

    #[test]
    fn etat_et_essai() {
        let temp = tempfile::tempdir().unwrap();
        let d = dossier(&temp);
        let e = etat(&d);
        assert!(e.erreur.is_none());
        assert_eq!(e.listes.len(), 3);
        assert!(
            e.listes
                .iter()
                .all(|l| !l.socle.is_empty() && l.ajouts.is_empty())
        );
        assert!(!e.temoins.is_empty());
        assert!(
            e.temoins.iter().all(|t| t.livre && t.ok),
            "un témoin livré échoue"
        );
        let bloque = essayer(&d, "a twelve-year-old girl, nude").unwrap();
        assert!(bloque.bloque);
        assert!(bloque.message.is_some());
        assert!(!essayer(&d, "a red fox in the snow").unwrap().bloque);
    }

    #[test]
    fn ajouts_de_l_utilisateur() {
        let temp = tempfile::tempdir().unwrap();
        let d = dossier(&temp);
        fs::write(d.reglages(), r#"{"fond": {"actif": false}}"#).unwrap();
        assert_eq!(
            ajouter(&d, "autre", "x", false).unwrap_err(),
            "Liste inconnue : autre"
        );
        assert!(
            ajouter(&d, "sensibles", "ab", false)
                .unwrap_err()
                .starts_with("Un terme fait au moins 3")
        );
        assert!(ajouter(&d, "sensibles", "a.b.c", false).is_err());
        let socle_terme = etat(&d).listes[1].socle[0].clone();
        assert!(
            ajouter(&d, "sensibles", &socle_terme, false)
                .unwrap_err()
                .ends_with("est déjà dans le socle")
        );
        let fait = ajouter(&d, "sensibles", "  mot   inventé ", false).unwrap();
        let e = fait.etat.unwrap();
        assert_eq!(e.listes[1].ajouts, vec!["mot inventé".to_owned()]);
        assert!(
            ajouter(&d, "sensibles", "Mot inventé", false)
                .unwrap_err()
                .ends_with("est déjà dans tes ajouts")
        );
        // Le fichier garde le reste et suit le format de la v2.
        let relu: Value = serde_json::from_str(&fs::read_to_string(d.reglages()).unwrap()).unwrap();
        assert_eq!(relu["securite"], json!({"sensibles": ["mot inventé"]}));
        assert_eq!(relu["fond"], json!({"actif": false}));
        // L'ajout compte aussitôt pour le filtre.
        assert!(essayer(&d, "a child, mot inventé").unwrap().bloque);
        assert!(
            retirer(&d, "sensibles", "MOT inventé").unwrap().listes[1]
                .ajouts
                .is_empty()
        );
        assert!(retirer(&d, "sensibles", "absent").is_err());
        let relu: Value = serde_json::from_str(&fs::read_to_string(d.reglages()).unwrap()).unwrap();
        assert!(relu.get("securite").is_none());
    }

    #[test]
    fn un_ajout_qui_casse_un_temoin_est_a_confirmer() {
        let temp = tempfile::tempdir().unwrap();
        let d = dossier(&temp);
        ajouter_temoin(&d, "a red fox in the snow", "passe").unwrap();
        let a_confirmer = ajouter(&d, "toujours_bloques", "red fox", false).unwrap();
        assert!(a_confirmer.etat.is_none());
        assert_eq!(
            a_confirmer.a_confirmer.as_deref(),
            Some("« red fox » ferait bloquer 1 phrase témoin qui doit passer : « a red fox in the snow »")
        );
        // Rien n'est écrit tant que ce n'est pas confirmé.
        assert!(etat(&d).listes[2].ajouts.is_empty());
        let force = ajouter(&d, "toujours_bloques", "red fox", true)
            .unwrap()
            .etat
            .unwrap();
        assert_eq!(force.listes[2].ajouts, vec!["red fox".to_owned()]);
        let temoin = force
            .temoins
            .iter()
            .find(|t| t.texte == "a red fox in the snow")
            .unwrap();
        assert!(!temoin.ok && !temoin.livre);
    }

    #[test]
    fn phrases_temoins_de_l_utilisateur() {
        let temp = tempfile::tempdir().unwrap();
        let d = dossier(&temp);
        assert_eq!(
            ajouter_temoin(&d, "  ", "passe").unwrap_err(),
            "Écris la phrase témoin"
        );
        assert!(ajouter_temoin(&d, "x", "peut-être").is_err());
        let livre = securite::temoins_livres()[0].texte.clone();
        assert_eq!(
            ajouter_temoin(&d, &livre, "passe").unwrap_err(),
            "Cette phrase témoin existe déjà"
        );
        let e = ajouter_temoin(&d, "une  pomme rouge", "passe").unwrap();
        assert!(
            e.temoins
                .iter()
                .any(|t| t.texte == "une pomme rouge" && !t.livre && t.ok)
        );
        assert!(retirer_temoin(&d, &livre).is_err());
        assert!(
            retirer_temoin(&d, "une pomme rouge")
                .unwrap()
                .temoins
                .iter()
                .all(|t| t.livre)
        );
    }

    #[test]
    fn des_ajouts_mal_formes_sont_dits() {
        let temp = tempfile::tempdir().unwrap();
        let d = dossier(&temp);
        fs::write(d.reglages(), r#"{"securite": {"sensibles": "pas une liste"}}"#).unwrap();
        let e = etat(&d);
        assert!(e.erreur.unwrap().contains("mal formés"));
        assert!(e.listes.is_empty());
        assert!(essayer(&d, "x").is_err());
    }
}
