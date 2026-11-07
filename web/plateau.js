// Plateau : création (idée → LLM → image), suivi de la commande, rushes et fiche détaillée.

const urlImage = (id) => `/api/creations/${encodeURIComponent(id)}/image`;
let ficheOuverte = null;

let optionsPlateau = null;

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
document.getElementById("workflow-creation").addEventListener("change", () => {
    afficherLoras("loras-creation", "workflow-creation");
    // Changer de modèle propose son format par défaut (réglable dans l'onglet Réglages).
    const w = tousWorkflows().find((x) => x.id === choixModele("workflow-creation").workflow);
    if (w?.format) document.getElementById("format-creation").value = w.format;
});
document.getElementById("workflow-retouche").addEventListener("change", () => {
    afficherLoras("loras-retouche", "workflow-retouche");
});

// Le LLM qui écrit pour un workflow (onglets d'usage de Modèles), sinon le LLM par défaut : le Scénario le propose.
const llmDuWorkflow = (id, o = optionsPlateau) => o?.llm.par_workflow?.[id] ?? o?.llm.defaut;

function remplirSelect(id, valeurs, defaut) {
    const select = document.getElementById(id);
    const garde = valeurs.some(([v]) => v === select.value) ? select.value : defaut;  // garde le choix en cours
    select.innerHTML = valeurs.map(([valeur, texte]) =>
        `<option value="${echapper(valeur)}" ${valeur === garde ? "selected" : ""}>${echapper(texte)}</option>`).join("");
}

// Une ligne par modèle : « workflow::fichier ». Le modèle d'origine d'abord, puis les variantes de la famille.
const lignesModeles = (tache) => tache.choix.flatMap((w) => w.modeles.map((m) => [
    `${w.id}::${m}`,
    (m === w.origine ? `${w.nom}${w.id === tache.defaut ? " ★" : ""}` : `   ↳ ${w.nom} · ${m.replace(/\.safetensors$/, "")}`)
        + (m === w.modele_defaut && m !== w.origine ? " (défaut)" : ""),
]));

// Les listes sont relues à chaque affichage du Plateau : un modèle déposé dans son dossier y apparaît aussitôt.
async function chargerOptionsPlateau() {
    const o = await api("GET", "/api/plateau/options");
    optionsPlateau = o;
    const t2i = o.workflows.texte_vers_image;
    remplirModelesCreation();
    const edit = o.workflows.retouche;
    const origineEdit = edit.choix.find((w) => w.id === edit.defaut);
    remplirSelect("workflow-retouche", lignesModeles(edit), origineEdit && `${origineEdit.id}::${origineEdit.modele_defaut}`);
    const choisi = t2i.choix.find((w) => w.id === choixModele("workflow-creation").workflow);
    remplirSelect("format-creation", o.formats.map((f) => [f, f]), choisi?.format ?? o.formats[0]);
    remplirSelect("nombre-creation", Array.from({ length: o.nombre_max }, (_, i) => [String(i + 1), String(i + 1)]), "1");
    afficherLoras("loras-creation", "workflow-creation");
    afficherLoras("loras-retouche", "workflow-retouche");
    rendreReferences();
}

// Références : 1 à 3 rushes pour S'inspirer (nouvelle scène, format au choix), 1 pour une Variation.
// Les modèles proposés changent avec le mode : édition (Qwen 2.1 Edit, 2511) ou texte vers image (img2img).

let referencesChoisies = [];  // toutes celles posées, gardées même quand la Variation n'en utilise qu'une
let modeReference = "inspirer";

const maxReferences = () => (modeReference === "variation" ? 1 : (optionsPlateau?.references_max ?? 3));
const referencesActives = () => referencesChoisies.filter(Boolean).slice(0, maxReferences());

const AIDES_REFERENCES = {
    inspirer: "Glisse jusqu'à 3 rushes (ou images de ton disque) dans les cases, ou « Comme référence » dans la fiche "
        + "d'un rush. Cite-les dans ton idée : « la femme de l'image 1 sur le dragon de l'image 2 ».",
    variation: "Une seule image : sa composition est gardée. Plus de liberté = plus de changements. Le format est celui "
        + "de l'image.",
};

function listeCreation() {
    const o = optionsPlateau;
    const n = referencesActives().length;
    if (n && modeReference === "inspirer") {
        return { ...o.workflows.retouche, defaut: o.references_defaut,
                 choix: o.workflows.retouche.choix.filter((w) => w.references >= n) };
    }
    if (n && modeReference === "variation") {
        return { ...o.workflows.texte_vers_image, choix: o.workflows.texte_vers_image.choix.filter((w) => w.variation) };
    }
    return o.workflows.texte_vers_image;
}

