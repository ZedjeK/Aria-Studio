//! L'en-tête d'un GGUF (format de llama.cpp) : les types de ses tenseurs, lus sans charger les poids, comme en v2
//! (`app/gguf.py`).
//!
//! Sert à choisir le moteur : un fork comme PrismML ajoute ses propres types (Bonsai 2 ternaire : 142 en PQ2_0, 143
//! en PTQ1_0), que le llama.cpp officiel refuse au chargement (« invalid ggml type 142. should be in [0, 43) »).

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Types que le llama.cpp officiel épinglé (catalogue/moteurs.yaml, b11734) sait lire : de 0 à 42.
pub const TYPES_OFFICIELS: u32 = 43;

/// Au-delà, un compte n'a pas de sens : fichier abîmé.
const MAXI: u64 = 100_000_000;
const CHAINE: u32 = 8;
const TABLEAU: u32 = 9;

/// Taille des valeurs fixes des métadonnées, par type GGUF (8 chaîne et 9 tableau ont leur propre longueur).
fn taille_fixe(type_: u32) -> Option<u64> {
    match type_ {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        10..=12 => Some(8),
        _ => None,
    }
}

struct Lecteur<R> {
    entree: R,
}

impl<R: Read + Seek> Lecteur<R> {
    fn entier(&mut self, octets: usize) -> Result<u64, String> {
        let mut tampon = [0u8; 8];
        let part = tampon
            .get_mut(..octets)
            .ok_or_else(|| "GGUF illisible".to_owned())?;
        self.entree
            .read_exact(part)
            .map_err(|_| "GGUF tronqué".to_owned())?;
        Ok(u64::from_le_bytes(tampon))
    }

    fn u32(&mut self) -> Result<u32, String> {
        u32::try_from(self.entier(4)?).map_err(|_| "GGUF illisible".to_owned())
    }

    fn compte(&mut self) -> Result<u64, String> {
        let n = self.entier(8)?;
        if n > MAXI {
            return Err("GGUF illisible".to_owned());
        }
        Ok(n)
    }

    fn sauter(&mut self, octets: u64) -> Result<(), String> {
        let octets = i64::try_from(octets).map_err(|_| "GGUF illisible".to_owned())?;
        self.entree
            .seek(SeekFrom::Current(octets))
            .map(|_| ())
            .map_err(|e| format!("GGUF illisible : {e}"))
    }

    fn sauter_valeur(&mut self, type_: u32) -> Result<(), String> {
        if let Some(taille) = taille_fixe(type_) {
            return self.sauter(taille);
        }
        match type_ {
            CHAINE => {
                let n = self.compte()?;
                self.sauter(n)
            }
            TABLEAU => {
                let type_element = self.u32()?;
                let n = self.compte()?;
                if let Some(taille) = taille_fixe(type_element) {
                    self.sauter(taille * n)
                } else {
                    for _ in 0..n {
                        self.sauter_valeur(type_element)?;
                    }
                    Ok(())
                }
            }
            _ => Err(format!("Type de métadonnée inconnu : {type_}")),
        }
    }
}

/// Les types des tenseurs d'un GGUF lu dans `entree`.
///
/// # Errors
/// Ce n'est pas un GGUF, il est trop ancien, tronqué ou abîmé.
pub fn types_tenseurs_de(entree: impl Read + Seek) -> Result<BTreeSet<u32>, String> {
    let mut l = Lecteur { entree };
    let mut magie = [0u8; 4];
    l.entree
        .read_exact(&mut magie)
        .map_err(|_| "Ce n'est pas un GGUF".to_owned())?;
    if &magie != b"GGUF" {
        return Err("Ce n'est pas un GGUF".to_owned());
    }
    if l.u32()? < 2 {
        return Err("GGUF version 1 : trop ancien".to_owned());
    }
    let tenseurs = l.compte()?;
    let cles = l.compte()?;
    for _ in 0..cles {
        let nom = l.compte()?;
        l.sauter(nom)?;
        let type_ = l.u32()?;
        l.sauter_valeur(type_)?;
    }
    let mut types = BTreeSet::new();
    for _ in 0..tenseurs {
        let nom = l.compte()?;
        l.sauter(nom)?;
        let dimensions = u64::from(l.u32()?);
        l.sauter(8 * dimensions)?;
        types.insert(l.u32()?);
        l.sauter(8)?; // position des données
    }
    Ok(types)
}

