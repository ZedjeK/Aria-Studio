//! Les workflows et leur préparation, comme en v2 (`app/workflows.py`, `app/reglages.py` pour la section
//! `workflows`) : quel checkpoint tourne, avec quels steps et quels réglages, les LoRA branchées, puis le graphe
//! ComfyUI rempli, prêt à envoyer. Le graphe livré n'est jamais modifié.
//!
//! Ordre de priorité d'une valeur : celle de l'appel, puis le réglage de l'utilisateur (`reglages.json`, section
//! `workflows.<id>`), puis le défaut de la fiche. Les steps font exception : ceux du checkpoint choisi, sauf s'ils
//! sont donnés par l'appel. Un checkpoint dont les steps ne sont pas connus est « à valider » et ne tourne pas.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::fiches::{self, LIVRES};
use crate::modeles::{self, EXTENSIONS_IMAGE};
use crate::securite::Filtre;
use crate::{Dossier, reglages};

/// Les formats d'image proposés (entrée `ratio`).
pub const FORMATS: [&str; 8] = [
    "1:1 (Square)",
    "2:3 (Portrait Photo)",
    "3:2 (Photo)",
    "3:4 (Portrait Standard)",
    "4:3 (Standard)",
    "9:16 (Portrait Widescreen)",
    "16:9 (Widescreen)",
    "21:9 (Ultrawide)",
];
/// Les steps qu'un utilisateur peut valider pour un checkpoint, au plus (comme en v2).
pub const STEPS_MAX: u64 = 150;
/// La force d'une LoRA, au plus.
pub const FORCE_LORA_MAX: f64 = 2.0;
/// Le type de nœud d'une LoRA sur le modèle.
const NOEUD_LORA: &str = "LoraLoaderModelOnly";
/// Le plafond d'Aria : 10 images de référence, ce que les modèles ComfyUI officiels de Qwen-Image 2.1 Edit reçoivent.
/// Chaque workflow garde sa propre limite, celle de son encodeur.
pub const REFERENCES_MAX: usize = 10;
/// La liberté d'une variation (denoise), au moins et au plus.
pub const LIBERTE: (f64, f64) = (0.05, 1.0);

/// Les encodeurs d'édition qui acceptent plusieurs images de référence : type, entrée de l'image n, nombre maximal
/// d'images, nœud de latent vide adapté au modèle.
const ENCODEURS_REFERENCES: [(&str, &str, usize, &str); 2] = [
    ("TextEncodeQwenImageEditPlus", "image{}", 3, "EmptySD3LatentImage"),
    ("TextEncodeQwenImage21", "images.image_{}", 16, "EmptyLatentImage"),
];

/// Les nœuds que le code ajoute aux graphes livrés (références, variation, LoRA) : un ComfyUI doit les connaître.
pub const NOEUDS_AJOUTES: [&str; 20] = [
    "ImageFromBatch",
    "SaveImage",
    "LoadImage",
    "ResolutionSelector",
    "EmptyLatentImage",
    "EmptySD3LatentImage",
    "ImageScale",
    "ImageScaleToTotalPixels",
    "VAEEncode",
    NOEUD_LORA,
    // MiniMax H3 : essai de voix et séquences du Montage (voix.rs, sequences.rs).
    "MiniMaxH3ReferenceToVideo",
    "MiniMaxH3AddGuide",
    "SaveAudio",
    "LoadAudio",
    "TrimAudioDuration",
    "LoadVideo",
    "GetVideoComponents",
    "Video Slice",
    "ConcatenateVideo",
    "SaveVideo",
];

/// Les nœuds ComfyUI dont Aria se sert : ceux des workflows livrés, et ceux que le code ajoute.
pub fn noeuds_utilises() -> std::collections::BTreeSet<String> {
    let mut noeuds: std::collections::BTreeSet<String> =
        NOEUDS_AJOUTES.iter().map(|n| (*n).to_owned()).collect();
    for wf in catalogue().0 {
        noeuds.extend(
            wf.api
                .as_object()
                .into_iter()
                .flat_map(|g| g.values())
                .filter_map(|n| n.get("class_type").and_then(Value::as_str))
                .map(str::to_owned),
        );
    }
    noeuds
}

fn encodeur(type_: &str) -> Option<(&'static str, usize, &'static str)> {
    ENCODEURS_REFERENCES
        .iter()
        .find(|(t, _, _, _)| *t == type_)
        .map(|(_, entree, max, latent)| (*entree, *max, *latent))
}

/// Un workflow livré : sa fiche et son graphe ComfyUI.
#[derive(Debug, Clone, PartialEq)]
pub struct Workflow {
    pub id: String,
    pub fiche: Value,
    pub api: Value,
}

/// Les workflows livrés dont la fiche est bonne, et les messages des fiches écartées.
pub fn catalogue() -> (Vec<Workflow>, Vec<String>) {
    let mut bons = Vec::new();
    let mut erreurs = Vec::new();
    for livre in &LIVRES {
        let lu = fiches::lire(livre.fiche).and_then(|fiche| {
            let problemes = fiches::problemes_workflow(&fiche);
            if !problemes.is_empty() {
                return Err(problemes.join(" ; "));
            }
            let api =
                serde_json::from_str::<Value>(livre.api).map_err(|e| format!("api.json illisible : {e}"))?;
            Ok(Workflow {
                id: livre.id.to_owned(),
                fiche,
                api,
            })
        });
        match lu {
            Ok(workflow) => bons.push(workflow),
            Err(probleme) => erreurs.push(format!("Fiche {} : {probleme}", livre.id)),
        }
    }
    (bons, erreurs)
}

/// Un workflow par son identifiant.
///
/// # Errors
/// Il n'existe pas, ou sa fiche a été écartée.
pub fn obtenir(id: &str) -> Result<Workflow, String> {
    catalogue()
        .0
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| format!("Workflow inconnu : {id}"))
}

/// Les réglages de l'utilisateur pour ce workflow : `reglages.json`, section `workflows.<id>`.
pub type ReglagesWorkflow = Map<String, Value>;

impl Workflow {
    fn texte(&self, cle: &str) -> &str {
        self.fiche.get(cle).and_then(Value::as_str).unwrap_or_default()
    }

    pub fn tache(&self) -> &str {
        self.texte("tache")
    }

    pub fn famille(&self) -> &str {
        self.texte("famille")
    }

