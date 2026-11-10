// Onglet Retouche : retouche au pinceau et agrandissement d'un rush, puis comparaison avec l'original.

const ANCRAGES = ["haut-gauche", "haut", "haut-droite", "gauche", "centre", "droite", "bas-gauche", "bas", "bas-droite"];
const FLECHES = ["↖", "↑", "↗", "←", "•", "→", "↙", "↓", "↘"];
const FRACTIONS_ANCRAGE = { "haut-gauche": [0, 0], haut: [0.5, 0], "haut-droite": [1, 0], gauche: [0, 0.5], centre: [0.5, 0.5],
    droite: [1, 0.5], "bas-gauche": [0, 1], bas: [0.5, 1], "bas-droite": [1, 1] };
let modeRetouche = "retouche";
let sourceRetouche = null;
let masqueGarde = null;  // { id, image, fondu } : le masque peint attend le retour du Scénario (✍️ L'écrire au Scénario)
let gomme = false;
let ancrage = "centre";
let resultatRetouche = null;  // id du résultat affiché dans la comparaison
let resultatsRetouche = [];  // tous les résultats de la dernière commande (variantes)
let lanceeRetouche = null;  // ce qui a été lancé : image d'origine, mode, taille et ancrage de l'agrandissement
let retoucheEnCours = false;

const $r = (id) => document.getElementById(id);
const canevas = $r("canevas-masque");
const ctx = canevas.getContext("2d");

groupeBoutons("mode-retouche", "Action");  // web/commun.js : Retoucher │ Agrandir, le select reste la source
brancherNombre("nombre-retouche");

// Rushes : la même galerie rangée que le Plateau (web/galerie.js). Un clic sur une vignette la prend comme image à retoucher.
const rushesRetouche = creerGalerie({
    genre: "images",
    racine: $r("rushes-retouche"),
    mot: "image",
    vides: {
        tous: "Aucun rush pour l'instant : crée une image au Plateau, ou importe-en une.",
        non_ranges: "Tout est rangé, bravo !",
        archives: "Aucune archive.",
        corbeille: "La corbeille est vide.",
        dossier: "Dossier vide : glisse des vignettes sur son nom, ou choisis-le dans « 📥 Ranger dans » avant de lancer.",
    },
    vignette: (f, choisi) => `<img src="${urlImage(f.id)}" alt="" loading="lazy" draggable="true"
        data-creation="${echapper(f.id)}" class="${choisi ? "choisi" : ""}">`,
    ouvrir: (f) => prendreCommeSource(f),
    actions: (f) => [["Ouvrir la fiche", () => ouvrirFiche(f)]],
    destination: $r("destination-retouche"),
    cleDestination: "aria.destination.retouche",  // son propre choix, indépendant de celui du Plateau
    libelleAucun: "Comme l'image d'origine",
});

async function prendreCommeSource(f) {
    $r("resultat-retouche").hidden = true;
    await ouvrirRetouche(f, modeRetouche);
    window.scrollTo({ top: 0, behavior: "smooth" });
}

async function importerPourRetouche(fichier) {
    const f = await importerFichier(fichier, false, rushesRetouche);  // web/plateau.js
    if (f) await prendreCommeSource(f);
}
$r("importer-retouche").addEventListener("change", async (e) => {
    const fichier = e.target.files[0];
    if (!fichier) return;
    await importerPourRetouche(fichier);
    e.target.value = "";
});

function allerRetouche() {
    const onglet = document.querySelector('.onglet[data-vue="retouche"]');
    if (!onglet.classList.contains("actif")) onglet.click();
}

// Retouche : « Celui de l'image » (par défaut) ou un format, qui recadre l'image au centre avant la retouche.
function remplirFormatRetouche() {
    if (!optionsPlateau) return;
    remplirSelect("format-retouche", [[optionsPlateau.format_image, "Celui de l'image"],
        ...optionsPlateau.formats.map((f) => [f, f])], optionsPlateau.format_image);
}
const formatRetouche = () => {
    const v = $r("format-retouche").value;
    return !v || v === optionsPlateau?.format_image ? null : v;
};

