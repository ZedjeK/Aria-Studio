// Montage : tourner des clips avec le son (MiniMax H3), en animant un rush ou à partir d'un texte,
// et les Bobines, rangées comme les Rushes (mêmes dossiers, web/galerie.js).

const urlVideo = (id) => `/api/creations/${encodeURIComponent(id)}/video`;
let optionsMontage = null;
let sourceMontage = null;  // rush à animer
let modeMontage = "animer";
let commandeMontage = null;
let clipOuvert = null;

const bobines = creerGalerie({
    genre: "clips",
    racine: document.getElementById("bobines-montage"),
    mot: "clip",
    vides: {
        tous: "Aucun clip pour l'instant : silence, on tourne !",
        non_ranges: "Tout est rangé, bravo !",
        archives: "Aucune archive.",
        corbeille: "La corbeille est vide.",
        dossier: "Dossier vide : tourne depuis ce dossier, ou glisse des vignettes sur son nom.",
    },
    // Vignette = première image ; la vidéo se lance au survol.
    vignette: (f, choisi) => `<video src="${urlVideo(f.id)}" poster="${urlImage(f.id)}" muted loop playsinline
        preload="none" draggable="true" data-creation="${echapper(f.id)}" class="${choisi ? "choisi" : ""}"></video>`,
    ouvrir: (f) => ouvrirClip(f),
});

for (const conteneur of [document.querySelector("#bobines-montage [data-role=vignettes]"), document.getElementById("clips-montage")]) {
    conteneur.addEventListener("mouseover", (e) => { if (e.target.tagName === "VIDEO") e.target.play().catch(() => {}); });
    conteneur.addEventListener("mouseout", (e) => { if (e.target.tagName === "VIDEO") e.target.pause(); });
}

// Checkpoints de MiniMax H3 (image et texte vers vidéo partagent la famille) : [[fichier, libellé]] et le défaut
// réglé dans l'onglet Réglages.
function checkpointsVideo() {
    const t = optionsMontage.workflows.image_vers_video;
    const w = t.choix.find((x) => x.id === t.defaut) ?? t.choix[0];
    if (!w) return { lignes: [], defaut: null };
    return {
        lignes: w.modeles.map((m) => [m, `${m.replace(/\.safetensors$/, "")}${m === w.origine ? " (origine)" : ""}`
                                          + `${m === w.modele_defaut ? " ★" : ""}`]),
        defaut: w.modele_defaut,
    };
}

async function preparerMontage() {
    optionsMontage = await api("GET", "/api/plateau/options");
    const v = optionsMontage.video;
    const ck = checkpointsVideo();
    remplirSelect("modele-montage", ck.lignes, ck.defaut);
    remplirSelect("qualite-montage", v.qualites.map((q) => [q.cle, `${q.nom} · ${q.megapixels} MP · ${q.steps} steps`]),
        v.defaut_qualite);
    remplirSelect("duree-montage", v.durees.map((d) => [String(d), `${d} s`]), "5");
    remplirSelect("format-montage", optionsMontage.formats.map((f) => [f, f]), "16:9 (Widescreen)");
    remplirSelect("nombre-montage", Array.from({ length: v.nombre_max }, (_, i) => [String(i + 1), String(i + 1)]), "1");
    rendreSource();
    majEstimation();
    await bobines.charger();
}

// Estimation d'après les mesures sur la RTX 4080 (5 s) : la durée coûte un peu plus que proportionnellement.
function estimationSecondes() {
    const q = optionsMontage?.video.qualites.find((x) => x.cle === document.getElementById("qualite-montage").value);
    if (!q) return 0;
    const duree = Number(document.getElementById("duree-montage").value);
    return q.estimation_s * (duree / 5) ** 1.25 * Number(document.getElementById("nombre-montage").value);
}

const enMinutes = (s) => (s >= 60 ? `${Math.floor(s / 60)} min ${String(Math.round(s % 60)).padStart(2, "0")}` : `${Math.round(s)} s`);

function majEstimation() {
    document.getElementById("estimation-montage").textContent = `≈ ${enMinutes(estimationSecondes())} sur ta carte`
        + " (à chaud ; le premier clip charge le modèle)";
}
["qualite-montage", "duree-montage", "nombre-montage"].forEach((id) =>
    document.getElementById(id).addEventListener("change", majEstimation));

// Mode et source

// La séquence découpe avec le LLM du texte vers vidéo (onglet Modèles, Vidéo) ; le clip seul n'en a plus.
function suivreLlmMontage() {
    if (!optionsMontage) return;
    const llms = optionsMontage.llm.choix.map((m) => [m.fichier, m.favori ? `${m.nom} ★` : m.nom]);
    remplirSelect("llm-sequence", llms, optionsMontage.llm.defaut);  // un LLM qui vient d'arriver y figure
    document.getElementById("llm-sequence").value = llmDuWorkflow(optionsMontage.video.workflows.texte, optionsMontage);
}

