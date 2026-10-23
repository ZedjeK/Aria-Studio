"""Traitements d'image autour de la retouche : masque recollé, canevas d'agrandissement.

Le modèle de retouche réinterprète toute l'image. Pour ne modifier que la zone voulue,
on remet ensuite l'original au pixel près partout ailleurs, avec un fondu sur le bord.
"""

import base64
import io

from PIL import Image, ImageFilter, ImageOps, ImageStat

ANCRAGES = {
    "centre": (0.5, 0.5), "gauche": (0.0, 0.5), "droite": (1.0, 0.5), "haut": (0.5, 0.0), "bas": (0.5, 1.0),
    "haut-gauche": (0.0, 0.0), "haut-droite": (1.0, 0.0), "bas-gauche": (0.0, 1.0), "bas-droite": (1.0, 1.0),
}
TAILLE_MAX = 4096


class ErreurRetouche(ValueError):
    pass


def ouvrir(donnees: bytes) -> Image.Image:
    try:
        image = Image.open(io.BytesIO(donnees))
        image.load()
    except Exception as e:
        raise ErreurRetouche(f"Image illisible : {e}")
    return ImageOps.exif_transpose(image).convert("RGB")


def png(image: Image.Image) -> bytes:
    tampon = io.BytesIO()
    image.save(tampon, format="PNG")
    return tampon.getvalue()


def depuis_data_url(data_url: str) -> bytes:
    """« data:image/png;base64,... » (envoyé par le navigateur) -> octets."""
    try:
        return base64.b64decode(data_url.split(",", 1)[1] if data_url.startswith("data:") else data_url)
    except Exception as e:
        raise ErreurRetouche(f"Données d'image invalides : {e}")


def masque_net(donnees: bytes, taille: tuple[int, int]) -> Image.Image | None:
    """Masque peint dans l'interface : blanc = zone à retoucher. None si rien n'est peint."""
    masque = Image.open(io.BytesIO(donnees))
    if masque.mode in ("RGBA", "LA"):
        masque = masque.getchannel("A")  # le canevas du navigateur peint en opaque sur fond transparent
    masque = masque.convert("L").resize(taille, Image.Resampling.BILINEAR)
    masque = masque.point(lambda v: 255 if v > 127 else 0)
    return masque if masque.getbbox() else None


