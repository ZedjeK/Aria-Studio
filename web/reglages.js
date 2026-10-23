// Réglages : LLM (par GGUF) et workflows. Le serveur valide et ne garde que les écarts aux valeurs d'origine.

const NOMS_CONSIGNES = {
    reecriture: "Réécriture (idée → prompt)",
    retouche: "Consigne de retouche",
    peaufinage: "Peaufinage (Scénario)",
    precisions: "Traduction des précisions",
    references: "Créer depuis des références (Plateau)",
    video: "Vidéo (Montage)",
    decoupage: "Découpage d'une séquence (Scénario)",
};
const AIDES_CONSIGNES = {
    peaufinage: "{mots_max} est remplacé par la limite de mots du modèle cible.",
    decoupage: "{mots_max} : limite de mots du modèle vidéo ; {regle_personnage} : règle selon qu'une image de référence est choisie ou non.",
};
let etatReglages = null;

async function chargerReglages() {
    try {
        etatReglages = await api("GET", "/api/reglages");
    } catch (err) {
        document.getElementById("params-llm").innerHTML = `<p class="ko">${echapper(err.message)}</p>`;
        return;
    }
    const llm = document.getElementById("llm-reglages");
    const gardeLlm = llm.value || etatReglages.defaut_llm;
    llm.innerHTML = etatReglages.llm.map((m) =>
        `<option value="${echapper(m.fichier)}" ${m.fichier === gardeLlm ? "selected" : ""}>${echapper(m.nom)}</option>`).join("");
    const wf = document.getElementById("workflow-reglages");
    const gardeWf = wf.value || etatReglages.workflows[0]?.id;
    const groupes = { texte_vers_image: "Texte vers image", retouche: "Retouche",
                      texte_vers_video: "Texte vers vidéo", image_vers_video: "Image vers vidéo" };
    wf.innerHTML = Object.entries(groupes).map(([tache, titre]) => `<optgroup label="${titre}">${etatReglages.workflows
        .filter((w) => w.tache === tache)
        .map((w) => `<option value="${echapper(w.id)}" ${w.id === gardeWf ? "selected" : ""}>${echapper(w.nom)}</option>`)
        .join("")}</optgroup>`).join("");
    document.getElementById("clause-reglages").textContent = etatReglages.clause_mineurs;
    document.getElementById("source-samplers").textContent = etatReglages.samplers_de_comfyui
        ? "Samplers et schedulers : liste annoncée par ComfyUI."
        : "Samplers et schedulers : liste de secours (ComfyUI n'a pas encore démarré depuis l'installation).";
    afficherLlm();
    afficherWorkflow();
}

const formatValeur = (p, v) => p.type === "booleen" ? (v ? "oui" : "non") : String(v);

function champReglage(p) {
    const id = `reglage-${p.cle}`;
    let saisie;
    if (p.type === "booleen") {
        saisie = `<label class="case"><input id="${id}" type="checkbox" data-cle="${p.cle}" ${p.valeur ? "checked" : ""}> activée</label>`;
    } else if (p.type === "choix") {
        saisie = `<select id="${id}" data-cle="${p.cle}">${p.choix.map((c) =>
            `<option value="${echapper(c)}" ${c === p.valeur ? "selected" : ""}>${echapper(c)}</option>`).join("")}</select>`;
    } else {
        saisie = `<input id="${id}" type="number" data-cle="${p.cle}" min="${p.min}" max="${p.max}" step="${p.pas ?? "any"}"
                         value="${p.valeur}">`;
    }
    return `<div class="param ${p.modifie ? "modifie" : ""}">
        <div class="entete-param">
            <label for="${id}">${echapper(p.libelle)}</label>
            ${p.modifie ? `<button type="button" class="petit restaurer" data-restaurer="${p.cle}"
                title="Revenir à la valeur d'origine">↺ ${echapper(formatValeur(p, p.defaut))}</button>` : ""}
        </div>
        ${saisie}
        ${p.aide ? `<small class="discret">${echapper(p.aide)}</small>` : ""}
    </div>`;
}

// Valeurs saisies qui diffèrent de celles en vigueur.
function changementsParams(conteneur, params) {
    const changements = {};
    params.forEach((p) => {
        const champ = conteneur.querySelector(`[data-cle="${p.cle}"]`);
        let v;
        if (p.type === "booleen") v = champ.checked;
        else if (p.type === "choix") v = champ.value;
        else {
            if (champ.value === "") return;
            v = Number(champ.value);
        }
        if (v !== p.valeur) changements[p.cle] = v;
    });
    return changements;
}

const llmAffiche = () => etatReglages.llm.find((m) => m.fichier === document.getElementById("llm-reglages").value);
const workflowAffiche = () => etatReglages.workflows.find((w) => w.id === document.getElementById("workflow-reglages").value);

