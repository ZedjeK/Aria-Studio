// Casting : lancement des bancs et comparaison à l'aveugle.

const NOMS_TACHES = { texte_vers_image: "Texte vers image", retouche: "Retouche", reecriture: "Réécriture (LLM)",
                      image_vers_video: "Image vers vidéo", texte_vers_video: "Texte vers vidéo" };
const AVEC_SOURCE = ["retouche", "image_vers_video"];

// Une vidéo se lit en boucle, muette au départ (🔊 dans les contrôles) ; une image s'ouvre en grand.
const media = (url, titre) => url.endsWith(".mp4")
    ? `<video src="${url}" title="${titre}" loop muted autoplay playsinline controls preload="metadata"></video>`
    : `<a href="${url}" target="_blank"><img src="${url}" alt="${titre}" loading="lazy"></a>`;
const CONTROLES = { anglais: "anglais", longueur: "longueur", sans_preambule: "sans préambule", texte_conserve: "texte gardé" };
let bancAffiche = null;
let suiviBanc = null;

async function chargerCasting() {
    const { actif, bancs } = await api("GET", "/api/bancs");
    afficherProgression(actif);

    const select = document.getElementById("liste-bancs");
    select.innerHTML = bancs.map((b) => `<option value="${echapper(b.id)}">
        ${echapper(b.date.replace("T", " "))} · ${NOMS_TACHES[b.tache] ?? echapper(b.tache)}
        · ${b.nb_workflows} modèles${b.revele ? "" : " · à l'aveugle"}</option>`).join("");
    if (!bancs.length) {
        document.getElementById("banc").innerHTML = '<p class="discret">Aucun banc pour l\'instant.</p>';
        return;
    }
    if (!bancs.some((b) => b.id === bancAffiche)) bancAffiche = bancs[0].id;
    select.value = bancAffiche;
    chargerBanc();
}

function afficherProgression(actif) {
    const bloc = document.getElementById("progression-banc");
    const erreur = document.getElementById("erreur-banc");
    const enCours = actif && ["attente", "en_cours"].includes(actif.etat);
    bloc.hidden = !enCours;
    document.getElementById("lancer-banc").disabled = enCours;
    erreur.hidden = !(actif && actif.etat === "erreur");
    if (actif && actif.etat === "erreur") erreur.textContent = actif.erreur;
    if (enCours) {
        bloc.querySelector(".barre div").style.width = `${(100 * actif.fait / actif.total).toFixed(1)}%`;
        bloc.querySelector("span").textContent = `${actif.fait} / ${actif.total} · ${actif.courant || "préparation…"}`;
        if (!suiviBanc) suiviBanc = setInterval(suivreBanc, 2000);
    } else if (suiviBanc) {
        clearInterval(suiviBanc);
        suiviBanc = null;
        chargerCasting();
    }
}

async function suivreBanc() {
    const { actif } = await api("GET", "/api/bancs");
    afficherProgression(actif);
}

document.getElementById("lancer-banc").addEventListener("click", async () => {
    const erreur = document.getElementById("erreur-banc");
    erreur.hidden = true;
    try {
        const actif = await api("POST", "/api/bancs", { tache: document.getElementById("tache-banc").value });
        bancAffiche = actif.id;
        afficherProgression(actif);
    } catch (err) {
        erreur.textContent = err.message;
        erreur.hidden = false;
    }
});

document.getElementById("liste-bancs").addEventListener("change", (e) => {
    bancAffiche = e.target.value;
    chargerBanc();
});

async function chargerBanc() {
    afficherBanc(await api("GET", `/api/bancs/${bancAffiche}`));
}

// Affichage d'un banc

function etoiles(modele, g) {
    const boutons = [1, 2, 3, 4, 5].map((n) =>
        `<button class="${g.note >= n ? "allumee" : ""}" title="${n}/5"
            data-noter="${n}" data-cle="${echapper(modele.cle)}" data-fichier="${echapper(g.fichier)}">★</button>`).join("");
    const source = g.peut_servir_de_source
        ? (g.est_la_source
            ? '<span class="petit ok">source ✓</span>'
            : `<button class="petit" data-source="1" data-cle="${echapper(modele.cle)}"
                  data-fichier="${echapper(g.fichier)}">Utiliser comme source</button>`)
        : "";
    return `<div class="etoiles">${boutons}${source}</div>`;
}