document.querySelectorAll('input[name="mode-montage"]').forEach((radio) => {
    radio.addEventListener("change", () => {
        modeMontage = radio.value;
        suivreLlmMontage();
        document.getElementById("bloc-source-montage").hidden = modeMontage !== "animer";
        document.getElementById("bloc-format-montage").hidden = modeMontage !== "texte";
        // Séquence : le formulaire du clip seul s'efface au profit du panneau de la séquence (sequence.js).
        const enSequence = modeMontage === "sequence";
        document.getElementById("form-montage").closest(".carte").classList.toggle("mode-sequence", enSequence);
        document.getElementById("sequence-montage").hidden = !enSequence;
        if (enSequence) preparerSequence();
    });
});

function rendreSource() {
    const case_ = document.getElementById("source-montage");
    case_.className = `emplacement ${sourceMontage ? "plein" : "vide"}`;
    case_.innerHTML = sourceMontage
        ? `<img src="${urlImage(sourceMontage)}" alt="" draggable="false">
           <button type="button" class="petit retirer" data-retirer="1" title="Retirer">✕</button>`
        : '<span class="numero">＋</span>';
}

// Le Scénario envoie un prompt prêt : il part tel quel, sur le rush à animer s'il y en a un, sinon du texte seul.
async function recevoirAuMontage({ prompt, images, role, duree, lancer }) {
    if (!optionsMontage) await preparerMontage();  // remplit les listes du Montage avant d'y poser nos valeurs
    const animer = role === "animer";
    document.querySelector(`input[name="mode-montage"][value="${animer ? "animer" : "texte"}"]`).click();
    if (animer) {
        sourceMontage = images[0];
        rendreSource();
    }
    document.getElementById("prompt-clip").value = prompt;
    const choixDuree = document.getElementById("duree-montage");
    choixDuree.value = String(duree);
    choixDuree.dispatchEvent(new Event("change"));
    document.querySelector('.onglet[data-vue="montage"]').click();
    window.scrollTo({ top: 0, behavior: "smooth" });
    if (lancer) document.getElementById("form-montage").requestSubmit();
    else document.getElementById("prompt-clip").focus();
}

// « 🎞️ Animer » d'un rush : le prompt du clip s'écrit au Scénario, qui voit le rush, puis revient ici.
function animerRush(id) {
    ouvrirScenario("clip", { animer: id });
}

document.getElementById("vers-scenario-montage").addEventListener("click", () =>
    ouvrirScenario("clip", { animer: modeMontage === "animer" ? sourceMontage : null }));

document.getElementById("source-montage").addEventListener("click", (e) => {
    if (e.target.closest("[data-retirer]")) {
        sourceMontage = null;
        rendreSource();
    } else if (!sourceMontage) {
        ouvrirChoixRush();
    }
});
document.getElementById("choisir-rush").addEventListener("click", () => ouvrirChoixRush());

// Le sélecteur sert au rush à animer, et au mode Séquence (personnage, rush de départ ou d'arrivée d'un plan).
const choisirSource = (id) => { sourceMontage = id; rendreSource(); };
let quandRushChoisi = choisirSource;

async function ouvrirChoixRush(quandChoisi = choisirSource, titre = "Quel rush animer ?") {
    quandRushChoisi = quandChoisi;
    document.getElementById("titre-choix-rush").textContent = titre;
    const liste = await api("GET", "/api/creations?genre=images");
    document.getElementById("liste-choix-rush").innerHTML = liste.length
        ? liste.map((f) => `<img src="${urlImage(f.id)}" alt="" loading="lazy" data-choix="${echapper(f.id)}">`).join("")
        + '<button type="button" id="depuis-disque" class="petit">Une image de mon disque…</button>'
        : '<p class="discret">Aucun rush : crée une image sur le Plateau, ou prends-en une sur ton disque.</p>'
          + '<button type="button" id="depuis-disque" class="petit">Une image de mon disque…</button>';
    document.getElementById("choix-rush").hidden = false;
}

document.getElementById("liste-choix-rush").addEventListener("click", (e) => {
    if (e.target.id === "depuis-disque") {
        document.getElementById("fichier-montage").click();
        return;
    }
    const id = e.target.dataset?.choix;
    if (!id) return;
    document.getElementById("choix-rush").hidden = true;
    quandRushChoisi(id);
});
document.getElementById("fermer-choix-rush").addEventListener("click", () => { document.getElementById("choix-rush").hidden = true; });