function remplirModelesCreation() {
    if (!optionsPlateau) return;
    const t = listeCreation();
    const origine = t.choix.find((w) => w.id === t.defaut) ?? t.choix[0];
    // Présélection : le checkpoint par défaut réglé dans l'onglet Réglages (l'origine tant que rien n'est réglé).
    remplirSelect("workflow-creation", lignesModeles(t), origine && `${origine.id}::${origine.modele_defaut}`);
    afficherLoras("loras-creation", "workflow-creation");
    const variation = referencesActives().length > 0 && modeReference === "variation";
    const format = document.getElementById("format-creation");
    format.disabled = variation;
    format.title = variation ? "Une variation garde le format de son image de référence" : "";
}

const liberteReference = () => Number(document.getElementById("liberte-reference").value);

function rendreReferences() {
    referencesChoisies = referencesChoisies.filter(Boolean);
    const max = maxReferences();
    document.getElementById("emplacements-references").innerHTML = Array.from({ length: max }, (_, i) => {
        const id = referencesChoisies[i];
        return id
            ? `<div class="emplacement plein" data-index="${i}">
                   <img src="${urlImage(id)}" alt="" draggable="false">
                   <span class="numero">${i + 1}</span>
                   <button type="button" class="petit retirer" data-retirer="${i}" title="Retirer">✕</button>
               </div>`
            : `<div class="emplacement vide" data-index="${i}"
                   title="Glisse un rush ou une image ici, ou clique pour choisir un fichier"><span class="numero">${i + 1}</span></div>`;
    }).join("");
    const n = referencesActives().length;
    document.getElementById("compte-references").textContent = n
        ? `· ${n} image${n > 1 ? "s" : ""} · ${modeReference === "inspirer" ? "s'inspirer" : "variation"}` : "";
    document.getElementById("bloc-liberte").hidden = modeReference !== "variation";
    document.getElementById("aide-references").textContent = AIDES_REFERENCES[modeReference];
    remplirModelesCreation();
}

function placerReference(id, index = null) {
    referencesChoisies = referencesChoisies.filter((r) => r !== id);
    const position = index ?? Math.min(referencesChoisies.length, maxReferences() - 1);
    referencesChoisies[position] = id;
    document.getElementById("zone-references").open = true;
    rendreReferences();
}

document.querySelectorAll('input[name="mode-reference"]').forEach((radio) => {
    radio.addEventListener("change", () => {
        modeReference = radio.value;
        rendreReferences();
    });
});

document.getElementById("liberte-reference").addEventListener("input", (e) => {
    document.getElementById("valeur-liberte").textContent = virgule(e.target.value);
});

const emplacements = document.getElementById("emplacements-references");
let indexFichierReference = 0;

emplacements.addEventListener("click", (e) => {
    const retirer = e.target.closest("[data-retirer]");
    if (retirer) {
        referencesChoisies.splice(Number(retirer.dataset.retirer), 1);
        rendreReferences();
        return;
    }
    const vide = e.target.closest(".emplacement.vide");
    if (vide) {
        indexFichierReference = Number(vide.dataset.index);
        document.getElementById("fichier-reference").click();
    }
});

document.getElementById("fichier-reference").addEventListener("change", async (e) => {
    const fichier = e.target.files[0];
    if (!fichier) return;
    const f = await importerFichier(fichier, false);  // l'image rejoint les rushes, puis la case
    if (f) placerReference(f.id, indexFichierReference);
    e.target.value = "";
});

// Glisser dans une case : un rush de la galerie, ou un fichier venu du disque (sans ouvrir sa fiche).
emplacements.addEventListener("dragover", (e) => {
    const types = e.dataTransfer?.types ?? [];
    if (!e.target.closest(".emplacement") || !(types.includes("text/aria-rush") || types.includes("Files"))) return;
    e.preventDefault();
    e.target.closest(".emplacement").classList.add("survol");
});
emplacements.addEventListener("dragleave", (e) => e.target.closest(".emplacement")?.classList.remove("survol"));
emplacements.addEventListener("drop", async (e) => {
    const cible = e.target.closest(".emplacement");
    if (!cible) return;
    e.preventDefault();
    e.stopPropagation();  // pas l'import global du Plateau (qui ouvrirait la fiche)
    survolsDepot = 0;
    voileDepot.hidden = true;
    cible.classList.remove("survol");
    const index = Number(cible.dataset.index);
    const rush = e.dataTransfer.getData("text/aria-rush");
    if (rush) {
        placerReference(rush, index);
        return;
    }
    const fichier = [...e.dataTransfer.files].find((f) => TYPES_IMPORT.includes(f.type));
    if (!fichier) {
        alert("Dépose une image PNG, JPEG ou WebP.");
        return;
    }
    const f = await importerFichier(fichier, false);
    if (f) placerReference(f.id, index);
});

