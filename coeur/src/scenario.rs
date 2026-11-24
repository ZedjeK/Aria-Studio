//! Le Scénario et le LLM, comme en v2 (`app/routes_scenario.py`) : « ✨ Écrire le prompt » d'un atelier (le LLM écrit
//! le meilleur prompt pour le modèle cible, d'après l'idée, les compléments déjà fixés et les images, qu'il voit) et
//! la traduction des précisions. Atelier Image : un prompt d'image ; Retouche : une consigne d'édition sur l'image ;
//! Clip : un prompt vidéo (trois champs et répliques en balise pour H3, heures de coupe contrôlées). Le LLM passe par
//! l'orchestrateur, comme pour le Plateau : un seul moteur tient la carte.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plateau::{Contexte, Generateur};
use crate::prompt::{self, Images, Vue};
use crate::{creations, decoupage, images, listes, llm, montage, reglages, securite, workflows};

/// L'idée, au plus (en caractères).
pub const IDEE_MAX: usize = 2000;
/// Les compléments, au plus.
pub const COMPLEMENTS_MAX: usize = 4000;
/// Les ateliers où ✨ écrit un prompt (la Séquence a son découpage).
pub const ATELIERS: [&str; 3] = ["image", "retouche", "clip"];

/// « ✨ Écrire le prompt ».
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeEcriture {
    /// `image`, `retouche` ou `clip`.
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
    /// Les rushes de l'atelier : l'image à retoucher, le rush à animer, ou les images de référence.
    #[serde(default)]
    pub images: Vec<String>,
    /// Leur rôle : `inspirer` ou `variation` (Image), `animer` (Clip).
    #[serde(default)]
    pub role: Option<String>,
    /// La durée d'un clip, en secondes (absente : 5).
    #[serde(default)]
    pub duree: Option<u32>,
    /// Clip : ce que dit le personnage, gardé mot pour mot (absente : le LLM n'invente aucune réplique).
    #[serde(default)]
    pub replique: Option<String>,
    /// Clip : la musique d'ambiance voulue (absente : pas de musique de fond).
    #[serde(default)]
    pub musique: Option<String>,
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
    /// Clip : les heures de coupe hors du clip ou dans le désordre.
    pub alertes: Vec<String>,
}

