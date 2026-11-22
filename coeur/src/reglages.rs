//! Les réglages de l'appli, enregistrés dans `reglages.json`. Jamais de secret ici : ils vont dans `secrets\`.
//! Chaque appli ajoute ses propres réglages à [`Reglages`].

use std::fs;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{Dossier, Erreur, donnees};

/// Vitesse du fond animé. Dans le fichier : « lente », « normale », « rapide ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Vitesse {
    Lente,
    #[default]
    Normale,
    Rapide,
}

/// Une valeur inconnue, ou qui n'est pas du texte, donne « normale » : un fichier modifié à la main ne casse rien.
impl<'de> Deserialize<'de> for Vitesse {
    fn deserialize<D: Deserializer<'de>>(entree: D) -> Result<Self, D::Error> {
        let valeur = serde_json::Value::deserialize(entree)?;
        Ok(
            match valeur.as_str().map(|s| s.trim().to_lowercase()).as_deref() {
                Some("lente") => Self::Lente,
                Some("rapide") => Self::Rapide,
                _ => Self::Normale,
            },
        )
    }
}

/// Le fond animé, réglages obligatoires de toute appli Aria : `fond.actif`, `fond.vitesse`, `fond.minimum` et
/// `fond.maximum` (nombre d'éléments tirés au hasard à chaque passage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesFond {
    pub actif: bool,
    pub vitesse: Vitesse,
    pub minimum: i64,
    pub maximum: i64,
}

impl ReglagesFond {
    /// Le moins d'éléments qu'un passage puisse tirer.
    pub const ELEMENTS_MIN: i64 = 1;
    /// Le plus d'éléments qu'un passage puisse tirer.
    pub const ELEMENTS_MAX: i64 = 12;

    /// Bornes ramenées entre 1 et 12, et remises dans l'ordre si le fichier les a inversées.
    #[must_use]
    pub fn assaini(self) -> Self {
        let bas = self.minimum.clamp(Self::ELEMENTS_MIN, Self::ELEMENTS_MAX);
        let haut = self.maximum.clamp(Self::ELEMENTS_MIN, Self::ELEMENTS_MAX);
        Self {
            minimum: bas.min(haut),
            maximum: bas.max(haut),
            ..self
        }
    }
}

impl Default for ReglagesFond {
    fn default() -> Self {
        Self {
            actif: true,
            vitesse: Vitesse::Normale,
            minimum: 2,
            maximum: 5,
        }
    }
}

/// Les réglages de l'appli.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    /// Fond animé ; absent du fichier, c'est celui par défaut.
    pub fond: ReglagesFond,

    /// Proposer de s'installer quand l'exe est lancé hors de `programme\` (zip décompressé). Décoché par « Ne plus
    /// me proposer » ; se recoche dans les réglages.
    #[serde(rename = "proposerInstallation")]
    pub proposer_installation: bool,

    /// Chercher une nouvelle version au lancement : une requête vers l'API GitHub. Décoché : plus aucune connexion
    /// pour ça.
    #[serde(rename = "chercherMisesAJour")]
    pub chercher_mises_a_jour: bool,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            fond: ReglagesFond::default(),
            proposer_installation: true,
            chercher_mises_a_jour: true,
        }
    }
}

impl Reglages {
    /// Valeurs ramenées dans leurs bornes.
    #[must_use]
    pub fn assaini(self) -> Self {
        Self {
            fond: self.fond.assaini(),
            ..self
        }
    }

    /// Réglages lus dans un texte JSON ; ceux par défaut si le texte est illisible.
    pub fn depuis_json(texte: &str) -> Self {
        serde_json::from_str::<Self>(texte)
            .map(Self::assaini)
            .unwrap_or_default()
    }

    /// Le texte JSON du fichier, indenté pour rester lisible à la main.
    ///
    /// # Errors
    /// Seulement si la sérialisation échoue, ce qui n'arrive pas avec ces types.
    pub fn vers_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.clone().assaini())
    }

    /// Réglages enregistrés ; ceux par défaut si le fichier est absent, illisible ou incohérent.
    pub fn charger(dossier: &Dossier) -> Self {
        fs::read_to_string(dossier.reglages())
            .map(|texte| Self::depuis_json(&texte))
            .unwrap_or_default()
    }

    /// Enregistre les réglages, assainis, sans jamais laisser un fichier à moitié écrit.
    ///
    /// # Errors
    /// [`Erreur::Fichier`] si l'écriture échoue ; l'ancien fichier reste alors intact.
    pub fn enregistrer(&self, dossier: &Dossier) -> Result<(), Erreur> {
        let texte = self
            .vers_json()
            .map_err(|e| Erreur::fichier("préparation de", dossier.reglages(), e.into()))?;
        donnees::ecrire_entier(&dossier.reglages(), texte.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_fichier_absent_donne_les_reglages_par_defaut() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(Reglages::charger(&Dossier::a(temp.path())), Reglages::default());
    }

    #[test]
    fn un_fichier_illisible_donne_les_reglages_par_defaut() {
        assert_eq!(Reglages::depuis_json("{ pas du json"), Reglages::default());
        assert_eq!(Reglages::depuis_json("[]"), Reglages::default());
    }

    #[test]
    fn enregistrer_puis_charger_rend_les_memes_reglages() {
        let temp = tempfile::tempdir().unwrap();
        let dossier = Dossier::a(temp.path());
        let reglages = Reglages {
            fond: ReglagesFond {
                actif: false,
                vitesse: Vitesse::Rapide,
                minimum: 3,
                maximum: 9,
            },
            proposer_installation: false,
            chercher_mises_a_jour: false,
        };
        reglages.enregistrer(&dossier).unwrap();
        assert_eq!(Reglages::charger(&dossier), reglages);
    }

    #[test]
    fn le_fichier_garde_les_noms_francais() {
        let texte = Reglages::default().vers_json().unwrap();
        for cle in [
            "\"fond\"",
            "\"actif\"",
            "\"vitesse\": \"normale\"",
            "\"proposerInstallation\"",
            "\"chercherMisesAJour\"",
        ] {
            assert!(texte.contains(cle), "{cle} absent de {texte}");
        }
    }

    #[test]
    fn les_bornes_du_fond_sont_ramenees_et_remises_dans_l_ordre() {
        let r = Reglages::depuis_json(r#"{ "fond": { "minimum": 40, "maximum": -3 } }"#);
        assert_eq!((r.fond.minimum, r.fond.maximum), (1, 12));
        let r = Reglages::depuis_json(r#"{ "fond": { "minimum": 8, "maximum": 4 } }"#);
        assert_eq!((r.fond.minimum, r.fond.maximum), (4, 8));
    }

    #[test]
    fn une_vitesse_inconnue_donne_normale() {
        for valeur in ["\"turbo\"", "12", "null", "\" RAPIDE \""] {
            let texte = String::from(r#"{ "fond": { "vitesse": "#) + valeur + " } }";
            let r = Reglages::depuis_json(&texte);
            let attendue = if valeur.contains("RAPIDE") {
                Vitesse::Rapide
            } else {
                Vitesse::Normale
            };
            assert_eq!(r.fond.vitesse, attendue, "{valeur}");
        }
    }

    #[test]
    fn un_champ_manquant_prend_sa_valeur_par_defaut() {
        let r = Reglages::depuis_json(r#"{ "chercherMisesAJour": false }"#);
        assert!(!r.chercher_mises_a_jour);
        assert!(r.proposer_installation);
        assert_eq!(r.fond, ReglagesFond::default());
    }
}
