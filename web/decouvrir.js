// Rechercher (onglet Modèles, app/decouvrir.py) : modèles image, modèles vidéo, LoRA et LLM sur Civitai et Hugging
// Face. Seuls les modèles des familles d'Aria sont montrés (bases Civitai et dépôts d'origine HF de leurs fiches) ;
// le dossier de destination en est déduit, et ce que tu as déjà porte « installé ✓ ».

const AIDES_DECOUVRIR = {
    image: "Checkpoints qui vont dans un de tes workflows d'image : le dossier de leur famille est choisi pour toi.",
    video: "Checkpoints pour tes workflows vidéo (MiniMax H3) : le dossier de leur famille est choisi pour toi.",
    lora: "LoRA pour une famille d'Aria : choisis la famille pour ne voir que les siennes.",
    llm: "GGUF pour llama.cpp (Hugging Face), avec un verdict : tient-il dans ta VRAM ?",
};
const VERDICTS_VRAM = { tient: "✓ tient en VRAM", juste: "~ juste", deborde: "✗ déborde (plus lent)" };
let typeDecouvrir = "image";
let dossiersDecouvrir = [];
let curseurDecouvrir = null;
let preparationDecouvrir = null;  // une seule préparation, attendue par tous (onglet ouvert, « Chercher des LoRA »…)
let jetonsDecouvrir = { civitai: true, huggingface: true };  // présence des clés d'accès (jamais leur valeur)
const versionsCivitai = {};  // id du modèle -> versions, renvoyées avec les résultats de recherche

const valeur = (id) => document.getElementById(id).value;

function preparerDecouvrir() {
    preparationDecouvrir ??= (async () => {
        try {
            document.getElementById("nsfw-decouvrir").checked = localStorage.getItem("aria-nsfw") === "oui";
        } catch { /* stockage indisponible : aperçus floutés par défaut */ }
        dossiersDecouvrir = (await api("GET", "/api/modeles")).dossiers;
        jetonsDecouvrir = await api("GET", "/api/jetons");
        await majTypes();
    })();
    return preparationDecouvrir;
}

// Les pastilles de type : la source et les familles proposées en dépendent.
async function majTypes() {
    document.querySelectorAll("#types-decouvrir button").forEach((b) => b.classList.toggle("actif", b.dataset.type === typeDecouvrir));
    const llm = typeDecouvrir === "llm";
    const source = document.getElementById("source-decouvrir");
    source.querySelector('option[value="civitai"]').disabled = llm;
    if (llm) source.value = "huggingface";
    document.getElementById("bloc-famille-decouvrir").hidden = llm;
    majAide();
    const familles = llm ? [] : await api("GET", `/api/decouvrir/familles?type=${typeDecouvrir}`);
    const select = document.getElementById("famille-decouvrir");
    const garde = select.value;
    select.innerHTML = '<option value="">Toutes</option>'
        + familles.map((f) => `<option value="${echapper(f.famille)}">${echapper(f.famille)}${typeDecouvrir === "lora"
            ? ` (${f.nature === "video" ? "vidéo" : "image"})` : ""}</option>`).join("");
    if (familles.some((f) => f.famille === garde)) select.value = garde;
}

function majAide() {
    const sansCle = valeur("source-decouvrir") === "civitai" && !jetonsDecouvrir.civitai;
    document.getElementById("aide-decouvrir").textContent = AIDES_DECOUVRIR[typeDecouvrir] + (sansCle
        ? " ⚠ Pas de clé Civitai (variable CIVITAI_TOKEN) : beaucoup de modèles refuseront le téléchargement."
        : "");
}
document.getElementById("source-decouvrir").addEventListener("change", majAide);

document.getElementById("types-decouvrir").addEventListener("click", async (e) => {
    const type = e.target.closest("[data-type]")?.dataset.type;
    if (!type || type === typeDecouvrir) return;
    typeDecouvrir = type;
    await majTypes();
    document.getElementById("resultats-decouvrir").innerHTML = "";
    document.getElementById("plus-decouvrir").hidden = true;
    document.getElementById("message-decouvrir").textContent = "";
});