// Le Scénario envoie un prompt prêt : il part tel quel, avec son modèle cible et ses images de référence.
// Le prompt reçu du Scénario, avec son idée et le LLM qui l'a écrit : ils partent avec lui tant qu'il n'est pas changé.
let envoiScenario = null;

async function recevoirAuPlateau({ prompt, workflow, images, role, liberte = null, lancer, idee = "", llm = null }) {
    envoiScenario = { prompt: prompt.trim(), idee, llm };
    await chargerOptionsPlateau();
    referencesChoisies = [...images];
    if (role) {
        modeReference = role;
        document.querySelector(`input[name="mode-reference"][value="${role}"]`).checked = true;
        document.getElementById("zone-references").open = true;
    }
    if (role === "variation" && liberte !== null) {
        document.getElementById("liberte-reference").value = liberte;
        document.getElementById("valeur-liberte").textContent = virgule(liberte);
    }
    rendreReferences();  // refait la liste des modèles : édition pour s'inspirer, variation, texte vers image
    const w = listeCreation().choix.find((x) => x.id === workflow);
    if (w && choixModele("workflow-creation").workflow !== workflow) {  // même workflow : on garde la variante
        const select = document.getElementById("workflow-creation");
        select.value = `${w.id}::${w.origine}`;
        select.dispatchEvent(new Event("change"));
    }
    document.getElementById("prompt-plateau").value = prompt;
    document.querySelector('.onglet[data-vue="plateau"]').click();
    if (lancer) document.getElementById("form-creation").requestSubmit();
    else document.getElementById("prompt-plateau").focus();
}

function optionsReferences() {
    return referencesActives().length
        ? { references: referencesActives(), mode_reference: modeReference, liberte: liberteReference() }
        : {};
}

// Ouvrir une liste la relit : un fichier déposé pendant que la page est ouverte apparaît sans recharger.
["workflow-creation", "workflow-retouche"].forEach((id) =>
    document.getElementById(id).addEventListener("focus", chargerOptionsPlateau));

function choixModele(idSelect) {
    const [workflow, modele] = document.getElementById(idSelect).value.split("::");
    return { workflow, modele };
}

function optionsFormulaire() {
    const seed = document.getElementById("seed-creation").value;
    return {
        ...choixModele("workflow-creation"),
        format: document.getElementById("format-creation").value,
        nombre: Number(document.getElementById("nombre-creation").value),
        seed: seed === "" ? null : Number(seed),
        dossier: rushes.dossierOuvert(),  // créé depuis un dossier ouvert : toutes les images y sont rangées
        ...optionsReferences(),
    };
}

async function lancerCommande(corps, url = "/api/creations", erreur = document.getElementById("erreur-creation")) {
    erreur.hidden = true;
    let commande;
    try {
        commande = await api("POST", url, corps);
    } catch (err) {
        afficherErreur(erreur, err.message);
        return false;
    }
    document.getElementById("erreur-creation").hidden = true;
    suivreCommande(commande.id, Date.now());
    return true;
}

// Commandes du Plateau suivies en ce moment (création, variante, retouche) : la barre montre la dernière lancée,
// chacune affiche son résultat en finissant, et le formulaire se débloque quand toutes sont finies.
const suiviesPlateau = [];

