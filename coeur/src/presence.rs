//! Ce que montrent l'icône de notification, son infobulle et le bloc « État » de son menu, pour une appli qui sert
//! en arrière-plan.
//!
//! - icône : délavée quand rien n'est actif, pleine dès que quelque chose l'est ;
//! - pastille : une seule, par priorité : orange (à regarder), cyan (agit en ce moment), bleue (information) ;
//! - infobulle : la même chose en une ligne, « Aria Nom · mise à jour 0.2.0 disponible ».

/// Pastille de l'icône. Une seule à la fois : la plus prioritaire l'emporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pastille {
    /// Bleue : une information, rien d'urgent (une mise à jour disponible, par exemple).
    Information,
    /// Cyan : l'appli agit en ce moment.
    EnCours,
    /// Orange : à regarder.
    Attention,
}

impl Pastille {
    /// La couleur de la pastille, celle de la palette Aria.
    pub fn couleur(self) -> [u8; 3] {
        match self {
            Self::Information => [0x5c, 0x8b, 0xff],
            Self::EnCours => [0x7d, 0xf9, 0xff],
            Self::Attention => [0xff, 0x8a, 0x1f],
        }
    }
}

/// Couleur du point devant une ligne « État » du menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ton {
    /// Gris : rien d'actif.
    Repos,
    /// Cyan : actif, ou en cours.
    Actif,
    /// Orange : à regarder.
    Attention,
    /// Bleu : une information.
    Information,
}

impl Ton {
    /// La couleur du point, celle de la palette Aria.
    pub fn couleur(self) -> [u8; 3] {
        match self {
            Self::Repos => [0x93, 0xa8, 0xc8],
            Self::Actif => [0x7d, 0xf9, 0xff],
            Self::Attention => [0xff, 0x8a, 0x1f],
            Self::Information => [0x5c, 0x8b, 0xff],
        }
    }
}

/// Une ligne du bloc « État » : un texte, avec un point de couleur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LigneEtat {
    pub ton: Ton,
    pub texte: String,
}

/// L'état de l'icône. Chaque champ est un texte court en minuscules (« surveillance en place »), ou rien.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EtatIcone {
    /// Ce qui est actif (icône pleine, sans pastille).
    pub actif: Option<String>,
    /// Ce que l'appli fait en ce moment (pastille cyan).
    pub en_cours: Option<String>,
    /// Ce qui est à regarder (pastille orange).
    pub attention: Option<String>,
    /// Une information (pastille bleue).
    pub information: Option<String>,
}

/// Longueur maximale de l'infobulle d'une icône de notification (limite de Windows).
pub const INFOBULLE_MAX: usize = 127;

fn present(texte: Option<&String>) -> Option<&str> {
    texte.map(|t| t.trim()).filter(|t| !t.is_empty())
}