// Depuis un onglet d'usage : « Chercher des LoRA pour cette famille », « Chercher des checkpoints ».
async function chercherDepuis(type, famille) {
    await preparerDecouvrir();
    typeDecouvrir = type;
    await majTypes();
    document.getElementById("famille-decouvrir").value = famille || "";
    chercher(false);
}

document.getElementById("nsfw-decouvrir").addEventListener("change", (e) => {
    try {
        localStorage.setItem("aria-nsfw", e.target.checked ? "oui" : "non");
    } catch { /* tant pis : le choix ne sera pas retenu */ }
    document.querySelectorAll(".vignette-decouvrir.nsfw").forEach((v) => v.classList.toggle("floue", !e.target.checked));
});

document.getElementById("form-decouvrir").addEventListener("submit", (e) => {
    e.preventDefault();
    chercher(false);
});
document.getElementById("plus-decouvrir").addEventListener("click", () => chercher(true));

async function chercher(suite) {
    const message = document.getElementById("message-decouvrir");
    const params = new URLSearchParams({
        source: valeur("source-decouvrir"), type: typeDecouvrir,
        texte: valeur("texte-decouvrir"), tri: valeur("tri-decouvrir"),
    });
    if (typeDecouvrir !== "llm" && valeur("famille-decouvrir")) params.set("famille", valeur("famille-decouvrir"));
    if (suite && curseurDecouvrir) params.set("curseur", curseurDecouvrir);
    message.textContent = "Recherche…";
    try {
        const r = await api("GET", `/api/decouvrir?${params}`);
        curseurDecouvrir = r.curseur;
        r.resultats.filter((x) => x.versions).forEach((x) => { versionsCivitai[x.id] = x.versions; });
        const conteneur = document.getElementById("resultats-decouvrir");
        const cartes = r.resultats.map(carte).join("");
        conteneur.innerHTML = suite ? conteneur.innerHTML + cartes : cartes;
        message.textContent = r.resultats.length || suite ? "" : "Aucun résultat pour une famille d'Aria.";
        document.getElementById("plus-decouvrir").hidden = !curseurDecouvrir;
    } catch (err) {
        message.textContent = err.message;
    }
}

function nombre(n) {
    return n >= 1e6 ? `${(n / 1e6).toFixed(1)} M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)} k` : String(n);
}

function carte(r) {
    const voirNsfw = document.getElementById("nsfw-decouvrir").checked;
    const apercu = r.apercu
        ? `<div class="vignette-decouvrir ${r.apercu.nsfw ? "nsfw" : ""} ${r.apercu.nsfw && !voirNsfw ? "floue" : ""}">
               <img src="${echapper(r.apercu.url)}" alt="" loading="lazy" referrerpolicy="no-referrer"></div>`
        : "";
    return `<article class="resultat" data-source="${r.source}" data-id="${echapper(String(r.id))}"
                     data-type="${echapper(r.type)}" data-famille="${echapper(r.famille ?? "")}">
        ${apercu}
        <div class="corps">
            <h3><a href="${echapper(r.page)}" target="_blank" rel="noopener">${echapper(r.nom)}</a></h3>
            <span class="stats">par ${echapper(r.auteur ?? "?")} · ↓ ${nombre(r.telechargements)} · ♥ ${nombre(r.appreciations)}</span>
            <div>${r.installe ? '<span class="badge ok">installé ✓</span>' : ""}
                ${r.famille ? `<span class="badge" title="Famille d'Aria où il ira">${echapper(r.famille)}</span>` : ""}
                ${r.protege ? '<span class="badge">accès protégé</span>' : ""}</div>
            <button type="button" class="petit" data-voir="1">Voir les fichiers</button>
            <div class="fichiers-decouvrir" hidden></div>
        </div>
    </article>`;
}