function tableauResume(b) {
    const llm = b.tache === "reecriture";
    const ligne = (m) => {
        const notes = m.generations.map((g) => g.note).filter(Boolean);
        const moyenne = notes.length ? (notes.reduce((a, n) => a + n, 0) / notes.length).toFixed(2) : "—";
        const r = m.resume ?? {};
        const l = m.liberation ?? {};
        const favori = b.favori === m.cle
            ? '<span class="ok">favori ✓</span>'
            : `<button class="petit" data-favori="${echapper(m.cle)}">Choisir comme favori</button>`;
        const mesures = llm
            ? `<td>${r.chargement_s ?? "—"} s</td><td>${r.ecriture_tps_moyen ?? "—"} tok/s</td>
               <td>${r.duree_moyenne_s ?? "—"} s</td><td>${r.pic_vram_go ?? "—"} Go</td><td>${r.controles_reussis ?? "—"}</td>`
            : `<td>${r.temps_froid_s ?? "—"} s</td><td>${r.temps_chaud_moyen_s ?? "—"} s</td>
               <td>${r.pic_ram_go ?? "—"} Go</td><td>${l.vram_avant_go ?? "—"} → ${l.vram_apres_go ?? "—"} Go</td>`;
        return `<tr><td>${echapper(m.nom)}</td><td>${moyenne}</td>${mesures}<td>${favori}</td></tr>`;
    };
    const titres = llm
        ? "<th>Chargement</th><th>Écriture</th><th>Durée moy.</th><th>Pic VRAM</th><th>Contrôles</th>"
        : "<th>À froid</th><th>À chaud</th><th>Pic RAM</th><th>VRAM /free</th>";
    return `<table class="resume">
        <thead><tr><th>Modèle</th><th>Note moy.</th>${titres}<th></th></tr></thead>
        <tbody>${b.modeles.map(ligne).join("")}</tbody>
    </table>`;
}

function detailsPrompt(g) {
    if (!g.texte) return "";
    const controles = Object.entries(CONTROLES)
        .filter(([cle]) => g.controles && g.controles[cle] !== null && g.controles[cle] !== undefined)
        .map(([cle, nom]) => `<span class="${g.controles[cle] ? "ok" : "ko"}">${g.controles[cle] ? "✓" : "✗"} ${nom}</span>`)
        .join(" · ");
    return `<p class="texte-prompt discret">${echapper(g.texte)}</p><p class="discret">${controles} · ${g.controles.mots} mots</p>`;
}

function afficherBanc(b) {
    const conteneur = document.getElementById("banc");
    const complet = b.notees === b.total;
    const entete = `<div class="entete-banc">
        <span class="discret">${NOMS_TACHES[b.tache] ?? echapper(b.tache)} · ${b.notees} / ${b.total} ${b.tache.endsWith("video") ? "clips notés" : "images notées"}
            · VRAM au repos ${b.vram_base_go ?? "—"} Go${b.en_cours ? " · banc en cours…" : ""}</span>
        ${b.revele ? "" : `<button id="reveler" ${b.en_cours ? "disabled" : ""}
            class="${complet ? "principal" : ""}">Révéler les modèles</button>`}
    </div>
    ${b.revele ? tableauResume(b) : ""}`;

    const colonnes = b.modeles.length + (AVEC_SOURCE.includes(b.tache) ? 1 : 0);
    const blocs = b.prompts.map((p) => {
        const source = AVEC_SOURCE.includes(b.tache)
            ? `<div class="colonne"><h4>Source</h4><div class="vignette">
                <a href="/api/sources/${encodeURIComponent(p.source)}" target="_blank">
                <img src="/api/sources/${encodeURIComponent(p.source)}" alt="source" loading="lazy"></a></div></div>`
            : "";
        const modeles = b.modeles.map((m) => `<div class="colonne">
            <h4>${echapper(m.nom)}</h4>
            <div class="seeds" style="grid-template-columns: repeat(${b.seeds.length}, 1fr)">
            ${m.generations.filter((g) => g.prompt === p.id).map((g) => `
                <div class="vignette ${g.est_la_source ? "source" : ""}">
                    ${media(g.url, `seed ${g.seed}`)}
                    ${etoiles(m, g)}
                    ${detailsPrompt(g)}
                </div>`).join("")}
            </div>
        </div>`).join("");
        return `<div class="bloc-prompt">
            <h3>${echapper(p.id)} <span class="discret">· ${echapper(p.teste)}</span></h3>
            <p class="discret">${echapper(p.prompt)}</p>
            <div class="grille" style="grid-template-columns: repeat(${colonnes}, 1fr)">${source}${modeles}</div>
        </div>`;
    }).join("");

    conteneur.innerHTML = entete + blocs;
}

// Actions : notes, révélation, favori, source

document.getElementById("banc").addEventListener("click", async (e) => {
    const cible = e.target.closest("button");
    if (!cible) return;
    const url = `/api/bancs/${bancAffiche}`;
    try {
        if (cible.dataset.noter) {
            afficherBanc(await api("POST", `${url}/notes`, {
                cle: cible.dataset.cle, fichier: cible.dataset.fichier, note: Number(cible.dataset.noter),
            }));
        } else if (cible.id === "reveler") {
            const b = await api("GET", url);
            if (b.notees < b.total && !confirm(`Seulement ${b.notees} ${b.tache.endsWith("video") ? "clips notés" : "images notées"} sur ${b.total}. Révéler quand même ?`)) return;
            afficherBanc(await api("POST", `${url}/reveler`));
            chargerCasting();
        } else if (cible.dataset.favori) {
            afficherBanc(await api("POST", `${url}/favori`, { workflow: cible.dataset.favori }));
        } else if (cible.dataset.source) {
            afficherBanc(await api("POST", `${url}/source`, { cle: cible.dataset.cle, fichier: cible.dataset.fichier }));
        }
    } catch (err) {
        alert(err.message);
    }
});
