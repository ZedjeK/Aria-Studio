// Section LoRAs : seulement celles de la famille du modèle choisi (dossier modeles/<Famille>/loras). La même pour
// le Plateau (création, retouche), un clip du Montage et une séquence (famille MiniMax-H3).
const tousWorkflows = () => optionsPlateau
    ? [...optionsPlateau.workflows.texte_vers_image.choix, ...optionsPlateau.workflows.retouche.choix] : [];
// La force maximale d'une LoRA, donnée par le serveur avec les options (modeles.FORCE_LORA).
const forceMaxLora = () => (optionsPlateau ?? optionsMontage)?.lora_force_max ?? 2;

function afficherLoras(idConteneur, idSelect) {
    const { workflow } = choixModele(idSelect);
    rendreLoras(idConteneur, tousWorkflows().find((x) => x.id === workflow));
}

// `w` : le workflow (sa famille et ses LoRAs). `choisies` : [{fichier, force, declencheurs}] à cocher (une séquence
// rouverte) ; sinon, ce qui est déjà coché ou réglé est gardé quand la liste est refaite.
function rendreLoras(idConteneur, w, choisies = null) {
    const conteneur = document.getElementById(idConteneur);
    if (!w) {
        conteneur.innerHTML = "";
        return;
    }
    const avant = Object.fromEntries((choisies ? choisies.map((l) => ({ ...l, actif: true }))
        : lorasChoisies(idConteneur, true)).map((l) => [l.fichier, l]));
    if (!w.loras.length) {
        conteneur.innerHTML = `<h3>LoRAs · ${echapper(w.famille)}</h3>
            <p class="discret">Aucune LoRA : dépose des .safetensors dans modeles/${echapper(w.famille)}/loras.</p>`;
        return;
    }
    const ouvert = conteneur.querySelector("details")?.open ?? w.loras.some((l) => avant[l.fichier]?.actif);
    conteneur.innerHTML = `<details class="choix-loras" ${ouvert ? "open" : ""}>
        <summary><h3>LoRAs · ${echapper(w.famille)} <span class="compte-loras"></span></h3></summary>`
        + w.loras.map((l) => {
            const a = avant[l.fichier];
            const force = a ? a.force : l.force;
            return `<div class="lora ${a?.actif ? "active" : ""}" data-fichier="${echapper(l.fichier)}">
                <label title="${echapper(l.fichier)}"><input type="checkbox" ${a?.actif ? "checked" : ""}> ${echapper(l.nom)}</label>
                <input type="range" min="0" max="${forceMaxLora()}" step="0.05" value="${force}">
                <span class="valeur">${virgule(force)}</span>
                <input type="text" placeholder="mots déclencheurs (facultatif)" value="${echapper(a ? a.declencheurs : l.declencheurs)}">
            </div>`;
        }).join("") + `<p class="conseil-loras" hidden>💡 Plusieurs LoRAs cochées : baisse un peu la force de chacune
            (vers 0,6 à 0,8), sinon elles se marchent dessus et « cuisent » l'image.</p></details>`;
    majConseilLoras(conteneur);
}

// Le conseil quand plusieurs LoRAs sont cochées, et le compte dans le titre replié.
function majConseilLoras(conteneur) {
    const cochees = conteneur.querySelectorAll(".lora input[type=checkbox]:checked").length;
    const conseil = conteneur.querySelector(".conseil-loras");
    if (conseil) conseil.hidden = cochees < 2;
    const compte = conteneur.querySelector(".compte-loras");
    if (compte) compte.textContent = cochees ? `· ${cochees} active${cochees > 1 ? "s" : ""}` : "· aucune active";
}

function lorasChoisies(idConteneur, toutes = false) {
    return [...document.querySelectorAll(`#${idConteneur} .lora`)]
        .map((ligne) => ({
            fichier: ligne.dataset.fichier,
            actif: ligne.querySelector("input[type=checkbox]").checked,
            force: Number(ligne.querySelector("input[type=range]").value),
            declencheurs: ligne.querySelector("input[type=text]").value,
        }))
        .filter((l) => toutes || l.actif)
        .map(({ actif, ...l }) => (toutes ? { actif, ...l } : l));
}

// `quandChange` : appelé à chaque réglage (une séquence s'en sert pour savoir qu'elle est à retourner).
function brancherLoras(id, quandChange = null) {
    const conteneur = document.getElementById(id);
    conteneur.addEventListener("input", (e) => {
        const ligne = e.target.closest(".lora");
        if (!ligne) return;
        if (e.target.type === "range") ligne.querySelector(".valeur").textContent = virgule(e.target.value);
        if (e.target.type === "checkbox") {
            ligne.classList.toggle("active", e.target.checked);
            majConseilLoras(conteneur);
        }
        if (quandChange) quandChange();
    });
}
["loras-creation", "loras-retouche", "loras-montage"].forEach((id) => brancherLoras(id));