// Le dossier proposé d'abord ; un LLM ne va que dans llm, un modèle de diffusion jamais.
function choixDossier(propose) {
    const possibles = dossiersDecouvrir.filter((d) => (typeDecouvrir === "llm") === (d === "llm"));
    return `<select>${propose ? "" : '<option value="">— dossier —</option>'}${possibles.map((d) =>
        `<option value="${echapper(d)}" ${d === propose ? "selected" : ""}>${echapper(d)}</option>`).join("")}</select>`;
}

function ligneFichier(f) {
    const vram = f.vram ? `<span class="vram-${f.vram}">${VERDICTS_VRAM[f.vram]}</span>` : "";
    return `<div class="fichier-decouvrir" data-lien="${echapper(f.lien)}">
        <span class="nom">${echapper(f.nom)} · ${taille(f.taille)} ${vram}</span>
        <span>${f.installe ? '<span class="ok">installé ✓</span>' : ""}</span>
        ${choixDossier(f.dossier)}
        <button type="button" class="petit principal" data-telecharger="1" ${f.installe ? "disabled" : ""}>Télécharger</button>
    </div>`;
}

async function voirFichiers(article) {
    const zone = article.querySelector(".fichiers-decouvrir");
    zone.hidden = !zone.hidden;
    if (zone.hidden || zone.dataset.charge) return;
    zone.dataset.charge = "1";
    try {
        if (article.dataset.source === "huggingface") {
            const params = new URLSearchParams({ depot: article.dataset.id, type: article.dataset.type });
            const famille = article.dataset.famille || valeur("famille-decouvrir");
            if (famille && article.dataset.type !== "llm") params.set("famille", famille);
            const r = await api("GET", `/api/decouvrir/hf/fichiers?${params}`);
            zone.innerHTML = r.fichiers.length
                ? (article.dataset.type === "llm"
                    ? `<span class="discret">VRAM disponible pour le LLM : ~${r.vram_libre_go} / ${r.vram_totale_go} Go</span>` : "")
                  + r.fichiers.map(ligneFichier).join("")
                : `<span class="discret">Aucun fichier ${article.dataset.type === "llm" ? ".gguf" : ".safetensors"}.</span>`;
        } else {
            const versions = versionsCivitai[article.dataset.id] ?? [];
            zone.innerHTML = versions.map((v) => `
                <div><strong>${echapper(v.nom ?? "")}</strong> <span class="badge">${echapper(v.base ?? "?")}</span>
                ${v.mots_declencheurs.length ? `<br><span class="discret">Déclencheurs : ${echapper(v.mots_declencheurs.join(", "))}</span>` : ""}</div>
                ${v.fichiers.filter((f) => f.principal || v.fichiers.length === 1).map((f) =>
                    ligneFichier({ ...f, lien: v.lien, dossier: v.dossier })).join("")}`).join("");
        }
    } catch (err) {
        zone.innerHTML = `<span class="ko">${echapper(err.message)}</span>`;
    }
}

document.getElementById("resultats-decouvrir").addEventListener("click", async (e) => {
    const article = e.target.closest(".resultat");
    if (!article) return;
    if (e.target.dataset.voir) {
        voirFichiers(article);
    } else if (e.target.dataset.telecharger) {
        const ligne = e.target.closest(".fichier-decouvrir");
        const dossier = ligne.querySelector("select").value;
        if (!dossier) {
            alert("Choisis d'abord le dossier de destination (la famille du modèle).");
            return;
        }
        e.target.disabled = true;
        try {
            const t = await api("POST", "/api/decouvrir/telecharger", { lien: ligne.dataset.lien, dossier });
            e.target.textContent = "En file ✓";
            document.getElementById("message-decouvrir").textContent =
                `${t.nom} (${taille(t.taille)}) ajouté à la file : suivi sous ⬇ Téléchargements.`;
            chargerTelechargements();
        } catch (err) {
            e.target.disabled = false;
            alert(err.message);
        }
    }
});