// Ouvre l'atelier sur une image (fiche d'un rush), dans le mode demandé. Rend une promesse tenue quand l'image est
// chargée et le masque gardé repeint : une retouche lancée dans la foulée (Scénario) part avec lui.
function ouvrirRetouche(f, mode = modeRetouche) {
    sourceRetouche = f;
    modeRetouche = mode;
    if (typeof fermerFiche === "function" && ficheOuverte) fermerFiche();
    allerRetouche();
    $r("mode-retouche").value = mode;
    const agrandir = mode === "agrandissement";
    $r("titre-retouche").textContent = agrandir ? "Agrandir" : "Retoucher";
    $r("vide-retouche").hidden = true;
    $r("atelier-retouche").hidden = false;
    $r("changer-image-retouche").hidden = false;
    $r("outils-pinceau").hidden = agrandir;
    $r("outils-agrandir").hidden = !agrandir;
    canevas.hidden = agrandir;
    $r("consigne-retouche").placeholder = agrandir
        ? "Précision facultative, en français : « ajoute la mer à droite » (traduite par le LLM par défaut)"
        : "Ta consigne, telle qu'elle partira au modèle (en anglais) : « give her a red leather jacket »…";
    // Retouche : la consigne part telle quelle ; le Scénario l'écrit et l'optimise en voyant l'image.
    $r("entete-consigne-retouche").hidden = agrandir;
    $r("aide-consigne-retouche").textContent =
        "Consigne envoyée telle quelle. Pour la traduire et l'optimiser en voyant l'image :";
    $r("consigne-retouche").value = "";
    $r("fondu-retouche").value = agrandir ? 96 : 24;
    majFondu();
    remplirFormatRetouche();
    $r("format-retouche").value = optionsPlateau?.format_image ?? "";  // chaque image repart de son propre format
    $r("bloc-format-retouche").hidden = agrandir;
    $r("erreur-retouche").hidden = true;

    const img = $r("image-retouche");
    const garde = mode === "retouche" && masqueGarde?.id === f.id ? masqueGarde : null;
    masqueGarde = null;
    const pret = new Promise((fin) => {
        img.onload = () => {
            canevas.width = img.naturalWidth;
            canevas.height = img.naturalHeight;
            ctx.clearRect(0, 0, canevas.width, canevas.height);
            if (!garde) return fin();
            $r("fondu-retouche").value = garde.fondu;
            majFondu();
            const masque = new Image();
            masque.onload = () => {
                ctx.drawImage(masque, 0, 0);
                fin();
            };
            masque.onerror = () => fin();
            masque.src = garde.image;
        };
        img.onerror = () => fin();
    });
    img.src = urlImage(f.id);
    if (agrandir) {
        majFormatsAgrandir();
        $r("largeur-agrandir").value = Math.round(f.largeur * 1.3 / 8) * 8;
        $r("hauteur-agrandir").value = f.hauteur;
        choisirAncrage("centre");
    }
    return pret;
}

// Retoucher │ Agrandir : on garde l'image, on change d'outil.
$r("mode-retouche").addEventListener("change", () => {
    const mode = $r("mode-retouche").value;
    if (mode === modeRetouche) return;
    if (sourceRetouche) ouvrirRetouche(sourceRetouche, mode);
    else {
        modeRetouche = mode;
        $r("titre-retouche").textContent = mode === "agrandissement" ? "Agrandir" : "Retoucher";
    }
});

// Choisir l'image : le sélecteur de rushes du Montage (web/montage.js), avec l'import depuis le disque.
async function choisirImageRetouche() {
    ouvrirChoixRush(async (id) => prendreCommeSource(await api("GET", `/api/creations/${encodeURIComponent(id)}`)),
        modeRetouche === "agrandissement" ? "Quelle image agrandir ?" : "Quelle image retoucher ?");
}
["choisir-rush-retouche", "changer-image-retouche", "source-retouche"].forEach((id) =>
    $r(id).addEventListener("click", choisirImageRetouche));
