// Scénario, mode Séquence (cible vidéo) : un synopsis, des plans (type, durée, note), une image de référence
// facultative ; Llama écrit le prompt de chaque plan en une fois (app/sequences.decouper), puis la séquence
// est créée dans le Montage, prête à tourner. Les listes du Scénario donnent le personnage et le décor.

const TYPES_DEPART = [["texte", "✍️ Départ du texte"], ["rush", "🖼️ Départ de la référence"]];
const TYPES_SUITE = [["prolonger", "↪ Prolonger"], ["couper", "✂ Couper"], ["transition", "🌀 Transition"]];
const NOTES_TYPES = {
    texte: "ce que montre le premier plan (facultatif)",
    rush: "ce qui bouge dans la référence (facultatif)",
    prolonger: "la suite du même plan (facultatif)",
    couper: "le nouveau moment ou angle (facultatif)",
    transition: "où la caméra arrive (facultatif)",
};

let formeVideo = "clip";
const decoupage = { reference: null, synopsis: "", contrainte: "", plans: [], resultat: null, perime: false };
let decoupagePret = false;

const enSequence = () => cibleVideo() && formeVideo === "sequence";

function majForme() {
    document.getElementById("vue-scenario").classList.toggle("forme-sequence", enSequence());
    if (enSequence()) preparerDecoupage();
}

document.querySelectorAll('input[name="forme-video"]').forEach((radio) => radio.addEventListener("change", () => {
    formeVideo = radio.value;
    majForme();
}));

function preparerDecoupage() {
    if (!decoupagePret) {
        decoupagePret = true;
        const max = optionsPlateau.video.sequence.plans_max;
        remplirSelect("nombre-plans-scenario", Array.from({ length: max - 1 }, (_, i) => [String(i + 2), String(i + 2)]), "3");
        decoupage.plans = [nouveauPlanDecoupage(0), nouveauPlanDecoupage(1, "prolonger"), nouveauPlanDecoupage(2)];
    }
    rendreReference();
    rendrePlansDecoupage();
    rendreResultat();
}

const nouveauPlanDecoupage = (k, enchainement) => ({
    enchainement: enchainement ?? (k === 0 ? (decoupage.reference ? "rush" : "texte") : "couper"),
    duree: 5, note: "", rush: null,
});

// Le réglage a changé : le découpage affiché ne lui correspond plus (on le garde, il peut avoir été retouché).
function perime() {
    if (decoupage.resultat) {
        decoupage.perime = true;
        rendreResultat();
    }
}

function caseImage(id, attribut, titre) {
    return `<div class="emplacement ${id ? "plein" : "vide"}" ${attribut} title="${echapper(titre)}">${id
        ? `<img src="${urlImage(id)}" alt="" draggable="false">
           <button type="button" class="petit retirer" data-retirer="1" title="Retirer">✕</button>`
        : '<span class="numero">＋</span>'}</div>`;
}

function rendreReference() {
    const id = decoupage.reference;
    const caseRef = document.getElementById("reference-scenario");
    caseRef.className = `emplacement ${id ? "plein" : "vide"}`;
    caseRef.innerHTML = id
        ? `<img src="${urlImage(id)}" alt="" draggable="false">
           <button type="button" class="petit retirer" data-retirer="1" title="Retirer">✕</button>`
        : '<span class="numero">👤</span>';
}

function rendrePlansDecoupage() {
    const durees = optionsPlateau.video.durees;
    document.getElementById("nombre-plans-scenario").value = String(decoupage.plans.length);
    document.getElementById("plans-scenario").innerHTML = decoupage.plans.map((p, k) => {
        const types = k === 0 ? TYPES_DEPART : TYPES_SUITE;
        return `<li class="plan-scenario" data-plan="${k}">
            <strong>Plan ${k + 1}</strong>
            <select data-champ="enchainement">${types.map(([v, t]) => `<option value="${v}"
                ${v === p.enchainement ? "selected" : ""} ${v === "rush" && !decoupage.reference ? "disabled" : ""}>${t}</option>`).join("")}</select>
            <select data-champ="duree">${durees.map((d) =>
                `<option value="${d}" ${d === p.duree ? "selected" : ""}>${d} s</option>`).join("")}</select>
            <input data-champ="note" type="text" maxlength="500" value="${echapper(p.note)}"
                   placeholder="${echapper(NOTES_TYPES[p.enchainement])}">
            ${p.enchainement === "transition"
                ? caseImage(p.rush, "data-arrivee", "Rush d'arrivée (facultatif : sinon, à choisir dans le Montage)") : ""}
        </li>`;
    }).join("");
    const total = decoupage.plans.reduce((t, p) => t + p.duree, 0);
    document.getElementById("duree-sequence-scenario").textContent = `film d'environ ${total} s`;
}

