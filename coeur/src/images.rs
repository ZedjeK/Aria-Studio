//! Les traitements d'image autour des créations, comme en v2 (`app/retouche.py` et `Vue` de `app/prompt.py`) :
//! ouverture (orientation EXIF appliquée), recadrage au centre, masque peint, recollage de la retouche, canevas
//! d'agrandissement et recollage de la source, image réduite pour la vision du LLM.
//!
//! Le modèle de retouche réinterprète toute l'image. Pour ne modifier que la zone voulue, on remet ensuite l'original
//! au pixel près partout ailleurs, avec un fondu sur le bord. Les rééchantillonnages et les flous suivent ceux de
//! Pillow (Lanczos, bilinéaire, gaussien) sans en reproduire chaque arrondi.

use std::io::Cursor;

use image::imageops::{self, FilterType};
use image::{DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, Luma, Rgb, RgbImage};

/// La plus grande image acceptée, en pixels de côté.
pub const TAILLE_MAX: u32 = 4096;
/// Au-delà, une image montrée au LLM est réduite : environ 600 tokens de vision (Bonsai 2, mesuré en v2).
pub const PIXELS_VUE: u64 = 1024 * 1024;
/// Où placer la source dans un canevas agrandi : fraction de la place ajoutée laissée à gauche, puis en haut.
pub const ANCRAGES: [(&str, (f64, f64)); 9] = [
    ("centre", (0.5, 0.5)),
    ("gauche", (0.0, 0.5)),
    ("droite", (1.0, 0.5)),
    ("haut", (0.5, 0.0)),
    ("bas", (0.5, 1.0)),
    ("haut-gauche", (0.0, 0.0)),
    ("haut-droite", (1.0, 0.0)),
    ("bas-gauche", (0.0, 1.0)),
    ("bas-droite", (1.0, 1.0)),
];
/// Le gris neutre de la zone ajoutée par un agrandissement, que le modèle remplace.
const GRIS: Rgb<u8> = Rgb([128, 128, 128]);

/// Un arrondi comme `round()` de Python : au pair le plus proche sur un demi.
fn arrondi(x: f64) -> i64 {
    // Les dimensions d'image tiennent largement dans un i64.
    #[allow(clippy::cast_possible_truncation)]
    let r = x.round_ties_even() as i64;
    r
}

fn en_u32(x: i64) -> u32 {
    u32::try_from(x.max(0)).unwrap_or(u32::MAX)
}

/// Ouvre une image (PNG, JPEG, WebP), redressée d'après son orientation EXIF, en RVB.
///
/// # Errors
/// Image illisible ou d'un format non pris en charge.
pub fn ouvrir(octets: &[u8]) -> Result<RgbImage, String> {
    let illisible = |e: &dyn std::fmt::Display| format!("Image illisible : {e}");
    let mut decodeur = ImageReader::new(Cursor::new(octets))
        .with_guessed_format()
        .map_err(|e| illisible(&e))?
        .into_decoder()
        .map_err(|e| illisible(&e))?;
    let orientation = decodeur.orientation().map_err(|e| illisible(&e))?;
    let mut image = DynamicImage::from_decoder(decodeur).map_err(|e| illisible(&e))?;
    image.apply_orientation(orientation);
    Ok(image.into_rgb8())
}

/// L'image en PNG.
///
/// # Errors
/// L'encodage échoue (mémoire).
pub fn png(image: &RgbImage) -> Result<Vec<u8>, String> {
    let mut sortie = Cursor::new(Vec::new());
    image
        .write_to(&mut sortie, ImageFormat::Png)
        .map_err(|e| format!("PNG impossible : {e}"))?;
    Ok(sortie.into_inner())
}