async function suivreCommande(id, debut) {
    const bloc = document.getElementById("progression-creation");
    suiviesPlateau.push(id);
    document.querySelectorAll("#form-creation button").forEach((b) => { b.disabled = true; });
    bloc.hidden = false;
    suivreAnnulable(bloc, id);
    let c = null;
    try {
        c = await suivreJusquauBout(id, (r) => {
            if (suiviesPlateau.at(-1) !== id) return;
            const secondes = Math.round((Date.now() - debut) / 1000);
            bloc.querySelector(".barre div").style.width = `${Math.round(100 * r.progression)}%`;
            bloc.querySelector("span").textContent = r.etat === "attente"
                ? `En file d'attente… ${secondes} s`
                : `${r.etape || "Finalisation"} · ${Math.round(100 * r.progression)} % · ${secondes} s`;
        }, 700);
    } catch (err) {
        const erreur = document.getElementById("erreur-creation");
        afficherErreur(erreur, `Suivi interrompu (${err.message}) : la génération continue peut-être, regarde les rushes.`);
    } finally {
        suiviesPlateau.splice(suiviesPlateau.indexOf(id), 1);
        if (suiviesPlateau.length) {
            suivreAnnulable(bloc, suiviesPlateau.at(-1));
        } else {
            bloc.hidden = true;
            document.querySelectorAll("#form-creation button").forEach((b) => { b.disabled = false; });
        }
    }
    if (!c) return;
    if (c.etat === "erreur" || c.etat === "annule") {
        const erreur = document.getElementById("erreur-creation");
        afficherErreur(erreur, c.etat === "annule" ? messageAnnulation(c, "image", true) : c.erreur);
        if (!c.images.length) return;
        afficherResultat(c.images, c.prompt ?? "", c.type);
        chargerGalerie();
        return;
    }
    afficherResultat(c.images, c.prompt, c.type);
    chargerGalerie();
}

function afficherResultat(images, prompt, type = "creation") {
    document.getElementById("images-creation").innerHTML = images.map((id) =>
        `<img src="${urlImage(id)}" alt="" data-creation="${echapper(id)}">`).join("");
    // Une création reprend le prompt du champ ci-dessus ; une retouche montre la consigne envoyée au modèle.
    const consigne = document.getElementById("consigne-resultat");
    consigne.textContent = type === "creation" ? "" : `Consigne envoyée au modèle de retouche : ${prompt}`;
    consigne.hidden = type === "creation";
    document.getElementById("resultat-creation").hidden = false;
}

// Le prompt part tel quel : le LLM n'écrit qu'au Scénario.
document.getElementById("form-creation").addEventListener("submit", (e) => {
    e.preventDefault();
    const prompt = document.getElementById("prompt-plateau").value.trim();
    const meta = envoiScenario?.prompt === prompt ? { idee: envoiScenario.idee, ecrit_par: envoiScenario.llm } : {};
    lancerCommande({ prompt, ...meta, ...optionsFormulaire(), loras: lorasChoisies("loras-creation") });
});

document.getElementById("vers-scenario").addEventListener("click", () => ouvrirScenario("image"));

// Rushes : galerie rangée partagée avec les Bobines du Montage (web/galerie.js).

const rushes = creerGalerie({
    genre: "images",
    racine: document.getElementById("rushes-plateau"),
    mot: "image",
    vides: {
        tous: "Aucun rush pour l'instant : à toi de tourner !",
        non_ranges: "Tout est rangé, bravo !",
        archives: "Aucune archive. Archive les rushes que tu veux garder sans les voir partout.",
        corbeille: "La corbeille est vide.",
        dossier: "Dossier vide : glisse des vignettes sur son nom, ou crée directement depuis ce dossier.",
    },
    vignette: (f, choisi) => `<img src="${urlImage(f.id)}" alt="" loading="lazy" draggable="true"
        data-creation="${echapper(f.id)}" class="${choisi ? "choisi" : ""}">`,
    ouvrir: (f) => ouvrirFiche(f),
});

async function chargerGalerie() {
    await rushes.charger();
}

// Résultats de la dernière création (hors galerie) : un clic ouvre la fiche.
document.getElementById("images-creation").addEventListener("click", async (e) => {
    const id = e.target.dataset?.creation;
    if (id) ouvrirFiche(await api("GET", `/api/creations/${encodeURIComponent(id)}`));
});

const TYPES_CREATION = { creation: "Création", retouche: "Retouche", agrandissement: "Agrandissement", import: "Image importée" };