function rendreResultat() {
    const r = decoupage.resultat;
    document.getElementById("creation-sequence-scenario").hidden = !r;
    document.getElementById("alertes-decoupage").innerHTML = [
        ...(decoupage.perime ? ["Tes réglages ont changé depuis ce découpage : redécoupe pour en tenir compte."] : []),
        ...(r?.alertes ?? []),
    ].map((a) => `<li>${echapper(a)}</li>`).join("");
    document.getElementById("resultat-decoupage").innerHTML = (r?.plans ?? []).map((p, k) => `
        <li class="plan-decoupe" data-plan="${k}">
            <div class="ligne entete-decoupe">
                <strong>Plan ${k + 1}</strong>
                <span class="discret">${echapper([...TYPES_DEPART, ...TYPES_SUITE].find(([v]) => v === p.enchainement)?.[1] ?? "")}
                    · ${p.duree} s</span>
                <button type="button" class="petit" data-peaufiner title="Peaufiner ce plan avec Llama">✨</button>
            </div>
            <input data-champ="idee" type="text" maxlength="1000" value="${echapper(p.idee)}" title="Résumé en français (affiché dans le Montage)">
            <textarea data-champ="prompt" rows="5">${echapper(p.prompt)}</textarea>
            <small class="discret mots-decoupe">${p.prompt.split(/\s+/).filter(Boolean).length} mots</small>
        </li>`).join("");
}

// Événements : référence, synopsis, nombre et réglages des plans

document.getElementById("reference-scenario").addEventListener("click", (e) => {
    if (e.target.closest("[data-retirer]")) {
        decoupage.reference = null;
        if (decoupage.plans[0]?.enchainement === "rush") decoupage.plans[0].enchainement = "texte";
        rendreReference();
        rendrePlansDecoupage();
        perime();
        return;
    }
    ouvrirChoixRush((id) => {
        decoupage.reference = id;
        rendreReference();
        rendrePlansDecoupage();
        perime();
    }, "Quelle image de référence pour le personnage ?");
});

document.getElementById("synopsis-scenario").addEventListener("input", (e) => {
    decoupage.synopsis = e.target.value;
    perime();
});

document.getElementById("contrainte-scenario").addEventListener("input", (e) => {
    decoupage.contrainte = e.target.value;  // donnée à Llama, puis gardée au casting de la séquence créée
    perime();
});

document.getElementById("nombre-plans-scenario").addEventListener("change", (e) => {
    const n = Number(e.target.value);
    decoupage.plans = decoupage.plans.slice(0, n);
    while (decoupage.plans.length < n) decoupage.plans.push(nouveauPlanDecoupage(decoupage.plans.length));
    rendrePlansDecoupage();
    perime();
});

const plansScenario = document.getElementById("plans-scenario");
plansScenario.addEventListener("change", (e) => {
    const champ = e.target.dataset.champ;
    if (!champ || champ === "note") return;
    const p = decoupage.plans[Number(e.target.closest(".plan-scenario").dataset.plan)];
    p[champ] = champ === "duree" ? Number(e.target.value) : e.target.value;
    if (champ === "enchainement" && p.enchainement !== "transition") p.rush = null;
    rendrePlansDecoupage();
    perime();
});
plansScenario.addEventListener("input", (e) => {
    if (e.target.dataset.champ !== "note") return;
    decoupage.plans[Number(e.target.closest(".plan-scenario").dataset.plan)].note = e.target.value;
    perime();
});
plansScenario.addEventListener("click", (e) => {
    const caseArrivee = e.target.closest("[data-arrivee]");
    if (!caseArrivee) return;
    const p = decoupage.plans[Number(caseArrivee.closest(".plan-scenario").dataset.plan)];
    if (e.target.closest("[data-retirer]")) {
        p.rush = null;
        rendrePlansDecoupage();
        perime();
        return;
    }
    ouvrirChoixRush((id) => {
        p.rush = id;
        rendrePlansDecoupage();
        perime();
    }, "Sur quel rush finit la transition ?");
});