$r("source-retouche").addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        choisirImageRetouche();
    }
});

const majFondu = () => {
    $r("valeur-fondu").textContent = `${$r("fondu-retouche").value} px`;
};
$r("fondu-retouche").addEventListener("input", majFondu);

// Pinceau : on peint en coordonnées de l'image réelle, quelle que soit la taille affichée.
let peinture = null;

function point(e) {
    const r = canevas.getBoundingClientRect();
    return [(e.clientX - r.left) * canevas.width / r.width, (e.clientY - r.top) * canevas.height / r.height];
}

function tracer(depuis, vers) {
    const echelle = canevas.width / canevas.getBoundingClientRect().width;
    ctx.globalCompositeOperation = gomme ? "destination-out" : "source-over";
    ctx.strokeStyle = "#7df9ff";  // --accent de la palette Aria
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.lineWidth = Number($r("taille-pinceau").value) * echelle;
    ctx.beginPath();
    ctx.moveTo(...depuis);
    ctx.lineTo(...vers);
    ctx.stroke();
}

canevas.addEventListener("pointerdown", (e) => {
    canevas.setPointerCapture(e.pointerId);
    peinture = point(e);
    tracer(peinture, peinture);
});
canevas.addEventListener("pointermove", (e) => {
    if (!peinture) return;
    const p = point(e);
    tracer(peinture, p);
    peinture = p;
});
["pointerup", "pointercancel"].forEach((t) => canevas.addEventListener(t, () => { peinture = null; }));

function choisirOutil(estGomme) {
    gomme = estGomme;
    $r("mode-pinceau").classList.toggle("actif", !gomme);
    $r("mode-gomme").classList.toggle("actif", gomme);
}
$r("mode-pinceau").addEventListener("click", () => choisirOutil(false));
$r("mode-gomme").addEventListener("click", () => choisirOutil(true));
$r("effacer-masque").addEventListener("click", () => ctx.clearRect(0, 0, canevas.width, canevas.height));

function masquePeint() {
    const pixels = ctx.getImageData(0, 0, canevas.width, canevas.height).data;
    for (let i = 3; i < pixels.length; i += 4) {
        if (pixels[i] > 0) return canevas.toDataURL("image/png");
    }
    return null;
}

// Agrandissement : ancrage et aperçu schématique.
$r("ancrages").innerHTML = ANCRAGES.map((a, i) =>
    `<button type="button" data-ancrage="${a}" title="${a}">${FLECHES[i]}</button>`).join("");
$r("ancrages").addEventListener("click", (e) => {
    if (e.target.dataset.ancrage) choisirAncrage(e.target.dataset.ancrage);
});
["largeur-agrandir", "hauteur-agrandir"].forEach((id) =>
    $r(id).addEventListener("input", () => {
        marquerFormat(null);  // une taille tapée à la main n'est plus un format
        choisirAncrage(ancrage);
    }));

// Formats : les mêmes que ceux du Plateau, en raccourcis qui remplissent largeur et hauteur. La taille choisie est la plus
// petite de ce format qui contienne l'image d'origine (multiples de 8) ; si l'image a déjà ce format, elle grandit de 25 %.
function majFormatsAgrandir() {
    const formats = (optionsPlateau?.formats ?? []).filter((f) => /^\d+:\d+/.test(f));
    $r("formats-agrandir").innerHTML = "<span>Format</span>" + formats.map((f) =>
        `<button type="button" class="choix" data-format="${echapper(f)}" title="${echapper(f)}">${echapper(f.split(" ")[0])}</button>`).join("");
}

function marquerFormat(format) {
    document.querySelectorAll("#formats-agrandir button").forEach((b) => b.classList.toggle("actif", b.dataset.format === format));
}