function ouvrirFiche(f) {
    ficheOuverte = f;
    document.getElementById("image-detail").src = urlImage(f.id);
    const lignes = [["Type", TYPES_CREATION[f.type] ?? f.type]];
    if (f.type === "import") {
        lignes.push(["Fichier d'origine", f.nom_origine || "—"], ["Taille", `${f.largeur}×${f.hauteur}`]);
    } else {
        lignes.push(
            [f.type === "creation" ? "Idée" : "Consigne", f.idee || "—"],
            [f.type === "creation" ? "Prompt" : "Consigne envoyée", f.prompt],
            ["LLM", f.llm ? f.llm.replace(".gguf", "") : "aucun"],
            ["Modèle", f.modele ? `${f.workflow} · ${f.modele}` : f.workflow],
            ["Format", f.type === "creation" ? `${f.format} · ${f.largeur}×${f.hauteur}` : `${f.largeur}×${f.hauteur}`],
            ["Seed", f.seed],
            ["LoRAs", f.loras?.length
                ? f.loras.map((l) => `${l.fichier.replace(/\.safetensors$/, "")} (${l.force})`).join(", ") : "aucune"],
            ["Durée", `${f.duree_s} s`],
        );
        if (f.type === "retouche") lignes.push(["Masque", f.masque ? `oui, fondu ${f.fondu} px` : "non (image entière)"]);
        if (f.type === "agrandissement") lignes.push(["Ancrage", `${f.ancrage}, fondu ${f.fondu} px`]);
    }
    if (f.references?.length) {
        lignes.push(["Références", `${f.references.length} image${f.references.length > 1 ? "s" : ""} · `
            + (f.mode_reference === "variation" ? `variation, liberté ${f.liberte}` : "s'inspirer")]);
    }
    lignes.push(["Date", f.date.replace("T", " ")]);
    if (f.parent) lignes.push(["Issue de", f.parent]);
    lignes.push(["Rangement", [f.dossier ? `📁 ${f.dossier}` : "non rangé", f.archive ? "📦 archivé" : "",
                               f.corbeille ? "🗑 dans la corbeille" : ""].filter(Boolean).join(" · ")]);
    document.getElementById("fiche-detail").innerHTML = lignes.map(([t, v]) =>
        `<dt>${t}</dt><dd>${echapper(v)}</dd>`).join("");
    document.getElementById("variante").hidden = f.type !== "creation";
    document.getElementById("reutiliser").hidden = f.type !== "creation";
    document.getElementById("supprimer-creation").hidden = Boolean(f.corbeille);
    const telecharger = document.getElementById("telecharger");
    telecharger.href = urlImage(f.id);
    telecharger.download = `aria-${f.id}.png`;
    document.getElementById("detail-creation").hidden = false;
}

function fermerFiche() {
    document.getElementById("detail-creation").hidden = true;
    ficheOuverte = null;
}

document.getElementById("fermer-detail").addEventListener("click", fermerFiche);
document.getElementById("detail-creation").addEventListener("click", (e) => {
    if (e.target.id === "detail-creation") fermerFiche();
});
document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && ficheOuverte) fermerFiche();
});

function reprendreReglages(f) {
    document.getElementById("prompt-plateau").value = f.prompt ?? f.idee ?? "";
    // Les références de la fiche reviennent avec elle (et la liste des modèles suit leur mode).
    referencesChoisies = [...(f.references ?? [])];
    if (f.mode_reference) {
        modeReference = f.mode_reference;
        document.querySelector(`input[name="mode-reference"][value="${f.mode_reference}"]`).checked = true;
        document.getElementById("zone-references").open = true;
    }
    if (f.liberte) {
        document.getElementById("liberte-reference").value = f.liberte;
        document.getElementById("valeur-liberte").textContent = virgule(f.liberte);
    }
    rendreReferences();
    const select = document.getElementById("workflow-creation");
    const exact = `${f.workflow}::${f.modele}`;
    // Fiches d'avant les familles : pas de modèle noté, on prend la première ligne de ce workflow.
    const ligne = [...select.options].find((o) => o.value === exact)
        ?? [...select.options].find((o) => o.value.startsWith(`${f.workflow}::`));
    if (ligne) select.value = ligne.value;
    document.getElementById("format-creation").value = f.format;
}

document.getElementById("variante").addEventListener("click", () => {
    const f = ficheOuverte;
    fermerFiche();
    reprendreReglages(f);
    window.scrollTo({ top: 0, behavior: "smooth" });
    lancerCommande({ idee: f.idee, prompt: f.prompt, workflow: f.workflow, modele: f.modele ?? null,
        format: f.format, nombre: 1, parent: f.id, loras: f.loras ?? [], ...optionsReferences() });
});

document.getElementById("reutiliser").addEventListener("click", () => {
    const f = ficheOuverte;
    fermerFiche();
    reprendreReglages(f);
    window.scrollTo({ top: 0, behavior: "smooth" });
    document.getElementById("prompt-plateau").focus();
});