function afficherLlm() {
    const m = llmAffiche();
    if (!m) return;
    document.getElementById("params-llm").innerHTML = m.params.map(champReglage).join("");
    document.getElementById("consignes-llm").innerHTML = m.consignes.map((c) => `
        <details class="consigne ${c.modifie ? "modifie" : ""}" data-consigne="${c.cle}">
            <summary>${echapper(NOMS_CONSIGNES[c.cle] ?? c.cle)}${c.modifie ? ' <span class="badge">modifiée</span>' : ""}</summary>
            <textarea rows="10" spellcheck="false">${echapper(c.valeur)}</textarea>
            <div class="ligne">
                <small class="discret">${echapper(AIDES_CONSIGNES[c.cle] ?? "")}</small>
                ${c.modifie ? `<button type="button" class="petit restaurer" data-restaurer-consigne="${c.cle}">↺ Consigne d'origine</button>` : ""}
            </div>
        </details>`).join("");
    majBoutonLlm();
}

function changementsConsignes(m) {
    const changements = {};
    m.consignes.forEach((c) => {
        const texte = document.querySelector(`#consignes-llm [data-consigne="${c.cle}"] textarea`).value.trim();
        if (texte === c.valeur.trim()) return;
        // Revenir mot pour mot au texte d'origine, c'est restaurer : rien à garder dans le fichier.
        changements[c.cle] = texte === "" || texte === c.defaut.trim() ? null : texte;
    });
    return changements;
}

function majBoutonLlm() {
    const m = llmAffiche();
    const n = Object.keys(changementsParams(document.getElementById("params-llm"), m.params)).length
        + Object.keys(changementsConsignes(m)).length;
    document.getElementById("enregistrer-llm").disabled = !n;
    if (n) document.getElementById("etat-llm").textContent = `${n} changement${n > 1 ? "s" : ""} non enregistré${n > 1 ? "s" : ""}`;
}

function afficherWorkflow() {
    const w = workflowAffiche();
    if (!w) return;
    document.getElementById("params-workflow").innerHTML = w.params.map(champReglage).join("");
    majBoutonWorkflow();
}

function majBoutonWorkflow() {
    const n = Object.keys(changementsParams(document.getElementById("params-workflow"), workflowAffiche().params)).length;
    document.getElementById("enregistrer-workflow").disabled = !n;
    if (n) document.getElementById("etat-workflow").textContent = `${n} changement${n > 1 ? "s" : ""} non enregistré${n > 1 ? "s" : ""}`;
}

async function enregistrerReglages(url, corps, idEtat) {
    const etat = document.getElementById(idEtat);
    etat.classList.remove("ko");
    try {
        await api("PUT", url, corps);
        await chargerReglages();
        etat.textContent = "Enregistré ✓";
    } catch (err) {
        etat.textContent = err.message;
        etat.classList.add("ko");
    }
}

const urlLlm = () => `/api/reglages/llm/${encodeURIComponent(llmAffiche().fichier)}`;
const urlWorkflow = () => `/api/reglages/workflows/${encodeURIComponent(workflowAffiche().id)}`;

document.getElementById("llm-reglages").addEventListener("change", () => {
    document.getElementById("etat-llm").textContent = "";
    afficherLlm();
});
document.getElementById("workflow-reglages").addEventListener("change", () => {
    document.getElementById("etat-workflow").textContent = "";
    afficherWorkflow();
});

["input", "change"].forEach((type) => {
    document.getElementById("params-llm").addEventListener(type, majBoutonLlm);
    document.getElementById("consignes-llm").addEventListener(type, majBoutonLlm);
    document.getElementById("params-workflow").addEventListener(type, majBoutonWorkflow);
});

document.getElementById("enregistrer-llm").addEventListener("click", () => {
    const m = llmAffiche();
    enregistrerReglages(urlLlm(), {
        valeurs: changementsParams(document.getElementById("params-llm"), m.params),
        consignes: changementsConsignes(m),
    }, "etat-llm");
});

document.getElementById("enregistrer-workflow").addEventListener("click", () => {
    enregistrerReglages(urlWorkflow(), {
        valeurs: changementsParams(document.getElementById("params-workflow"), workflowAffiche().params),
    }, "etat-workflow");
});

// ↺ : retour immédiat à la valeur d'origine (les autres saisies en cours sont abandonnées).
document.getElementById("vue-reglages").addEventListener("click", (e) => {
    const bouton = e.target.closest(".restaurer");
    if (!bouton) return;
    if (bouton.dataset.restaurerConsigne) {
        enregistrerReglages(urlLlm(), { consignes: { [bouton.dataset.restaurerConsigne]: null } }, "etat-llm");
    } else if (bouton.closest("#params-llm")) {
        enregistrerReglages(urlLlm(), { valeurs: { [bouton.dataset.restaurer]: null } }, "etat-llm");
    } else {
        enregistrerReglages(urlWorkflow(), { valeurs: { [bouton.dataset.restaurer]: null } }, "etat-workflow");
    }
});