function tailleAuFormat(largeur0, hauteur0, ratioL, ratioH) {
    const r = ratioL / ratioH;
    const multiple8 = (v) => Math.ceil(v / 8) * 8;
    let [L, H] = r >= largeur0 / hauteur0 ? [multiple8(hauteur0 * r), hauteur0] : [largeur0, multiple8(largeur0 / r)];
    if (L === largeur0 && H === hauteur0) {
        L = multiple8(largeur0 * 1.25);
        H = multiple8(L / r);
    }
    return [L, H];
}

$r("formats-agrandir").addEventListener("click", (e) => {
    const format = e.target.closest("button")?.dataset.format;
    if (!format || !sourceRetouche) return;
    const [, rl, rh] = format.match(/^(\d+):(\d+)/);
    const [L, H] = tailleAuFormat(sourceRetouche.largeur, sourceRetouche.hauteur, Number(rl), Number(rh));
    $r("largeur-agrandir").value = L;
    $r("hauteur-agrandir").value = H;
    choisirAncrage(ancrage);
    marquerFormat(format);
});

function choisirAncrage(a) {
    ancrage = a;
    document.querySelectorAll("#ancrages button").forEach((b) => b.classList.toggle("actif", b.dataset.ancrage === a));
    const f = sourceRetouche;
    const L = Number($r("largeur-agrandir").value) || f.largeur;
    const H = Number($r("hauteur-agrandir").value) || f.hauteur;
    const echelle = Math.min(160 / L, 100 / H);
    const cadre = $r("apercu-cadre");
    cadre.style.width = `${L * echelle}px`;
    cadre.style.height = `${H * echelle}px`;
    const [fx, fy] = FRACTIONS_ANCRAGE[a];
    const image = $r("apercu-image");
    image.style.width = `${f.largeur * echelle}px`;
    image.style.height = `${f.hauteur * echelle}px`;
    image.style.left = `${Math.max(0, L - f.largeur) * fx * echelle}px`;
    image.style.top = `${Math.max(0, H - f.hauteur) * fy * echelle}px`;
}

// Lancer : la commande est suivie ici (barre, ✕ Annuler), puis le résultat s'affiche face à l'original.
$r("lancer-retouche").addEventListener("click", async () => {
    if (!sourceRetouche || retoucheEnCours) return;
    const consigne = $r("consigne-retouche").value;
    const agrandir = modeRetouche === "agrandissement";
    const commun = {
        source: sourceRetouche.id,
        // Retouche : la consigne du Scénario part telle quelle. Agrandissement : la précision est traduite.
        reecrire: agrandir,
        llm: null,
        ...choixModele("workflow-retouche"),
        fondu: Number($r("fondu-retouche").value),
        loras: lorasChoisies("loras-retouche"),
        seed: $r("seed-retouche").value === "" ? null : Number($r("seed-retouche").value),
        nombre: nombreDuChamp("nombre-retouche"),
        dossier: rushesRetouche.dossierCible(),  // null : le résultat reste dans le dossier de l'image d'origine
    };
    const lancee = {
        source: sourceRetouche, mode: modeRetouche, ancrage,
        largeur: Number($r("largeur-agrandir").value), hauteur: Number($r("hauteur-agrandir").value),
        modele: $r("workflow-retouche").selectedOptions[0]?.textContent.trim() ?? "",
    };
    const erreur = $r("erreur-retouche");
    erreur.hidden = true;
    let commande;
    try {
        commande = agrandir
            ? await api("POST", "/api/agrandissements", { ...commun, precision: consigne, ancrage,
                largeur: lancee.largeur, hauteur: lancee.hauteur })
            : await api("POST", "/api/retouches", { ...commun, instruction: consigne, masque: masquePeint(),
                format: formatRetouche() });
    } catch (err) {
        afficherErreur(erreur, err.message);
        return;
    }
    suivreRetouche(commande.id, lancee);
});