document.getElementById("fichier-montage").addEventListener("change", async (e) => {
    const fichier = e.target.files[0];
    if (!fichier) return;
    const f = await importerFichier(fichier, false);  // l'image rejoint les rushes, puis devient le choix
    if (f) {
        document.getElementById("choix-rush").hidden = true;
        quandRushChoisi(f.id);
    }
    e.target.value = "";
});

// Glisser une image du disque sur la case source (sans passer par l'import global du Plateau).
const caseSource = document.getElementById("source-montage");
caseSource.addEventListener("dragover", (e) => {
    if (!e.dataTransfer?.types.includes("Files")) return;
    e.preventDefault();
    caseSource.classList.add("survol");
});
caseSource.addEventListener("dragleave", () => caseSource.classList.remove("survol"));
caseSource.addEventListener("drop", async (e) => {
    e.preventDefault();
    caseSource.classList.remove("survol");
    const fichier = [...e.dataTransfer.files].find((f) => TYPES_IMPORT.includes(f.type));
    if (!fichier) {
        alert("Dépose une image PNG, JPEG ou WebP.");
        return;
    }
    const f = await importerFichier(fichier, false);
    if (f) {
        sourceMontage = f.id;
        rendreSource();
    }
});

// Tournage

function reglagesMontage() {
    const seed = document.getElementById("seed-montage").value;
    return {
        mode: modeMontage,
        source: modeMontage === "animer" ? sourceMontage : null,
        qualite: document.getElementById("qualite-montage").value,
        modele: document.getElementById("modele-montage").value || null,
        duree: Number(document.getElementById("duree-montage").value),
        format: document.getElementById("format-montage").value,
        nombre: Number(document.getElementById("nombre-montage").value),
        seed: seed === "" ? null : Number(seed),
        dossier: bobines.dossierOuvert(),  // tourné depuis un dossier ouvert : tous les clips y sont rangés
    };
}

async function tourner(corps) {
    const erreur = document.getElementById("erreur-montage");
    erreur.hidden = true;
    try {
        commandeMontage = await api("POST", "/api/clips", corps);
    } catch (err) {
        erreur.textContent = err.message;
        erreur.hidden = false;
        return;
    }
    document.querySelectorAll("#form-montage button").forEach((b) => { b.disabled = true; });
    document.getElementById("progression-montage").hidden = false;
    suivreAnnulable(document.getElementById("progression-montage"), commandeMontage.id);
    suivreMontage(Date.now(), estimationSecondes());
}

// Le prompt part tel quel : le LLM n'écrit qu'au Scénario.
document.getElementById("form-montage").addEventListener("submit", (e) => {
    e.preventDefault();
    tourner({ prompt: document.getElementById("prompt-clip").value.trim(), ...reglagesMontage() });
});

async function suivreMontage(debut, estimation) {
    const c = await api("GET", `/api/commandes/${commandeMontage.id}`);
    const bloc = document.getElementById("progression-montage");
    const ecoule = (Date.now() - debut) / 1000;
    bloc.querySelector(".barre div").style.width = `${Math.round(100 * c.progression)}%`;
    const reste = Math.max(0, estimation - ecoule);
    bloc.querySelector("span").textContent = c.etat === "attente"
        ? `En file d'attente… ${enMinutes(ecoule)}`
        : `${c.etape || "Finalisation"} · ${Math.round(100 * c.progression)} % · ${enMinutes(ecoule)}`
          + (reste > 0 ? ` · encore ≈ ${enMinutes(reste)}` : "");
    if (["attente", "en_cours"].includes(c.etat)) {
        setTimeout(() => suivreMontage(debut, estimation), 1500);
        return;
    }
    bloc.hidden = true;
    document.querySelectorAll("#form-montage button").forEach((b) => { b.disabled = false; });
    if (c.etat === "erreur" || c.etat === "annule") {
        const erreur = document.getElementById("erreur-montage");
        erreur.textContent = c.etat === "annule" ? messageAnnulation(c, "clip") : c.erreur;
        erreur.hidden = false;
        if (c.images.length) bobines.charger();
        return;
    }
    document.getElementById("clips-montage").innerHTML = c.images.map((id) =>
        `<video src="${urlVideo(id)}" poster="${urlImage(id)}" controls loop playsinline data-creation="${echapper(id)}"></video>`).join("");
    document.getElementById("resultat-montage").hidden = false;
    bobines.charger();
}

// Fiche d'un clip

const NOMS_QUALITES = { rapide: "⚡ Rapide", compromis: "⚖️ Compromis", qualite: "💎 Qualité" };