def recoller(original: Image.Image, retouche: Image.Image, masque: Image.Image, fondu: int) -> Image.Image:
    """Garde l'original hors du masque ; la zone retouchée se fond sur `fondu` pixels."""
    retouche = retouche.resize(original.size, Image.Resampling.LANCZOS)
    if fondu > 0:
        # On élargit d'abord la zone pour que le fondu mange l'original, pas la retouche.
        masque = masque.filter(ImageFilter.MaxFilter(2 * (fondu // 2) + 1)).filter(ImageFilter.GaussianBlur(fondu / 8))
        masque = _seuiller(masque)
    return Image.composite(retouche, original, masque)


def _seuiller(masque: Image.Image) -> Image.Image:
    """Coupe les traînes du flou gaussien : hors du fondu, l'original reste strictement intact."""
    return masque.point(lambda v: 0 if v < 6 else 255 if v > 249 else v)


def canevas_agrandi(source: Image.Image, largeur: int, hauteur: int, ancrage: str) -> tuple[Image.Image, tuple[int, int]]:
    """Agrandit le canevas : la source est placée selon l'ancrage, le reste est un prolongement flou
    de l'image (meilleur point de départ qu'un aplat pour le modèle). Renvoie le canevas et la position de la source."""
    if ancrage not in ANCRAGES:
        raise ErreurRetouche(f"Ancrage inconnu : {ancrage}")
    if largeur < source.width or hauteur < source.height:
        raise ErreurRetouche(f"La nouvelle taille doit contenir l'image d'origine ({source.width}×{source.height})")
    if (largeur, hauteur) == source.size:
        raise ErreurRetouche("La nouvelle taille est identique à l'originale")
    if max(largeur, hauteur) > TAILLE_MAX:
        raise ErreurRetouche(f"Taille maximale : {TAILLE_MAX} px de côté")
    fx, fy = ANCRAGES[ancrage]
    x, y = round((largeur - source.width) * fx), round((hauteur - source.height) * fy)
    if REMPLISSAGE == "gris":
        fond = Image.new("RGB", (largeur, hauteur), (128, 128, 128))
        fond.paste(source, (x, y))
        return fond, (x, y)
    return _prolonger_bords(source, largeur, hauteur, x, y), (x, y)


# « gris » : aplat neutre que le modèle remplace ; « bords » : bords étirés et floutés (le modèle
# les garde parfois tels quels, comme un flou d'arrière-plan voulu).
REMPLISSAGE = "gris"


def _prolonger_bords(source: Image.Image, largeur: int, hauteur: int, x: int, y: int) -> Image.Image:
    """Remplit la zone ajoutée en étirant les quelques pixels du bord de la source, puis floute.
    Étirer toute l'image ferait apparaître des silhouettes fantômes que le modèle redessinerait."""
    w, h, bord = source.width, source.height, 4
    fond = Image.new("RGB", (largeur, hauteur))
    droite, bas = largeur - x - w, hauteur - y - h
    etirer = lambda boite, taille: source.crop(boite).resize(taille, Image.Resampling.BILINEAR)  # noqa: E731
    if x:
        fond.paste(etirer((0, 0, bord, h), (x, h)), (0, y))
    if droite:
        fond.paste(etirer((w - bord, 0, w, h), (droite, h)), (x + w, y))
    if y:
        fond.paste(etirer((0, 0, w, bord), (w, y)), (x, 0))
    if bas:
        fond.paste(etirer((0, h - bord, w, h), (w, bas)), (x, y + h))
    # Coins : la couleur du coin correspondant de la source.
    for cx, cy, lx, ly, px, py in ((0, 0, x, y, 0, 0), (w - 1, 0, droite, y, x + w, 0),
                                   (0, h - 1, x, bas, 0, y + h), (w - 1, h - 1, droite, bas, x + w, y + h)):
        if lx and ly:
            fond.paste(source.getpixel((cx, cy)), (px, py, px + lx, py + ly))
    fond = fond.filter(ImageFilter.GaussianBlur(24))
    fond.paste(source, (x, y))
    return fond


def _accorder_couleurs(resultat: Image.Image, reference: Image.Image, zone: tuple[int, int, int, int]) -> Image.Image:
    """Le modèle régénère toute la scène et décale souvent les teintes : on aligne moyenne et contraste
    de chaque canal du résultat sur l'original, mesurés sur la zone qu'ils ont en commun."""
    genere = ImageStat.Stat(resultat.crop(zone))
    original = ImageStat.Stat(reference)
    canaux = []
    for c, canal in enumerate(resultat.split()):
        gain = original.stddev[c] / genere.stddev[c] if genere.stddev[c] > 1 else 1.0
        gain = max(0.7, min(gain, 1.4))  # garde-fou : pas de correction extrême
        decalage = original.mean[c] - genere.mean[c] * gain
        canaux.append(canal.point(lambda v, g=gain, d=decalage: max(0, min(255, round(v * g + d)))))
    return Image.merge("RGB", canaux)


def recoller_agrandissement(source: Image.Image, position: tuple[int, int], resultat: Image.Image, fondu: int) -> Image.Image:
    """Remet la source intacte à sa place dans l'image agrandie générée, avec un fondu vers l'intérieur."""
    resultat = _accorder_couleurs(resultat, source, (position[0], position[1],
                                                     position[0] + source.width, position[1] + source.height))
    masque_source = Image.new("L", resultat.size, 0)
    # Le fondu occupe une bande de `fondu` pixels, seulement sur les bords de la source qui touchent
    # une zone ajoutée ; un bord collé au bord du canevas reste net (rien à raccorder).
    x, y = position
    demi = fondu // 2
    gauche = demi if x > 0 else -fondu
    haut = demi if y > 0 else -fondu
    droite = demi if x + source.width < resultat.width else -fondu
    bas = demi if y + source.height < resultat.height else -fondu
    masque_source.paste(255, (x + gauche, y + haut, x + source.width - droite, y + source.height - bas))
    if fondu > 0:
        masque_source = _seuiller(masque_source.filter(ImageFilter.GaussianBlur(fondu / 6)))
    original_place = resultat.copy()
    original_place.paste(source, position)
    return Image.composite(original_place, resultat, masque_source)