document.getElementById("comme-reference").addEventListener("click", () => {
    const f = ficheOuverte;
    fermerFiche();
    placerReference(f.id);
    window.scrollTo({ top: 0, behavior: "smooth" });
});

document.getElementById("supprimer-creation").addEventListener("click", async () => {
    const f = ficheOuverte;
    await api("DELETE", `/api/creations/${encodeURIComponent(f.id)}`);
    fermerFiche();
    chargerGalerie();
});

// Retouche au pinceau et agrandissement

const ANCRAGES = ["haut-gauche", "haut", "haut-droite", "gauche", "centre", "droite", "bas-gauche", "bas", "bas-droite"];
const FLECHES = ["↖", "↑", "↗", "←", "•", "→", "↙", "↓", "↘"];
let modeRetouche = "retouche";
let sourceRetouche = null;
let masqueGarde = null;  // { id, image, fondu } : le masque peint attend le retour du Scénario (✍️ L'écrire au Scénario)
let gomme = false;
let ancrage = "centre";

const canevas = document.getElementById("canevas-masque");
const ctx = canevas.getContext("2d");

function ouvrirRetouche(f, mode) {
    sourceRetouche = f;
    modeRetouche = mode;
    fermerFiche();
    const agrandir = mode === "agrandissement";
    document.getElementById("titre-retouche").textContent = agrandir ? "Agrandir" : "Retoucher";
    document.getElementById("outils-pinceau").hidden = agrandir;
    document.getElementById("outils-agrandir").hidden = !agrandir;
    canevas.hidden = agrandir;
    document.getElementById("consigne-retouche").placeholder = agrandir
        ? "Précision facultative, en français : « ajoute la mer à droite » (traduite par le LLM par défaut)"
        : "Ta consigne, telle qu'elle partira au modèle (en anglais) : « give her a red leather jacket »…";
    // Retouche : la consigne part telle quelle ; le Scénario l'écrit et l'optimise en voyant l'image.
    document.getElementById("entete-consigne-retouche").hidden = agrandir;
    document.getElementById("aide-consigne-retouche").textContent =
        "Consigne envoyée telle quelle. Pour la traduire et l'optimiser en voyant l'image :";
    document.getElementById("consigne-retouche").value = "";
    document.getElementById("fondu-retouche").value = agrandir ? 96 : 24;
    majFondu();
    document.getElementById("erreur-retouche").hidden = true;

    // Prête quand l'image est chargée et le masque gardé (même image) repeint : une retouche lancée dans la foulée
    // (Scénario, « Retoucher ») part avec lui.
    const img = document.getElementById("image-retouche");
    const garde = mode === "retouche" && masqueGarde?.id === f.id ? masqueGarde : null;
    masqueGarde = null;
    const pret = new Promise((fin) => {
        img.onload = () => {
            canevas.width = img.naturalWidth;
            canevas.height = img.naturalHeight;
            ctx.clearRect(0, 0, canevas.width, canevas.height);
            if (!garde) return fin();
            document.getElementById("fondu-retouche").value = garde.fondu;
            majFondu();
            const masque = new Image();
            masque.onload = () => {
                ctx.drawImage(masque, 0, 0);
                fin();
            };
            masque.onerror = () => fin();
            masque.src = garde.image;
        };
        img.onerror = () => fin();
    });
    img.src = urlImage(f.id);
    if (agrandir) {
        document.getElementById("largeur-agrandir").value = Math.round(f.largeur * 1.3 / 8) * 8;
        document.getElementById("hauteur-agrandir").value = f.hauteur;
        choisirAncrage("centre");
    }
    document.getElementById("panneau-retouche").hidden = false;
    return pret;
}

const majFondu = () => {
    document.getElementById("valeur-fondu").textContent = `${document.getElementById("fondu-retouche").value} px`;
};
document.getElementById("fondu-retouche").addEventListener("input", majFondu);

function fermerRetouche() {
    document.getElementById("panneau-retouche").hidden = true;
}

// Pinceau : on peint en coordonnées de l'image réelle, quelle que soit la taille affichée.
let peinture = null;

function point(e) {
    const r = canevas.getBoundingClientRect();
    return [(e.clientX - r.left) * canevas.width / r.width, (e.clientY - r.top) * canevas.height / r.height];
}