function ouvrirClip(f) {
    clipOuvert = f;
    const video = document.getElementById("video-clip");
    video.src = urlVideo(f.id);
    video.poster = urlImage(f.id);
    const modes = { animer: "🎬 Rush animé", texte: "✍️ Texte → vidéo", film: "📽️ Film d'une séquence",
                    sequence: `📽️ Plan ${f.plan} d'une séquence (${f.enchainement})` };
    const lignes = [
        ["Mode", modes[f.mode_video] ?? f.mode_video],
        ["Idée", f.idee || "—"],
        ["Prompt", f.prompt],
        ["LLM", f.llm ? f.llm.replace(".gguf", "") : "aucun"],
        ["Qualité", [NOMS_QUALITES[f.qualite] ?? f.qualite, f.megapixels && `${f.megapixels} MP`,
                     f.steps && `${f.steps} steps`].filter(Boolean).join(" · ")],
        ["Clip", [`${f.duree} s`, `${f.largeur}×${f.hauteur}`, f.format].filter(Boolean).join(" · ")],
        ["Seed", f.seed],
        ["Tournage", enMinutes(f.duree_s)],
        ["Date", f.date.replace("T", " ")],
        ["Rangement", [f.dossier ? `📁 ${f.dossier}` : "non rangé", f.archive ? "📦 archivé" : "",
                       f.corbeille ? "🗑 dans la corbeille" : ""].filter(Boolean).join(" · ")],
    ];
    if (f.parent) lignes.splice(1, 0, ["Rush source", f.parent]);
    document.getElementById("fiche-clip").innerHTML = lignes.map(([t, v]) => `<dt>${t}</dt><dd>${echapper(v)}</dd>`).join("");
    const telecharger = document.getElementById("telecharger-clip");
    telecharger.href = urlVideo(f.id);
    telecharger.download = `aria-${f.id}.mp4`;
    document.getElementById("corbeille-clip").hidden = Boolean(f.corbeille);
    // Plan ou film d'une séquence : il se retourne depuis sa séquence, pas comme un clip seul.
    document.getElementById("variante-clip").hidden = Boolean(f.sequence);
    document.getElementById("reutiliser-clip").hidden = Boolean(f.sequence);
    document.getElementById("ouvrir-sequence-clip").hidden = !f.sequence;
    document.getElementById("detail-clip").hidden = false;
    video.play().catch(() => {});
}

function fermerClip() {
    const video = document.getElementById("video-clip");
    video.pause();
    video.removeAttribute("src");
    document.getElementById("detail-clip").hidden = true;
    clipOuvert = null;
}

document.getElementById("fermer-clip").addEventListener("click", fermerClip);
document.getElementById("detail-clip").addEventListener("click", (e) => { if (e.target.id === "detail-clip") fermerClip(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && clipOuvert) fermerClip(); });

document.getElementById("variante-clip").addEventListener("click", () => {
    const f = clipOuvert;
    fermerClip();
    window.scrollTo({ top: 0, behavior: "smooth" });
    tourner({ idee: f.idee, prompt: f.prompt, mode: f.mode_video, source: f.parent, qualite: f.qualite, duree: f.duree,
              format: f.mode_video === "texte" ? f.format : "16:9 (Widescreen)", nombre: 1, dossier: f.dossier ?? null });
});

document.getElementById("reutiliser-clip").addEventListener("click", () => {
    const f = clipOuvert;
    fermerClip();
    document.getElementById("prompt-clip").value = f.prompt ?? "";
    document.querySelector(`input[name="mode-montage"][value="${f.mode_video}"]`).click();
    sourceMontage = f.mode_video === "animer" ? f.parent : sourceMontage;
    document.getElementById("qualite-montage").value = f.qualite;
    document.getElementById("duree-montage").value = String(f.duree);
    if (f.mode_video === "texte") document.getElementById("format-montage").value = f.format;
    rendreSource();
    majEstimation();
    window.scrollTo({ top: 0, behavior: "smooth" });
});

document.getElementById("ouvrir-sequence-clip").addEventListener("click", () => {
    const id = clipOuvert.sequence;
    fermerClip();
    ouvrirSequence(id);
});

document.getElementById("corbeille-clip").addEventListener("click", async () => {
    if (!confirm("Mettre ce clip à la corbeille ? Tu pourras le restaurer tant que la corbeille n'est pas vidée.")) return;
    await api("DELETE", `/api/creations/${encodeURIComponent(clipOuvert.id)}`);
    fermerClip();
    bobines.charger();
});

// Depuis la fiche d'un rush du Plateau.
document.getElementById("animer-rush").addEventListener("click", () => {
    const id = ficheOuverte.id;
    fermerFiche();
    animerRush(id);
});

// Un fichier lâché ailleurs que sur la case source ne doit pas faire ouvrir l'image par le navigateur.
["dragover", "drop"].forEach((type) => window.addEventListener(type, (e) => {
    if (!document.getElementById("vue-montage").hidden && e.dataTransfer?.types.includes("Files")
        && !e.target.closest?.("#source-montage")) {
        e.preventDefault();
    }
}));