    /// Le nœud et l'entrée du graphe derrière un nom logique (`prompt`, `seed`, `steps`, `modele`…).
    pub fn entree(&self, nom: &str) -> Option<(String, String)> {
        let paire = self.fiche.get("entrees")?.get(nom)?.as_array()?;
        Some((
            paire.first()?.as_str()?.to_owned(),
            paire.get(1)?.as_str()?.to_owned(),
        ))
    }

    fn a_l_entree(&self, nom: &str) -> bool {
        self.entree(nom).is_some()
    }

    /// `<nature>/<famille>/modeles`.
    pub fn dossier_modeles(&self) -> String {
        modeles::dossier_famille(self.famille(), "modeles").unwrap_or_default()
    }

    /// `<nature>/<famille>/loras`.
    pub fn dossier_loras(&self) -> String {
        modeles::dossier_famille(self.famille(), "loras").unwrap_or_default()
    }

    /// Le checkpoint d'origine : le premier fichier de la fiche rangé dans le dossier des modèles de la famille.
    pub fn modele_origine(&self) -> Option<String> {
        let dossier = self.dossier_modeles();
        self.fichiers_fiche()
            .into_iter()
            .find(|f| f.get("dossier").and_then(Value::as_str) == Some(dossier.as_str()))
            .and_then(|f| f.get("nom").and_then(Value::as_str).map(str::to_owned))
    }

    fn fichiers_fiche(&self) -> Vec<&Value> {
        self.fiche
            .get("fichiers")
            .and_then(Value::as_array)
            .map(|l| l.iter().collect())
            .unwrap_or_default()
    }

    /// Les checkpoints présents dans le dossier de la famille, l'origine en tête si elle est là.
    pub fn presents(&self, dossier_modeles: &Path) -> Vec<String> {
        let mut presents = modeles::fichiers(dossier_modeles, &self.dossier_modeles(), &EXTENSIONS_IMAGE);
        let origine = self.modele_origine();
        presents.sort_by_key(|m| Some(m) != origine.as_ref());
        presents
    }

    /// Les fichiers de la fiche absents du disque. Le checkpoint d'origine n'est pas exigé si la famille en a déjà un.
    pub fn manquants(&self, dossier_modeles: &Path) -> Vec<Value> {
        let famille = self.dossier_modeles();
        let un_checkpoint = !self.presents(dossier_modeles).is_empty();
        self.fichiers_fiche()
            .into_iter()
            .filter(|f| {
                let dossier = f.get("dossier").and_then(Value::as_str).unwrap_or_default();
                let nom = f.get("nom").and_then(Value::as_str).unwrap_or_default();
                if dossier == famille && un_checkpoint {
                    return false;
                }
                !modeles::destination(dossier_modeles, dossier, nom).is_ok_and(|p| p.is_file())
            })
            .cloned()
            .collect()
    }

