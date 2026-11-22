//! Modèles → les onglets d'usage (Texte → image, Image → image, Vidéo), comme en v2 (`app/collection.py`) : les
//! workflows de chaque usage, ce qui leur manque, leurs checkpoints et leurs LoRA, le LLM qui réécrit pour eux (et les
//! LLM entraînés pour ce modèle), et les favoris : le workflow dont partent le Plateau, la Retouche et le Montage
//! (`reglages.json`, section `favoris.generation`, comme en v2).

use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::installations::{self, Licence};
use crate::plateau::{self, ChoixLlm};
use crate::{Dossier, collection, fiches, llm, prompt, reglages, workflows};

/// Un onglet d'usage : ses tâches et les favoris qu'on y règle.
struct DefUsage {
    id: &'static str,
    nom: &'static str,
    taches: &'static [&'static str],
    favoris: &'static [(&'static str, &'static str)],
}

const USAGES: [DefUsage; 3] = [
    DefUsage {
        id: "texte_vers_image",
        nom: "Texte → image",
        taches: &["texte_vers_image"],
        favoris: &[("texte_vers_image", "Créer une image")],
    },
    DefUsage {
        id: "image_vers_image",
        nom: "Image → image",
        taches: &["retouche"],
        favoris: &[
            ("retouche", "Retoucher"),
            ("references", "Créer depuis des références"),
        ],
    },
    DefUsage {
        id: "video",
        nom: "Vidéo",
        taches: &["image_vers_video", "texte_vers_video"],
        favoris: &[
            ("image_vers_video", "Animer une image"),
            ("texte_vers_video", "Clip depuis un texte"),
        ],
    },
];

/// Un favori d'un usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Favori {
    pub cle: String,
    pub libelle: String,
    /// Le workflow choisi ; rien : aucun.
    pub valeur: Option<String>,
}

/// Une LoRA de la famille d'un workflow.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoraFamille {
    pub nom: String,
    pub declencheurs: String,
    pub force: f64,
}

/// Un fichier qui manque à un workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Manquant {
    pub dossier: String,
    pub nom: String,
}

/// Un LLM entraîné pour ce modèle par son auteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Specialiste {
    pub fichier: String,
    pub nom: String,
    pub page: String,
    /// `creation` ou `retouche`.
    pub usage: String,
    pub taille: u64,
    pub a_telecharger: u64,
    pub installe: bool,
    pub choisi: bool,
    pub vision: bool,
    pub licence: Licence,
}

/// Un spécialiste connu mais pas encore utilisable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AVenir {
    pub nom: String,
    pub page: String,
    pub raison: String,
}

/// Ce que le workflow attend du LLM, et le LLM qui réécrit pour lui.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reecriture {
    /// Le LLM choisi pour ce workflow ; rien : le LLM par défaut.
    pub llm: Option<String>,
    /// Celui qui réécrira vraiment (le choisi s'il est installé, sinon le défaut).
    pub effectif: String,
    pub style: String,
    pub langue: String,
    pub mots_max: Option<u64>,
    /// Le nombre de règles propres au modèle.
    pub regles: usize,
    pub specialistes: Vec<Specialiste>,
    pub a_venir: Vec<AVenir>,
    /// Un spécialiste choisi pour un autre modèle : les workflows pour qui il est fait (Aria passera au défaut).
    pub ailleurs: Option<Vec<String>>,
}

/// Un workflow dans son onglet d'usage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowUsage {
    pub id: String,
    pub nom: String,
    pub tache: String,
    pub famille: String,
    pub dossier_modeles: String,
    pub dossier_loras: String,
    pub licence: Licence,
    pub vram_estimee_go: Option<f64>,
    pub vram_min_go: Option<f64>,
    pub taille: u64,
    pub pret: bool,
    pub manquants: Vec<Manquant>,
    pub a_telecharger: u64,
    /// Les checkpoints prêts à tourner.
    pub modeles: Vec<String>,
    /// Les checkpoints présents dont les steps sont à valider.
    pub a_valider: Vec<String>,
    pub origine: Option<String>,
    /// Le checkpoint par défaut (réglage `modele`), pour un workflow qui en choisit un.
    pub modele_defaut: Option<String>,
    pub loras: Vec<LoraFamille>,
    /// Les favoris que ce workflow peut tenir, et ceux qu'il tient.
    pub possibles: Vec<String>,
    pub favori_pour: Vec<String>,
    pub references: usize,
    pub variation: bool,
    pub reecriture: Reecriture,
}

/// Un onglet d'usage.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Usage {
    pub id: String,
    pub nom: String,
    pub workflows: Vec<WorkflowUsage>,
    pub favoris: Vec<Favori>,
    /// Image → image : les workflows texte → image qui savent redessiner une image.
    pub variation: Vec<String>,
}

