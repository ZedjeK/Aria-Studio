//! Le catalogue des moteurs, `catalogue\moteurs.yaml` : les versions de ComfyUI et de llama.cpp testées ensemble,
//! avec l'adresse, la taille et l'empreinte SHA-256 de chaque archive. Il est embarqué dans l'exe et contrôlé à la
//! lecture : une empreinte mal formée, un nom qui sortirait de son dossier ou une archive hors de GitHub le font
//! refuser en entier.
//!
//! Même fichier et même format que la v2 (`app/moteurs.py`).

use std::collections::BTreeMap;

use serde::Deserialize;

/// Le texte du catalogue, tel qu'il est dans le dépôt.
pub const TEXTE: &str = include_str!("../../catalogue/moteurs.yaml");

/// Le seul format de catalogue que cette version sait lire.
pub const FORMAT: u32 = 1;

/// Le catalogue des moteurs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Catalogue {
    /// Version du format du fichier.
    pub format: u32,
    /// Branche minimale du pilote NVIDIA (CUDA 13.x : 580).
    pub pilote_min: u32,
    /// Les moteurs, par identifiant (`comfyui`, `llama`, `llama-prism`).
    pub moteurs: BTreeMap<String, Moteur>,
}

/// Un moteur et les archives qui l'installent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Moteur {
    pub nom: String,
    /// Ce qu'il fait, en une phrase.
    pub role: String,
    pub version: String,
    /// Son sous-dossier dans le dossier des moteurs.
    pub dossier: String,
    /// Le fichier dont la présence montre qu'il est installé, relatif à son dossier.
    pub preuve: String,
    /// Le port qu'il prend, désigné par son nom (`comfyui`, `llama`).
    pub port: String,
    /// Un moteur que l'assistant n'installe pas d'office.
    #[serde(default)]
    pub facultatif: bool,
    pub archives: Vec<Archive>,
}

/// Une archive d'une release GitHub officielle.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Archive {
    /// `propriétaire/dépôt` sur GitHub.
    pub depot: String,
    pub tag: String,
    /// Le nom de l'archive dans la release.
    pub fichier: String,
    /// Sa taille en octets.
    pub taille: u64,
    /// Son empreinte SHA-256, en hexadécimal minuscule.
    pub sha256: String,
    /// Le dossier de l'archive dont on prend le contenu ; absent, l'archive est à plat.
    #[serde(default)]
    pub racine: Option<String>,
}

impl Archive {
    /// L'adresse de téléchargement, toujours sur github.com en HTTPS.
    pub fn adresse(&self) -> String {
        format!(
            "https://github.com/{}/releases/download/{}/{}",
            self.depot, self.tag, self.fichier
        )
    }
}

impl Catalogue {
    /// Le catalogue embarqué dans l'exe.
    ///
    /// # Errors
    /// Un message si le fichier ne se lit pas ou ne passe pas les contrôles : il ne doit pas arriver avec le
    /// catalogue du dépôt, que les tests vérifient.
    pub fn embarque() -> Result<Self, String> {
        Self::depuis_yaml(TEXTE)
    }

    /// Un catalogue lu dans un texte YAML, puis contrôlé.
    ///
    /// # Errors
    /// Un message qui dit ce qui ne va pas, et où.
    pub fn depuis_yaml(texte: &str) -> Result<Self, String> {
        let catalogue: Self =
            serde_saphyr::from_str(texte).map_err(|e| format!("catalogue des moteurs illisible : {e}"))?;
        catalogue.verifier()?;
        Ok(catalogue)
    }

    /// Un moteur par son identifiant.
    pub fn moteur(&self, id: &str) -> Option<&Moteur> {
        self.moteurs.get(id)
    }

    fn verifier(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!(
                "catalogue des moteurs au format {} : cette version ne lit que le format {FORMAT}",
                self.format
            ));
        }
        if self.moteurs.is_empty() {
            return Err("catalogue des moteurs vide".to_owned());
        }
        for (id, moteur) in &self.moteurs {
            let erreur = |quoi: &str| format!("catalogue des moteurs, {id} : {quoi}");
            if !nom_simple(id) {
                return Err(erreur("identifiant refusé"));
            }
            if !nom_simple(&moteur.dossier) {
                return Err(erreur("dossier refusé"));
            }
            if !chemin_relatif(&moteur.preuve) {
                return Err(erreur("preuve refusée"));
            }
            if moteur.archives.is_empty() {
                return Err(erreur("aucune archive"));
            }
            for archive in &moteur.archives {
                archive
                    .verifier()
                    .map_err(|quoi| erreur(&format!("{} : {quoi}", archive.fichier)))?;
            }
        }
        Ok(())
    }
}

impl Archive {
    fn verifier(&self) -> Result<(), &'static str> {
        let mut morceaux = self.depot.split('/');
        let depot_valide = matches!(
            (morceaux.next(), morceaux.next(), morceaux.next()),
            (Some(proprietaire), Some(nom), None) if nom_simple(proprietaire) && nom_simple(nom)
        );
        if !depot_valide {
            return Err("dépôt refusé (propriétaire/nom attendu)");
        }
        if !nom_simple(&self.tag) {
            return Err("tag refusé");
        }
        if !nom_simple(&self.fichier) {
            return Err("nom de fichier refusé");
        }
        if self.taille == 0 {
            return Err("taille nulle");
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err("empreinte SHA-256 mal formée (64 caractères hexadécimaux minuscules)");
        }
        if self.racine.as_deref().is_some_and(|r| !nom_simple(r)) {
            return Err("racine refusée");
        }
        Ok(())
    }
}

