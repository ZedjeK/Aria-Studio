//! Ce que montrent l'icône de notification, son infobulle et le bloc « État » de son menu, pour une appli qui sert
//! en arrière-plan.
//!
//! - icône : délavée quand rien n'est actif, pleine dès que quelque chose l'est ;
//! - pastille : une seule, par priorité : orange (à regarder), cyan (agit en ce moment), bleue (information) ;
//! - infobulle : la même chose en une ligne, « Aria Nom · mise à jour 0.2.0 disponible ».
//!
//! [`Lecture`] est ce que l'appli relève toutes les quelques secondes (moteurs, VRAM, file du Plateau, téléchargements,
//! installation d'un moteur) ; [`EtatIcone::lire`] en fait les lignes du menu, la pastille et l'infobulle.

use serde::{Deserialize, Serialize};

use crate::carte::Vram;
use crate::moteurs::{EtatMoteur, Statut};
use crate::plateau::{Commande, EtatCommande};

/// Longueur maximale d'une ligne d'erreur dans le menu : le menu est étroit, le détail est dans le Journal.
const ERREUR_MAX: usize = 110;

/// Les pages où le menu de l'icône envoie, avec leur nom dans l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageMenu {
    Plateau,
    Montage,
    Modeles,
    Reglages,
    Parametres,
}

impl PageMenu {
    /// Le nom de la page dans l'événement « aller-a » que reçoit l'interface.
    pub fn identifiant(self) -> &'static str {
        match self {
            Self::Plateau => "plateau",
            Self::Montage => "montage",
            Self::Modeles => "modeles",
            Self::Reglages => "reglages",
            Self::Parametres => "parametres",
        }
    }
}

/// Ce que dit un moteur dans le bloc « État ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moteur {
    /// Pas encore lu.
    Inconnu,
    /// Pas installé ; `facultatif` : Aria s'en passe.
    Absent { facultatif: bool },
    /// Installé, il attend : Aria le lance au besoin.
    AuRepos,
    /// Un programme écoute sur son port.
    EnMarche,
}

impl Moteur {
    /// L'état d'un moteur d'après les moteurs du catalogue qui le portent (llama-server en a deux : officiel et
    /// PrismML). Un seul qui tourne suffit à dire « en marche » ; tous absents, « absent ».
    pub fn depuis(moteurs: &[&EtatMoteur]) -> Self {
        if moteurs.is_empty() {
            Self::Inconnu
        } else if moteurs.iter().any(|m| m.occupe) {
            Self::EnMarche
        } else if moteurs.iter().all(|m| m.statut == Statut::Absent) {
            Self::Absent {
                facultatif: moteurs.iter().all(|m| m.facultatif),
            }
        } else {
            Self::AuRepos
        }
    }

    fn ligne(self, nom: &str) -> LigneEtat {
        let (ton, detail) = match self {
            Self::Inconnu => (Ton::Repos, "lecture…"),
            Self::Absent { facultatif: true } => (Ton::Repos, "non installé"),
            Self::Absent { facultatif: false } => (Ton::Attention, "à installer"),
            Self::AuRepos => (Ton::Repos, "au repos"),
            Self::EnMarche => (Ton::Actif, "en marche"),
        };
        LigneEtat {
            ton,
            texte: format!("{nom} : {detail}"),
        }
    }
}

/// Ce que l'appli relève de ses moteurs et de son travail.
#[derive(Debug, Clone, PartialEq)]
pub struct Lecture {
    pub comfyui: Moteur,
    pub llm: Moteur,
    /// Rien sans carte NVIDIA, ou sans lecture.
    pub vram: Option<Vram>,
    /// Ce qu'Aria fait en ce moment : la file du Plateau, des téléchargements, l'installation d'un moteur.
    pub en_cours: Option<String>,
    /// La dernière commande terminée, si elle a échoué.
    pub erreur: Option<String>,
}

/// Le nom d'un genre de commande du Plateau, pour une ligne du menu.
fn nom_du_genre(genre: &str) -> &'static str {
    match genre {
        "creation" => "création",
        "retouche" | "agrandissement" => "retouche",
        "clip" => "clip",
        "sequence" => "séquence",
        "casting" => "casting",
        _ => "génération",
    }
}

