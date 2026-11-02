// Onglet Modèles (app/collection.py) : sous-onglets Rechercher (web/decouvrir.js), Ma collection, et un onglet par
// usage (Texte → image, Image → image, Vidéo) avec ses workflows, leurs favoris, checkpoints et LoRA.
// Le badge ⬇ ouvre la file des téléchargements (web/app.js), visible depuis tous les sous-onglets.

const ORIGINES = {
    aria: ["téléchargé par Aria ✓", "ok"],
    retrouve: ["source retrouvée ✓", "ok"],
    main: ["déposé à la main", "discret"],
};
let sousOngletModeles = "collection";
let sourcesEnCours = false;

function ouvrirModeles() {
    chargerTelechargements();
    afficherSousOnglet(sousOngletModeles);
}

function afficherSousOnglet(sous) {
    sousOngletModeles = sous;
    document.querySelectorAll("#vue-modeles .sous-onglet").forEach((b) => b.classList.toggle("actif", b.dataset.sous === sous));
    document.querySelectorAll("#vue-modeles .panneau-modeles").forEach((p) => { p.hidden = p.dataset.panneau !== sous; });
    if (sous === "rechercher") preparerDecouvrir();
    else if (sous === "collection") {
        chargerModeles();
        suivreSources();
    } else chargerUsages();
}

document.querySelectorAll("#vue-modeles .sous-onglet").forEach((b) => {
    b.addEventListener("click", () => afficherSousOnglet(b.dataset.sous));
});

document.getElementById("badge-telechargements").addEventListener("click", () => {
    const panneau = document.getElementById("panneau-telechargements");
    panneau.hidden = !panneau.hidden;
    if (!panneau.hidden) chargerTelechargements();
});

// Ma collection : ce que tu as, d'où ça vient, à quoi ça sert

async function chargerModeles() {
    const [c, m] = await Promise.all([api("GET", "/api/collection"), api("GET", "/api/modeles")]);
    dossiers = m.dossiers;  // web/app.js : dossiers proposés pour un lien collé
    document.getElementById("espace").textContent = `· ${taille(c.espace_libre)} libres`;
    const bouton = document.getElementById("retrouver-sources");
    bouton.textContent = c.orphelins ? `🔍 Retrouver les sources (${c.orphelins})` : "🔍 Retrouver les sources";
    bouton.disabled = !c.orphelins || sourcesEnCours;
    bouton.title = c.orphelins
        ? "Envoie l'empreinte SHA-256 de chaque fichier déposé à la main à Civitai, pour retrouver sa page"
        : "Tous tes fichiers ont une source connue";
    document.getElementById("groupes-collection").innerHTML = c.groupes.map(groupeCollection).join("")
        + c.fiches_ecartees.map((e) => `<p class="ko">Fiche de workflow écartée : ${echapper(e)}</p>`).join("");
}

const TYPES_CHERCHER = { image: "des modèles image", video: "des modèles vidéo", lora: "des LoRA", llm: "un LLM" };

function groupeCollection(g) {
    const titre = `${echapper(g.nom)} <span class="discret">· ${g.elements.length} · ${taille(g.taille)}</span>`;
    const liste = g.elements.length
        ? `<ul class="liste collection">${g.elements.map(elementCollection).join("")}</ul>`
        : `<p class="discret">Rien pour l'instant.</p>`;
    const chercher = TYPES_CHERCHER[g.id]
        ? `<button type="button" class="petit" data-chercher="${g.id}">🔎 Chercher ${TYPES_CHERCHER[g.id]}</button>` : "";
    if (g.id === "commun") {
        return `<details class="carte groupe-collection"><summary><h2>${titre}</h2></summary>
            <p class="discret">Encodeurs de texte, VAE et LoRA internes : les workflows les chargent eux-mêmes.</p>${liste}</details>`;
    }
    return `<section class="carte groupe-collection"><div class="ligne"><h2>${titre}</h2>${chercher}</div>${liste}</section>`;
}

