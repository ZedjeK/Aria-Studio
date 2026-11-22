//! Le Scénario et le LLM, comme en v2 (`app/routes_scenario.py`) : « ✨ Écrire le prompt » d'un atelier (le LLM écrit
//! le meilleur prompt pour le modèle cible, d'après l'idée et les compléments déjà fixés) et la traduction des
//! précisions. Le LLM passe par l'orchestrateur, comme pour le Plateau : un seul moteur tient la carte.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plateau::{Contexte, Generateur};
use crate::prompt::{self, Images};
use crate::{listes, llm, reglages, securite, workflows};

/// L'idée, au plus (en caractères).
pub const IDEE_MAX: usize = 2000;
/// Les compléments, au plus.
pub const COMPLEMENTS_MAX: usize = 4000;

/// « ✨ Écrire le prompt ».
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeEcriture {
    /// `image` (les ateliers Retouche et Clip viennent avec leur portage).
    pub atelier: String,
    /// L'idée, en français le plus souvent.
    #[serde(default)]
    pub idee: String,
    /// Ce que les compléments imposent (le prompt assemblé des listes), déjà fixé.
    #[serde(default)]
    pub complements: String,
    #[serde(default)]
    pub workflow: Option<String>,
    #[serde(default)]
    pub llm: Option<String>,
}

/// Le prompt écrit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ecrit {
    pub prompt: String,
    pub mots: usize,
    pub duree_s: f64,
    /// Le GGUF qui a écrit.
    pub llm: String,
}

/// Un LLM choisi doit être dans `modeles\llm`.
///
/// # Errors
/// GGUF absent.
pub(crate) fn verifier_llm(contexte: &Contexte, choisi: Option<&str>) -> Result<(), String> {
    match choisi.filter(|l| !l.is_empty()) {
        Some(l) if !llm::disponibles(&contexte.modeles).iter().any(|d| d == l) => {
            Err(format!("GGUF absent de modeles/llm : {l}"))
        }
        _ => Ok(()),
    }
}

