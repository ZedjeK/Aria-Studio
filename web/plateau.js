// Plateau : création (idée → LLM → image), suivi de la commande, rushes et fiche détaillée.

let ficheOuverte = null;

let optionsPlateau = null;

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

// Une ligne par checkpoint : « workflow::fichier ». Celui par défaut de la famille d'abord, sous le nom du workflow
// (avec son fichier s'il n'est pas celui d'origine), puis les autres checkpoints déposés dans son dossier.
const nomCheckpoint = (m) => m.replace(/\.safetensors$/, "");
const lignesModeles = (tache) => tache.choix.flatMap((w) => [w.modele_defaut, ...w.modeles.filter((m) => m !== w.modele_defaut)]
    .map((m, i) => [`${w.id}::${m}`, i === 0
        ? `${w.nom}${w.id === tache.defaut ? " ★" : ""}${m === w.origine ? "" : ` · ${nomCheckpoint(m)}`}`
        : `   ↳ ${w.nom} · ${nomCheckpoint(m)}`]));

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
    poserMaxNombre("nombre-creation", o.nombre_max);  // un champ à taper : le max vient du serveur
    afficherLoras("loras-creation", "workflow-creation");
    afficherLoras("loras-retouche", "workflow-retouche");
    rendreReferences();
}

// Références : 1 à 10 rushes pour S'inspirer (nouvelle scène, format au choix), 1 pour une Variation.
// Les modèles proposés changent avec le mode : édition (Qwen 2.1 Edit, 2511) ou texte vers image (img2img).

let referencesChoisies = [];  // toutes celles posées, gardées même quand la Variation n'en utilise qu'une
let modeReference = "inspirer";

const maxReferences = () => (modeReference === "variation" ? 1 : (optionsPlateau?.references_max ?? 3));
const referencesActives = () => referencesChoisies.filter(Boolean).slice(0, maxReferences());

const AIDES_REFERENCES = {
    inspirer: () => "Glisse jusqu'à " + maxReferences() + " rushes (ou images de ton disque) dans les cases, ou « Comme "
        + "référence » dans la fiche d'un rush ; une case de plus apparaît quand les autres sont remplies. Cite-les "
        + "dans ton idée : « la femme de l'image 1 sur le dragon de l'image 2 ».",
    variation: "Une seule image : sa composition est gardée. Plus de liberté = plus de changements. Le format est celui "
        + "de l'image.",
};