/// La commande qui tourne, avec son avancement.
fn texte_en_cours(commande: &Commande) -> String {
    let genre = nom_du_genre(&commande.genre);
    if commande.annulee {
        "annulation en cours".to_owned()
    } else if commande.progression > 0.0 {
        let pour_cent = (commande.progression.clamp(0.0, 1.0) * 100.0).round();
        format!("{genre} en cours ({pour_cent:.0} %)")
    } else {
        format!("{genre} en cours")
    }
}

/// Ce que fait la file du Plateau : la commande en cours (avec son avancement) et celles qui attendent.
fn texte_file(commandes: &[Commande]) -> Option<String> {
    let en_attente = commandes
        .iter()
        .filter(|c| c.etat == EtatCommande::Attente)
        .count();
    let en_cours = commandes
        .iter()
        .find(|c| c.etat == EtatCommande::EnCours)
        .map(texte_en_cours);
    let attente = (en_attente > 0).then(|| format!("{en_attente} en attente"));
    match (en_cours, attente) {
        (Some(cours), Some(attente)) => Some(format!("{cours} · {attente}")),
        (cours, attente) => cours.or(attente),
    }
}

/// Les téléchargements qui travaillent encore (en attente, en cours ou en vérification).
fn texte_telechargements(etats: &[String]) -> Option<String> {
    let actifs = etats
        .iter()
        .filter(|e| matches!(e.as_str(), "attente" | "en_cours" | "verification"))
        .count();
    match actifs {
        0 => None,
        1 => Some("téléchargement en cours".to_owned()),
        n => Some(format!("{n} téléchargements en cours")),
    }
}

/// L'échec de la dernière commande terminée (une annulation ne compte pas, une commande en cours non plus : le
/// message disparaît dès qu'un rendu suivant réussit).
fn erreur_de_la_file(commandes: &[Commande]) -> Option<String> {
    let derniere = commandes
        .iter()
        .find(|c| matches!(c.etat, EtatCommande::Termine | EtatCommande::Erreur))?;
    if derniere.etat != EtatCommande::Erreur {
        return None;
    }
    let genre = nom_du_genre(&derniere.genre);
    let detail = derniere.erreur.as_deref().unwrap_or_default().trim();
    let texte = if detail.is_empty() {
        format!("{genre} en échec")
    } else {
        format!("{genre} en échec : {}", une_ligne(detail))
    };
    Some(couper(&texte, ERREUR_MAX))
}

/// Le texte coupé à `max` caractères, avec une ellipse.
fn couper(texte: &str, max: usize) -> String {
    if texte.chars().count() <= max {
        return texte.to_owned();
    }
    let mut coupe: String = texte.chars().take(max.saturating_sub(1)).collect();
    coupe.push('…');
    coupe
}

/// La commande que « Annuler la génération en cours » vise : celle qui tourne, si elle n'est pas déjà en train de
/// s'annuler. `commandes` : la plus récente en premier, comme `Plateau::commandes`.
pub fn commande_a_annuler(commandes: &[Commande]) -> Option<String> {
    commandes
        .iter()
        .find(|c| c.etat == EtatCommande::EnCours && !c.annulee)
        .map(|c| c.id.clone())
}

/// Des Go à la française, au dixième, sans « ,0 » inutile : 9,8 ou 16.
fn en_go(valeur: f64) -> String {
    let texte = format!("{valeur:.1}").replace('.', ",");
    match texte.strip_suffix(",0") {
        Some(entier) => entier.to_owned(),
        None => texte,
    }
}

/// La ligne de la VRAM : « VRAM : 9,8 Go sur 16 Go ».
fn texte_vram(vram: Vram) -> String {
    let utilisee = en_go(vram.utilisee_go.min(vram.totale_go));
    let totale = en_go(vram.totale_go);
    format!("VRAM : {utilisee} Go sur {totale} Go")
}

