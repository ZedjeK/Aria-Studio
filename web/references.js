// Scénario, 1 · Départ : les images dont part l'atelier. Image : aucune image (texte vers image), 1 à 10 rushes dont s'inspirer (« Picture 1… »
// pour un modèle d'édition) ou une image de départ pour une variation ; Retouche : l'image à retoucher ; Clip : le
// rush à animer ; Séquence : le personnage de référence (decoupage.js). Le LLM les voit quand il écrit (✨ Écrire le
// prompt, 💬 M'aider à préciser), et elles partent avec le prompt vers le Plateau, le panneau de retouche ou le Montage.

// role : texte (aucune image, le mode de base), inspirer ou variation. Les images posées pour S'inspirer sont gardées
// quand on change de mode, mais seul le mode choisi les envoie.
const refsScenario = { role: "texte", inspirer: [], variation: null, liberte: 0.6, animer: null, retouche: null };

const maxReferencesScenario = () => optionsPlateau?.references_max ?? 10;

const AIDES_REFERENCE = {
    texte: "Pas d'image : le modèle crée tout à partir de ton idée et de tes compléments (sujet, décor, mise en scène).",
    inspirer: () => "Tes images sont des ingrédients : le modèle d'édition les regarde et compose une nouvelle scène "
        + "(format libre). Jusqu'à " + maxReferencesScenario() + " images, une case de plus apparaît quand les autres "
        + "sont remplies. Cite-les par numéro dans ton idée (« la femme de l'image 1 sur le dragon de l'image 2 »). "
        + "Le LLM écrit le prompt en gardant ce que chaque image montre, sauf si tu demandes de le changer. "
        + "Pas de curseur de force.",
    variation: "Ton image est le point de départ : le modèle la redessine en partie. Composition, sujets et style "
        + "sont gardés, ton idée dit ce qui change. La Liberté (le curseur du Plateau, de 0,3 à 0,9) décide de la "
        + "force : plus elle monte, plus l'image change. Une seule image, au format de l'original.",
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
    if (formeScenario === "image" && refsScenario.role === "texte") return [];  // texte vers image : aucune image
    const seule = caseSeule();
    if (seule) return refsScenario[seule] ? [refsScenario[seule]] : [];
    return refsScenario.inspirer.filter(Boolean).slice(0, maxReferencesScenario());
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
    // S'inspirer : trois cases au départ, une de plus dès que toutes sont remplies, jusqu'au maximum du modèle.
    const nombre = seule ? 1 : Math.min(maxReferencesScenario(), Math.max(3, ids.filter(Boolean).length + 1));
    const sansImage = formeScenario === "image" && refsScenario.role === "texte";
    document.getElementById("cases-reference-scenario").innerHTML = sansImage ? "" : Array.from({ length: nombre }, (_, i) =>
        caseReference(ids[i] ?? null, i, TITRES_CASE[formeScenario] ?? "Clique pour choisir un rush")).join("");
    document.querySelector(`input[name="role-reference"][value="${refsScenario.role}"]`).checked = true;
    const aide = AIDES_REFERENCE[formeScenario === "image" ? refsScenario.role : formeScenario];
    document.getElementById("aide-reference-scenario").textContent = typeof aide === "function" ? aide() : aide;
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