function tracer(depuis, vers) {
    const echelle = canevas.width / canevas.getBoundingClientRect().width;
    ctx.globalCompositeOperation = gomme ? "destination-out" : "source-over";
    ctx.strokeStyle = "#7df9ff";  // --accent de la palette Aria
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.lineWidth = Number(document.getElementById("taille-pinceau").value) * echelle;
    ctx.beginPath();
    ctx.moveTo(...depuis);
    ctx.lineTo(...vers);
    ctx.stroke();
}

canevas.addEventListener("pointerdown", (e) => {
    canevas.setPointerCapture(e.pointerId);
    peinture = point(e);
    tracer(peinture, peinture);
});
canevas.addEventListener("pointermove", (e) => {
    if (!peinture) return;
    const p = point(e);
    tracer(peinture, p);
    peinture = p;
});
["pointerup", "pointercancel"].forEach((t) => canevas.addEventListener(t, () => { peinture = null; }));

function choisirOutil(estGomme) {
    gomme = estGomme;
    document.getElementById("mode-pinceau").classList.toggle("actif", !gomme);
    document.getElementById("mode-gomme").classList.toggle("actif", gomme);
}
document.getElementById("mode-pinceau").addEventListener("click", () => choisirOutil(false));
document.getElementById("mode-gomme").addEventListener("click", () => choisirOutil(true));
document.getElementById("effacer-masque").addEventListener("click", () => ctx.clearRect(0, 0, canevas.width, canevas.height));

function masquePeint() {
    const pixels = ctx.getImageData(0, 0, canevas.width, canevas.height).data;
    for (let i = 3; i < pixels.length; i += 4) {
        if (pixels[i] > 0) return canevas.toDataURL("image/png");
    }
    return null;
}

// Agrandissement : ancrage et aperçu schématique.
document.getElementById("ancrages").innerHTML = ANCRAGES.map((a, i) =>
    `<button type="button" data-ancrage="${a}" title="${a}">${FLECHES[i]}</button>`).join("");
document.getElementById("ancrages").addEventListener("click", (e) => {
    if (e.target.dataset.ancrage) choisirAncrage(e.target.dataset.ancrage);
});
["largeur-agrandir", "hauteur-agrandir"].forEach((id) =>
    document.getElementById(id).addEventListener("input", () => choisirAncrage(ancrage)));

function choisirAncrage(a) {
    ancrage = a;
    document.querySelectorAll("#ancrages button").forEach((b) => b.classList.toggle("actif", b.dataset.ancrage === a));
    const f = sourceRetouche;
    const L = Number(document.getElementById("largeur-agrandir").value) || f.largeur;
    const H = Number(document.getElementById("hauteur-agrandir").value) || f.hauteur;
    const echelle = Math.min(160 / L, 100 / H);
    const cadre = document.getElementById("apercu-cadre");
    cadre.style.width = `${L * echelle}px`;
    cadre.style.height = `${H * echelle}px`;
    const [fx, fy] = { "haut-gauche": [0, 0], haut: [0.5, 0], "haut-droite": [1, 0], gauche: [0, 0.5], centre: [0.5, 0.5],
        droite: [1, 0.5], "bas-gauche": [0, 1], bas: [0.5, 1], "bas-droite": [1, 1] }[a];
    const image = document.getElementById("apercu-image");
    image.style.width = `${f.largeur * echelle}px`;
    image.style.height = `${f.hauteur * echelle}px`;
    image.style.left = `${Math.max(0, L - f.largeur) * fx * echelle}px`;
    image.style.top = `${Math.max(0, H - f.hauteur) * fy * echelle}px`;
}

document.getElementById("lancer-retouche").addEventListener("click", async () => {
    const consigne = document.getElementById("consigne-retouche").value;
    const commun = {
        source: sourceRetouche.id,
        // Retouche : la consigne du Scénario part telle quelle. Agrandissement : la précision est traduite.
        reecrire: modeRetouche === "agrandissement",
        llm: null,
        ...choixModele("workflow-retouche"),
        fondu: Number(document.getElementById("fondu-retouche").value),
        loras: lorasChoisies("loras-retouche"),
    };
    const ok = modeRetouche === "agrandissement"
        ? await lancerCommande({
            ...commun, precision: consigne, ancrage,
            largeur: Number(document.getElementById("largeur-agrandir").value),
            hauteur: Number(document.getElementById("hauteur-agrandir").value),
        }, "/api/agrandissements", document.getElementById("erreur-retouche"))
        : await lancerCommande({ ...commun, instruction: consigne, masque: masquePeint() },
            "/api/retouches", document.getElementById("erreur-retouche"));
    if (ok) {
        fermerRetouche();
        window.scrollTo({ top: 0, behavior: "smooth" });
    }
});