impl Lecture {
    /// Ce qui se lit de l'état des moteurs, de la carte et du travail d'Aria. `commandes` : la plus récente en
    /// premier ; `telechargements` : l'état de chaque téléchargement ; `installation` : le moteur en cours
    /// d'installation, s'il y en a un.
    pub fn depuis(
        moteurs: &[EtatMoteur],
        vram: Option<Vram>,
        commandes: &[Commande],
        telechargements: &[String],
        installation: Option<&str>,
    ) -> Self {
        let de = |accepte: fn(&str) -> bool| -> Moteur {
            let portes: Vec<&EtatMoteur> = moteurs.iter().filter(|m| accepte(&m.id)).collect();
            Moteur::depuis(&portes)
        };
        let travaux: Vec<String> = [
            texte_file(commandes),
            texte_telechargements(telechargements),
            installation.map(|nom| format!("installation de {nom}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self {
            comfyui: de(|id| id == "comfyui"),
            llm: de(|id| id.starts_with("llama")),
            vram,
            en_cours: (!travaux.is_empty()).then(|| travaux.join(" · ")),
            erreur: erreur_de_la_file(commandes),
        }
    }
}

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
    /// Les lignes des moteurs et de la VRAM, en tête du bloc « État » (voir [`EtatIcone::lire`]).
    pub moteurs: Vec<LigneEtat>,
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

    /// Prend une nouvelle lecture : les lignes ComfyUI, LLM et VRAM, l'icône pleine tant qu'un moteur tourne, la
    /// pastille cyan pendant que l'appli travaille, et l'orange sur un échec. `echec` (une action du menu qui a
    /// échoué) passe avant l'échec de la file. L'information (mise à jour) n'est pas touchée.
    pub fn lire(&mut self, lecture: &Lecture, echec: Option<&str>) {
        let mut lignes = vec![lecture.comfyui.ligne("ComfyUI"), lecture.llm.ligne("LLM")];
        if let Some(vram) = lecture.vram {
            lignes.push(LigneEtat {
                ton: Ton::Repos,
                texte: texte_vram(vram),
            });
        }
        self.moteurs = lignes;
        let en_marche: Vec<&str> = [(lecture.comfyui, "ComfyUI"), (lecture.llm, "LLM")]
            .into_iter()
            .filter(|(moteur, _)| *moteur == Moteur::EnMarche)
            .map(|(_, nom)| nom)
            .collect();
        self.actif = if en_marche.is_empty() {
            None
        } else {
            Some(format!("{} en marche", en_marche.join(" et ")))
        };
        self.en_cours.clone_from(&lecture.en_cours);
        self.attention = echec.map(str::to_owned).or_else(|| lecture.erreur.clone());
    }

    /// Lignes du bloc « État », dans l'ordre de priorité ; « Rien d'actif » s'il n'y a rien.
    pub fn lignes(&self) -> Vec<LigneEtat> {
        // Les lignes des moteurs disent déjà ce qui tourne : « actif » ne les double pas.
        let avec_actif = self.moteurs.is_empty();
        let suite: Vec<LigneEtat> = [
            (Ton::Attention, &self.attention, true),
            (Ton::Actif, &self.en_cours, true),
            (Ton::Actif, &self.actif, avec_actif),
            (Ton::Information, &self.information, true),
        ]
        .into_iter()
        .filter_map(|(ton, texte, garde)| {
            if !garde {
                return None;
            }
            present(texte.as_ref()).map(|t| LigneEtat {
                ton,
                texte: phrase(t),
            })
        })
        .collect();
        let mut lignes = self.moteurs.clone();
        lignes.extend(suite);
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
            ..EtatIcone::default()
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

    fn moteur(id: &str, statut: Statut, occupe: bool, facultatif: bool) -> EtatMoteur {
        EtatMoteur {
            id: id.to_owned(),
            nom: id.to_owned(),
            role: String::new(),
            facultatif,
            dossier: id.to_owned(),
            version: "1".to_owned(),
            version_installee: None,
            statut,
            chemin: String::new(),
            externe: false,
            precedente: false,
            a_telecharger: 0,
            occupe,
            prerequis: None,
        }
    }

    fn commande(genre: &str, etat: EtatCommande) -> Commande {
        let mut c = Commande::nouvelle(
            format!("{genre}-{etat:?}"),
            genre,
            "un renard".to_owned(),
            "krea".to_owned(),
            "carré",
        );
        c.etat = etat;
        c
    }

    fn lecture(moteurs: &[EtatMoteur], commandes: &[Commande]) -> Lecture {
        Lecture::depuis(moteurs, None, commandes, &[], None)
    }

    #[test]
    fn un_moteur_en_marche_prime_sur_le_repos_et_l_absence() {
        let comfy = moteur("comfyui", Statut::AJour, true, false);
        let absent = moteur("llama", Statut::Absent, false, true);
        let prism = moteur("llama-prism", Statut::AJour, false, true);
        assert_eq!(Moteur::depuis(&[]), Moteur::Inconnu);
        assert_eq!(Moteur::depuis(&[&comfy]), Moteur::EnMarche);
        assert_eq!(Moteur::depuis(&[&absent]), Moteur::Absent { facultatif: true });
        assert_eq!(Moteur::depuis(&[&absent, &prism]), Moteur::AuRepos);
    }

    #[test]
    fn la_lecture_separe_comfyui_et_les_llama() {
        let moteurs = [
            moteur("comfyui", Statut::AJour, false, false),
            moteur("llama", Statut::AJour, true, true),
            moteur("llama-prism", Statut::Absent, false, true),
        ];
        let lue = lecture(&moteurs, &[]);
        assert_eq!(lue.comfyui, Moteur::AuRepos);
        assert_eq!(lue.llm, Moteur::EnMarche);
        assert_eq!(lue.en_cours, None);
        assert_eq!(lue.erreur, None);
    }

    #[test]
    fn le_menu_montre_comfyui_le_llm_et_la_vram_puis_l_erreur() {
        let moteurs = [
            moteur("comfyui", Statut::AJour, true, false),
            moteur("llama", Statut::Absent, false, false),
        ];
        let mut echec = commande("creation", EtatCommande::Erreur);
        echec.erreur = Some("ComfyUI n'a renvoyé aucune image".to_owned());
        let vram = Vram {
            utilisee_go: 9.84,
            totale_go: 16.0,
        };
        let lue = Lecture::depuis(&moteurs, Some(vram), &[echec], &[], None);
        let mut etat = EtatIcone::default();
        etat.lire(&lue, None);
        let textes: Vec<String> = etat.lignes().into_iter().map(|l| l.texte).collect();
        assert_eq!(
            textes,
            [
                "ComfyUI : en marche",
                "LLM : à installer",
                "VRAM : 9,8 Go sur 16 Go",
                "Création en échec : ComfyUI n'a renvoyé aucune image",
            ]
        );
        assert_eq!(etat.lignes()[0].ton, Ton::Actif);
        assert_eq!(etat.lignes()[1].ton, Ton::Attention);
        assert_eq!(etat.pastille(), Some(Pastille::Attention));
        assert!(!etat.delavee());
    }

    #[test]
    fn la_pastille_est_cyan_pendant_un_rendu_et_l_infobulle_le_dit() {
        let mut en_cours = commande("creation", EtatCommande::EnCours);
        en_cours.progression = 0.456;
        let attente = commande("retouche", EtatCommande::Attente);
        let lue = lecture(&[], &[attente, en_cours]);
        assert_eq!(
            lue.en_cours.as_deref(),
            Some("création en cours (46 %) · 1 en attente")
        );
        let mut etat = EtatIcone::default();
        etat.lire(&lue, None);
        assert_eq!(etat.pastille(), Some(Pastille::EnCours));
        assert!(etat.infobulle("Aria Studio").contains("création en cours (46 %)"));
    }

    #[test]
    fn un_rendu_reussi_apres_un_echec_efface_l_erreur() {
        let echec = commande("creation", EtatCommande::Erreur);
        let reussi = commande("clip", EtatCommande::Termine);
        assert!(lecture(&[], std::slice::from_ref(&echec)).erreur.is_some());
        assert_eq!(lecture(&[], &[reussi, echec]).erreur, None);
    }

    #[test]
    fn une_annulation_ni_une_commande_en_cours_ne_cachent_l_erreur() {
        let echec = commande("creation", EtatCommande::Erreur);
        let annulee = commande("creation", EtatCommande::Annule);
        let en_cours = commande("creation", EtatCommande::EnCours);
        let lue = lecture(&[], &[en_cours, annulee, echec]);
        assert_eq!(lue.erreur.as_deref(), Some("création en échec"));
    }

    #[test]
    fn une_erreur_trop_longue_est_coupee_sur_une_ligne() {
        let mut echec = commande("clip", EtatCommande::Erreur);
        echec.erreur = Some(format!("une\nligne {}", "é".repeat(300)));
        let texte = lecture(&[], &[echec]).erreur.unwrap_or_default();
        assert_eq!(texte.chars().count(), ERREUR_MAX);
        assert!(texte.starts_with("clip en échec : une ligne "));
        assert!(texte.ends_with('…'));
    }

    #[test]
    fn un_echec_du_menu_passe_avant_celui_de_la_file() {
        let lue = lecture(&[], &[commande("creation", EtatCommande::Erreur)]);
        let mut etat = EtatIcone::default();
        etat.lire(&lue, Some("libération impossible"));
        assert_eq!(etat.attention.as_deref(), Some("libération impossible"));
        etat.lire(&lue, None);
        assert_eq!(etat.attention.as_deref(), Some("création en échec"));
    }

    #[test]
    fn la_lecture_ne_touche_pas_a_l_information() {
        let mut etat = EtatIcone {
            information: texte("mise à jour 0.2.0 disponible"),
            ..EtatIcone::default()
        };
        etat.lire(&lecture(&[], &[]), None);
        assert_eq!(etat.information.as_deref(), Some("mise à jour 0.2.0 disponible"));
        assert_eq!(etat.pastille(), Some(Pastille::Information));
    }

    #[test]
    fn les_telechargements_et_l_installation_comptent_comme_du_travail() {
        let etats: Vec<String> = ["en_cours", "attente", "termine", "erreur"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let lue = Lecture::depuis(&[], None, &[], &etats, Some("ComfyUI"));
        assert_eq!(
            lue.en_cours.as_deref(),
            Some("2 téléchargements en cours · installation de ComfyUI")
        );
        let seul = Lecture::depuis(&[], None, &[], &["verification".to_owned()], None);
        assert_eq!(seul.en_cours.as_deref(), Some("téléchargement en cours"));
        let fini = Lecture::depuis(&[], None, &[], &["termine".to_owned()], None);
        assert_eq!(fini.en_cours, None);
    }

    #[test]
    fn annuler_vise_la_commande_qui_tourne_une_seule_fois() {
        let attente = commande("creation", EtatCommande::Attente);
        let en_cours = commande("clip", EtatCommande::EnCours);
        let id = en_cours.id.clone();
        assert_eq!(commande_a_annuler(&[attente.clone(), en_cours.clone()]), Some(id));
        assert_eq!(commande_a_annuler(&[attente]), None);
        let mut s_annule = en_cours;
        s_annule.annulee = true;
        assert_eq!(commande_a_annuler(&[s_annule]), None);
    }

    #[test]
    fn les_go_se_disent_a_la_francaise() {
        assert_eq!(en_go(16.0), "16");
        assert_eq!(en_go(9.84), "9,8");
        assert_eq!(en_go(0.0), "0");
        let vram = Vram {
            utilisee_go: 20.0,
            totale_go: 16.0,
        };
        assert_eq!(texte_vram(vram), "VRAM : 16 Go sur 16 Go");
    }

    #[test]
    fn les_pages_du_menu_portent_le_nom_de_l_interface() {
        for page in [
            PageMenu::Plateau,
            PageMenu::Montage,
            PageMenu::Modeles,
            PageMenu::Reglages,
            PageMenu::Parametres,
        ] {
            assert_eq!(serde_json::to_value(page).ok(), Some(page.identifiant().into()));
        }
    }
}