/// L'image coupée au centre au rapport d'un format (« 16:9 (Widescreen) »), sans la déformer ni la rééchantillonner :
/// on retire ce qui dépasse, rien d'autre.
///
/// # Errors
/// Format illisible.
pub fn recadrer_centre(image: &RgbImage, format: &str) -> Result<RgbImage, String> {
    let inconnu = || format!("Format inconnu : {format}");
    let (avant, apres) = format.trim().split_once(':').ok_or_else(inconnu)?;
    let nombre = |t: &str| -> Option<f64> {
        let t: String = t
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        t.parse::<f64>().ok().filter(|v| *v > 0.0)
    };
    let (Some(l), Some(h)) = (nombre(avant), nombre(apres)) else {
        return Err(inconnu());
    };
    let rapport = l / h;
    let (largeur, hauteur) = image.dimensions();
    let (lf, hf) = (f64::from(largeur), f64::from(hauteur));
    let (x, y, nl, nh) = if lf / hf > rapport {
        let nl = en_u32(arrondi(hf * rapport).max(1));
        ((largeur - nl.min(largeur)) / 2, 0, nl.min(largeur), hauteur)
    } else {
        let nh = en_u32(arrondi(lf / rapport).max(1));
        (0, (hauteur - nh.min(hauteur)) / 2, largeur, nh.min(hauteur))
    };
    Ok(imageops::crop_imm(image, x, y, nl, nh).to_image())
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 standard, avec remplissage.
pub fn base64(octets: &[u8]) -> String {
    let mut sortie = String::with_capacity(octets.len().div_ceil(3) * 4);
    for bloc in octets.chunks(3) {
        let b = [
            bloc[0],
            bloc.get(1).copied().unwrap_or(0),
            bloc.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, decalage) in [18u32, 12, 6, 0].iter().enumerate() {
            if i <= bloc.len() {
                sortie.push(char::from(ALPHABET[((n >> decalage) & 63) as usize]));
            } else {
                sortie.push('=');
            }
        }
    }
    sortie
}

fn valeur_base64(c: u8) -> Option<u32> {
    ALPHABET
        .iter()
        .position(|a| *a == c)
        .and_then(|p| u32::try_from(p).ok())
}

/// Les octets d'une data URL (« data:image/png;base64,… », envoyée par l'interface) ou d'un base64 seul.
///
/// # Errors
/// Base64 invalide.
pub fn depuis_data_url(texte: &str) -> Result<Vec<u8>, String> {
    let invalide = || "Données d'image invalides : base64 attendu".to_owned();
    let donnees = if texte.starts_with("data:") {
        texte.split_once(',').map(|(_, d)| d).ok_or_else(invalide)?
    } else {
        texte
    };
    let propres: Vec<u8> = donnees.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let sans_remplissage = propres
        .strip_suffix(b"==")
        .or_else(|| propres.strip_suffix(b"="))
        .unwrap_or(&propres);
    let mut sortie = Vec::with_capacity(sans_remplissage.len() * 3 / 4);
    for bloc in sans_remplissage.chunks(4) {
        if bloc.len() == 1 {
            return Err(invalide());
        }
        let mut n = 0u32;
        for (i, c) in bloc.iter().enumerate() {
            n |= valeur_base64(*c).ok_or_else(invalide)? << (18 - 6 * i);
        }
        let octets = n.to_be_bytes();
        sortie.extend_from_slice(&octets[1..bloc.len()]);
    }
    Ok(sortie)
}

/// Le masque peint dans l'interface, à la taille de l'image : blanc = zone à retoucher. Un canevas transparent est lu
/// par sa transparence (le navigateur peint en opaque sur fond transparent). Rien si rien n'est peint.
///
/// # Errors
/// Masque illisible.
pub fn masque_net(octets: &[u8], taille: (u32, u32)) -> Result<Option<GrayImage>, String> {
    let image = image::load_from_memory(octets).map_err(|e| format!("Masque illisible : {e}"))?;
    let gris: GrayImage = if image.color().has_alpha() {
        let la = image.to_luma_alpha8();
        GrayImage::from_fn(la.width(), la.height(), |x, y| Luma([la.get_pixel(x, y)[1]]))
    } else {
        image.to_luma8()
    };
    let mut masque = imageops::resize(&gris, taille.0, taille.1, FilterType::Triangle);
    let mut peint = false;
    for p in masque.pixels_mut() {
        p[0] = if p[0] > 127 { 255 } else { 0 };
        peint |= p[0] == 255;
    }
    Ok(peint.then_some(masque))
}

/// Élargit la zone blanche d'un masque : chaque pixel prend le maximum d'un carré de `taille` pixels de côté.
fn dilater(masque: &GrayImage, taille: u32) -> GrayImage {
    let rayon = i64::from(taille / 2);
    let (l, h) = masque.dimensions();
    let max_ligne = |source: &GrayImage, horizontal: bool| {
        GrayImage::from_fn(l, h, |x, y| {
            let mut m = 0u8;
            for d in -rayon..=rayon {
                let (px, py) = if horizontal {
                    (i64::from(x) + d, i64::from(y))
                } else {
                    (i64::from(x), i64::from(y) + d)
                };
                if px >= 0 && py >= 0 && px < i64::from(l) && py < i64::from(h) {
                    m = m.max(source.get_pixel(en_u32(px), en_u32(py))[0]);
                }
            }
            Luma([m])
        })
    };
    let horizontale = max_ligne(masque, true);
    max_ligne(&horizontale, false)
}

fn flouter(masque: &GrayImage, rayon: f32) -> GrayImage {
    // Un rayon infime ne change rien (Pillow non plus).
    if rayon < 0.3 {
        masque.clone()
    } else {
        imageops::blur(masque, rayon)
    }
}

/// Coupe les traînes du flou : hors du fondu, l'original reste strictement intact.
fn seuiller(masque: &mut GrayImage) {
    for p in masque.pixels_mut() {
        p[0] = match p[0] {
            0..=5 => 0,
            250..=255 => 255,
            v => v,
        };
    }
}

/// `dessus` là où le masque est blanc, `dessous` là où il est noir, mélangés entre les deux.
fn composer(dessus: &RgbImage, dessous: &RgbImage, masque: &GrayImage) -> RgbImage {
    RgbImage::from_fn(dessous.width(), dessous.height(), |x, y| {
        let m = u32::from(masque.get_pixel(x, y)[0]);
        let a = dessus.get_pixel(x, y);
        let b = dessous.get_pixel(x, y);
        let mut p = [0u8; 3];
        for (c, sortie) in p.iter_mut().enumerate() {
            let v = (u32::from(a[c]) * m + u32::from(b[c]) * (255 - m) + 127) / 255;
            *sortie = u8::try_from(v).unwrap_or(u8::MAX);
        }
        Rgb(p)
    })
}

/// Garde l'original hors du masque ; la zone retouchée se fond sur `fondu` pixels.
pub fn recoller(original: &RgbImage, retouche: &RgbImage, masque: &GrayImage, fondu: u32) -> RgbImage {
    let (l, h) = original.dimensions();
    let retouche = imageops::resize(retouche, l, h, FilterType::Lanczos3);
    let masque = if masque.dimensions() == (l, h) {
        masque.clone()
    } else {
        imageops::resize(masque, l, h, FilterType::Triangle)
    };
    let masque = if fondu > 0 {
        // La zone s'élargit d'abord, pour que le fondu mange l'original et pas la retouche.
        #[allow(clippy::cast_precision_loss)] // un rayon de quelques pixels
        let rayon = fondu as f32 / 8.0;
        let mut m = flouter(&dilater(&masque, 2 * (fondu / 2) + 1), rayon);
        seuiller(&mut m);
        m
    } else {
        masque
    };
    composer(&retouche, original, &masque)
}

/// Agrandit le canevas : la source est placée selon l'ancrage, le reste est un aplat gris que le modèle remplace.
/// Rend le canevas et la position de la source.
///
/// # Errors
/// Ancrage inconnu, taille plus petite que la source, identique, ou au-delà de [`TAILLE_MAX`].
pub fn canevas_agrandi(
    source: &RgbImage,
    largeur: u32,
    hauteur: u32,
    ancrage: &str,
) -> Result<(RgbImage, (u32, u32)), String> {
    let (fx, fy) = ANCRAGES
        .iter()
        .find(|(nom, _)| *nom == ancrage)
        .map(|(_, f)| *f)
        .ok_or_else(|| format!("Ancrage inconnu : {ancrage}"))?;
    let (l, h) = source.dimensions();
    if largeur < l || hauteur < h {
        return Err(format!(
            "La nouvelle taille doit contenir l'image d'origine ({l}×{h})"
        ));
    }
    if (largeur, hauteur) == (l, h) {
        return Err("La nouvelle taille est identique à l'originale".to_owned());
    }
    if largeur.max(hauteur) > TAILLE_MAX {
        return Err(format!("Taille maximale : {TAILLE_MAX} px de côté"));
    }
    let x = en_u32(arrondi(f64::from(largeur - l) * fx));
    let y = en_u32(arrondi(f64::from(hauteur - h) * fy));
    let mut fond = RgbImage::from_pixel(largeur, hauteur, GRIS);
    imageops::replace(&mut fond, source, i64::from(x), i64::from(y));
    Ok((fond, (x, y)))
}

/// Moyenne et écart type (de population, comme `ImageStat` de Pillow) de chaque canal.
fn statistiques(pixels: impl Iterator<Item = Rgb<u8>>) -> [(f64, f64); 3] {
    let mut somme = [0f64; 3];
    let mut carres = [0f64; 3];
    let mut n = 0f64;
    for p in pixels {
        for ((s, q), v) in somme.iter_mut().zip(carres.iter_mut()).zip(p.0) {
            let v = f64::from(v);
            *s += v;
            *q += v * v;
        }
        n += 1.0;
    }
    let mut sortie = [(0.0, 0.0); 3];
    if n >= 1.0 {
        for ((o, s), q) in sortie.iter_mut().zip(somme).zip(carres) {
            let moyenne = s / n;
            *o = (moyenne, (q / n - moyenne * moyenne).max(0.0).sqrt());
        }
    }
    sortie
}

/// Le modèle régénère toute la scène et décale souvent les teintes : moyenne et contraste de chaque canal du résultat
/// sont alignés sur l'original, mesurés sur la zone qu'ils ont en commun.
fn accorder_couleurs(resultat: &RgbImage, reference: &RgbImage, zone: (u32, u32, u32, u32)) -> RgbImage {
    let (x, y, l, h) = zone;
    let genere = statistiques(
        imageops::crop_imm(resultat, x, y, l, h)
            .to_image()
            .pixels()
            .copied(),
    );
    let original = statistiques(reference.pixels().copied());
    let reglage: Vec<(f64, f64)> = genere
        .iter()
        .zip(original.iter())
        .map(|(g, o)| {
            let gain = if g.1 > 1.0 {
                (o.1 / g.1).clamp(0.7, 1.4)
            } else {
                1.0
            };
            (gain, o.0 - g.0 * gain)
        })
        .collect();
    let mut sortie = resultat.clone();
    for p in sortie.pixels_mut() {
        for (v, (gain, decalage)) in p.0.iter_mut().zip(&reglage) {
            let nouvelle = arrondi(f64::from(*v) * gain + decalage).clamp(0, 255);
            *v = u8::try_from(nouvelle).unwrap_or(u8::MAX);
        }
    }
    sortie
}

/// Remet la source intacte à sa place dans l'image agrandie générée (déjà à la taille du canevas), avec un fondu
/// vers l'intérieur, seulement sur les bords de la source qui touchent une zone ajoutée.
pub fn recoller_agrandissement(
    source: &RgbImage,
    position: (u32, u32),
    resultat: &RgbImage,
    fondu: u32,
) -> RgbImage {
    let (x, y) = position;
    let (l, h) = source.dimensions();
    let (rl, rh) = resultat.dimensions();
    let resultat = accorder_couleurs(resultat, source, (x, y, l, h));
    let demi = i64::from(fondu / 2);
    let fondu_i = i64::from(fondu);
    let gauche = if x > 0 { demi } else { -fondu_i };
    let haut = if y > 0 { demi } else { -fondu_i };
    let droite = if x + l < rl { demi } else { -fondu_i };
    let bas = if y + h < rh { demi } else { -fondu_i };
    let (x0, y0) = (i64::from(x) + gauche, i64::from(y) + haut);
    let (x1, y1) = (i64::from(x + l) - droite, i64::from(y + h) - bas);
    let mut masque = GrayImage::from_fn(rl, rh, |px, py| {
        let (px, py) = (i64::from(px), i64::from(py));
        Luma([if px >= x0 && px < x1 && py >= y0 && py < y1 {
            255
        } else {
            0
        }])
    });
    if fondu > 0 {
        #[allow(clippy::cast_precision_loss)] // un rayon de quelques pixels
        let rayon = fondu as f32 / 6.0;
        masque = flouter(&masque, rayon);
        seuiller(&mut masque);
    }
    let mut original_place = resultat.clone();
    imageops::replace(&mut original_place, source, i64::from(x), i64::from(y));
    composer(&original_place, &resultat, &masque)
}

/// Une image à montrer au LLM : telle quelle jusqu'à [`PIXELS_VUE`], réduite au-delà (Lanczos), en PNG.
///
/// # Errors
/// Image illisible.
pub fn pour_la_vision(octets: &[u8]) -> Result<Vec<u8>, String> {
    let (l, h) = image::ImageReader::new(Cursor::new(octets))
        .with_guessed_format()
        .map_err(|e| format!("Image illisible : {e}"))?
        .into_dimensions()
        .map_err(|e| format!("Image illisible : {e}"))?;
    let pixels = u64::from(l) * u64::from(h);
    if pixels <= PIXELS_VUE {
        return Ok(octets.to_vec());
    }
    #[allow(clippy::cast_precision_loss)] // des dimensions d'image
    let f = (PIXELS_VUE as f64 / pixels as f64).sqrt();
    let image = ouvrir(octets)?;
    let reduite = imageops::resize(
        &image,
        en_u32(arrondi(f64::from(l) * f)).max(1),
        en_u32(arrondi(f64::from(h) * f)).max(1),
        FilterType::Lanczos3,
    );
    png(&reduite)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uni(l: u32, h: u32, couleur: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(l, h, Rgb(couleur))
    }

    #[test]
    fn ouvre_et_reencode() {
        let image = uni(20, 10, [200, 10, 10]);
        let octets = png(&image).unwrap();
        assert_eq!(&octets[..4], b"\x89PNG");
        assert_eq!(ouvrir(&octets).unwrap(), image);
        assert!(
            ouvrir(b"pas une image")
                .unwrap_err()
                .starts_with("Image illisible")
        );
    }

    #[test]
    fn recadrage_au_centre() {
        let mut image = uni(200, 100, [0, 0, 0]);
        image.put_pixel(100, 50, Rgb([255, 255, 255]));
        let carre = recadrer_centre(&image, "1:1 (Square)").unwrap();
        assert_eq!(carre.dimensions(), (100, 100));
        assert_eq!(carre.get_pixel(50, 50), &Rgb([255, 255, 255]));
        assert_eq!(
            recadrer_centre(&image, "9:16 (Portrait Widescreen)")
                .unwrap()
                .dimensions(),
            (56, 100)
        );
        assert_eq!(
            recadrer_centre(&image, "21:9 (Ultrawide)").unwrap().dimensions(),
            (200, 86)
        );
        assert_eq!(recadrer_centre(&image, "2:1").unwrap().dimensions(), (200, 100));
        assert!(recadrer_centre(&image, "carré").is_err());
        assert!(recadrer_centre(&image, "0:1").is_err());
    }

    #[test]
    fn base64_aller_retour() {
        for octets in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            let texte = base64(octets);
            assert_eq!(depuis_data_url(&texte).unwrap(), octets);
            assert_eq!(
                depuis_data_url(&format!("data:image/png;base64,{texte}")).unwrap(),
                octets
            );
        }
        assert_eq!(base64(b"Aria"), "QXJpYQ==");
        assert!(depuis_data_url("data:image/png;base64,@@@@").is_err());
        assert!(depuis_data_url("data:sans-virgule").is_err());
    }

    #[test]
    fn masque_peint() {
        // Un canevas transparent avec un carré opaque : sa transparence fait le masque.
        let mut canevas = image::RgbaImage::new(10, 10);
        for x in 2..6 {
            for y in 2..6 {
                canevas.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let mut octets = Cursor::new(Vec::new());
        canevas.write_to(&mut octets, ImageFormat::Png).unwrap();
        let masque = masque_net(octets.get_ref(), (20, 20)).unwrap().unwrap();
        assert_eq!(masque.dimensions(), (20, 20));
        assert_eq!(masque.get_pixel(8, 8)[0], 255);
        assert_eq!(masque.get_pixel(18, 18)[0], 0);
        assert!(masque.pixels().all(|p| p[0] == 0 || p[0] == 255));
        // Rien de peint : pas de masque.
        let vide = image::RgbaImage::new(10, 10);
        let mut octets = Cursor::new(Vec::new());
        vide.write_to(&mut octets, ImageFormat::Png).unwrap();
        assert!(masque_net(octets.get_ref(), (20, 20)).unwrap().is_none());
    }

    #[test]
    fn la_retouche_ne_touche_que_le_masque() {
        let original = uni(40, 40, [0, 0, 255]);
        let retouche = uni(40, 40, [255, 0, 0]);
        let mut masque = GrayImage::new(40, 40);
        for x in 10..20 {
            for y in 10..20 {
                masque.put_pixel(x, y, Luma([255]));
            }
        }
        let net = recoller(&original, &retouche, &masque, 0);
        assert_eq!(net.get_pixel(15, 15), &Rgb([255, 0, 0]));
        assert_eq!(net.get_pixel(30, 30), &Rgb([0, 0, 255]));
        let fondu = recoller(&original, &retouche, &masque, 8);
        assert_eq!(fondu.get_pixel(15, 15), &Rgb([255, 0, 0]));
        // Loin du masque, l'original reste strictement intact.
        assert_eq!(fondu.get_pixel(35, 35), &Rgb([0, 0, 255]));
        assert_eq!(fondu.get_pixel(0, 0), &Rgb([0, 0, 255]));
    }

    #[test]
    fn canevas_d_agrandissement() {
        let source = uni(10, 10, [10, 200, 10]);
        let (canevas, position) = canevas_agrandi(&source, 30, 20, "centre").unwrap();
        assert_eq!((canevas.dimensions(), position), ((30, 20), (10, 5)));
        assert_eq!(canevas.get_pixel(15, 10), &Rgb([10, 200, 10]));
        assert_eq!(canevas.get_pixel(0, 0), &GRIS);
        assert_eq!(
            canevas_agrandi(&source, 30, 20, "bas-droite").unwrap().1,
            (20, 10)
        );
        assert_eq!(canevas_agrandi(&source, 30, 20, "haut-gauche").unwrap().1, (0, 0));
        // Un demi s'arrondit au pair, comme Python : (15 - 10) × 0,5 = 2,5 → 2.
        assert_eq!(canevas_agrandi(&source, 15, 10, "centre").unwrap().1, (2, 0));
        assert!(canevas_agrandi(&source, 30, 20, "nulle-part").is_err());
        assert!(canevas_agrandi(&source, 8, 20, "centre").is_err());
        assert!(canevas_agrandi(&source, 10, 10, "centre").is_err());
        assert!(canevas_agrandi(&source, 5000, 20, "centre").is_err());
    }

    #[test]
    fn la_source_revient_intacte_dans_l_agrandissement() {
        let source = uni(10, 10, [100, 100, 100]);
        // Le modèle a décalé les teintes et rempli les bords.
        let resultat = uni(30, 10, [130, 130, 130]);
        let final_ = recoller_agrandissement(&source, (10, 0), &resultat, 4);
        assert_eq!(final_.dimensions(), (30, 10));
        // Au cœur de la source : l'original au pixel près.
        assert_eq!(final_.get_pixel(15, 5), &Rgb([100, 100, 100]));
        // Couleurs accordées : le décalage de teinte du modèle est corrigé hors de la source aussi.
        assert_eq!(final_.get_pixel(0, 5), &Rgb([100, 100, 100]));
    }

    #[test]
    fn image_reduite_pour_la_vision() {
        let petite = png(&uni(100, 100, [1, 2, 3])).unwrap();
        assert_eq!(pour_la_vision(&petite).unwrap(), petite);
        let grande = png(&uni(2048, 1024, [1, 2, 3])).unwrap();
        let reduite = ouvrir(&pour_la_vision(&grande).unwrap()).unwrap();
        assert_eq!(reduite.dimensions(), (1448, 724));
        assert!(pour_la_vision(b"rien").is_err());
    }
}