// ⚡ Optimiser : propositions selon le matériel et les recommandations officielles. Rien n'est appliqué
// sans une case cochée : chaque ligne donne sa raison et sa source.

let propositionsOptimiser = [];

async function optimiser() {
    const carte = document.getElementById("carte-optimiser");
    const etat = document.getElementById("etat-optimiser");
    carte.hidden = false;
    etat.classList.remove("ko");
    etat.textContent = "Analyse en cours…";
    let r;
    try {
        r = await api("GET", "/api/reglages/optimiser");
    } catch (err) {
        etat.textContent = err.message;
        etat.classList.add("ko");
        return;
    }
    etat.textContent = "";
    const m = r.materiel;
    document.getElementById("materiel-optimiser").textContent =
        `${m.gpu} · ${m.vram_totale_go} Go de VRAM (dont ${m.vram_autres_go} Go pris par tes autres applis) · `
        + `${m.ram_go} Go de RAM · ${m.coeurs} cœurs / ${m.threads} threads`;
    document.getElementById("alertes-optimiser").innerHTML = r.alertes.map((a) => `<li>${echapper(a)}</li>`).join("");
    propositionsOptimiser = r.propositions;
    document.getElementById("tableau-optimiser").innerHTML = r.propositions.length
        ? `<table class="tableau-optimiser">
            <thead><tr><th><input type="checkbox" id="tout-optimiser" title="Tout cocher"></th>
                <th>Modèle</th><th>Réglage</th><th>Actuel → proposé</th><th>Pourquoi</th></tr></thead>
            <tbody>${r.propositions.map((p, i) => `<tr>
                <td><input type="checkbox" data-proposition="${i}"></td>
                <td>${echapper(p.nom)}</td>
                <td>${echapper(p.libelle)}</td>
                <td class="valeurs">${echapper(String(p.actuel))} → <strong>${echapper(String(p.propose))}</strong></td>
                <td>${echapper(p.raison)}${p.source
                    ? ` · <a href="${echapper(p.source)}" target="_blank" rel="noopener">source</a>` : ""}</td>
            </tr>`).join("")}</tbody>
        </table>`
        : '<p class="ok">Rien à proposer : tes réglages suivent déjà les recommandations.</p>';
    document.getElementById("sans-reco-optimiser").textContent = r.sans_recommandation.length
        ? `Sans recommandation officielle rangée (seul le matériel compte) : ${r.sans_recommandation.join(", ")}.`
        : "";
    majBoutonOptimiser();
}

function majBoutonOptimiser() {
    const n = document.querySelectorAll("#tableau-optimiser [data-proposition]:checked").length;
    const bouton = document.getElementById("appliquer-optimiser");
    bouton.disabled = !n;
    bouton.textContent = n ? `Appliquer la sélection (${n})` : "Appliquer la sélection";
}

document.getElementById("optimiser").addEventListener("click", optimiser);
document.getElementById("fermer-optimiser").addEventListener("click", () => {
    document.getElementById("carte-optimiser").hidden = true;
});

document.getElementById("tableau-optimiser").addEventListener("change", (e) => {
    if (e.target.id === "tout-optimiser") {
        document.querySelectorAll("#tableau-optimiser [data-proposition]").forEach((c) => { c.checked = e.target.checked; });
    }
    majBoutonOptimiser();
});

document.getElementById("appliquer-optimiser").addEventListener("click", async () => {
    const choisies = [...document.querySelectorAll("#tableau-optimiser [data-proposition]:checked")]
        .map((c) => propositionsOptimiser[Number(c.dataset.proposition)]);
    // Une requête par modèle ou workflow, avec toutes ses valeurs cochées.
    const groupes = {};
    choisies.forEach((p) => {
        const cle = `${p.type}|${p.cible}`;
        (groupes[cle] ??= { type: p.type, cible: p.cible, valeurs: {} }).valeurs[p.cle] = p.propose;
    });
    const etat = document.getElementById("etat-optimiser");
    etat.classList.remove("ko");
    try {
        for (const g of Object.values(groupes)) {
            const url = g.type === "llm"
                ? `/api/reglages/llm/${encodeURIComponent(g.cible)}`
                : `/api/reglages/workflows/${encodeURIComponent(g.cible)}`;
            await api("PUT", url, { valeurs: g.valeurs });
        }
    } catch (err) {
        etat.textContent = err.message;
        etat.classList.add("ko");
        return;
    }
    await chargerReglages();
    await optimiser();
    etat.textContent = `${choisies.length} réglage${choisies.length > 1 ? "s" : ""} appliqué${choisies.length > 1 ? "s" : ""} ✓ (↺ pour revenir en arrière)`;
});