async function suivreRetouche(id, lancee) {
    const bloc = $r("progression-retouche");
    const debut = Date.now();
    const quoi = lancee.mode === "agrandissement" ? "Agrandissement" : "Retouche";
    retoucheEnCours = true;
    $r("lancer-retouche").disabled = true;
    bloc.hidden = false;
    bloc.querySelector(".barre div").style.width = "0%";
    suivreAnnulable(bloc, id);
    let c = null;
    try {
        c = await suivreJusquauBout(id, (r) => {
            const secondes = Math.round((Date.now() - debut) / 1000);
            bloc.querySelector(".barre div").style.width = `${Math.round(100 * r.progression)}%`;
            bloc.querySelector("span").textContent = r.etat === "attente"
                ? `En file d'attente… ${secondes} s`
                : `${r.etape || "Finalisation"} · ${Math.round(100 * r.progression)} % · ${secondes} s`;
        }, 700);
    } catch (err) {
        afficherErreur($r("erreur-retouche"),
            `Suivi interrompu (${err.message}) : la génération continue peut-être, regarde les rushes.`);
    } finally {
        retoucheEnCours = false;
        $r("lancer-retouche").disabled = false;
        bloc.hidden = true;
    }
    if (!c) return;
    chargerGalerie();  // web/plateau.js : le résultat rejoint les rushes
    if (c.etat === "erreur" || c.etat === "annule") {
        const message = c.etat === "annule" ? messageAnnulation(c, "image", true) : (c.erreur || "Le job a échoué.");
        const accord = lancee.mode === "agrandissement" ? "" : "e";
        notifierFin(`${quoi} ${c.etat === "annule" ? `annulé${accord}` : "en erreur"}`, message, "retouche", false);
        afficherErreur($r("erreur-retouche"), message);
        if (!c.images.length) return;
    } else {
        const n = c.images.length;
        notifierFin(`${quoi} terminé${lancee.mode === "agrandissement" ? "" : "e"}`,
            `${n} image${n > 1 ? "s" : ""} dans les rushes, prête${n > 1 ? "s" : ""} à comparer`, "retouche");
    }
    const duree = Math.round((Date.now() - debut) / 1000);
    afficherComparaison(lancee, c.images, c.prompt ?? "", `${quoi} · ${lancee.modele} · ${duree} s`);
}

// Comparaison : un curseur à glisser (par défaut) ou les deux images côte à côte.
function afficherComparaison(lancee, ids, consigne, infos) {
    lanceeRetouche = lancee;
    resultatsRetouche = ids;
    // Plusieurs variantes : une bande de vignettes pour passer de l'une à l'autre face à l'original.
    $r("variantes-retouche").hidden = ids.length < 2;
    $r("variantes-retouche").innerHTML = ids.length < 2 ? "" : ids.map((id, i) =>
        `<img src="${urlImage(id)}" alt="Variante ${i + 1}" data-variante="${i}" title="Variante ${i + 1}">`).join("");
    $r("consigne-resultat-retouche").textContent = consigne ? `Consigne envoyée au modèle : ${consigne}` : "";
    $r("consigne-resultat-retouche").hidden = !consigne;
    $r("infos-resultat-retouche").textContent = infos;
    montrerVariante(0);
    $r("resultat-retouche").hidden = false;
    $r("resultat-retouche").scrollIntoView({ behavior: "smooth", block: "start" });
}

$r("variantes-retouche").addEventListener("click", (e) => {
    const i = e.target.closest("[data-variante]")?.dataset.variante;
    if (i !== undefined) montrerVariante(Number(i));
});

