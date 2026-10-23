// Decouvrir : recherche sur Civitai et Hugging Face, téléchargement direct dans le bon dossier.

const TYPES_DECOUVRIR = {
    civitai: [["lora", "LoRA"], ["modele", "Modèle d'image"]],
    huggingface: [["llm", "LLM (GGUF)"], ["modele", "Modèle d'image"]],
};
const VERDICTS_VRAM = { tient: "✓ tient en VRAM", juste: "~ juste", deborde: "✗ déborde (plus lent)" };
let dossiersDecouvrir = [];
let curseurDecouvrir = null;
let decouvrirPret = false;
const versionsCivitai = {};  // id du modèle -> versions, renvoyées avec les résultats de recherche

const valeur = (id) => document.getElementById(id).value;

async function preparerDecouvrir() {
    if (decouvrirPret) return;
    decouvrirPret = true;
    try {
        document.getElementById("nsfw-decouvrir").checked = localStorage.getItem("aria-nsfw") === "oui";
    } catch { /* stockage indisponible : aperçus floutés par défaut */ }
    dossiersDecouvrir = (await api("GET", "/api/modeles")).dossiers;
    const familles = [...new Set(Object.values(optionsPlateau?.workflows ?? {})
        .flatMap((t) => t.choix.map((w) => w.famille)))].sort();
    document.getElementById("famille-decouvrir").innerHTML = '<option value="">Toutes</option>'
        + familles.map((f) => `<option value="${echapper(f)}">${echapper(f)}</option>`).join("");
    majTypes();
}

function majTypes() {
    const source = valeur("source-decouvrir");
    document.getElementById("type-decouvrir").innerHTML = TYPES_DECOUVRIR[source]
        .map(([v, t]) => `<option value="${v}">${t}</option>`).join("");
    document.getElementById("famille-decouvrir").disabled = source !== "civitai";
}
document.getElementById("source-decouvrir").addEventListener("change", majTypes);

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
        source: valeur("source-decouvrir"), type: valeur("type-decouvrir"),
        texte: valeur("texte-decouvrir"), tri: valeur("tri-decouvrir"),
    });
    if (valeur("source-decouvrir") === "civitai" && valeur("famille-decouvrir")) params.set("famille", valeur("famille-decouvrir"));
    if (suite && curseurDecouvrir) params.set("curseur", curseurDecouvrir);
    message.textContent = "Recherche…";
    try {
        const r = await api("GET", `/api/decouvrir?${params}`);
        curseurDecouvrir = r.curseur;
        r.resultats.filter((x) => x.versions).forEach((x) => { versionsCivitai[x.id] = x.versions; });
        const conteneur = document.getElementById("resultats-decouvrir");
        const cartes = r.resultats.map(carte).join("");
        conteneur.innerHTML = suite ? conteneur.innerHTML + cartes : cartes;
        message.textContent = r.resultats.length || suite ? "" : "Aucun résultat.";
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
    const base = r.versions?.[0]?.base;
    return `<article class="resultat" data-source="${r.source}" data-id="${echapper(String(r.id))}" data-type="${echapper(r.type)}">
        ${apercu}
        <div class="corps">
            <h3><a href="${echapper(r.page)}" target="_blank" rel="noopener">${echapper(r.nom)}</a></h3>
            <span class="stats">par ${echapper(r.auteur ?? "?")} · ↓ ${nombre(r.telechargements)} · ♥ ${nombre(r.appreciations)}</span>
            <div>${base ? `<span class="badge">${echapper(base)}</span>` : ""}
                ${r.protege ? '<span class="badge">accès protégé</span>' : ""}</div>
            <button type="button" class="petit" data-voir="1">Voir les fichiers</button>
            <div class="fichiers-decouvrir" hidden></div>
        </div>
    </article>`;
}

function choixDossier(propose) {
    return `<select>${propose ? "" : '<option value="">— dossier —</option>'}${dossiersDecouvrir.map((d) =>
        `<option value="${echapper(d)}" ${d === propose ? "selected" : ""}>${echapper(d)}</option>`).join("")}</select>`;
}

function ligneFichier(f) {
    const vram = f.vram ? `<span class="vram-${f.vram}">${VERDICTS_VRAM[f.vram]}</span>` : "";
    return `<div class="fichier-decouvrir" data-lien="${echapper(f.lien)}">
        <span class="nom">${echapper(f.nom)} · ${taille(f.taille)} ${vram}</span>
        <span></span>
        ${choixDossier(f.dossier)}
        <button type="button" class="petit principal" data-telecharger="1">Télécharger</button>
    </div>`;
}

async function voirFichiers(article) {
    const zone = article.querySelector(".fichiers-decouvrir");
    zone.hidden = !zone.hidden;
    if (zone.hidden || zone.dataset.charge) return;
    zone.dataset.charge = "1";
    try {
        if (article.dataset.source === "huggingface") {
            const r = await api("GET", `/api/decouvrir/hf/fichiers?${new URLSearchParams({
                depot: article.dataset.id, type: article.dataset.type })}`);
            zone.innerHTML = (r.fichiers.length
                ? `<span class="discret">VRAM disponible pour le LLM : ~${r.vram_libre_go} / ${r.vram_totale_go} Go</span>`
                  + r.fichiers.map(ligneFichier).join("")
                : '<span class="discret">Aucun fichier .safetensors ou .gguf.</span>');
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
                `${t.nom} (${taille(t.taille)}) ajouté à la file : suivi dans l'onglet Modèles.`;
        } catch (err) {
            e.target.disabled = false;
            alert(err.message);
        }
    }
});