function elementCollection(e) {
    const [origine, classe] = ORIGINES[e.origine];
    const source = e.page
        ? `<a href="${echapper(e.page)}" target="_blank" rel="noopener">${echapper(e.nom_modele || e.source || "page")}</a>`
        : "";
    const details = [source, e.base ? `base ${echapper(e.base)}` : "", e.licence ? `licence ${echapper(e.licence)}` : ""]
        .filter(Boolean).join(" · ");
    return `<li>
        <div class="ligne">
            <strong class="nom-collection">${echapper(e.nom)}</strong>
            ${e.famille ? `<span class="badge">${echapper(e.famille)}</span>` : ""}
            <span class="discret">${taille(e.taille)}</span>
            <span class="${classe}">${origine}</span>
            <button type="button" class="petit" data-ouvrir="modeles" data-sous="${echapper(e.dossier)}" title="Ouvrir son dossier">📂</button>
            ${e.supprimable ? `<button type="button" class="petit" data-supprimer="${echapper(e.cle)}" data-taille="${e.taille}">Supprimer</button>` : ""}
        </div>
        ${details ? `<div class="discret">${details}</div>` : ""}
        ${e.declencheurs.length ? `<div class="discret">Déclencheurs : ${echapper(e.declencheurs.join(", "))}</div>` : ""}
        <div class="discret">${e.utilise_par.length ? `Utilisé par : ${echapper(e.utilise_par.join(", "))}` : "Utilisé par aucun workflow"}</div>
    </li>`;
}

// Retrouver les sources : seulement sur un clic (l'empreinte part chez Civitai)

document.getElementById("retrouver-sources").addEventListener("click", async () => {
    if (!confirm("Aria va calculer l'empreinte SHA-256 de chaque fichier déposé à la main (une trentaine de secondes "
        + "pour 13 Go), la comparer à ses fiches, puis la demander à Civitai pour retrouver la page du modèle. Continuer ?")) return;
    try {
        await api("POST", "/api/collection/sources");
        suivreSources();
    } catch (err) {
        alert(err.message);
    }
});

async function suivreSources() {
    const s = await api("GET", "/api/collection/sources");
    const zone = document.getElementById("suivi-sources");
    zone.hidden = s.etat === "repos";
    if (s.etat === "en_cours") {
        sourcesEnCours = true;
        document.getElementById("retrouver-sources").disabled = true;
        zone.textContent = `Fichier ${Math.min(s.fait + 1, s.total)} sur ${s.total} : ${s.fichier ?? "…"} · `
            + (s.phase === "civitai" ? "question à Civitai…" : "calcul de l'empreinte…");
        setTimeout(suivreSources, 1000);
        return;
    }
    if (s.etat !== "termine") return;
    zone.innerHTML = [
        s.trouves.length ? `<span class="ok">✓ Retrouvés : ${s.trouves.map((t) => `${echapper(t.nom)} (${echapper(t.ou)})`).join(", ")}</span>` : "",
        s.introuvables.length ? `Introuvables sur Civitai : ${echapper(s.introuvables.join(", "))}` : "",
        ...s.erreurs.map((e) => `<span class="ko">${echapper(e.nom)} : ${echapper(e.erreur)}</span>`),
    ].filter(Boolean).join("<br>") || "Aucun fichier à retrouver.";
    if (sourcesEnCours) {
        sourcesEnCours = false;
        chargerModeles();
    }
}

// Onglets d'usage : Texte → image, Image → image, Vidéo

async function chargerUsages() {
    const u = await api("GET", "/api/usages");
    for (const [id, usage] of Object.entries(u)) {
        document.querySelector(`.usage[data-usage="${id}"]`).innerHTML = rendreUsage(id, usage);
    }
}

function rendreUsage(id, u) {
    const nomDe = (wid) => u.workflows.find((w) => w.id === wid)?.nom ?? wid ?? "—";
    const favoris = `<section class="carte"><h2>Par défaut</h2><dl class="machine-assistant">${u.favoris.map((f) =>
        `<dt>${echapper(f.libelle)}</dt><dd>${f.valeur ? `★ ${echapper(nomDe(f.valeur))}` : "—"}</dd>`).join("")}</dl>
        <p class="discret">Le Plateau, le Montage et le Scénario partent de ces workflows. Le checkpoint par défaut de chacun
            se règle dans Réglages.</p></section>`;
    const variation = u.variation.length
        ? `<p class="discret">🎨 Variation (Plateau, zone Références) : elle part d'une image avec un modèle texte vers image :
            ${echapper(u.variation.join(", "))}.</p>` : "";
    return favoris + u.workflows.map((wf) => carteWorkflow(wf, id, u)).join("") + variation
        + (u.workflows.length ? "" : '<p class="discret">Aucun workflow pour cet usage.</p>');
}

