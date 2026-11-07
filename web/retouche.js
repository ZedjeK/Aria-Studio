// Retouche au pinceau et agrandissement

const ANCRAGES = ["haut-gauche", "haut", "haut-droite", "gauche", "centre", "droite", "bas-gauche", "bas", "bas-droite"];
const FLECHES = ["↖", "↑", "↗", "←", "•", "→", "↙", "↓", "↘"];
let modeRetouche = "retouche";
let sourceRetouche = null;
let masqueGarde = null;  // { id, image, fondu } : le masque peint attend le retour du Scénario (✍️ L'écrire au Scénario)
let gomme = false;
let ancrage = "centre";

const canevas = document.getElementById("canevas-masque");
const ctx = canevas.getContext("2d");

function ouvrirRetouche(f, mode) {
    sourceRetouche = f;
    modeRetouche = mode;
    fermerFiche();
    const agrandir = mode === "agrandissement";
    document.getElementById("titre-retouche").textContent = agrandir ? "Agrandir" : "Retoucher";
    document.getElementById("outils-pinceau").hidden = agrandir;
    document.getElementById("outils-agrandir").hidden = !agrandir;
    canevas.hidden = agrandir;
    document.getElementById("consigne-retouche").placeholder = agrandir
        ? "Précision facultative, en français : « ajoute la mer à droite » (traduite par le LLM par défaut)"
        : "Ta consigne, telle qu'elle partira au modèle (en anglais) : « give her a red leather jacket »…";
    // Retouche : la consigne part telle quelle ; le Scénario l'écrit et l'optimise en voyant l'image.
    document.getElementById("entete-consigne-retouche").hidden = agrandir;
    document.getElementById("aide-consigne-retouche").textContent =
        "Consigne envoyée telle quelle. Pour la traduire et l'optimiser en voyant l'image :";
    document.getElementById("consigne-retouche").value = "";
    document.getElementById("fondu-retouche").value = agrandir ? 96 : 24;
    majFondu();
    document.getElementById("erreur-retouche").hidden = true;

    // Prête quand l'image est chargée et le masque gardé (même image) repeint : une retouche lancée dans la foulée
    // (Scénario, « Retoucher ») part avec lui.
    const img = document.getElementById("image-retouche");
    const garde = mode === "retouche" && masqueGarde?.id === f.id ? masqueGarde : null;
    masqueGarde = null;
    const pret = new Promise((fin) => {
        img.onload = () => {
            canevas.width = img.naturalWidth;
            canevas.height = img.naturalHeight;
            ctx.clearRect(0, 0, canevas.width, canevas.height);
            if (!garde) return fin();
            document.getElementById("fondu-retouche").value = garde.fondu;
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
        document.getElementById("largeur-agrandir").value = Math.round(f.largeur * 1.3 / 8) * 8;
        document.getElementById("hauteur-agrandir").value = f.hauteur;
        choisirAncrage("centre");
    }
    document.getElementById("panneau-retouche").hidden = false;
    return pret;
}

const majFondu = () => {
    document.getElementById("valeur-fondu").textContent = `${document.getElementById("fondu-retouche").value} px`;
};
document.getElementById("fondu-retouche").addEventListener("input", majFondu);

function fermerRetouche() {
    document.getElementById("panneau-retouche").hidden = true;
}

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
    ctx.lineWidth = Number(document.getElementById("taille-pinceau").value) * echelle;
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
    document.getElementById("mode-pinceau").classList.toggle("actif", !gomme);
    document.getElementById("mode-gomme").classList.toggle("actif", gomme);
}
document.getElementById("mode-pinceau").addEventListener("click", () => choisirOutil(false));
document.getElementById("mode-gomme").addEventListener("click", () => choisirOutil(true));
document.getElementById("effacer-masque").addEventListener("click", () => ctx.clearRect(0, 0, canevas.width, canevas.height));

function masquePeint() {
    const pixels = ctx.getImageData(0, 0, canevas.width, canevas.height).data;
    for (let i = 3; i < pixels.length; i += 4) {
        if (pixels[i] > 0) return canevas.toDataURL("image/png");
    }
    return null;
}

// Agrandissement : ancrage et aperçu schématique.
document.getElementById("ancrages").innerHTML = ANCRAGES.map((a, i) =>
    `<button type="button" data-ancrage="${a}" title="${a}">${FLECHES[i]}</button>`).join("");
document.getElementById("ancrages").addEventListener("click", (e) => {
    if (e.target.dataset.ancrage) choisirAncrage(e.target.dataset.ancrage);
});
["largeur-agrandir", "hauteur-agrandir"].forEach((id) =>
    document.getElementById(id).addEventListener("input", () => choisirAncrage(ancrage)));

function choisirAncrage(a) {
    ancrage = a;
    document.querySelectorAll("#ancrages button").forEach((b) => b.classList.toggle("actif", b.dataset.ancrage === a));
    const f = sourceRetouche;
    const L = Number(document.getElementById("largeur-agrandir").value) || f.largeur;
    const H = Number(document.getElementById("hauteur-agrandir").value) || f.hauteur;
    const echelle = Math.min(160 / L, 100 / H);
    const cadre = document.getElementById("apercu-cadre");
    cadre.style.width = `${L * echelle}px`;
    cadre.style.height = `${H * echelle}px`;
    const [fx, fy] = { "haut-gauche": [0, 0], haut: [0.5, 0], "haut-droite": [1, 0], gauche: [0, 0.5], centre: [0.5, 0.5],
        droite: [1, 0.5], "bas-gauche": [0, 1], bas: [0.5, 1], "bas-droite": [1, 1] }[a];
    const image = document.getElementById("apercu-image");
    image.style.width = `${f.largeur * echelle}px`;
    image.style.height = `${f.hauteur * echelle}px`;
    image.style.left = `${Math.max(0, L - f.largeur) * fx * echelle}px`;
    image.style.top = `${Math.max(0, H - f.hauteur) * fy * echelle}px`;
}

document.getElementById("lancer-retouche").addEventListener("click", async () => {
    const consigne = document.getElementById("consigne-retouche").value;
    const commun = {
        source: sourceRetouche.id,
        // Retouche : la consigne du Scénario part telle quelle. Agrandissement : la précision est traduite.
        reecrire: modeRetouche === "agrandissement",
        llm: null,
        ...choixModele("workflow-retouche"),
        fondu: Number(document.getElementById("fondu-retouche").value),
        loras: lorasChoisies("loras-retouche"),
    };
    const ok = modeRetouche === "agrandissement"
        ? await lancerCommande({
            ...commun, precision: consigne, ancrage,
            largeur: Number(document.getElementById("largeur-agrandir").value),
            hauteur: Number(document.getElementById("hauteur-agrandir").value),
        }, "/api/agrandissements", document.getElementById("erreur-retouche"))
        : await lancerCommande({ ...commun, instruction: consigne, masque: masquePeint() },
            "/api/retouches", document.getElementById("erreur-retouche"));
    if (ok) {
        fermerRetouche();
        window.scrollTo({ top: 0, behavior: "smooth" });
    }
});

document.getElementById("vers-scenario-retouche").addEventListener("click", () => {
    const id = sourceRetouche.id;
    masqueGarde = { id, image: canevas.toDataURL(), fondu: document.getElementById("fondu-retouche").value };
    fermerRetouche();
    ouvrirScenario("retouche", { retoucher: id });
});

// Le Scénario envoie une consigne prête : le panneau s'ouvre sur l'image, consigne posée (masque et fondu ici).
async function recevoirRetouche({ prompt, images, workflow, lancer }) {
    await chargerOptionsPlateau();
    document.querySelector('.onglet[data-vue="plateau"]').click();
    await ouvrirRetouche(await api("GET", `/api/creations/${encodeURIComponent(images[0])}`), "retouche");
    document.getElementById("consigne-retouche").value = prompt;
    const select = document.getElementById("workflow-retouche");
    const ligne = [...select.options].find((o) => o.value.startsWith(`${workflow}::`));
    if (ligne && choixModele("workflow-retouche").workflow !== workflow) {
        select.value = ligne.value;
        select.dispatchEvent(new Event("change"));
    }
    if (lancer) document.getElementById("lancer-retouche").click();
}
document.getElementById("ouvrir-retouche").addEventListener("click", () => ouvrirRetouche(ficheOuverte, "retouche"));
document.getElementById("ouvrir-agrandir").addEventListener("click", () => ouvrirRetouche(ficheOuverte, "agrandissement"));
document.getElementById("fermer-retouche").addEventListener("click", fermerRetouche);