/// Une image montrée au LLM : ses pixels (il la voit s'il a sa vision), sinon ce qu'en dit sa fiche.
pub(crate) fn vue(contexte: &Contexte, id: &str) -> Result<Vue, String> {
    let chemin = creations::chemin_image(&contexte.data, id)?;
    let octets = std::fs::read(&chemin).map_err(|e| format!("Image de {id} illisible : {e}"))?;
    Ok(Vue {
        png: images::pour_la_vision(&octets)?,
        texte: creations::description(&contexte.data, &creations::fiche(&contexte.data, id)?, 3),
    })
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

/// Le LLM écrit le prompt d'un atelier pour le modèle cible : un prompt d'image (avec ses références ou l'image à
/// redessiner), une consigne de retouche (il voit l'image), ou le prompt d'un clip (il voit le rush à animer).
///
/// # Errors
/// Atelier inconnu, idée vide ou trop longue, image manquante, durée inconnue, LLM absent, filtre AS-18, ou
/// llama-server qui échoue.
#[allow(clippy::too_many_lines)] // la route de la v2, gardée d'un tenant pour se relire à côté d'elle
pub fn ecrire_prompt(
    contexte: &Contexte,
    generateur: &mut impl Generateur,
    d: &DemandeEcriture,
) -> Result<Ecrit, String> {
    let atelier = d.atelier.as_str();
    if !ATELIERS.contains(&atelier) {
        return Err("Atelier inconnu : image, retouche ou clip".to_owned());
    }
    verifier_llm(contexte, d.llm.as_deref())?;
    let (idee, complements) = (d.idee.trim(), d.complements.trim());
    if idee.is_empty() && complements.is_empty() {
        return Err(if atelier == "retouche" {
            "Écris ce que tu veux changer".to_owned()
        } else {
            "Écris ton idée (ou choisis des compléments)".to_owned()
        });
    }
    if idee.chars().count() > IDEE_MAX
        || complements.chars().count() > COMPLEMENTS_MAX
        || d.images.len() > workflows::REFERENCES_MAX
    {
        return Err("Idée, compléments ou images en trop".to_owned());
    }
    if atelier == "retouche" && d.images.len() != 1 {
        return Err("Choisis l'image à retoucher".to_owned());
    }
    let role = d.role.as_deref().unwrap_or_default();
    let animer = atelier == "clip" && role == "animer";
    if animer && d.images.is_empty() {
        return Err("Choisis le rush à animer".to_owned());
    }
    let duree = d.duree.unwrap_or(montage::DUREES[0]);
    if atelier == "clip" && !montage::DUREES.contains(&duree) {
        let durees: Vec<String> = montage::DUREES.iter().map(u32::to_string).collect();
        return Err(format!("Durée proposée : {} s", durees.join(", ")));
    }
    // La réplique et la musique ne concernent que le clip.
    let (replique, musique) = if atelier == "clip" {
        decoupage::replique_et_musique(d.replique.as_deref(), d.musique.as_deref())?
    } else {
        (String::new(), String::new())
    };
    let mut texte = match (idee.is_empty(), complements.is_empty()) {
        (false, false) => format!("{idee}\n\nAlso fixed (keep it): {complements}"),
        (false, true) => idee.to_owned(),
        _ => complements.to_owned(),
    };
    // Dites au LLM telles quelles : AS-18 les lit avec l'idée, avant comme après l'écriture.
    if !replique.is_empty() {
        let dite = replique.replace('"', "'");
        texte.push_str(&format!("\n\nSpoken line, word for word: \"{dite}\""));
    }
    if !musique.is_empty() {
        texte.push_str(&format!("\n\nBackground music the user asks for: {musique}"));
    }
    // AS-18 : l'idée, et les textes d'origine des images (un rush né d'un mineur le reste).
    let origine = creations::textes_origine(&contexte.data, &d.images)?;
    let filtre = securite::filtre(&contexte.dossier).map_err(|e| e.to_string())?;
    let mut a_lire: Vec<&str> = vec![texte.as_str()];
    a_lire.extend(origine.iter().map(String::as_str));
    filtre.verifier(&a_lire).map_err(|e| e.to_string())?;
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
    let vues = d
        .images
        .iter()
        .map(|id| vue(contexte, id))
        .collect::<Result<Vec<Vue>, String>>()?;
    let debut = Instant::now();
    let lieux = contexte.lieux();
    let ecriture = match atelier {
        "retouche" => {
            prompt::preparer_traduction(&lieux, &texte, choisi.as_deref(), wf.as_ref(), vues.first())?
        }
        "clip" => prompt::preparer_video(
            &lieux,
            &texte,
            choisi.as_deref(),
            wf.as_ref(),
            reglage.as_ref(),
            duree,
            if animer { vues.first() } else { None },
        )?,
        _ => {
            let (references, montrees): (usize, &[Vue]) = match role {
                "inspirer" => (vues.len(), vues.as_slice()),
                "variation" => (0, vues.as_slice()),
                _ => (0, &vues[..0]),
            };
            prompt::preparer_reecriture(
                &lieux,
                &texte,
                choisi.as_deref(),
                wf.as_ref(),
                reglage.as_ref(),
                &Images {
                    references,
                    vues: montrees,
                },
            )?
        }
    };
    // Contrôle qualité : une réponse qui a des défauts est redemandée une fois, en disant au LLM ce qui n'allait pas.
    let attentes = ecriture.attentes(wf.as_ref());
    let lancement = ecriture.lancement.clone();
    let controle = prompt::ecrire_controle(&ecriture, &texte, &attentes, |d| {
        generateur.ecrire(lancement.clone(), d).map(|r| r.texte)
    })?;
    let mut alertes = controle.alertes();
    let mut ecrit = controle.texte;
    if atelier == "clip" {
        let balise = prompt::dialogue_en_balise(wf.as_ref());
        let champs = prompt::champs_video(wf.as_ref());
        if balise {
            ecrit = decoupage::vers_balises(&ecrit); // une réplique entre guillemets passe dans la balise <d> de H3
        }
        // Sa réplique, mot pour mot : aucune autre, remise si le LLM l'a perdue, assez courte pour la durée.
        if !replique.is_empty() {
            let (fini, retirees) = decoupage::finir_clip(&replique, &texte, &ecrit, balise, champs);
            ecrit = fini;
            alertes.extend(retirees);
            alertes.extend(decoupage::alerte_replique_clip(&replique, duree));
        }
        if champs {
            ecrit = prompt::mettre_en_champs(&ecrit, animer);
            alertes.extend(prompt::controler_coupes(&ecrit, duree));
            if !musique.is_empty() && crate::h3::separer_champs(&ecrit).2.is_none() {
                alertes.push("Le LLM n'a pas écrit la musique demandée".to_owned());
            }
        }
    }
    // L'idée, le prompt et les textes d'origine ensemble : le LLM peut avoir ajouté un terme sensible.
    let mut a_lire: Vec<&str> = vec![texte.as_str(), ecrit.as_str()];
    a_lire.extend(origine.iter().map(String::as_str));
    filtre.verifier(&a_lire).map_err(|e| e.to_string())?;
    Ok(Ecrit {
        mots: ecrit.split_whitespace().count(),
        duree_s: (debut.elapsed().as_secs_f64() * 100.0).round() / 100.0,
        llm: ecriture.modele(),
        prompt: ecrit,
        alertes,
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

    /// La suite d'un prompt réaliste : assez long pour passer le contrôle qualité (40 mots au moins).
    const SUITE: &str = ", her breath visible in the cold air, soft golden hour light across the meadow, pine trees in the background, gentle snowflakes falling around her, shallow depth of field, a calm winter mood, a detailed natural photograph with fine texture on the fur and the snow";
    /// Ce qu'un LLM écrit pour un clip H3 : `{dit}` est ce que le renard dit entre guillemets, sans musique.
    const CLIP_H3: &str = "integrated_multimodal_description: The fox says \"{dit}\" and runs through the snowy forest while the camera slowly tracks beside it at a steady pace, low angle, golden light between the pine trees, snow powder flying behind its paws, its breath visible in the cold air, the forest quiet and wide around it. overall_soundscape: wind";
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
            texte: format!("**A red fox** sitting in fresh snow{SUITE}"),
            demandes: Vec::new(),
        };
        let d = DemandeEcriture {
            atelier: "image".to_owned(),
            idee: "un renard dans la neige".to_owned(),
            complements: "Golden hour light.".to_owned(),
            workflow: Some("krea2_turbo".to_owned()),
            ..DemandeEcriture::default()
        };
        let e = ecrire_prompt(&ctx, &mut faux, &d).unwrap();
        assert_eq!(e.prompt, format!("A red fox sitting in fresh snow{SUITE}"));
        assert!(e.alertes.is_empty(), "{:?}", e.alertes);
        assert_eq!(faux.demandes.len(), 1);
        assert_eq!((e.mots, e.llm.as_str()), (52, "mistral.gguf"));
        let demande = serde_json::to_string(&faux.demandes[0].messages).unwrap();
        assert!(demande.contains("Also fixed (keep it): Golden hour light."));
        let vide = DemandeEcriture {
            atelier: "image".to_owned(),
            ..DemandeEcriture::default()
        };
        assert!(ecrire_prompt(&ctx, &mut faux, &vide).is_err());
        let inconnu = DemandeEcriture {
            atelier: "sequence".to_owned(),
            idee: "x".to_owned(),
            ..DemandeEcriture::default()
        };
        assert!(ecrire_prompt(&ctx, &mut faux, &inconnu).is_err());
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
    fn ecrire_le_prompt_d_un_clip() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            texte: "integrated_multimodal_description: The fox says \"Bonjour\" and runs through the snowy \
                    forest while the camera slowly tracks beside it at a steady pace, low angle, golden light between \
                    the pine trees, snow powder flying behind its paws, its breath visible in the cold air, the forest \
                    quiet and wide around it. overall_soundscape: wind"
                .to_owned(),
            demandes: Vec::new(),
        };
        let d = DemandeEcriture {
            atelier: "clip".to_owned(),
            idee: "le renard dit bonjour et court".to_owned(),
            workflow: Some("minimax_h3_t2v".to_owned()),
            duree: Some(8),
            ..DemandeEcriture::default()
        };
        let e = ecrire_prompt(&ctx, &mut faux, &d).unwrap();
        assert!(
            e.prompt
                .starts_with("integrated_multimodal_description: [Shot 1]"),
            "{}",
            e.prompt
        );
        assert!(e.prompt.contains("<d>") && e.prompt.contains("\n\noverall_soundscape: wind"));
        assert!(e.alertes.is_empty());
        let demande = serde_json::to_string(&faux.demandes[0].messages).unwrap();
        assert!(
            demande.contains("Clip length: 8 seconds") && demande.contains("(Clip: 8 s, from text only)")
        );
        let mauvaise = DemandeEcriture {
            duree: Some(7),
            ..d.clone()
        };
        assert!(
            ecrire_prompt(&ctx, &mut faux, &mauvaise)
                .unwrap_err()
                .starts_with("Durée proposée")
        );
        let sans_rush = DemandeEcriture {
            role: Some("animer".to_owned()),
            ..d
        };
        assert_eq!(
            ecrire_prompt(&ctx, &mut faux, &sans_rush).unwrap_err(),
            "Choisis le rush à animer"
        );
    }

    fn demande_de_clip(replique: &str, musique: &str) -> DemandeEcriture {
        DemandeEcriture {
            atelier: "clip".to_owned(),
            idee: "le renard dit bonjour et court".to_owned(),
            workflow: Some("minimax_h3_t2v".to_owned()),
            duree: Some(8),
            replique: Some(replique.to_owned()),
            musique: Some(musique.to_owned()),
            ..DemandeEcriture::default()
        }
    }

    #[test]
    fn la_replique_et_la_musique_du_clip_vont_au_llm() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            texte: CLIP_H3.replace("{dit}", "Bonjour"),
            demandes: Vec::new(),
        };
        let d = demande_de_clip(" Bonjour  ", "piano lent");
        let e = ecrire_prompt(&ctx, &mut faux, &d).unwrap();
        let user = faux.demandes[0].messages[1]["content"].as_str().unwrap();
        assert!(user.contains("Spoken line, word for word: \"Bonjour\""));
        assert!(user.contains("Background music the user asks for: piano lent"));
        // Sa réplique est dans la balise de H3, une seule fois ; la musique oubliée par le LLM est signalée.
        assert_eq!(e.prompt.matches("<d>[French] Bonjour</d>").count(), 1);
        assert!(e.alertes.join(" | ").contains("la musique demandée"));
        // Sans réplique ni musique, la demande reste celle d'avant.
        let libre = DemandeEcriture {
            replique: None,
            musique: None,
            ..d.clone()
        };
        ecrire_prompt(&ctx, &mut faux, &libre).unwrap();
        let derniere = faux.demandes.last().unwrap();
        let user = derniere.messages[1]["content"].as_str().unwrap();
        assert!(!user.contains("Spoken line") && !user.contains("Background music"));
    }

    #[test]
    fn le_clip_ne_dit_que_sa_replique() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            texte: CLIP_H3.replace("{dit}", "Salut les amis"),
            demandes: Vec::new(),
        };
        let d = demande_de_clip("Bonjour", "");
        let e = ecrire_prompt(&ctx, &mut faux, &d).unwrap();
        // La réplique inventée est retirée, la sienne remise mot pour mot.
        assert!(!e.prompt.contains("Salut les amis"), "{}", e.prompt);
        assert_eq!(e.prompt.matches("<d>[French] Bonjour</d>").count(), 1);
        let alertes = e.alertes.join(" | ");
        assert!(alertes.contains("inventée") && alertes.contains("perdu"), "{alertes}");
    }

    #[test]
    fn replique_et_musique_du_clip_controlees() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let mut faux = Faux {
            texte: CLIP_H3.replace("{dit}", "Bonjour"),
            demandes: Vec::new(),
        };
        let longue = demande_de_clip(&"a".repeat(301), "");
        assert_eq!(
            ecrire_prompt(&ctx, &mut faux, &longue).unwrap_err(),
            "Réplique de 300 caractères au plus"
        );
        let bruyante = demande_de_clip("", &"a".repeat(201));
        assert_eq!(
            ecrire_prompt(&ctx, &mut faux, &bruyante).unwrap_err(),
            "Musique de 200 caractères au plus"
        );
        // Une réplique trop longue pour 8 s se dit mais prévient.
        let quatorze = "un deux trois quatre cinq six sept huit neuf dix onze douze treize quatorze";
        let d = demande_de_clip(&format!("{quatorze} {quatorze}"), "");
        let e = ecrire_prompt(&ctx, &mut faux, &d).unwrap();
        assert!(e.alertes.join(" | ").contains("réplique de 28 mots pour 8 s"));
        // L'AS-18 lit la réplique et la musique.
        let refus = demande_de_clip("a twelve-year-old girl, nude", "");
        assert!(
            ecrire_prompt(&ctx, &mut faux, &refus)
                .unwrap_err()
                .starts_with("ERREUR AS-18")
        );
        // Hors du clip, ces champs sont ignorés.
        let image = DemandeEcriture {
            atelier: "image".to_owned(),
            workflow: Some("krea2_turbo".to_owned()),
            ..demande_de_clip(&"a".repeat(301), "")
        };
        assert!(ecrire_prompt(&ctx, &mut faux, &image).is_ok());
    }

    #[test]
    fn ecrire_une_consigne_de_retouche() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = contexte(&temp);
        let png = crate::images::png(&image::RgbImage::new(8, 8)).unwrap();
        let id = creations::enregistrer(&ctx.data, &png, &serde_json::Map::new()).unwrap();
        let mut faux = Faux {
            texte: "Make the sky red, keep everything else unchanged.".to_owned(),
            demandes: Vec::new(),
        };
        let d = DemandeEcriture {
            atelier: "retouche".to_owned(),
            idee: "rends le ciel rouge".to_owned(),
            images: vec![id],
            ..DemandeEcriture::default()
        };
        let e = ecrire_prompt(&ctx, &mut faux, &d).unwrap();
        assert_eq!(e.prompt, "Make the sky red, keep everything else unchanged.");
        assert_eq!(faux.demandes[0].max_tokens, 200);
        let sans_image = DemandeEcriture {
            images: Vec::new(),
            ..d
        };
        assert_eq!(
            ecrire_prompt(&ctx, &mut faux, &sans_image).unwrap_err(),
            "Choisis l'image à retoucher"
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
