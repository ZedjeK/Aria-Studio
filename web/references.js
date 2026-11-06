// Scénario, onglet Image de référence : les images dont part le prompt. Image : 1 à 3 rushes dont s'inspirer
// (« Picture 1… » pour un modèle d'édition) ou une image de départ pour une variation ; Clip : le rush à animer ;
// Séquence : le personnage de référence (decoupage.js). Le LLM les voit quand il écrit (/api/prompt/ecrire), et elles
// partent avec le prompt vers le Plateau ou le Montage.

const refsScenario = { role: "inspirer", inspirer: [], variation: null, liberte: 0.6, animer: null };

const AIDES_REFERENCE = {
    inspirer: "Jusqu'à 3 images : cite-les dans la Mise en scène ou les précisions (« la femme de l'image 1 sur le "
        + "dragon de l'image 2 »). Le prompt partira vers un modèle d'édition qui les reçoit.",
    variation: "Une seule image : sa composition est gardée, le prompt dit ce qui change. Plus de liberté = plus de "
        + "changements.",
    clip: "Le rush à animer : il devient la première image du clip. Sans lui, le clip part du texte seul.",
    sequence: "Le personnage de référence de la séquence (facultatif) : son visage et sa tenue tiennent sur les plans.",
};

// Les images qui comptent pour la forme en cours, et leur rôle.
function imagesReference() {
    if (formeScenario === "clip") return refsScenario.animer ? [refsScenario.animer] : [];
    if (formeScenario !== "image") return [];
    return refsScenario.role === "variation"
        ? (refsScenario.variation ? [refsScenario.variation] : [])
        : refsScenario.inspirer.filter(Boolean).slice(0, optionsPlateau?.references_max ?? 3);
}

function roleReference() {
    if (!imagesReference().length) return null;
    return formeScenario === "clip" ? "animer" : refsScenario.role;
}

function caseReference(id, index, titre) {
    return id
        ? `<div class="emplacement plein" data-index="${index}">
               <img src="${urlImage(id)}" alt="" draggable="false">
               ${formeScenario === "image" && refsScenario.role === "inspirer" ? `<span class="numero">${index + 1}</span>` : ""}
               <button type="button" class="petit retirer" data-retirer="${index}" title="Retirer">✕</button>
           </div>`
        : `<div class="emplacement vide" data-index="${index}" title="${echapper(titre)}">
               <span class="numero">${formeScenario === "clip" ? "🎬" : index + 1}</span></div>`;
}

function rendreReferencesScenario() {
    const image = formeScenario === "image";
    const inspirer = image && refsScenario.role === "inspirer";
    const max = inspirer ? (optionsPlateau?.references_max ?? 3) : 1;
    const ids = formeScenario === "clip" ? [refsScenario.animer]
        : inspirer ? refsScenario.inspirer : [refsScenario.variation];
    document.getElementById("cases-reference-scenario").innerHTML = Array.from({ length: max }, (_, i) =>
        caseReference(ids[i] ?? null, i, formeScenario === "clip" ? "Clique pour choisir le rush à animer"
            : "Clique pour choisir un rush")).join("");
    document.getElementById("bloc-liberte-scenario").hidden = !(image && refsScenario.role === "variation");
    document.querySelector(`input[name="role-reference"][value="${refsScenario.role}"]`).checked = true;
    document.getElementById("liberte-scenario").value = refsScenario.liberte;
    document.getElementById("valeur-liberte-scenario").textContent = refsScenario.liberte.toFixed(2).replace(".", ",");
    document.getElementById("aide-reference-scenario").textContent =
        AIDES_REFERENCE[formeScenario === "image" ? refsScenario.role : formeScenario];
    const n = formeScenario === "sequence" ? (decoupage.reference ? 1 : 0) : imagesReference().length;
    document.getElementById("compte-reference-scenario").textContent = n ? `· ${n}` : "";
    const role = roleReference();
    document.getElementById("peaufiner-prompt").textContent = role ? "✨ Écrire avec l'image" : "✨ Peaufiner";
    document.getElementById("peaufiner-prompt").title = role
        ? "Le LLM voit l'image et écrit le prompt qui s'en sert" : "Le LLM rend le prompt fluide, sans rien ajouter";
}

// Un changement d'images peut changer de famille de modèles (édition pour s'inspirer) : la liste est refaite.
async function referencesChangees() {
    peaufinage = null;
    document.getElementById("peaufine-prompt").hidden = true;
    rendreReferencesScenario();
    await chargerModelesPrompt();
    planifierAssemblage();
}

document.getElementById("cases-reference-scenario").addEventListener("click", (e) => {
    const retirer = e.target.closest("[data-retirer]");
    const index = Number(e.target.closest(".emplacement")?.dataset.index ?? 0);
    const poser = (id) => {
        if (formeScenario === "clip") refsScenario.animer = id;
        else if (refsScenario.role === "variation") refsScenario.variation = id;
        else {
            refsScenario.inspirer = refsScenario.inspirer.filter((r) => r && r !== id);
            refsScenario.inspirer.splice(Math.min(index, refsScenario.inspirer.length), 0, id);
        }
        referencesChangees();
    };
    if (retirer) {
        if (formeScenario === "clip") refsScenario.animer = null;
        else if (refsScenario.role === "variation") refsScenario.variation = null;
        else refsScenario.inspirer.splice(index, 1);
        referencesChangees();
        return;
    }
    if (e.target.closest(".emplacement.vide")) {
        ouvrirChoixRush(poser, formeScenario === "clip" ? "Quel rush animer ?" : "Quelle image de référence ?");
    }
});

document.querySelectorAll('input[name="role-reference"]').forEach((radio) => radio.addEventListener("change", () => {
    refsScenario.role = radio.value;
    referencesChangees();
}));

document.getElementById("liberte-scenario").addEventListener("input", (e) => {
    refsScenario.liberte = Number(e.target.value);
    document.getElementById("valeur-liberte-scenario").textContent = refsScenario.liberte.toFixed(2).replace(".", ",");
    garderBrouillon();
});
