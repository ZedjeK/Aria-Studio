//! Les notifications de fin de rendu : ce que dit Windows quand une image, une retouche, un clip, une séquence ou un
//! casting se termine. Ici, seulement les mots ; la notification Windows elle-même est dans la crate `aria-appli`.
//! Le réglage (`notifications.active` et `notifications.son`, comme en v2) est dans [`crate::reglages`].

use crate::identite;
use crate::plateau::{Commande, EtatCommande};

/// Longueur maximale, en caractères, de la ligne de texte : Windows coupe de toute façon ce qui dépasse.
const LONGUEUR_MAX: usize = 120;

/// Ce que dit une notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annonce {
    pub titre: String,
    pub texte: String,
}

/// Le nom du résultat d'une commande de ce genre, et comment on dit qu'il est fini ; rien pour un genre qu'on
/// n'attend pas (l'essai d'une voix dure quelques secondes).
fn quoi(genre: &str) -> Option<(&'static str, &'static str)> {
    match genre {
        "creation" => Some(("image", "image prête")),
        "retouche" | "agrandissement" => Some(("retouche", "retouche terminée")),
        "clip" => Some(("clip", "clip terminé")),
        "sequence" => Some(("séquence", "séquence terminée")),
        "casting" => Some(("casting", "casting terminé")),
        _ => None,
    }
}

/// Le texte sur une ligne, coupé à [`LONGUEUR_MAX`] caractères.
fn court(texte: &str) -> String {
    let propre = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    if propre.chars().count() <= LONGUEUR_MAX {
        return propre;
    }
    let coupe: String = propre.chars().take(LONGUEUR_MAX).collect();
    format!("{}…", coupe.trim_end())
}

/// Ce qu'il faut annoncer quand la commande est finie : le résultat prêt, ou l'échec avec son message. Rien pour une
/// annulation (l'utilisateur vient de la demander), pour une commande pas finie, ni pour un genre qu'on n'attend pas.
pub fn annonce(commande: &Commande) -> Option<Annonce> {
    let (nom, fait) = quoi(&commande.genre)?;
    let (titre, texte, secours) = match commande.etat {
        EtatCommande::Termine => (
            format!("{} : {fait}", identite::NOM),
            commande.idee.as_str(),
            "Le rendu est terminé.",
        ),
        EtatCommande::Erreur => (
            format!("{} : {nom} en échec", identite::NOM),
            commande.erreur.as_deref().unwrap_or_default(),
            "Le rendu a échoué.",
        ),
        EtatCommande::Attente | EtatCommande::EnCours | EtatCommande::Annule => return None,
    };
    let texte = court(texte);
    Some(Annonce {
        titre,
        texte: if texte.is_empty() {
            secours.to_owned()
        } else {
            texte
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commande(genre: &str, etat: EtatCommande) -> Commande {
        let mut c = Commande::nouvelle(
            "c1".to_owned(),
            genre,
            "un renard dans la neige".to_owned(),
            "krea".to_owned(),
            "carré",
        );
        c.etat = etat;
        c
    }

    #[test]
    fn un_rendu_fini_est_annonce_avec_l_idee() {
        let a = annonce(&commande("creation", EtatCommande::Termine)).unwrap();
        assert_eq!(a.titre, "Aria Studio : image prête");
        assert_eq!(a.texte, "un renard dans la neige");
        for (genre, titre) in [
            ("retouche", "Aria Studio : retouche terminée"),
            ("agrandissement", "Aria Studio : retouche terminée"),
            ("clip", "Aria Studio : clip terminé"),
            ("sequence", "Aria Studio : séquence terminée"),
            ("casting", "Aria Studio : casting terminé"),
        ] {
            let a = annonce(&commande(genre, EtatCommande::Termine)).unwrap();
            assert_eq!(a.titre, titre, "{genre}");
        }
    }

    #[test]
    fn un_echec_dit_son_message() {
        let mut c = commande("clip", EtatCommande::Erreur);
        c.erreur = Some("ComfyUI n'a renvoyé aucune image".to_owned());
        let a = annonce(&c).unwrap();
        assert_eq!(a.titre, "Aria Studio : clip en échec");
        assert_eq!(a.texte, "ComfyUI n'a renvoyé aucune image");
        c.erreur = None;
        assert_eq!(annonce(&c).unwrap().texte, "Le rendu a échoué.");
    }

    #[test]
    fn rien_pour_une_annulation_une_commande_en_route_ou_une_voix() {
        for etat in [
            EtatCommande::Annule,
            EtatCommande::Attente,
            EtatCommande::EnCours,
        ] {
            assert_eq!(annonce(&commande("creation", etat)), None);
        }
        assert_eq!(annonce(&commande("voix", EtatCommande::Termine)), None);
        assert_eq!(annonce(&commande("inconnu", EtatCommande::Termine)), None);
    }

    #[test]
    fn le_texte_tient_sur_une_ligne_courte() {
        let mut c = commande("creation", EtatCommande::Termine);
        c.idee = format!("un renard\n  dans la neige {}", "très ".repeat(60));
        let texte = annonce(&c).unwrap().texte;
        assert!(texte.starts_with("un renard dans la neige"));
        assert!(texte.ends_with('…'));
        assert!(texte.chars().count() <= LONGUEUR_MAX + 1);
        assert!(!texte.contains('\n'));
        c.idee = "  ".to_owned();
        assert_eq!(annonce(&c).unwrap().texte, "Le rendu est terminé.");
    }
}