/// Les onglets d'usage et les LLM proposés.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usages {
    pub llm_defaut: String,
    pub llm_choix: Vec<ChoixLlm>,
    pub usages: Vec<Usage>,
}

fn texte(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// Les favoris de génération (`favoris.generation`).
fn generation(dossier: &Dossier) -> Map<String, Value> {
    reglages::section(dossier, "favoris")
        .and_then(|f| f.get("generation").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

/// Les favoris qu'un workflow peut tenir dans un usage.
fn possibles(u: &DefUsage, wf: &workflows::Workflow) -> Vec<String> {
    u.favoris
        .iter()
        .map(|(cle, _)| *cle)
        .filter(|cle| *cle == wf.tache() || (*cle == "references" && wf.accepte_references() > 0))
        .map(str::to_owned)
        .collect()
}

fn reecriture(
    dossier: &Dossier,
    modeles_dir: &Path,
    wf: &workflows::Workflow,
    reglage: Option<&Map<String, Value>>,
) -> Reecriture {
    let profil = wf.fiche.get("profil_prompt");
    let choisi = reglages::section(dossier, "divers").and_then(|d| {
        d.get("llm_par_workflow")?
            .get(&wf.id)?
            .as_str()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
    });
    let regles = profil
        .and_then(|p| p.get("regles"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        + profil
            .and_then(|p| p.get("consigne"))
            .and_then(Value::as_object)
            .map_or(0, |c| c.values().filter_map(Value::as_array).map(Vec::len).sum());
    let pour_ce_workflow = |s: &Value| {
        s.get("workflows")
            .and_then(Value::as_array)
            .is_some_and(|l| l.iter().any(|w| w.as_str() == Some(wf.id.as_str())))
    };
    let etats = installations::llm(modeles_dir);
    let specialistes = fiches::catalogue_llm()
        .unwrap_or_default()
        .iter()
        .filter(|f| f.get("specialiste").is_some_and(pour_ce_workflow))
        .filter_map(|f| {
            let id = texte(f.get("id"));
            let e = etats.iter().find(|e| e.fichier == id)?;
            Some(Specialiste {
                choisi: choisi.as_deref() == Some(id.as_str()),
                fichier: id,
                nom: e.nom.clone(),
                page: e.page.clone(),
                usage: e.specialiste.clone().unwrap_or_default(),
                taille: e.taille,
                a_telecharger: e.a_telecharger,
                installe: e.installe,
                vision: e.vision,
                licence: e.licence.clone(),
            })
        })
        .collect();
    let a_venir = fiches::a_venir()
        .iter()
        .filter(|&a| pour_ce_workflow(a))
        .map(|a| AVenir {
            nom: texte(a.get("nom")),
            page: texte(a.get("page")),
            raison: texte(a.get("raison")),
        })
        .collect();
    let ailleurs = choisi
        .as_deref()
        .and_then(fiches::specialiste)
        .filter(|s| !pour_ce_workflow(s))
        .map(|s| {
            let catalogue = workflows::catalogue().0;
            s.get("workflows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| {
                    catalogue
                        .iter()
                        .find(|w| w.id == id)
                        .and_then(|w| w.fiche.get("nom").and_then(Value::as_str))
                        .unwrap_or(id)
                        .to_owned()
                })
                .collect()
        });
    Reecriture {
        effectif: llm::de_workflow(dossier, modeles_dir, &wf.id),
        style: texte(profil.and_then(|p| p.get("style"))),
        langue: texte(profil.and_then(|p| p.get("langue"))),
        mots_max: prompt::mots_max_de(Some(wf), reglage),
        regles,
        specialistes,
        a_venir,
        ailleurs,
        llm: choisi,
    }
}

fn workflow_usage(
    dossier: &Dossier,
    data: &Path,
    modeles_dir: &Path,
    u: &DefUsage,
    wf: &workflows::Workflow,
    section: &Map<String, Value>,
    favoris: &Map<String, Value>,
) -> WorkflowUsage {
    let reglage = section.get(&wf.id).and_then(Value::as_object);
    let manquants: Vec<Manquant> = wf
        .manquants(modeles_dir)
        .iter()
        .map(|f| Manquant {
            dossier: texte(f.get("dossier")),
            nom: texte(f.get("nom")),
        })
        .collect();
    let a_telecharger = wf
        .manquants(modeles_dir)
        .iter()
        .filter_map(|f| f.get("taille").and_then(Value::as_u64))
        .sum();
    let possibles = possibles(u, wf);
    let materiel = |cle: &str| {
        wf.fiche
            .get("materiel")
            .and_then(|m| m.get(cle))
            .and_then(Value::as_f64)
    };
    WorkflowUsage {
        id: wf.id.clone(),
        nom: texte(wf.fiche.get("nom")),
        tache: wf.tache().to_owned(),
        famille: wf.famille().to_owned(),
        dossier_modeles: wf.dossier_modeles(),
        dossier_loras: wf.dossier_loras(),
        licence: installations::licence(wf.fiche.get("licence")),
        vram_estimee_go: materiel("vram_estimee_go"),
        vram_min_go: materiel("vram_min_go"),
        taille: wf
            .fiche
            .get("fichiers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|f| f.get("taille").and_then(Value::as_u64))
            .sum(),
        pret: manquants.is_empty(),
        manquants,
        a_telecharger,
        modeles: wf.variantes(modeles_dir, reglage),
        a_valider: wf
            .presents(modeles_dir)
            .into_iter()
            .filter(|m| wf.a_valider(reglage, m))
            .collect(),
        origine: wf.modele_origine(),
        modele_defaut: wf.entree("modele").and_then(|_| {
            wf.regle(modeles_dir, reglage)
                .get("modele")?
                .as_str()
                .map(str::to_owned)
        }),
        loras: collection::loras(dossier, data, modeles_dir, &wf.dossier_loras())
            .into_iter()
            .map(|(nom, r)| LoraFamille {
                nom,
                declencheurs: r.declencheurs,
                force: r.force,
            })
            .collect(),
        favori_pour: possibles
            .iter()
            .filter(|cle| favoris.get(cle.as_str()).and_then(Value::as_str) == Some(wf.id.as_str()))
            .cloned()
            .collect(),
        possibles,
        references: wf.accepte_references(),
        variation: wf.accepte_variation(),
        reecriture: reecriture(dossier, modeles_dir, wf, reglage),
    }
}

/// Les onglets d'usage.
pub fn usages(dossier: &Dossier, data: &Path, modeles_dir: &Path) -> Usages {
    let catalogue = workflows::catalogue().0;
    let section = reglages::section(dossier, "workflows").unwrap_or_default();
    let favoris = generation(dossier);
    Usages {
        llm_defaut: llm::defaut(modeles_dir),
        llm_choix: plateau::llm_au_choix(modeles_dir),
        usages: USAGES
            .iter()
            .map(|u| Usage {
                id: u.id.to_owned(),
                nom: u.nom.to_owned(),
                workflows: catalogue
                    .iter()
                    .filter(|wf| u.taches.contains(&wf.tache()))
                    .map(|wf| workflow_usage(dossier, data, modeles_dir, u, wf, &section, &favoris))
                    .collect(),
                favoris: u
                    .favoris
                    .iter()
                    .map(|(cle, libelle)| Favori {
                        cle: (*cle).to_owned(),
                        libelle: (*libelle).to_owned(),
                        valeur: favoris.get(*cle).and_then(Value::as_str).map(str::to_owned),
                    })
                    .collect(),
                variation: if u.id == "image_vers_image" {
                    catalogue
                        .iter()
                        .filter(|wf| wf.tache() == "texte_vers_image" && wf.accepte_variation())
                        .map(|wf| texte(wf.fiche.get("nom")))
                        .collect()
                } else {
                    Vec::new()
                },
            })
            .collect(),
    }
}

/// Le workflow utilisé par défaut pour un usage (`favoris.generation.<cle>`).
///
/// # Errors
/// Usage ou workflow inconnus, workflow qui ne sert pas à cet usage ou incomplet, `reglages.json` qui ne s'écrit pas.
pub fn definir_favori(
    dossier: &Dossier,
    modeles_dir: &Path,
    cle: &str,
    workflow: &str,
) -> Result<(), String> {
    let usage = USAGES
        .iter()
        .find(|u| u.favoris.iter().any(|(c, _)| *c == cle))
        .ok_or_else(|| format!("Usage inconnu : {cle}"))?;
    let wf = workflows::obtenir(workflow)?;
    let nom = texte(wf.fiche.get("nom"));
    if !usage.taches.contains(&wf.tache()) || !possibles(usage, &wf).iter().any(|c| c == cle) {
        let libelle = usage
            .favoris
            .iter()
            .find(|(c, _)| *c == cle)
            .map_or(cle, |(_, l)| *l);
        return Err(format!("{nom} ne sert pas à « {libelle} »"));
    }
    if !wf.manquants(modeles_dir).is_empty() {
        return Err(format!(
            "{nom} n'est pas complet : télécharge d'abord ce qui manque"
        ));
    }
    reglages::modifier_section(dossier, "favoris", |s| {
        let mut generation = match s.remove("generation") {
            Some(Value::Object(g)) => g,
            _ => Map::new(),
        };
        generation.insert(cle.to_owned(), Value::String(workflow.to_owned()));
        s.insert("generation".to_owned(), Value::Object(generation));
    })
    .map_err(|e| e.to_string())
}

/// Le LLM qui réécrit les prompts de ce workflow (`divers.llm_par_workflow`) ; rien : le LLM par défaut.
///
/// # Errors
/// Workflow inconnu, GGUF absent, ou `reglages.json` qui ne s'écrit pas.
pub fn definir_llm(
    dossier: &Dossier,
    modeles_dir: &Path,
    workflow: &str,
    choisi: Option<&str>,
) -> Result<(), String> {
    workflows::obtenir(workflow)?;
    if let Some(l) = choisi
        && !llm::disponibles(modeles_dir).iter().any(|d| d == l)
    {
        return Err(format!("GGUF absent de modeles\\llm : {l}"));
    }
    reglages::modifier_section(dossier, "divers", |s| {
        let mut par_workflow = match s.remove("llm_par_workflow") {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        };
        match choisi {
            Some(l) => {
                par_workflow.insert(workflow.to_owned(), Value::String(l.to_owned()));
            }
            None => {
                par_workflow.remove(workflow);
            }
        }
        s.insert("llm_par_workflow".to_owned(), Value::Object(par_workflow));
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn usages_et_favoris() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let data = temp.path().join("data");
        let modeles = temp.path().join("modeles");
        let u = usages(&dossier, &data, &modeles);
        let ids: Vec<&str> = u.usages.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["texte_vers_image", "image_vers_image", "video"]);
        let t2i = &u.usages[0];
        let krea = t2i.workflows.iter().find(|w| w.id == "krea2_turbo").unwrap();
        assert!(!krea.pret && krea.a_telecharger > 0);
        assert_eq!(krea.possibles, vec!["texte_vers_image".to_owned()]);
        assert_eq!(krea.reecriture.langue, "en");
        assert!(u.usages[1].variation.contains(&"Krea 2".to_owned()));
        let qwen = t2i.workflows.iter().find(|w| w.id == "qwen_image_2_1").unwrap();
        assert!(
            qwen.reecriture
                .specialistes
                .iter()
                .any(|s| s.usage == "creation" && !s.installe)
        );
        // Un workflow incomplet ne devient pas favori ; complet, si.
        assert!(
            definir_favori(&dossier, &modeles, "texte_vers_image", "krea2_turbo")
                .unwrap_err()
                .contains("pas complet")
        );
        assert!(definir_favori(&dossier, &modeles, "retouche", "krea2_turbo").is_err());
        assert!(definir_favori(&dossier, &modeles, "inconnu", "krea2_turbo").is_err());
        for (fiche, dossier_fichier) in installations::manquants(&modeles, "krea2_turbo").unwrap() {
            let chemin = crate::modeles::destination(&modeles, &dossier_fichier, &fiche.nom).unwrap();
            fs::create_dir_all(chemin.parent().unwrap()).unwrap();
            fs::write(chemin, b"x").unwrap();
        }
        definir_favori(&dossier, &modeles, "texte_vers_image", "krea2_turbo").unwrap();
        let u = usages(&dossier, &data, &modeles);
        assert_eq!(u.usages[0].favoris[0].valeur.as_deref(), Some("krea2_turbo"));
        let krea = u.usages[0]
            .workflows
            .iter()
            .find(|w| w.id == "krea2_turbo")
            .unwrap();
        assert_eq!(krea.favori_pour, vec!["texte_vers_image".to_owned()]);
        assert_eq!(krea.modeles.len(), 1);
    }

    #[test]
    fn llm_d_un_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let modeles = temp.path().join("modeles");
        assert!(definir_llm(&dossier, &modeles, "krea2_turbo", Some("absent.gguf")).is_err());
        fs::create_dir_all(modeles.join("llm")).unwrap();
        fs::write(modeles.join("llm/Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf"), b"x").unwrap();
        definir_llm(
            &dossier,
            &modeles,
            "krea2_turbo",
            Some("Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf"),
        )
        .unwrap();
        let u = usages(&dossier, &temp.path().join("data"), &modeles);
        let krea = u.usages[0]
            .workflows
            .iter()
            .find(|w| w.id == "krea2_turbo")
            .unwrap();
        assert_eq!(
            krea.reecriture.llm.as_deref(),
            Some("Qwen-Image-2.1-PE-T2I.Q5_K_M.gguf")
        );
        assert_eq!(
            krea.reecriture.ailleurs,
            Some(vec!["Qwen-Image 2.1".to_owned()]),
            "un spécialiste d'un autre modèle"
        );
        definir_llm(&dossier, &modeles, "krea2_turbo", None).unwrap();
        let u = usages(&dossier, &temp.path().join("data"), &modeles);
        let krea = u.usages[0]
            .workflows
            .iter()
            .find(|w| w.id == "krea2_turbo")
            .unwrap();
        assert!(krea.reecriture.llm.is_none());
    }
}