function montrerVariante(i) {
    const lancee = lanceeRetouche;
    const idResultat = resultatsRetouche[i];
    resultatRetouche = idResultat;
    document.querySelectorAll("#variantes-retouche img").forEach((img, j) => img.classList.toggle("choisi", j === i));
    const avant = urlImage(lancee.source.id);
    const apres = urlImage(idResultat);
    $r("avant-curseur").src = avant;
    $r("avant-cote").src = avant;
    $r("apres-curseur").src = apres;
    $r("apres-cote").src = apres;
    // Agrandissement : l'original n'occupe qu'une partie du cadre agrandi, à l'endroit de son ancrage.
    const img = $r("avant-curseur");
    if (lancee.mode === "agrandissement" && lancee.largeur && lancee.hauteur) {
        const [fx, fy] = FRACTIONS_ANCRAGE[lancee.ancrage] ?? [0.5, 0.5];
        const w = lancee.source.largeur / lancee.largeur;
        const h = lancee.source.hauteur / lancee.hauteur;
        Object.assign(img.style, { left: `${(1 - w) * fx * 100}%`, top: `${(1 - h) * fy * 100}%`,
            width: `${w * 100}%`, height: `${h * 100}%` });
    } else {
        Object.assign(img.style, { left: "0", top: "0", width: "100%", height: "100%" });
    }
    placerCurseur(50);
    const lien = $r("telecharger-retouche");
    lien.href = apres;
    lien.download = `aria-${idResultat}.png`;
}

function placerCurseur(valeur) {
    $r("curseur-comparaison").value = valeur;
    $r("comparaison-curseur").style.setProperty("--position", `${valeur}%`);
}
$r("curseur-comparaison").addEventListener("input", (e) => placerCurseur(e.target.value));

let coteACote = false;
$r("bascule-comparaison").addEventListener("click", () => {
    coteACote = !coteACote;
    $r("comparaison-curseur").hidden = coteACote;
    $r("comparaison-cote").hidden = !coteACote;
    $r("aide-curseur").hidden = coteACote;
    $r("bascule-comparaison").textContent = coteACote ? "Passer au curseur" : "Passer en côte à côte";
});

// Le résultat devient l'image de départ : on enchaîne les retouches sans repasser par les rushes.
$r("repartir-retouche").addEventListener("click", async () => {
    if (!resultatRetouche) return;
    const f = await api("GET", `/api/creations/${encodeURIComponent(resultatRetouche)}`);
    $r("resultat-retouche").hidden = true;
    await ouvrirRetouche(f, modeRetouche);
    window.scrollTo({ top: 0, behavior: "smooth" });
});
$r("fiche-resultat-retouche").addEventListener("click", async () => {
    if (resultatRetouche) ouvrirFiche(await api("GET", `/api/creations/${encodeURIComponent(resultatRetouche)}`));
});

$r("vers-scenario-retouche").addEventListener("click", () => {
    const id = sourceRetouche.id;
    masqueGarde = { id, image: canevas.toDataURL(), fondu: $r("fondu-retouche").value };
    ouvrirScenario("retouche", { retoucher: id });
});

// Le Scénario envoie une consigne prête : l'onglet s'ouvre sur l'image, consigne posée (masque et fondu ici).
async function recevoirRetouche({ prompt, images, workflow, lancer, format = null }) {
    await chargerOptionsPlateau();
    await ouvrirRetouche(await api("GET", `/api/creations/${encodeURIComponent(images[0])}`), "retouche");
    $r("consigne-retouche").value = prompt;
    if (format) $r("format-retouche").value = format;  // choisi au Scénario ; sinon « Celui de l'image »
    const select = $r("workflow-retouche");
    const ligne = [...select.options].find((o) => o.value.startsWith(`${workflow}::`));
    if (ligne && choixModele("workflow-retouche").workflow !== workflow) {
        select.value = ligne.value;
        select.dispatchEvent(new Event("change"));
    }
    if (lancer) $r("lancer-retouche").click();
}
$r("ouvrir-retouche").addEventListener("click", () => ouvrirRetouche(ficheOuverte, "retouche"));
$r("ouvrir-agrandir").addEventListener("click", () => ouvrirRetouche(ficheOuverte, "agrandissement"));