function carteWorkflow(wf, idUsage, u) {
    const libelle = (cle) => u.favoris.find((f) => f.cle === cle)?.libelle ?? cle;
    const favoris = wf.possibles.map((cle) => (wf.favori_pour.includes(cle)
        ? `<span class="favori">★ ${echapper(libelle(cle))}</span>`
        : `<button type="button" class="petit" data-favori="${cle}" data-workflow="${wf.id}" ${wf.pret ? "" : "disabled"}
               title="${wf.pret ? "" : "Télécharge d'abord ce qui manque"}">☆ Par défaut pour « ${echapper(libelle(cle))} »</button>`)).join(" ");
    const vram = wf.vram_min_go
        ? `VRAM mesurée : ${wf.vram_min_go} Go minimum` : `VRAM estimée ${wf.vram_estimee_go} Go`;
    const capacites = [wf.references ? `jusqu'à ${wf.references} images de référence` : "", wf.variation ? "variation" : ""]
        .filter(Boolean).join(" · ");
    const nature = idUsage === "video" ? "video" : "image";
    const origine = wf.modeles[0];
    return `<section class="carte workflow-usage">
        <div class="ligne"><h2>${echapper(wf.nom)}</h2>
            ${wf.pret ? '<span class="ok">prêt ✓</span>'
                : `<span class="ko">il manque ${taille(wf.a_telecharger)}</span>
                   <button type="button" class="petit principal" data-telecharger-wf="${wf.id}" data-taille="${wf.a_telecharger}"
                           data-licence="${echapper(wf.licence)}">⬇ Télécharger ce qui manque</button>`}</div>
        <div class="favoris-workflow">${favoris}</div>
        <p class="discret">Famille ${echapper(wf.famille)} · ${taille(wf.taille)} · ${vram}${capacites ? ` · ${capacites}` : ""}</p>
        <p class="discret licence-fiche">${resumeLicence(wf.licence_fiche)}</p>
        ${wf.manquants.length ? `<p class="discret">Manquants : ${wf.manquants.map((f) => echapper(`${f.dossier}/${f.nom}`)).join(", ")}</p>` : ""}
        <div class="colonnes-workflow">
            <div>
                <h3>Checkpoints <button type="button" class="petit" data-ouvrir="modeles" data-sous="${echapper(wf.famille)}/modeles">📂</button></h3>
                <ul class="liste">${wf.modeles.map((m) => `<li>${echapper(m)}
                    ${m === wf.modele_defaut ? '<span class="badge ok">par défaut</span>' : ""}
                    ${m === origine ? '<span class="badge">d\'origine</span>' : ""}</li>`).join("")}</ul>
                <button type="button" class="petit" data-chercher="${nature}" data-famille="${echapper(wf.famille)}">🔎 Chercher des checkpoints ${echapper(wf.famille)}</button>
            </div>
            <div>
                <h3>LoRA <button type="button" class="petit" data-ouvrir="modeles" data-sous="${echapper(wf.famille)}/loras">📂</button></h3>
                ${wf.loras.length ? `<ul class="liste">${wf.loras.map((l) => `<li>${echapper(l.nom)}${l.declencheurs
                    ? ` <span class="discret">· ${echapper(l.declencheurs)}</span>` : ""}</li>`).join("")}</ul>`
                    : '<p class="discret">Aucune LoRA pour cette famille.</p>'}
                <button type="button" class="petit" data-chercher="lora" data-famille="${echapper(wf.famille)}">🔎 Chercher des LoRA ${echapper(wf.famille)}</button>
            </div>
        </div>
    </section>`;
}

// Actions de l'onglet Modèles : favori, téléchargement d'un workflow, recherche préfiltrée, suppression

document.getElementById("vue-modeles").addEventListener("click", async (e) => {
    const cible = e.target.closest("[data-favori], [data-telecharger-wf], [data-chercher], [data-supprimer]");
    if (!cible) return;
    try {
        if (cible.dataset.favori) {
            cible.disabled = true;
            await api("PUT", "/api/usages/favori", { cle: cible.dataset.favori, workflow: cible.dataset.workflow });
            await chargerUsages();
        } else if (cible.dataset.telechargerWf) {
            if (!confirm(`Télécharger ce qui manque (${taille(Number(cible.dataset.taille))}) ? `
                + `En cliquant sur OK, tu acceptes la licence ${cible.dataset.licence}.`)) return;
            cible.disabled = true;
            await api("POST", `/api/workflows/${encodeURIComponent(cible.dataset.telechargerWf)}/telecharger`);
            document.getElementById("panneau-telechargements").hidden = false;
            chargerTelechargements();
        } else if (cible.dataset.chercher) {
            afficherSousOnglet("rechercher");
            await chercherDepuis(cible.dataset.chercher, cible.dataset.famille);
        } else if (cible.dataset.supprimer) {
            const cle = cible.dataset.supprimer;
            if (!confirm(`Supprimer définitivement ${cle} (${taille(Number(cible.dataset.taille))}) ?`)) return;
            await api("DELETE", `/api/modeles/${cle.split("/").map(encodeURIComponent).join("/")}`);
            chargerModeles();
        }
    } catch (err) {
        cible.disabled = false;
        alert(err.message);
    }
});

chargerTelechargements();  // le badge dit tout de suite si une file tourne