function listeCreation() {
    const o = optionsPlateau;
    const n = referencesActives().length;
    if (n && modeReference === "inspirer") {
        return { ...o.workflows.retouche, defaut: o.references_defaut,
                 choix: o.workflows.retouche.choix.filter((w) => w.references >= n && !w.reserve_retouche) };
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
    format.title = variation ? "Redessiner une image garde son format" : "";
}

const liberteReference = () => Number(document.getElementById("liberte-reference").value);

function rendreReferences() {
    referencesChoisies = referencesChoisies.filter(Boolean);
    // S'inspirer : trois cases au départ, une de plus dès que toutes sont remplies, jusqu'au maximum du modèle.
    const max = modeReference === "variation" ? 1 : Math.min(maxReferences(), Math.max(3, referencesChoisies.length + 1));
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
        ? `· ${n} image${n > 1 ? "s" : ""} · ${modeReference === "inspirer" ? "mélanger" : "redessiner"}` : "";
    document.getElementById("bloc-liberte").hidden = modeReference !== "variation";
    const aide = AIDES_REFERENCES[modeReference];
    document.getElementById("aide-references").textContent = typeof aide === "function" ? aide() : aide;
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
        select.value = `${w.id}::${w.modele_defaut}`;
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

brancherNombre("nombre-creation");  // un nombre hors limites est ramené dans les limites en quittant le champ

function optionsFormulaire() {
    const seed = document.getElementById("seed-creation").value;
    return {
        ...choixModele("workflow-creation"),
        format: document.getElementById("format-creation").value,
        nombre: nombreDuChamp("nombre-creation"),
        seed: seed === "" ? null : Number(seed),
        dossier: rushes.dossierCible(),  // dossier de destination choisi dans « 📥 Ranger dans »
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
    const quoi = c.type === "retouche" ? "Retouche" : "Image";
    if (c.etat === "erreur") notifierFin(`${quoi} en erreur`, c.erreur || "Le job a échoué.", "plateau", false);
    else if (c.etat === "annule") notifierFin(`${quoi} annulée`, messageAnnulation(c, "image", true), "plateau", false);
    else notifierFin(`${quoi} terminée`, `${c.images.length} image${c.images.length > 1 ? "s" : ""} dans les rushes`, "plateau");
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

// ✨ Écrire avec le LLM, sans quitter l'onglet : le texte du champ (une idée en français, ou un prompt à améliorer) part au
// LLM du modèle cible, comme le ✨ du Scénario ; le résultat remplace le texte, que ↩ rend. Partagé avec le Montage.
function brancherEcriture({ suffixe, champ, atelier, corps, retenir }) {
    const bouton = document.getElementById(`ecrire-${suffixe}`);
    const retour = document.getElementById(`annuler-ecriture-${suffixe}`);
    const statut = document.getElementById(`statut-ecriture-${suffixe}`);
    let avant = null;
    const dire = (texte, erreur = false) => {
        statut.textContent = texte;
        statut.hidden = !texte;
        statut.classList.toggle("ko", erreur);
    };
    bouton.addEventListener("click", async () => {
        const zone = document.getElementById(champ);
        const idee = zone.value.trim();
        if (!idee) {
            dire("Écris d'abord ton idée dans le champ, le LLM la transforme en prompt.", true);
            return;
        }
        bouton.disabled = true;
        dire("Le LLM écrit… (le premier appel charge le modèle)");
        try {
            const llm = typeof llmAtelier === "function" ? llmAtelier() : null;
            const r = await api("POST", "/api/prompt/optimiser", { atelier, idee, llm, ...corps() });
            avant = idee;
            zone.value = r.prompt;
            retenir?.({ prompt: r.prompt.trim(), idee, llm });
            retour.hidden = false;
            dire(`Écrit par le LLM · ${r.duree_s} s · ${r.mots} mots. Retouche-le si besoin avant de lancer.`);
        } catch (err) {
            dire(err.message, true);
        } finally {
            bouton.disabled = false;
        }
    });
    retour.addEventListener("click", () => {
        if (avant === null) return;
        document.getElementById(champ).value = avant;
        retenir?.(null);
        retour.hidden = true;
        dire("");
    });
}

brancherEcriture({
    suffixe: "plateau", champ: "prompt-plateau", atelier: "image",
    corps: () => {
        const images = referencesActives();
        return {
            workflow: choixModele("workflow-creation").workflow || null,
            images, role: images.length ? (modeReference === "variation" ? "variation" : "inspirer") : null,
        };
    },
    retenir: (envoi) => { envoiScenario = envoi; },  // l'idée et le LLM suivent le prompt jusque dans la fiche
});

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
        dossier: "Dossier vide : glisse des vignettes sur son nom, ou choisis-le dans « 📥 Ranger dans » avant de créer.",
    },
    vignette: (f, choisi) => `<img src="${urlImage(f.id)}" alt="" loading="lazy" draggable="true"
        data-creation="${echapper(f.id)}" class="${choisi ? "choisi" : ""}">`,
    ouvrir: (f) => ouvrirFiche(f),
    destination: document.getElementById("destination-plateau"),
    // Clic droit : on ouvre la fiche en coulisse puis on déclenche le bouton correspondant, une seule logique.
    actions: (f) => {
        const via = (bouton) => () => {
            ouvrirFiche(f);
            document.getElementById(bouton).click();
        };
        return [
            ...(f.type === "creation" ? [["Variante", via("variante")], ["Réutiliser le prompt", via("reutiliser")]] : []),
            ["Comme référence", via("comme-reference")],
            ["🎞️ Animer", via("animer-rush")],
            ["Retoucher", via("ouvrir-retouche")],
            ["Agrandir", via("ouvrir-agrandir")],
            ["Télécharger", () => Object.assign(document.createElement("a"),
                { href: urlImage(f.id), download: `aria-${f.id}.png` }).click()],
        ];
    },
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
            + (f.mode_reference === "variation" ? `redessiner, liberté ${f.liberte}` : "mélanger des images")]);
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

// Import d'une image personnelle

const TYPES_IMPORT = ["image/png", "image/jpeg", "image/webp"];

function importerFichier(fichier, ouvrir = true) {
    return new Promise((fin) => {
        const lecteur = new FileReader();
        lecteur.onload = async () => {
            let f = null;
            try {
                f = await api("POST", "/api/creations/import",
                    { image: lecteur.result, nom: fichier.name, dossier: rushes.dossierCible() });
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