/// Les espaces et retours à la ligne ramenés à un seul espace.
fn une_ligne(texte: &str) -> String {
    texte.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Une majuscule au début, pour une ligne du menu.
fn phrase(texte: &str) -> String {
    let ligne = une_ligne(texte);
    let mut lettres = ligne.chars();
    match lettres.next() {
        Some(premiere) => premiere.to_uppercase().chain(lettres).collect(),
        None => ligne,
    }
}

impl EtatIcone {
    /// La pastille affichée : orange, puis cyan, puis bleue.
    pub fn pastille(&self) -> Option<Pastille> {
        if present(self.attention.as_ref()).is_some() {
            Some(Pastille::Attention)
        } else if present(self.en_cours.as_ref()).is_some() {
            Some(Pastille::EnCours)
        } else if present(self.information.as_ref()).is_some() {
            Some(Pastille::Information)
        } else {
            None
        }
    }

    /// Icône délavée : rien n'est actif. Une simple information ne la rend pas pleine.
    pub fn delavee(&self) -> bool {
        [&self.actif, &self.en_cours, &self.attention]
            .iter()
            .all(|t| present(t.as_ref()).is_none())
    }

    /// Lignes du bloc « État », dans l'ordre de priorité ; « Rien d'actif » s'il n'y a rien.
    pub fn lignes(&self) -> Vec<LigneEtat> {
        let mut lignes: Vec<LigneEtat> = [
            (Ton::Attention, &self.attention),
            (Ton::Actif, &self.en_cours),
            (Ton::Actif, &self.actif),
            (Ton::Information, &self.information),
        ]
        .into_iter()
        .filter_map(|(ton, texte)| {
            present(texte.as_ref()).map(|t| LigneEtat {
                ton,
                texte: phrase(t),
            })
        })
        .collect();
        if lignes.is_empty() {
            lignes.push(LigneEtat {
                ton: Ton::Repos,
                texte: "Rien d'actif".to_owned(),
            });
        }
        lignes
    }

    /// Infobulle : le nom de l'appli puis les mêmes informations, en une ligne de 127 caractères au plus.
    pub fn infobulle(&self, nom: &str) -> String {
        let parties: Vec<String> = [&self.attention, &self.en_cours, &self.actif, &self.information]
            .iter()
            .filter_map(|t| present(t.as_ref()).map(une_ligne))
            .collect();
        let detail = if parties.is_empty() {
            "rien d'actif".to_owned()
        } else {
            parties.join(" · ")
        };
        let bulle = format!("{nom} · {detail}");
        if bulle.chars().count() <= INFOBULLE_MAX {
            bulle
        } else {
            let mut coupee: String = bulle.chars().take(INFOBULLE_MAX - 1).collect();
            coupee.push('…');
            coupee
        }
    }
}

/// L'icône de notification à afficher, à partir du logo en RGBA : délavée (moins opaque et grisée) si rien n'est
/// actif, avec sa pastille en bas à droite s'il y en a une.
pub fn composer_icone(
    rgba: &[u8],
    largeur: u32,
    hauteur: u32,
    delavee: bool,
    pastille: Option<Pastille>,
) -> Vec<u8> {
    let mut image = rgba.to_vec();
    if delavee {
        for [r, v, b, a] in image.as_chunks_mut::<4>().0 {
            let gris = (u16::from(*r) + u16::from(*v) + u16::from(*b)) / 3;
            for canal in [r, v, b] {
                *canal = u8::try_from((u16::from(*canal) + gris) / 2).unwrap_or(u8::MAX);
            }
            *a = u8::try_from(u16::from(*a) * 45 / 100).unwrap_or(u8::MAX);
        }
    }
    if let Some(pastille) = pastille {
        let [r, v, b] = pastille.couleur();
        let cote = largeur.min(hauteur);
        // Un disque du tiers de l'icône, en bas à droite, bordé de la couleur du fond pour se détacher du logo.
        let rayon = f64::from(cote) / 6.0;
        let (cx, cy) = (f64::from(largeur) - rayon - 0.5, f64::from(hauteur) - rayon - 0.5);
        for y in 0..hauteur {
            for x in 0..largeur {
                let distance = (f64::from(x) - cx).hypot(f64::from(y) - cy);
                let couleur = if distance <= rayon - 1.0 {
                    [r, v, b, 255]
                } else if distance <= rayon + 0.5 {
                    [0x03, 0x08, 0x12, 255]
                } else {
                    continue;
                };
                let i = usize::try_from((y * largeur + x) * 4).unwrap_or(usize::MAX);
                if let Some(pixel) = image.get_mut(i..i + 4) {
                    pixel.copy_from_slice(&couleur);
                }
            }
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texte(t: &str) -> Option<String> {
        Some(t.to_owned())
    }

    #[test]
    fn au_repos_l_icone_est_delavee_sans_pastille() {
        let etat = EtatIcone::default();
        assert!(etat.delavee());
        assert_eq!(etat.pastille(), None);
        assert_eq!(etat.lignes()[0].texte, "Rien d'actif");
        assert_eq!(etat.infobulle("Aria Essai"), "Aria Essai · rien d'actif");
    }

    #[test]
    fn une_information_seule_ne_rend_pas_l_icone_pleine() {
        let etat = EtatIcone {
            information: texte("mise à jour 0.2.0 disponible"),
            ..EtatIcone::default()
        };
        assert!(etat.delavee());
        assert_eq!(etat.pastille(), Some(Pastille::Information));
    }

    #[test]
    fn la_pastille_suit_la_priorite() {
        let etat = EtatIcone {
            actif: texte("surveillance en place"),
            en_cours: texte("analyse"),
            information: texte("info"),
            attention: None,
        };
        assert!(!etat.delavee());
        assert_eq!(etat.pastille(), Some(Pastille::EnCours));
        let etat = EtatIcone {
            attention: texte("disque presque plein"),
            ..etat
        };
        assert_eq!(etat.pastille(), Some(Pastille::Attention));
    }

    #[test]
    fn les_lignes_prennent_une_majuscule_et_l_ordre_de_priorite() {
        let etat = EtatIcone {
            actif: texte("surveillance  en\nplace"),
            attention: texte("à regarder"),
            ..EtatIcone::default()
        };
        let lignes = etat.lignes();
        assert_eq!(
            lignes[0],
            LigneEtat {
                ton: Ton::Attention,
                texte: "À regarder".into()
            }
        );
        assert_eq!(
            lignes[1],
            LigneEtat {
                ton: Ton::Actif,
                texte: "Surveillance en place".into()
            }
        );
    }

    #[test]
    fn un_texte_vide_compte_pour_rien() {
        let etat = EtatIcone {
            attention: texte("   "),
            ..EtatIcone::default()
        };
        assert_eq!(etat.pastille(), None);
    }

    #[test]
    fn l_infobulle_ne_depasse_pas_127_caracteres() {
        let etat = EtatIcone {
            actif: Some("é".repeat(300)),
            ..EtatIcone::default()
        };
        let bulle = etat.infobulle("Aria Essai");
        assert_eq!(bulle.chars().count(), INFOBULLE_MAX);
        assert!(bulle.ends_with('…'));
    }

    #[test]
    fn composer_delave_et_pose_la_pastille() {
        let (l, h) = (12u32, 12u32);
        let logo = [255u8, 0, 0, 255].repeat((l * h) as usize);
        let delave = composer_icone(&logo, l, h, true, None);
        assert!(delave[3] < 255 && delave[0] < 255 && delave[1] > 0);
        let avec = composer_icone(&logo, l, h, false, Some(Pastille::Attention));
        let coin = (((h - 2) * l + (l - 2)) * 4) as usize;
        assert_eq!(&avec[coin..coin + 3], &Pastille::Attention.couleur());
        assert_eq!(&avec[0..4], &logo[0..4]);
    }
}