// Découpage par Llama

document.getElementById("decouper-scenario").addEventListener("click", () => {
    const bouton = document.getElementById("decouper-scenario");
    bouton.disabled = true;
    avecLlama(async (statut) => {
        statut("Llama découpe la séquence… (le premier appel charge le modèle)");
        const etat = etatPrompt();
        const choix = Object.fromEntries(Object.entries(etat.choix).filter(([bloc]) =>
            !structurePrompt.blocs.find((b) => b.id === bloc)?.video));  // le Plan d'un clip seul ne compte pas ici
        const r = await api("POST", "/api/sequences/decouper", {
            synopsis: decoupage.synopsis, sujet: sujetPrompt, choix, age: etat.age,
            plans: decoupage.plans, reference: decoupage.reference, contrainte: decoupage.contrainte, llm: llmPrompt(),
        });
        decoupage.resultat = r;
        decoupage.perime = false;
        rendreResultat();
        statut(`Découpage en ${r.duree_s} s · relis et retouche chaque plan si besoin`);
    }).finally(() => { bouton.disabled = false; });
});

const resultatDecoupage = document.getElementById("resultat-decoupage");
resultatDecoupage.addEventListener("input", (e) => {
    const champ = e.target.dataset.champ;
    if (!champ) return;
    const k = Number(e.target.closest(".plan-decoupe").dataset.plan);
    decoupage.resultat.plans[k][champ] = e.target.value;
    if (champ === "prompt") {
        e.target.closest(".plan-decoupe").querySelector(".mots-decoupe").textContent =
            `${e.target.value.split(/\s+/).filter(Boolean).length} mots`;
    }
});
resultatDecoupage.addEventListener("click", (e) => {
    if (!e.target.closest("[data-peaufiner]")) return;
    const li = e.target.closest(".plan-decoupe");
    const p = decoupage.resultat.plans[Number(li.dataset.plan)];
    e.target.disabled = true;
    avecLlama(async (statut) => {
        statut(`Llama peaufine le plan ${Number(li.dataset.plan) + 1}…`);
        const r = await api("POST", "/api/prompt/peaufiner",
                            { prompt: p.prompt, llm: llmPrompt(), workflow: ciblePrompt(), duree: p.duree });
        p.prompt = r.prompt;
        rendreResultat();
        statut(`Plan peaufiné en ${r.duree_s} s`);
    }).finally(() => { e.target.disabled = false; });
});

// Création de la séquence dans le Montage : les prompts du découpage sont imposés (pas de nouvelle réécriture).

document.getElementById("creer-sequence-scenario").addEventListener("click", async () => {
    const titre = document.getElementById("titre-sequence-scenario");
    const erreur = document.getElementById("erreur-prompt");
    erreur.hidden = true;
    if (!titre.value.trim()) {
        titre.focus();
        erreur.textContent = "Donne un titre à la séquence : ses plans seront rangés dans un dossier à son nom.";
        erreur.hidden = false;
        return;
    }
    try {
        const s = await api("POST", "/api/sequences", {
            titre: titre.value.trim(), qualite: "rapide", format: "16:9 (Widescreen)", llm: llmPrompt(),
            casting: { personnage: decoupage.reference, voix: "auto", ambiance: "",
                       contrainte: decoupage.resultat.contrainte ?? "", reference_partout: true },
            plans: decoupage.resultat.plans.map((p, k) => ({
                idee: p.idee || p.note || `Plan ${k + 1}`, prompt: p.prompt, duree: p.duree,
                enchainement: p.enchainement, rush: p.rush, ambiance: true,
            })),
        });
        await ouvrirSequence(s.id);
    } catch (err) {
        erreur.textContent = err.message;
        erreur.hidden = false;
    }
});