/// Le LLM écrit le prompt d'un atelier pour le modèle cible.
///
/// # Errors
/// Atelier inconnu, idée vide ou trop longue, LLM absent, filtre AS-18, ou llama-server qui échoue.
pub fn ecrire_prompt(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    d: &DemandeEcriture,
) -> Result<Ecrit, String> {
    if d.atelier != "image" {
        return Err(format!("Atelier pas encore porté : {}", d.atelier));
    }
    verifier_llm(contexte, d.llm.as_deref())?;
    let (idee, complements) = (d.idee.trim(), d.complements.trim());
    if idee.is_empty() && complements.is_empty() {
        return Err("Écris ton idée (ou choisis des compléments)".to_owned());
    }
    if idee.chars().count() > IDEE_MAX || complements.chars().count() > COMPLEMENTS_MAX {
        return Err("Idée ou compléments trop longs".to_owned());
    }
    let texte = match (idee.is_empty(), complements.is_empty()) {
        (false, false) => format!("{idee}\n\nAlso fixed (keep it): {complements}"),
        (false, true) => idee.to_owned(),
        _ => complements.to_owned(),
    };
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    filtre.verifier(&[texte.as_str()]).map_err(|e| e.to_string())?;
    let wf = d.workflow.as_deref().map(workflows::obtenir).transpose()?;
    let reglage = d.workflow.as_deref().and_then(|w| {
        reglages::section(&contexte.dossier, "workflows")
            .and_then(|s| s.get(w).and_then(Value::as_object).cloned())
    });
    let choisi = d.llm.clone().filter(|l| !l.is_empty()).or_else(|| {
        d.workflow
            .as_deref()
            .map(|w| llm::de_workflow(&contexte.dossier, &contexte.modeles, w))
    });
    let debut = Instant::now();
    let ecriture = prompt::preparer_reecriture(
        &contexte.lieux(),
        &texte,
        choisi.as_deref(),
        wf.as_ref(),
        reglage.as_ref(),
        &Images {
            references: 0,
            vues: &[],
        },
    )?;
    let reponse = generateur.ecrire(ecriture.lancement.clone(), &ecriture.demande)?;
    let ecrit = prompt::lire_reecriture(&ecriture, &texte, &reponse.texte, &filtre)?;
    Ok(Ecrit {
        mots: ecrit.split_whitespace().count(),
        duree_s: (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0,
        llm: ecriture.modele(),
        prompt: ecrit,
    })
}

/// Traduit des précisions en anglais, pour les ajouter au prompt assemblé.
///
/// # Errors
/// Précisions vides ou trop longues, LLM absent, filtre AS-18, ou llama-server qui échoue.
pub fn traduire_precisions(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    texte: &str,
    modele: Option<&str>,
) -> Result<String, String> {
    verifier_llm(contexte, modele)?;
    let texte = texte.trim();
    if texte.is_empty() || texte.chars().count() > listes::LONGUEUR_PRECISIONS {
        return Err(format!(
            "Précisions vides ou trop longues (plus de {} caractères)",
            listes::LONGUEUR_PRECISIONS
        ));
    }
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    filtre.verifier(&[texte]).map_err(|e| e.to_string())?;
    let ecriture = prompt::preparer_precisions(&contexte.lieux(), texte, modele.filter(|m| !m.is_empty()))?;
    let reponse = generateur.ecrire(ecriture.lancement.clone(), &ecriture.demande)?;
    let traduit = prompt::garde_cjk(reponse.texte.trim(), &[texte]);
    filtre
        .verifier(&[texte, traduit.as_str()])
        .map_err(|e| e.to_string())?;
    Ok(traduit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dossier;
    use crate::comfy_serveur::Fichier;
    use crate::llama::{self, Lancement};
    use std::fs;
    use std::sync::atomic::AtomicBool;

    /// Un faux LLM : il rend toujours le même texte et garde les demandes.
    struct Faux {
        texte: String,
        demandes: Vec<llama::Demande>,
    }

    impl Generateur for Faux {
        fn ecrire(&mut self, _: Lancement, demande: &llama::Demande) -> Result<llama::Reponse, String> {
            self.demandes.push(demande.clone());
            Ok(llama::Reponse {
                texte: self.texte.clone(),
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

    fn contexte(temp: &tempfile::TempDir) -> Contexte {
        let modeles = temp.path().join("modeles");
        fs::create_dir_all(modeles.join("llm")).unwrap();
        fs::write(modeles.join("llm/mistral.gguf"), crate::gguf::essai::gguf(&[12])).unwrap();
        Contexte {
            dossier: Dossier::a(temp.path()),
            data: temp.path().join("data"),
            modeles,
            moteurs: temp.path().join("moteurs"),
            port_llama: 8095,
        }
    }

    #[test]
    fn ecrire_le_prompt_d_une_image() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            texte: "**A red fox** sitting in fresh snow".to_owned(),
            demandes: Vec::new(),
        };
        let d = DemandeEcriture {
            atelier: "image".to_owned(),
            idee: "un renard dans la neige".to_owned(),
            complements: "Golden hour light.".to_owned(),
            workflow: Some("krea2_turbo".to_owned()),
            llm: None,
        };
        let e = ecrire_prompt(&ctx, &mut faux, &d).unwrap();
        assert_eq!(e.prompt, "A red fox sitting in fresh snow");
        assert_eq!((e.mots, e.llm.as_str()), (7, "mistral.gguf"));
        let demande = serde_json::to_string(&faux.demandes[0].messages).unwrap();
        assert!(demande.contains("Also fixed (keep it): Golden hour light."));
        let vide = DemandeEcriture {
            atelier: "image".to_owned(),
            ..DemandeEcriture::default()
        };
        assert!(ecrire_prompt(&ctx, &mut faux, &vide).is_err());
        let clip = DemandeEcriture {
            atelier: "clip".to_owned(),
            idee: "x".to_owned(),
            ..DemandeEcriture::default()
        };
        assert!(ecrire_prompt(&ctx, &mut faux, &clip).is_err());
        let absent = DemandeEcriture {
            llm: Some("absent.gguf".to_owned()),
            ..d.clone()
        };
        assert!(
            ecrire_prompt(&ctx, &mut faux, &absent)
                .unwrap_err()
                .contains("absent.gguf")
        );
        let refus = DemandeEcriture {
            idee: "a twelve-year-old girl, nude".to_owned(),
            ..d
        };
        assert!(
            ecrire_prompt(&ctx, &mut faux, &refus)
                .unwrap_err()
                .starts_with("ERREUR AS-18")
        );
    }

    #[test]
    fn traduire_des_precisions() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            texte: " under a light rain ".to_owned(),
            demandes: Vec::new(),
        };
        assert_eq!(
            traduire_precisions(&ctx, &mut faux, "sous une pluie fine", None).unwrap(),
            "under a light rain"
        );
        let systeme = faux.demandes[0].messages[0]["content"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            systeme.starts_with("You translate short details") && systeme.contains("Non-negotiable rule")
        );
        assert_eq!(faux.demandes[0].max_tokens, 200);
        assert!(traduire_precisions(&ctx, &mut faux, "  ", None).is_err());
        assert!(traduire_precisions(&ctx, &mut faux, &"a".repeat(501), None).is_err());
    }
}