document.getElementById("vers-scenario-retouche").addEventListener("click", () => {
    const id = sourceRetouche.id;
    masqueGarde = { id, image: canevas.toDataURL(), fondu: document.getElementById("fondu-retouche").value };
    fermerRetouche();
    ouvrirScenario("retouche", { retoucher: id });
});

// Le Scénario envoie une consigne prête : le panneau s'ouvre sur l'image, consigne posée (masque et fondu ici).
async function recevoirRetouche({ prompt, images, workflow, lancer }) {
    await chargerOptionsPlateau();
    document.querySelector('.onglet[data-vue="plateau"]').click();
    await ouvrirRetouche(await api("GET", `/api/creations/${encodeURIComponent(images[0])}`), "retouche");
    document.getElementById("consigne-retouche").value = prompt;
    const select = document.getElementById("workflow-retouche");
    const ligne = [...select.options].find((o) => o.value.startsWith(`${workflow}::`));
    if (ligne && choixModele("workflow-retouche").workflow !== workflow) {
        select.value = ligne.value;
        select.dispatchEvent(new Event("change"));
    }
    if (lancer) document.getElementById("lancer-retouche").click();
}
document.getElementById("ouvrir-retouche").addEventListener("click", () => ouvrirRetouche(ficheOuverte, "retouche"));
document.getElementById("ouvrir-agrandir").addEventListener("click", () => ouvrirRetouche(ficheOuverte, "agrandissement"));
document.getElementById("fermer-retouche").addEventListener("click", fermerRetouche);

// Import d'une image personnelle

const TYPES_IMPORT = ["image/png", "image/jpeg", "image/webp"];

function importerFichier(fichier, ouvrir = true) {
    return new Promise((fin) => {
        const lecteur = new FileReader();
        lecteur.onload = async () => {
            let f = null;
            try {
                f = await api("POST", "/api/creations/import",
                    { image: lecteur.result, nom: fichier.name, dossier: rushes.dossierOuvert() });
                await chargerGalerie();
                if (ouvrir) ouvrirFiche(f);
            } catch (err) {
                alert(err.message);
            }
            fin(f);
        };
        lecteur.readAsDataURL(fichier);
    });
}

document.getElementById("importer").addEventListener("change", async (e) => {
    const fichier = e.target.files[0];
    if (!fichier) return;
    await importerFichier(fichier);
    e.target.value = "";
});

// Glisser-déposer : une image lâchée n'importe où sur le Plateau suit le même chemin que le bouton « Importer ».
// Seuls les fichiers venus de l'extérieur comptent (pas une vignette de la galerie qu'on déplace),
// et jamais pendant une retouche, pour ne pas perdre le masque en cours.
let survolsDepot = 0;
const voileDepot = document.getElementById("voile-depot");
const depotPossible = (e) => e.dataTransfer?.types.includes("Files")
    && !e.dataTransfer.types.includes("text/aria-rush")  // une vignette des rushes qu'on déplace n'est pas un import
    && !document.getElementById("vue-plateau").hidden
    && document.getElementById("panneau-retouche").hidden;

window.addEventListener("dragenter", (e) => {
    if (!depotPossible(e)) return;
    survolsDepot++;
    voileDepot.hidden = false;
});
window.addEventListener("dragleave", (e) => {
    if (!depotPossible(e)) return;
    survolsDepot = Math.max(0, survolsDepot - 1);
    if (!survolsDepot) voileDepot.hidden = true;
});
window.addEventListener("dragover", (e) => {
    if (depotPossible(e)) e.preventDefault();  // sinon le navigateur ouvrirait le fichier à la place de la page
});
window.addEventListener("drop", async (e) => {
    if (!depotPossible(e)) return;
    e.preventDefault();
    survolsDepot = 0;
    voileDepot.hidden = true;
    const fichiers = [...e.dataTransfer.files];
    const fichier = fichiers.find((f) => TYPES_IMPORT.includes(f.type));
    if (!fichier) {
        alert("Dépose une image PNG, JPEG ou WebP.");
        return;
    }
    if (fichiers.length > 1) alert(`Une image à la fois : j'importe « ${fichier.name} ».`);
    await importerFichier(fichier);
});

chargerOptionsPlateau();
chargerGalerie();