/// Les types des tenseurs d'un fichier GGUF.
///
/// # Errors
/// Fichier illisible, ou voir [`types_tenseurs_de`].
pub fn types_tenseurs(chemin: &Path) -> Result<BTreeSet<u32>, String> {
    let fichier = File::open(chemin).map_err(|e| format!("{} : {e}", chemin.display()))?;
    types_tenseurs_de(BufReader::with_capacity(1 << 20, fichier))
}

/// Les types que le llama.cpp officiel ne sait pas lire : ceux d'un fork.
///
/// # Errors
/// Voir [`types_tenseurs`].
pub fn types_inconnus(chemin: &Path) -> Result<Vec<u32>, String> {
    Ok(types_tenseurs(chemin)?
        .into_iter()
        .filter(|t| *t >= TYPES_OFFICIELS)
        .collect())
}

#[cfg(test)]
pub(crate) mod essai {
    /// Un GGUF minimal : quelques métadonnées (texte, entier, tableau de textes) et des tenseurs de ces types.
    pub fn gguf(types: &[u32]) -> Vec<u8> {
        let mut o = b"GGUF".to_vec();
        o.extend_from_slice(&3u32.to_le_bytes());
        o.extend_from_slice(&(types.len() as u64).to_le_bytes());
        o.extend_from_slice(&3u64.to_le_bytes());
        let texte = |o: &mut Vec<u8>, s: &str| {
            o.extend_from_slice(&(s.len() as u64).to_le_bytes());
            o.extend_from_slice(s.as_bytes());
        };
        texte(&mut o, "general.name");
        o.extend_from_slice(&8u32.to_le_bytes());
        texte(&mut o, "Bonsai");
        texte(&mut o, "general.file_type");
        o.extend_from_slice(&4u32.to_le_bytes());
        o.extend_from_slice(&7u32.to_le_bytes());
        texte(&mut o, "tokenizer.tokens");
        o.extend_from_slice(&9u32.to_le_bytes());
        o.extend_from_slice(&8u32.to_le_bytes());
        o.extend_from_slice(&2u64.to_le_bytes());
        texte(&mut o, "<s>");
        texte(&mut o, "</s>");
        for (i, t) in types.iter().enumerate() {
            texte(&mut o, &format!("blk.{i}.weight"));
            o.extend_from_slice(&2u32.to_le_bytes());
            o.extend_from_slice(&4096u64.to_le_bytes());
            o.extend_from_slice(&4096u64.to_le_bytes());
            o.extend_from_slice(&t.to_le_bytes());
            o.extend_from_slice(&0u64.to_le_bytes());
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn lit_les_types_des_tenseurs() {
        let types = types_tenseurs_de(Cursor::new(essai::gguf(&[0, 12, 12, 1]))).unwrap();
        assert_eq!(types.into_iter().collect::<Vec<_>>(), vec![0, 1, 12]);
    }

    #[test]
    fn un_type_de_fork_est_inconnu_de_l_officiel() {
        let temp = tempfile::tempdir().unwrap();
        let standard = temp.path().join("standard.gguf");
        let ternaire = temp.path().join("ternaire.gguf");
        std::fs::write(&standard, essai::gguf(&[1, 12])).unwrap();
        std::fs::write(&ternaire, essai::gguf(&[143, 1, 142])).unwrap();
        assert!(types_inconnus(&standard).unwrap().is_empty());
        assert_eq!(types_inconnus(&ternaire).unwrap(), vec![142, 143]);
        assert!(types_inconnus(&temp.path().join("absent.gguf")).is_err());
    }

    #[test]
    fn refuse_ce_qui_n_est_pas_un_gguf() {
        assert_eq!(
            types_tenseurs_de(Cursor::new(b"PK\x03\x04".to_vec())).unwrap_err(),
            "Ce n'est pas un GGUF"
        );
        let mut ancien = b"GGUF".to_vec();
        ancien.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            types_tenseurs_de(Cursor::new(ancien)).unwrap_err(),
            "GGUF version 1 : trop ancien"
        );
        let complet = essai::gguf(&[1]);
        assert_eq!(
            types_tenseurs_de(Cursor::new(complet[..complet.len() - 12].to_vec())).unwrap_err(),
            "GGUF tronqué"
        );
    }
}