    /// Les steps que dit la fiche, à proposer quand un checkpoint est à valider : le défaut, puis les
    /// recommandations, sans doublon.
    pub fn steps_suggeres(&self) -> Vec<u64> {
        let mut suggeres: Vec<u64> = Vec::new();
        let defaut = self.fiche.pointer("/defauts/steps").and_then(entier);
        let recommandes = self
            .fiche
            .pointer("/recommandations/valeurs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|r| r.get("cle").and_then(Value::as_str) == Some("steps"))
            .filter_map(|r| r.get("valeur").and_then(entier));
        for steps in defaut.into_iter().chain(recommandes) {
            if steps > 0 && !suggeres.contains(&steps) {
                suggeres.push(steps);
            }
        }
        suggeres
    }

    /// Les steps que la fiche fixe sans ambiguïté (défaut et recommandations d'accord) ; 0 sinon.
    pub fn steps_clairs(&self) -> u64 {
        if !self.a_l_entree("steps") {
            return 0;
        }
        let mut valeurs: BTreeSet<u64> = BTreeSet::new();
        if let Some(defaut) = self.fiche.pointer("/defauts/steps").and_then(entier) {
            valeurs.insert(defaut);
        }
        for r in self
            .fiche
            .pointer("/recommandations/valeurs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if r.get("cle").and_then(Value::as_str) == Some("steps")
                && let Some(v) = r.get("valeur").and_then(entier)
            {
                valeurs.insert(v);
            }
        }
        match valeurs.iter().collect::<Vec<_>>().as_slice() {
            [une] => **une,
            _ => 0,
        }
    }

    fn variante<'r>(&self, reglages: Option<&'r ReglagesWorkflow>, checkpoint: &str) -> Option<&'r Value> {
        reglages?.get("variantes")?.get(checkpoint)
    }

    /// Les steps d'un checkpoint : ceux validés par l'utilisateur, sinon ceux de la fiche pour l'origine, sinon 0.
    pub fn steps_de(&self, reglages: Option<&ReglagesWorkflow>, checkpoint: &str) -> u64 {
        if let Some(steps) = self
            .variante(reglages, checkpoint)
            .and_then(|v| v.get("steps"))
            .and_then(entier)
            .filter(|s| *s > 0)
        {
            return steps;
        }
        if self.modele_origine().as_deref() == Some(checkpoint) {
            self.steps_clairs()
        } else {
            0
        }
    }

    /// Un checkpoint dont les steps ne sont pas connus ne tourne pas.
    pub fn a_valider(&self, reglages: Option<&ReglagesWorkflow>, checkpoint: &str) -> bool {
        self.a_l_entree("steps") && self.steps_de(reglages, checkpoint) == 0
    }

    /// Les checkpoints prêts à tourner.
    pub fn variantes(&self, dossier_modeles: &Path, reglages: Option<&ReglagesWorkflow>) -> Vec<String> {
        self.presents(dossier_modeles)
            .into_iter()
            .filter(|m| !self.a_valider(reglages, m))
            .collect()
    }

    /// Les réglages qui s'appliquent : ceux de l'utilisateur, et le checkpoint par défaut (son choix s'il est
    /// encore prêt, sinon le premier prêt, sinon l'origine).
    pub fn regle(&self, dossier_modeles: &Path, reglages: Option<&ReglagesWorkflow>) -> Map<String, Value> {
        let mut regle: Map<String, Value> = reglages
            .map(|r| {
                r.iter()
                    .filter(|(k, _)| k.as_str() != "variantes")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if self.a_l_entree("modele") {
            let prets = self.variantes(dossier_modeles, reglages);
            let choisi = regle
                .get("modele")
                .and_then(Value::as_str)
                .filter(|m| prets.iter().any(|p| p == m));
            let modele = choisi
                .map(str::to_owned)
                .or_else(|| prets.first().cloned())
                .or_else(|| self.modele_origine());
            match modele {
                Some(m) => regle.insert("modele".to_owned(), Value::String(m)),
                None => regle.remove("modele"),
            };
        }
        if regle
            .get("ratio")
            .and_then(Value::as_str)
            .is_some_and(|r| !FORMATS.contains(&r))
        {
            regle.remove("ratio");
        }
        regle
    }

    /// Le graphe ComfyUI rempli avec ces valeurs (`prompt`, `seed`, `prefixe`, `ratio`, `modele`, `loras`…).
    ///
    /// # Errors
    /// Modèle absent, checkpoint à valider, LoRA absente ou trop forte, entrée inconnue du workflow, ou option pas
    /// encore prise en charge par cette version, ou prompt refusé par le filtre AS-18 (le message est celui du
    /// filtre : « ERREUR AS-18 · Génération bloquée : … »).
    pub fn preparer(
        &self,
        valeurs: &Map<String, Value>,
        dossier_modeles: &Path,
        reglages: Option<&ReglagesWorkflow>,
        filtre: &Filtre,
    ) -> Result<Value, String> {
        let mut api = self.api.clone();
        let regle = self.regle(dossier_modeles, reglages);
        let mut valeurs = valeurs.clone();
        if valeurs.get("modele").is_some_and(Value::is_null) {
            valeurs.remove("modele");
        }
        let mut tout: Map<String, Value> = self
            .fiche
            .get("defauts")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (cle, valeur) in &regle {
            if self.a_l_entree(cle) {
                tout.insert(cle.clone(), valeur.clone());
            }
        }
        for (cle, valeur) in &valeurs {
            tout.insert(cle.clone(), valeur.clone());
        }
        match tout.get("modele").cloned() {
            None => {}
            Some(Value::Null) => {
                tout.remove("modele");
            }
            Some(m) => {
                let nom = m.as_str().unwrap_or_default();
                if !self.presents(dossier_modeles).iter().any(|p| p == nom) {
                    return Err(format!("Modèle absent de {} : {nom}", self.dossier_modeles()));
                }
            }
        }
        let checkpoint = tout
            .get("modele")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| self.modele_origine())
            .unwrap_or_default();
        if self.a_valider(reglages, &checkpoint) {
            return Err(format!(
                "Checkpoint à valider : {checkpoint}. Ses steps ne sont pas connus : valide-les dans Modèles → Ma \
                 collection avant de t'en servir"
            ));
        }
        if self.a_l_entree("steps") && !valeurs.contains_key("steps") {
            tout.insert("steps".to_owned(), json!(self.steps_de(reglages, &checkpoint)));
        }
        let loras = tout.remove("loras").filter(|l| !l.is_null());
        // Images 2 et suivantes (déjà téléversées) ; l'image 1 est l'entrée « image ».
        let references: Vec<String> = tout
            .remove("references")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        // S'inspirer : le format choisi au lieu de celui de l'image 1.
        let format_sortie = tout
            .remove("format_sortie")
            .and_then(|v| v.as_str().map(str::to_owned));
        // {image : nom téléversé, denoise : la liberté, format : facultatif}.
        let variation = tout.remove("variation").filter(|v| !v.is_null());
        // Le point de passage obligé de tout prompt vers ComfyUI (comme en v2) : le filtre AS-18.
        if let Some(prompt) = tout.get("prompt") {
            let texte = prompt.as_str().map_or_else(|| prompt.to_string(), str::to_owned);
            filtre.verifier(&[texte.as_str()]).map_err(|e| e.to_string())?;
        }
        for (cle, valeur) in &tout {
            let (noeud, entree) = self
                .entree(cle)
                .ok_or_else(|| format!("{} n'a pas d'entrée « {cle} »", self.id))?;
            let place = api
                .get_mut(&noeud)
                .and_then(|n| n.get_mut("inputs"))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| format!("{} : nœud {noeud} introuvable dans le graphe", self.id))?;
            place.insert(entree, valeur.clone());
        }
        appliquer_noeuds(&mut api, &regle);
        let lora_workflow = self
            .variante(reglages, &checkpoint)
            .and_then(|v| v.get("lora_workflow"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !lora_workflow {
            retirer_loras_workflow(&mut api);
        }
        if !references.is_empty() || format_sortie.is_some() {
            self.brancher_references(&mut api, &references, format_sortie.as_deref())?;
        }
        if let Some(v) = variation {
            let image = v
                .get("image")
                .and_then(Value::as_str)
                .ok_or_else(|| "Variation sans image de départ".to_owned())?;
            let denoise = v
                .get("denoise")
                .and_then(Value::as_f64)
                .ok_or_else(|| "Variation sans liberté".to_owned())?;
            let format = v.get("format").and_then(Value::as_str);
            let megapixels = regle
                .get("megapixels")
                .and_then(Value::as_f64)
                .or_else(|| self.fiche.pointer("/defauts/megapixels").and_then(Value::as_f64))
                .unwrap_or(1.0);
            self.brancher_variation(&mut api, image, denoise, format, megapixels)?;
        }
        if let Some(Value::Array(choix)) = loras
            && !choix.is_empty()
        {
            let disponibles = modeles::fichiers(dossier_modeles, &self.dossier_loras(), &EXTENSIONS_IMAGE);
            let chargeur = self
                .entree("modele")
                .map(|(n, _)| n)
                .ok_or_else(|| format!("{} ne peut pas recevoir de LoRA", self.id))?;
            inserer_loras(&mut api, &chargeur, &choix, &disponibles, &self.dossier_loras())?;
        }
        Ok(api)
    }
}

/// Met en service un checkpoint de la famille de ce workflow avec les steps validés par l'utilisateur : ils
/// deviennent ses steps dans tous les workflows de la famille qui en ont (`reglages.json`,
/// `workflows.<id>.variantes.<checkpoint>.steps`), comme en v2.
///
/// # Errors
/// Steps hors de 1 à 150, workflow inconnu, checkpoint absent, ou `reglages.json` qui ne s'écrit pas.
pub fn valider_checkpoint(
    dossier: &Dossier,
    dossier_modeles: &Path,
    workflow: &str,
    checkpoint: &str,
    steps: u64,
) -> Result<(), String> {
    if !(1..=STEPS_MAX).contains(&steps) {
        return Err(format!("Steps : un entier entre 1 et {STEPS_MAX}"));
    }
    let wf = obtenir(workflow)?;
    let concernes: Vec<String> = catalogue()
        .0
        .into_iter()
        .filter(|w| w.famille() == wf.famille() && w.a_l_entree("steps"))
        .filter(|w| w.presents(dossier_modeles).iter().any(|p| p == checkpoint))
        .map(|w| w.id)
        .collect();
    if concernes.is_empty() {
        return Err(format!(
            "Checkpoint absent, ou sans workflow qui règle ses steps : {}/{checkpoint}",
            wf.dossier_modeles()
        ));
    }
    reglages::modifier_section(dossier, "workflows", |section| {
        for id in &concernes {
            let mut reglage = sortir(section, id);
            let mut variantes = sortir(&mut reglage, "variantes");
            let mut variante = sortir(&mut variantes, checkpoint);
            variante.insert("steps".to_owned(), json!(steps));
            variantes.insert(checkpoint.to_owned(), Value::Object(variante));
            reglage.insert("variantes".to_owned(), Value::Object(variantes));
            section.insert(id.clone(), Value::Object(reglage));
        }
    })
    .map_err(|e| e.to_string())
}

/// L'objet rangé sous `cle`, retiré de `map` ; un objet vide s'il n'y est pas ou n'est pas un objet.
fn sortir(map: &mut Map<String, Value>, cle: &str) -> Map<String, Value> {
    match map.remove(cle) {
        Some(Value::Object(objet)) => objet,
        _ => Map::new(),
    }
}

impl Workflow {
    /// Vidéo : enregistre aussi la première image, qui sert de vignette dans les Rushes.
    ///
    /// # Errors
    /// Le workflow ne produit pas de vidéo (pas de nœud `CreateVideo` unique).
    pub fn ajouter_affiche(&self, api: &mut Value, prefixe: &str) -> Result<(), String> {
        let cv =
            noeud_unique(api, "CreateVideo").ok_or_else(|| format!("{} ne produit pas de vidéo", self.id))?;
        let images = api
            .pointer(&format!("/{cv}/inputs/images"))
            .cloned()
            .ok_or_else(|| format!("{} : vidéo sans images", self.id))?;
        let graphe = api
            .as_object_mut()
            .ok_or_else(|| format!("{} : graphe illisible", self.id))?;
        graphe.insert(
            "aria_affiche_lot".to_owned(),
            json!({"class_type": "ImageFromBatch", "inputs": {"image": images, "batch_index": 0, "length": 1}}),
        );
        graphe.insert(
            "aria_affiche".to_owned(),
            json!({"class_type": "SaveImage",
                   "inputs": {"images": ["aria_affiche_lot", 0], "filename_prefix": format!("{prefixe}_affiche")}}),
        );
        Ok(())
    }

    /// Le nombre d'images de référence que ce workflow sait recevoir (0 : aucune) : il faut une entrée « image »
    /// pour la première, et un encodeur d'édition qui en accepte plusieurs.
    pub fn accepte_references(&self) -> usize {
        if !self.a_l_entree("image") {
            return 0;
        }
        self.api
            .as_object()
            .into_iter()
            .flat_map(|g| g.values())
            .filter_map(|n| n.get("class_type").and_then(Value::as_str).and_then(encodeur))
            .map(|(_, max, _)| max)
            .min()
            .map_or(0, |max| max.min(REFERENCES_MAX))
    }

    /// Réservé à la Retouche (fiche : `reserve: retouche`) : S'inspirer ne le propose pas.
    pub fn reserve_retouche(&self) -> bool {
        self.fiche.get("reserve").and_then(Value::as_str) == Some("retouche")
    }

    /// Redessiner une image (img2img) demande un KSampler et un VAELoader uniques.
    pub fn accepte_variation(&self) -> bool {
        noeud_unique(&self.api, "KSampler").is_some() && noeud_unique(&self.api, "VAELoader").is_some()
    }

    /// Images 2 et suivantes sur tous les encodeurs d'édition, et latent vide au format choisi (S'inspirer).
    fn brancher_references(
        &self,
        api: &mut Value,
        noms: &[String],
        format_sortie: Option<&str>,
    ) -> Result<(), String> {
        let encodeurs: Vec<(String, &str, &str)> = api
            .as_object()
            .into_iter()
            .flat_map(|g| g.iter())
            .filter_map(|(id, n)| {
                let (entree, _, latent) = encodeur(n.get("class_type").and_then(Value::as_str)?)?;
                Some((id.clone(), entree, latent))
            })
            .collect();
        if encodeurs.is_empty() || noms.len() + 1 > self.accepte_references() {
            return Err(format!(
                "{} n'accepte pas {} images de référence",
                self.id,
                noms.len() + 1
            ));
        }
        let graphe = api
            .as_object_mut()
            .ok_or_else(|| format!("{} : graphe illisible", self.id))?;
        for (i, nom) in noms.iter().enumerate() {
            let rang = i + 2;
            let id = format!("aria_ref_{rang}");
            graphe.insert(
                id.clone(),
                json!({"class_type": "LoadImage", "inputs": {"image": nom}}),
            );
            for (noeud, entree, _) in &encodeurs {
                if let Some(entrees) = graphe
                    .get_mut(noeud)
                    .and_then(|n| n.get_mut("inputs"))
                    .and_then(Value::as_object_mut)
                {
                    entrees.insert(entree.replace("{}", &rang.to_string()), json!([id, 0]));
                }
            }
        }
        if let Some(format) = format_sortie {
            let ks = noeud_unique(api, "KSampler")
                .ok_or_else(|| format!("{} n'a pas de KSampler unique", self.id))?;
            let latent = encodeurs.first().map_or("EmptyLatentImage", |(_, _, l)| *l);
            let graphe = api
                .as_object_mut()
                .ok_or_else(|| format!("{} : graphe illisible", self.id))?;
            graphe.insert(
                "aria_format".to_owned(),
                json!({"class_type": "ResolutionSelector",
                       "inputs": {"aspect_ratio": format, "megapixels": 1, "multiple": 32}}),
            );
            graphe.insert(
                "aria_latent".to_owned(),
                json!({"class_type": latent,
                       "inputs": {"width": ["aria_format", 0], "height": ["aria_format", 1], "batch_size": 1}}),
            );
            poser(api, &ks, "latent_image", &json!(["aria_latent", 0]));
        }
        Ok(())
    }

    /// img2img : le latent part de l'image de départ au lieu d'un latent vide ; denoise = liberté. Avec un format,
    /// l'image est recadrée au centre à ce format avant d'être redessinée ; sinon le résultat garde son format.
    fn brancher_variation(
        &self,
        api: &mut Value,
        nom: &str,
        denoise: f64,
        format: Option<&str>,
        megapixels: f64,
    ) -> Result<(), String> {
        let (Some(ks), Some(vae)) = (noeud_unique(api, "KSampler"), noeud_unique(api, "VAELoader")) else {
            return Err(format!("{} ne permet pas la variation", self.id));
        };
        if !(LIBERTE.0..=LIBERTE.1).contains(&denoise) {
            return Err(format!("Liberté entre 0,05 et 1 : {denoise}"));
        }
        let graphe = api
            .as_object_mut()
            .ok_or_else(|| format!("{} : graphe illisible", self.id))?;
        graphe.insert(
            "aria_var_image".to_owned(),
            json!({"class_type": "LoadImage", "inputs": {"image": nom}}),
        );
        match format {
            Some(f) => {
                if !FORMATS.contains(&f) {
                    return Err(format!("Format inconnu : {f}"));
                }
                graphe.insert(
                    "aria_var_format".to_owned(),
                    json!({"class_type": "ResolutionSelector",
                           "inputs": {"aspect_ratio": f, "megapixels": megapixels, "multiple": 32}}),
                );
                graphe.insert(
                    "aria_var_echelle".to_owned(),
                    json!({"class_type": "ImageScale", "inputs": {
                        "image": ["aria_var_image", 0], "upscale_method": "lanczos",
                        "width": ["aria_var_format", 0], "height": ["aria_var_format", 1], "crop": "center"}}),
                );
            }
            None => {
                graphe.insert(
                    "aria_var_echelle".to_owned(),
                    json!({"class_type": "ImageScaleToTotalPixels", "inputs": {
                        "image": ["aria_var_image", 0], "upscale_method": "lanczos",
                        "megapixels": megapixels, "resolution_steps": 16}}),
                );
            }
        }
        graphe.insert(
            "aria_var_latent".to_owned(),
            json!({"class_type": "VAEEncode", "inputs": {"pixels": ["aria_var_echelle", 0], "vae": [vae, 0]}}),
        );
        poser(api, &ks, "latent_image", &json!(["aria_var_latent", 0]));
        poser(api, &ks, "denoise", &json!(denoise));
        Ok(())
    }
}

/// Un entier positif, écrit `8` ou `8.0`.
fn entier(valeur: &Value) -> Option<u64> {
    valeur.as_u64().or_else(|| {
        valeur
            .as_f64()
            .filter(|f| f.fract() == 0.0 && *f >= 0.0)
            .and_then(|f| format!("{f:.0}").parse().ok())
    })
}

/// Le seul nœud d'un type dans le graphe ; rien s'il y en a zéro ou plusieurs.
pub fn noeud_unique(api: &Value, type_: &str) -> Option<String> {
    let trouves: Vec<&String> = api
        .as_object()?
        .iter()
        .filter(|(_, n)| n.get("class_type").and_then(Value::as_str) == Some(type_))
        .map(|(id, _)| id)
        .collect();
    match trouves.as_slice() {
        [seul] => Some((*seul).clone()),
        _ => None,
    }
}

fn poser(api: &mut Value, noeud: &str, entree: &str, valeur: &Value) {
    if let Some(entrees) = api
        .get_mut(noeud)
        .and_then(|n| n.get_mut("inputs"))
        .and_then(Value::as_object_mut)
    {
        entrees.insert(entree.to_owned(), valeur.clone());
    }
}

/// L'échantillonnage réglé par l'utilisateur : `cfg`, `sampler`, `scheduler` sur un KSampler unique (ou
/// `KSamplerSelect` et `BasicScheduler`), `shift` sur un `ModelSamplingAuraFlow` unique.
fn appliquer_noeuds(api: &mut Value, regle: &Map<String, Value>) {
    if let Some(k) = noeud_unique(api, "KSampler") {
        for (cle, entree) in [
            ("cfg", "cfg"),
            ("sampler", "sampler_name"),
            ("scheduler", "scheduler"),
        ] {
            if let Some(v) = regle.get(cle) {
                poser(api, &k, entree, v);
            }
        }
    } else {
        if let (Some(n), Some(v)) = (noeud_unique(api, "KSamplerSelect"), regle.get("sampler")) {
            poser(api, &n, "sampler_name", v);
        }
        if let (Some(n), Some(v)) = (noeud_unique(api, "BasicScheduler"), regle.get("scheduler")) {
            poser(api, &n, "scheduler", v);
        }
    }
    if let (Some(n), Some(v)) = (noeud_unique(api, "ModelSamplingAuraFlow"), regle.get("shift")) {
        poser(api, &n, "shift", v);
    }
}

/// Les entrées du graphe qui valent `[noeud, 0]` (ce qui lit la sortie 0 de ce nœud).
fn lecteurs(api: &Value, noeud: &str) -> Vec<(String, String)> {
    let cible = json!([noeud, 0]);
    let mut trouves = Vec::new();
    for (id, n) in api.as_object().into_iter().flatten() {
        for (entree, valeur) in n.get("inputs").and_then(Value::as_object).into_iter().flatten() {
            if *valeur == cible {
                trouves.push((id.clone(), entree.clone()));
            }
        }
    }
    trouves
}

/// Retire les LoRA livrées dans le graphe : chaque lecteur d'une LoRA lit à sa place le modèle qu'elle recevait.
fn retirer_loras_workflow(api: &mut Value) {
    let loras: Vec<(String, Value)> = api
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, n)| n.get("class_type").and_then(Value::as_str) == Some(NOEUD_LORA))
        .map(|(id, n)| {
            (
                id.clone(),
                n.pointer("/inputs/model").cloned().unwrap_or(Value::Null),
            )
        })
        .collect();
    for (id, modele) in loras {
        for (noeud, entree) in lecteurs(api, &id) {
            poser(api, &noeud, &entree, &modele);
        }
        if let Some(noeuds) = api.as_object_mut() {
            noeuds.remove(&id);
        }
    }
}

