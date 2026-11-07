// Scénario, 1 · Départ : les images dont part l'atelier. Image : 1 à 3 rushes dont s'inspirer (« Picture 1… »
// pour un modèle d'édition) ou une image de départ pour une variation ; Retouche : l'image à retoucher ; Clip : le
// rush à animer ; Séquence : le personnage de référence (decoupage.js). Le LLM les voit quand il écrit (✨ Optimiser,
// 💬 Aide), et elles partent avec le prompt vers le Plateau, le panneau de retouche ou le Montage.

const refsScenario = { role: "inspirer", inspirer: [], variation: null, liberte: 0.6, animer: null, retouche: null };

const AIDES_REFERENCE = {
    inspirer: "Jusqu'à 3 images : cite-les dans ton idée (« la femme de l'image 1 sur le dragon de l'image 2 »). Le "
        + "prompt partira vers un modèle d'édition qui les reçoit.",
    variation: "Une seule image : sa composition est gardée, ton idée dit ce qui change. La force du changement "
        + "(Liberté) se règle au Plateau.",
    retouche: "L'image à retoucher : le LLM la regarde pour écrire une consigne qui lui correspond. Le masque se "
        + "dessine ensuite dans le panneau de retouche.",
    clip: "Le rush à animer : il devient la première image du clip. Sans lui, le clip part du texte seul.",
    sequence: "Le personnage de référence de la séquence (facultatif) : son visage et sa tenue tiennent sur les plans.",
};

// Une case seule (clip, retouche, variation) : la clé de refsScenario qui la tient. Sinon : les références (liste).
function caseSeule() {
    if (formeScenario === "clip") return "animer";
    if (formeScenario === "retouche") return "retouche";
    return formeScenario === "image" && refsScenario.role === "variation" ? "variation" : null;
}

// Les images qui comptent pour l'atelier en cours, et leur rôle.
function imagesReference() {
    if (formeScenario === "sequence") return [];
    const seule = caseSeule();
    if (seule) return refsScenario[seule] ? [refsScenario[seule]] : [];
    return refsScenario.inspirer.filter(Boolean).slice(0, optionsPlateau?.references_max ?? 3);
}

function roleReference() {
    if (!imagesReference().length) return null;
    return { clip: "animer", retouche: "retouche" }[formeScenario] ?? refsScenario.role;
}

function caseReference(id, index, titre) {
    const numero = { clip: "🎬", retouche: "🖌️" }[formeScenario] ?? index + 1;
    return id
        ? `<div class="emplacement plein" data-index="${index}">
               <img src="${urlImage(id)}" alt="" draggable="false">
               ${caseSeule() ? "" : `<span class="numero">${index + 1}</span>`}
               <button type="button" class="petit retirer" data-retirer="${index}" title="Retirer">✕</button>
           </div>`
        : `<div class="emplacement vide" data-index="${index}" title="${echapper(titre)}">
               <span class="numero">${numero}</span></div>`;
}

const TITRES_CASE = { clip: "Clique pour choisir le rush à animer", retouche: "Clique pour choisir l'image à retoucher" };

function rendreReferencesScenario() {
    const seule = caseSeule();
    const ids = seule ? [refsScenario[seule]] : refsScenario.inspirer;
    const max = seule ? 1 : (optionsPlateau?.references_max ?? 3);
    document.getElementById("cases-reference-scenario").innerHTML = Array.from({ length: max }, (_, i) =>
        caseReference(ids[i] ?? null, i, TITRES_CASE[formeScenario] ?? "Clique pour choisir un rush")).join("");
    document.querySelector(`input[name="role-reference"][value="${refsScenario.role}"]`).checked = true;
    document.getElementById("aide-reference-scenario").textContent =
        AIDES_REFERENCE[formeScenario === "image" ? refsScenario.role : formeScenario];
}

// Un changement d'images peut changer de famille de modèles (édition pour s'inspirer) : la liste est refaite.
async function referencesChangees() {
    rendreReferencesScenario();
    await chargerModelesPrompt();
    planifierAssemblage();
}

document.getElementById("cases-reference-scenario").addEventListener("click", (e) => {
    const retirer = e.target.closest("[data-retirer]");
    const index = Number(e.target.closest(".emplacement")?.dataset.index ?? 0);
    const seule = caseSeule();
    const poser = (id) => {
        if (seule) refsScenario[seule] = id;
        else {
            refsScenario.inspirer = refsScenario.inspirer.filter((r) => r && r !== id);
            refsScenario.inspirer.splice(Math.min(index, refsScenario.inspirer.length), 0, id);
        }
        referencesChangees();
    };
    if (retirer) {
        if (seule) refsScenario[seule] = null;
        else refsScenario.inspirer.splice(index, 1);
        referencesChangees();
        return;
    }
    if (e.target.closest(".emplacement.vide")) {
        const titres = { clip: "Quel rush animer ?", retouche: "Quelle image retoucher ?" };
        ouvrirChoixRush(poser, titres[formeScenario] ?? "Quelle image de référence ?");
    }
});

document.querySelectorAll('input[name="role-reference"]').forEach((radio) => radio.addEventListener("change", () => {
    refsScenario.role = radio.value;
    referencesChangees();
}));