/// Un nom qui reste dans son dossier : lettres, chiffres, `.`, `-`, `_`, sans être `.` ni `..`.
fn nom_simple(nom: &str) -> bool {
    !nom.is_empty()
        && nom != "."
        && nom != ".."
        && nom
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Un chemin relatif fait de noms simples séparés par `/` (`ComfyUI/main.py`).
fn chemin_relatif(chemin: &str) -> bool {
    chemin.split('/').all(nom_simple)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_catalogue_du_depot_se_lit_et_passe_les_controles() {
        let catalogue = Catalogue::embarque().unwrap();
        assert_eq!(catalogue.format, FORMAT);
        assert_eq!(catalogue.pilote_min, 580);
        for id in ["comfyui", "llama", "llama-prism"] {
            assert!(catalogue.moteur(id).is_some(), "{id} manque");
        }
        let comfyui = catalogue.moteur("comfyui").unwrap();
        assert_eq!(comfyui.preuve, "ComfyUI/main.py");
        assert_eq!(
            comfyui.archives[0].racine.as_deref(),
            Some("ComfyUI_windows_portable")
        );
        assert!(!comfyui.facultatif);
        assert!(catalogue.moteur("llama-prism").unwrap().facultatif);
        assert_eq!(catalogue.moteur("llama").unwrap().archives.len(), 2);
    }

    #[test]
    fn chaque_archive_vient_de_github_en_https() {
        let catalogue = Catalogue::embarque().unwrap();
        for moteur in catalogue.moteurs.values() {
            for archive in &moteur.archives {
                assert!(
                    archive.adresse().starts_with("https://github.com/"),
                    "{}",
                    archive.adresse()
                );
            }
        }
        let llama = &catalogue.moteur("llama").unwrap().archives[0];
        assert_eq!(
            llama.adresse(),
            "https://github.com/ggml-org/llama.cpp/releases/download/b11734/llama-b11734-bin-win-cuda-13.4-x64.zip"
        );
    }

    const PETIT: &str = "format: 1
pilote_min: 580
moteurs:
    essai:
        nom: Essai
        role: Un moteur d'essai
        version: v1
        dossier: essai
        preuve: bin/essai.exe
        port: essai
        archives:
            - depot: proprio/depot
              tag: v1
              fichier: essai.zip
              taille: 10
              sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
";

    fn avec(ancien: &str, nouveau: &str) -> Result<Catalogue, String> {
        assert!(PETIT.contains(ancien), "{ancien}");
        Catalogue::depuis_yaml(&PETIT.replacen(ancien, nouveau, 1))
    }

    #[test]
    fn un_catalogue_minimal_se_lit() {
        let catalogue = Catalogue::depuis_yaml(PETIT).unwrap();
        let essai = catalogue.moteur("essai").unwrap();
        assert_eq!(essai.archives[0].racine, None);
        assert!(!essai.facultatif);
    }

    #[test]
    fn un_autre_format_est_refuse() {
        assert!(avec("format: 1", "format: 2").unwrap_err().contains("format 2"));
    }

    #[test]
    fn une_empreinte_mal_formee_est_refusee() {
        let valide = "a".repeat(64);
        assert!(avec(&valide, &"a".repeat(63)).is_err());
        assert!(avec(&valide, &"A".repeat(64)).is_err());
        assert!(avec(&valide, &"g".repeat(64)).is_err());
    }

    #[test]
    fn un_nom_qui_sortirait_de_son_dossier_est_refuse() {
        assert!(avec("dossier: essai", "dossier: ..").is_err());
        assert!(avec("dossier: essai", "dossier: a/b").is_err());
        assert!(avec("preuve: bin/essai.exe", "preuve: ../essai.exe").is_err());
        assert!(avec("preuve: bin/essai.exe", "preuve: C:/essai.exe").is_err());
        assert!(avec("fichier: essai.zip", "fichier: ../essai.zip").is_err());
        assert!(avec("tag: v1", "tag: v1/../x").is_err());
    }

    #[test]
    fn un_depot_hors_de_la_forme_proprietaire_nom_est_refuse() {
        assert!(avec("depot: proprio/depot", "depot: proprio").is_err());
        assert!(avec("depot: proprio/depot", "depot: a/b/c").is_err());
        assert!(
            avec("depot: proprio/depot", "depot: evil.com/x").is_ok(),
            "un point est permis dans un nom"
        );
    }

    #[test]
    fn un_texte_qui_n_est_pas_un_catalogue_est_refuse() {
        assert!(Catalogue::depuis_yaml("pas: [un catalogue").is_err());
        assert!(Catalogue::depuis_yaml("format: 1\npilote_min: 580\nmoteurs: {}\n").is_err());
    }
}