/// Branche les LoRA choisies en chaîne après le chargeur du modèle : tout ce qui lisait le chargeur lit la dernière.
fn inserer_loras(
    api: &mut Value,
    chargeur: &str,
    choix: &[Value],
    disponibles: &[String],
    dossier_loras: &str,
) -> Result<(), String> {
    let a_rebrancher = lecteurs(api, chargeur);
    let mut precedent = json!([chargeur, 0]);
    for (i, lora) in choix.iter().enumerate() {
        let fichier = lora.get("fichier").and_then(Value::as_str).unwrap_or_default();
        if !disponibles.iter().any(|d| d == fichier) {
            return Err(format!("LoRA absente de modeles/{dossier_loras} : {fichier}"));
        }
        let force = match lora.get("force") {
            None | Some(Value::Null) => 1.0,
            Some(f) => f
                .as_f64()
                .ok_or_else(|| format!("Force de la LoRA {fichier} : un nombre de 0 à 2"))?,
        };
        if !(0.0..=FORCE_LORA_MAX).contains(&force) {
            return Err(format!("Force de la LoRA {fichier} : un nombre de 0 à 2"));
        }
        let id = format!("aria_lora_{}", i + 1);
        if let Some(noeuds) = api.as_object_mut() {
            noeuds.insert(
                id.clone(),
                json!({
                    "class_type": NOEUD_LORA,
                    "inputs": { "lora_name": fichier, "strength_model": force, "model": precedent }
                }),
            );
        }
        precedent = json!([id, 0]);
    }
    for (noeud, entree) in a_rebrancher {
        poser(api, &noeud, &entree, &precedent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::securite::Ajouts;
    use std::fs;

    fn filtre() -> Filtre {
        Filtre::livre(&Ajouts::default()).unwrap()
    }

    fn deposer(modeles: &Path, dossier: &str, nom: &str) {
        let d = modeles.join(dossier);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(nom), "x").unwrap();
    }

    fn valeurs(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn krea_installe() -> (tempfile::TempDir, Workflow) {
        let temp = tempfile::tempdir().unwrap();
        let wf = obtenir("krea2_turbo").unwrap();
        for f in wf.fiche["fichiers"].as_array().unwrap() {
            deposer(
                temp.path(),
                f["dossier"].as_str().unwrap(),
                f["nom"].as_str().unwrap(),
            );
        }
        (temp, wf)
    }

    #[test]
    fn le_catalogue_livre_n_ecarte_rien() {
        let (bons, erreurs) = catalogue();
        assert_eq!(erreurs, Vec::<String>::new());
        assert_eq!(bons.len(), LIVRES.len());
        assert!(obtenir("inconnu").unwrap_err().contains("Workflow inconnu"));
    }

    #[test]
    fn les_steps_clairs_comme_en_v2() {
        assert_eq!(obtenir("krea2_turbo").unwrap().steps_clairs(), 8);
        assert_eq!(obtenir("qwen_image_2_1").unwrap().steps_clairs(), 0);
        assert_eq!(obtenir("minimax_h3_t2v").unwrap().steps_clairs(), 0);
    }

    #[test]
    fn rien_ne_manque_quand_tout_est_pose() {
        let (temp, wf) = krea_installe();
        assert_eq!(wf.manquants(temp.path()), Vec::<Value>::new());
        let vide = tempfile::tempdir().unwrap();
        assert_eq!(
            wf.manquants(vide.path()).len(),
            wf.fiche["fichiers"].as_array().unwrap().len()
        );
    }

    #[test]
    fn preparer_remplit_le_graphe_sans_toucher_a_l_original() {
        let (temp, wf) = krea_installe();
        let api = wf
            .preparer(
                &valeurs(json!({ "prompt": "a red fox", "seed": 42, "prefixe": "aria_plateau", "ratio": "16:9 (Widescreen)" })),
                temp.path(),
                None,
            &filtre(),
            )
            .unwrap();
        let (pn, pe) = wf.entree("prompt").unwrap();
        assert_eq!(api[&pn]["inputs"][&pe], "a red fox");
        let (sn, _) = wf.entree("seed").unwrap();
        assert_eq!(api[&sn]["inputs"]["seed"], 42);
        assert_eq!(api[&sn]["inputs"]["steps"], 8, "les steps clairs de Krea");
        let (rn, re) = wf.entree("ratio").unwrap();
        assert_eq!(api[&rn]["inputs"][&re], "16:9 (Widescreen)");
        let (fx, fe) = wf.entree("prefixe").unwrap();
        assert_eq!(api[&fx]["inputs"][&fe], "aria_plateau");
        let origine = wf.modele_origine().unwrap();
        let (mn, me) = wf.entree("modele").unwrap();
        assert_eq!(api[&mn]["inputs"][&me], origine.as_str());
        assert_eq!(
            wf.api,
            obtenir("krea2_turbo").unwrap().api,
            "le graphe livré ne change pas"
        );
    }

    #[test]
    fn la_valeur_de_l_appel_gagne_puis_le_reglage_puis_le_defaut() {
        let (temp, wf) = krea_installe();
        let reglages = valeurs(json!({ "ratio": "3:2 (Photo)", "cfg": 1.5, "sampler": "dpmpp_2m" }));
        let api = wf
            .preparer(
                &valeurs(json!({ "prompt": "x", "steps": 5 })),
                temp.path(),
                Some(&reglages),
                &filtre(),
            )
            .unwrap();
        let (rn, re) = wf.entree("ratio").unwrap();
        assert_eq!(api[&rn]["inputs"][&re], "3:2 (Photo)");
        let (sn, _) = wf.entree("steps").unwrap();
        assert_eq!(api[&sn]["inputs"]["steps"], 5);
        assert_eq!(api[&sn]["inputs"]["cfg"], 1.5);
        assert_eq!(api[&sn]["inputs"]["sampler_name"], "dpmpp_2m");
        let api = wf
            .preparer(
                &valeurs(json!({ "prompt": "x", "ratio": "1:1 (Square)" })),
                temp.path(),
                Some(&reglages),
                &filtre(),
            )
            .unwrap();
        assert_eq!(api[&rn]["inputs"][&re], "1:1 (Square)");
    }

    #[test]
    fn un_modele_absent_est_refuse() {
        let (temp, wf) = krea_installe();
        let erreur = wf
            .preparer(
                &valeurs(json!({ "prompt": "x", "modele": "ailleurs.safetensors" })),
                temp.path(),
                None,
                &filtre(),
            )
            .unwrap_err();
        assert!(
            erreur.starts_with("Modèle absent de image/Krea2/modeles"),
            "{erreur}"
        );
    }

    #[test]
    fn un_checkpoint_a_valider_ne_tourne_pas() {
        let temp = tempfile::tempdir().unwrap();
        let wf = obtenir("qwen_image_2_1").unwrap();
        for f in wf.fiche["fichiers"].as_array().unwrap() {
            deposer(
                temp.path(),
                f["dossier"].as_str().unwrap(),
                f["nom"].as_str().unwrap(),
            );
        }
        let origine = wf.modele_origine().unwrap();
        let erreur = wf
            .preparer(&valeurs(json!({ "prompt": "x" })), temp.path(), None, &filtre())
            .unwrap_err();
        assert!(
            erreur.starts_with(&format!("Checkpoint à valider : {origine}")),
            "{erreur}"
        );
        let mut variantes = Map::new();
        variantes.insert(origine.clone(), json!({ "steps": 30 }));
        let mut reglages = Map::new();
        reglages.insert("variantes".to_owned(), Value::Object(variantes));
        let api = wf
            .preparer(
                &valeurs(json!({ "prompt": "x" })),
                temp.path(),
                Some(&reglages),
                &filtre(),
            )
            .unwrap();
        let (sn, se) = wf.entree("steps").unwrap();
        assert_eq!(api[&sn]["inputs"][&se], 30);
        assert_eq!(wf.variantes(temp.path(), Some(&reglages)), vec![origine]);
    }

    #[test]
    fn une_entree_inconnue_ou_une_option_pas_encore_portee_est_refusee() {
        let (temp, wf) = krea_installe();
        assert!(
            wf.preparer(&valeurs(json!({ "duree": 5 })), temp.path(), None, &filtre())
                .unwrap_err()
                .contains("n'a pas d'entrée « duree »")
        );
        // Krea 2 n'a pas d'encodeur d'édition : pas de référence.
        assert!(
            wf.preparer(
                &valeurs(json!({ "prompt": "x", "references": ["a.png"] })),
                temp.path(),
                None,
                &filtre()
            )
            .unwrap_err()
            .contains("n'accepte pas 2 images de référence")
        );
    }

    #[test]
    fn les_loras_se_branchent_en_chaine_apres_le_chargeur() {
        let (temp, wf) = krea_installe();
        deposer(temp.path(), "image/Krea2/loras", "style.safetensors");
        deposer(temp.path(), "image/Krea2/loras", "detail.safetensors");
        let (chargeur, _) = wf.entree("modele").unwrap();
        let avant = lecteurs(&wf.api, &chargeur);
        assert!(!avant.is_empty());
        let api = wf
            .preparer(
                &valeurs(json!({ "prompt": "x", "loras": [
                    { "fichier": "style.safetensors", "force": 0.8 },
                    { "fichier": "detail.safetensors" }
                ] })),
                temp.path(),
                None,
                &filtre(),
            )
            .unwrap();
        assert_eq!(api["aria_lora_1"]["inputs"]["model"], json!([chargeur, 0]));
        assert_eq!(api["aria_lora_1"]["inputs"]["strength_model"], 0.8);
        assert_eq!(api["aria_lora_2"]["inputs"]["model"], json!(["aria_lora_1", 0]));
        assert_eq!(api["aria_lora_2"]["inputs"]["strength_model"], 1.0);
        for (noeud, entree) in avant {
            assert_eq!(
                api[&noeud]["inputs"][&entree],
                json!(["aria_lora_2", 0]),
                "{noeud}.{entree}"
            );
        }
    }

    #[test]
    fn une_lora_absente_ou_trop_forte_est_refusee() {
        let (temp, wf) = krea_installe();
        deposer(temp.path(), "image/Krea2/loras", "style.safetensors");
        let absente = json!({ "prompt": "x", "loras": [{ "fichier": "../ailleurs.safetensors" }] });
        assert!(
            wf.preparer(&valeurs(absente), temp.path(), None, &filtre())
                .unwrap_err()
                .starts_with("LoRA absente")
        );
        let forte = json!({ "prompt": "x", "loras": [{ "fichier": "style.safetensors", "force": 2.5 }] });
        assert!(
            wf.preparer(&valeurs(forte), temp.path(), None, &filtre())
                .unwrap_err()
                .contains("de 0 à 2")
        );
    }

    #[test]
    fn un_prompt_refuse_par_le_filtre_ne_va_jamais_a_comfyui() {
        let (temp, wf) = krea_installe();
        let erreur = wf
            .preparer(
                &valeurs(json!({ "prompt": "a 12 year old girl, nude" })),
                temp.path(),
                None,
                &filtre(),
            )
            .unwrap_err();
        assert!(
            erreur.starts_with("ERREUR AS-18 · Génération bloquée : "),
            "{erreur}"
        );
        assert!(
            wf.preparer(
                &valeurs(json!({ "prompt": "a red fox" })),
                temp.path(),
                None,
                &filtre()
            )
            .is_ok()
        );
    }

    #[test]
    fn retirer_la_lora_du_workflow_rebranche_son_modele() {
        let mut api = json!({
            "1": { "class_type": "UNETLoader", "inputs": { "unet_name": "m" } },
            "2": { "class_type": NOEUD_LORA, "inputs": { "model": ["1", 0], "lora_name": "l" } },
            "3": { "class_type": "BasicGuider", "inputs": { "model": ["2", 0] } }
        });
        retirer_loras_workflow(&mut api);
        assert!(api.get("2").is_none());
        assert_eq!(api["3"]["inputs"]["model"], json!(["1", 0]));
    }

    #[test]
    fn steps_suggeres_par_la_fiche() {
        assert_eq!(obtenir("qwen_image_2_1").unwrap().steps_suggeres(), vec![25, 40]);
        assert_eq!(obtenir("krea2_turbo").unwrap().steps_suggeres(), vec![8]);
    }

    #[test]
    fn valider_un_checkpoint_le_met_en_service_dans_sa_famille() {
        let temp = tempfile::tempdir().unwrap();
        let modeles = temp.path().join("modeles");
        let dossier = Dossier::a(temp.path());
        let wf = obtenir("qwen_image_2_1").unwrap();
        let origine = wf.modele_origine().unwrap();
        deposer(&modeles, &wf.dossier_modeles(), &origine);
        assert!(wf.variantes(&modeles, None).is_empty());
        // Le reste du fichier est gardé.
        fs::write(
            dossier.reglages(),
            r#"{"securite": {"ajouts": []}, "workflows": {"qwen_image_2_1": {"cfg": 2.5}}}"#,
        )
        .unwrap();

        assert_eq!(
            valider_checkpoint(&dossier, &modeles, "qwen_image_2_1", &origine, 0).unwrap_err(),
            "Steps : un entier entre 1 et 150"
        );
        assert!(valider_checkpoint(&dossier, &modeles, "qwen_image_2_1", &origine, 151).is_err());
        assert!(
            valider_checkpoint(&dossier, &modeles, "qwen_image_2_1", "absent.safetensors", 25)
                .unwrap_err()
                .starts_with("Checkpoint absent, ou sans workflow qui règle ses steps : ")
        );
        valider_checkpoint(&dossier, &modeles, "qwen_image_2_1", &origine, 25).unwrap();

        let section = reglages::section(&dossier, "workflows").unwrap();
        let famille: Vec<Workflow> = catalogue()
            .0
            .into_iter()
            .filter(|w| w.famille() == wf.famille() && w.a_l_entree("steps"))
            .collect();
        assert!(!famille.is_empty());
        for w in &famille {
            let reglage = section[w.id.as_str()].as_object().unwrap();
            assert_eq!(
                reglage["variantes"][origine.as_str()]["steps"],
                json!(25),
                "{}",
                w.id
            );
            assert_eq!(w.variantes(&modeles, Some(reglage)), vec![origine.clone()]);
        }
        assert_eq!(section["qwen_image_2_1"]["cfg"], json!(2.5));
        assert!(reglages::section(&dossier, "securite").is_some());
    }

    fn installe(id: &str) -> (tempfile::TempDir, Workflow) {
        let temp = tempfile::tempdir().unwrap();
        let wf = obtenir(id).unwrap();
        for f in wf.fiche["fichiers"].as_array().unwrap() {
            deposer(
                temp.path(),
                f["dossier"].as_str().unwrap(),
                f["nom"].as_str().unwrap(),
            );
        }
        (temp, wf)
    }

    #[test]
    fn ce_que_chaque_workflow_accepte() {
        let krea = obtenir("krea2_turbo").unwrap();
        let edit = obtenir("qwen_image_2_1_edit").unwrap();
        assert_eq!(krea.accepte_references(), 0);
        // L'encodeur de Qwen-Image 2.1 a 16 entrées ; Aria s'arrête à 10.
        assert_eq!(edit.accepte_references(), REFERENCES_MAX);
        // Qwen-Image 2.1 (texte → image) a l'encodeur mais pas d'entrée « image ».
        assert_eq!(obtenir("qwen_image_2_1").unwrap().accepte_references(), 0);
        assert!(!krea.reserve_retouche());
        assert!(krea.accepte_variation());
    }

    #[test]
    fn s_inspirer_branche_les_references_sur_l_encodeur() {
        let (temp, wf) = installe("qwen_image_2_1_edit");
        let mut variante = Map::new();
        variante.insert(wf.modele_origine().unwrap(), json!({"steps": 25}));
        let mut reglages = Map::new();
        reglages.insert("variantes".to_owned(), Value::Object(variante));
        let max = wf.accepte_references();
        let api = wf
            .preparer(
                &valeurs(json!({"prompt": "x", "image": "a.png", "references": ["b.png"], "format_sortie": "16:9 (Widescreen)"})),
                temp.path(),
                Some(&reglages),
                &filtre(),
            )
            .unwrap();
        assert_eq!(api["aria_ref_2"]["inputs"]["image"], json!("b.png"));
        let branche = api
            .as_object()
            .unwrap()
            .values()
            .filter(|n| encodeur(n["class_type"].as_str().unwrap_or_default()).is_some())
            .all(|n| {
                n["inputs"]
                    .as_object()
                    .unwrap()
                    .values()
                    .any(|v| *v == json!(["aria_ref_2", 0]))
            });
        assert!(branche);
        assert_eq!(
            api["aria_format"]["inputs"]["aspect_ratio"],
            json!("16:9 (Widescreen)")
        );
        let ks = noeud_unique(&api, "KSampler").unwrap();
        assert_eq!(
            api[ks.as_str()]["inputs"]["latent_image"],
            json!(["aria_latent", 0])
        );
        let trop: Vec<String> = (0..max).map(|i| format!("r{i}.png")).collect();
        assert!(
            wf.preparer(
                &valeurs(json!({"prompt": "x", "image": "a.png", "references": trop})),
                temp.path(),
                Some(&reglages),
                &filtre(),
            )
            .unwrap_err()
            .contains("n'accepte pas")
        );
    }

    #[test]
    fn redessiner_part_de_l_image() {
        let (temp, wf) = krea_installe();
        let api = wf
            .preparer(
                &valeurs(
                    json!({"prompt": "x", "variation": {"image": "a.png", "denoise": 0.6, "format": null}}),
                ),
                temp.path(),
                None,
                &filtre(),
            )
            .unwrap();
        let ks = noeud_unique(&api, "KSampler").unwrap();
        assert_eq!(
            api[ks.as_str()]["inputs"]["latent_image"],
            json!(["aria_var_latent", 0])
        );
        assert_eq!(api[ks.as_str()]["inputs"]["denoise"], json!(0.6));
        assert_eq!(
            api["aria_var_echelle"]["class_type"],
            json!("ImageScaleToTotalPixels")
        );
        let api = wf
            .preparer(
                &valeurs(json!({"prompt": "x", "variation": {"image": "a.png", "denoise": 0.6, "format": "1:1 (Square)"}})),
                temp.path(),
                None,
                &filtre(),
            )
            .unwrap();
        assert_eq!(api["aria_var_echelle"]["class_type"], json!("ImageScale"));
        assert!(
            wf.preparer(
                &valeurs(json!({"prompt": "x", "variation": {"image": "a.png", "denoise": 1.5}})),
                temp.path(),
                None,
                &filtre(),
            )
            .unwrap_err()
            .starts_with("Liberté entre 0,05 et 1")
        );
    }

    #[test]
    fn noeuds_ajoutes_par_le_code_tous_declares() {
        // Chaque « class_type » écrit par le code (hors tests) doit figurer dans NOEUDS_AJOUTES : sinon,
        // « J'ai déjà ComfyUI » ne vérifierait pas ce nœud.
        let motif = regex::Regex::new(r#""class_type":\s*"([^"]+)""#).unwrap();
        for source in [
            include_str!("workflows.rs"),
            include_str!("voix.rs"),
            include_str!("sequences.rs"),
            include_str!("h3.rs"),
        ] {
            let code = source.split("#[cfg(test)]").next().unwrap();
            for prise in motif.captures_iter(code) {
                assert!(
                    NOEUDS_AJOUTES.contains(&&prise[1]),
                    "nœud ajouté non déclaré : {}",
                    &prise[1]
                );
            }
        }
        for (_, _, _, latent) in ENCODEURS_REFERENCES {
            assert!(NOEUDS_AJOUTES.contains(&latent), "latent non déclaré : {latent}");
        }
        let utilises = noeuds_utilises();
        assert!(utilises.contains("KSampler") && utilises.contains(NOEUD_LORA));
    }
}
