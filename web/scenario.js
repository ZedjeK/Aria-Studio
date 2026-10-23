// Scénario : composer un prompt anglais en cliquant dans les listes de prompt/listes, puis l'envoyer au Plateau,
// ou au Montage quand le modèle cible est un modèle vidéo (bloc Plan, action, réplique, durée).
// Les règles (genre, âge, remplacements, incompatibilités) sont appliquées ici en direct pour guider,
// mais c'est le serveur qui fait foi : chaque changement repasse par /api/prompt/assembler.

const ICONES_SUJETS = { personnage: "👤", creature: "🐉", objet: "🏺", lieu: "🏞" };
let structurePrompt = null;
let sujetPrompt = "personnage";
// sujet -> { choix: {bloc: {sous-liste: valeur}}, age, texte, precisions, traduction: {source, texte},
//            action, traductionAction: {source, texte}, replique, toutes: lève le filtre par origine }
const etatsPrompt = {};
let definitionsPrompt = {};  // "bloc|sous-liste" -> sous-liste de la structure
let incompatPrompt = new Map();  // "bloc|sous-liste|valeur" -> [[bloc, sous-liste, valeur], …]
const groupesOuvertsPrompt = new Set();
let assemblagePrompt = null;
let peaufinage = null;  // { source } : le prompt assemblé que la version peaufinée a pris pour base
let occupePrompt = false;
let minuteurPrompt = null;
let numeroAssemblage = 0;
let retraitsPrompt = [];
let promptPret = false;
let personnagesEnregistres = [];

const etatPrompt = () => (etatsPrompt[sujetPrompt] ??=
    { choix: {}, age: null, texte: "", precisions: "", traduction: null, action: "", traductionAction: null, replique: "",
      toutes: false });
const cle = (...morceaux) => morceaux.join("|");
const llmPrompt = () => document.getElementById("llm-prompt").value || null;
const ciblePrompt = () => document.getElementById("workflow-prompt").value || null;
const cibleVideo = () => Boolean(optionsPlateau?.workflows.texte_vers_video?.choix.some((w) => w.id === ciblePrompt()));

async function preparerScenario() {
    if (!promptPret) {
        promptPret = true;
        const { sujets } = await api("GET", "/api/prompt/listes");
        document.getElementById("sujets-prompt").innerHTML = sujets.map((s) =>
            `<button type="button" data-sujet="${echapper(s.id)}">${ICONES_SUJETS[s.id] ?? ""} ${echapper(s.nom)}</button>`).join("");
    }
    await chargerModelesPrompt();
    await chargerPersonnages();
    await choisirSujet(sujetPrompt);  // relit les listes : une liste modifiée est prise en compte
}

async function chargerModelesPrompt() {
    if (!optionsPlateau) await chargerOptionsPlateau();
    const t2i = optionsPlateau.workflows.texte_vers_image;
    const t2v = optionsPlateau.workflows.texte_vers_video;
    const select = document.getElementById("workflow-prompt");
    const garde = select.value || choixModele("workflow-creation").workflow || t2i.defaut;
    const options = (tache) => tache.choix.map((w) =>
        `<option value="${echapper(w.id)}" ${w.id === garde ? "selected" : ""}>${echapper(w.nom)}${w.id === tache.defaut ? " ★" : ""}</option>`).join("");
    select.innerHTML = `<optgroup label="🖼️ Image">${options(t2i)}</optgroup>`
        + (t2v?.choix.length ? `<optgroup label="🎞️ Vidéo">${options(t2v)}</optgroup>` : "");
    remplirSelect("duree-prompt", optionsPlateau.video.durees.map((d) => [String(d), `${d} s`]), "5");
    const llm = document.getElementById("llm-prompt");
    const gardeLlm = llm.value || optionsPlateau.llm.defaut;
    llm.innerHTML = optionsPlateau.llm.choix.map((m) =>
        `<option value="${echapper(m.fichier)}" ${m.fichier === gardeLlm ? "selected" : ""}>${echapper(m.nom)}${m.favori ? " ★" : ""}</option>`).join("");
    majCible();
}

// Cible vidéo : bloc Plan, action, réplique et durée visibles ; « Texte dans l'image » masqué ; envoi au Montage.
function majCible() {
    const video = cibleVideo();
    document.querySelectorAll("[data-bloc-video]").forEach((b) => { b.hidden = !video; });
    document.querySelectorAll("[data-video-seule]").forEach((b) => { b.hidden = !video; });
    document.querySelectorAll("[data-image-seule]").forEach((b) => { b.hidden = video; });
    document.getElementById("envoyer-prompt").textContent = video ? "→ Envoyer au Montage" : "→ Envoyer au Plateau";
    document.getElementById("creer-prompt").textContent = video ? "Tourner" : "Créer";
    rendreEtiquettes();
    majForme();  // Clip seul ou Séquence (decoupage.js)
}

async function choisirSujet(sujet) {
    sujetPrompt = sujet;
    document.querySelectorAll("#sujets-prompt button").forEach((b) => b.classList.toggle("actif", b.dataset.sujet === sujet));
    document.getElementById("personnages-prompt").hidden = sujet !== "personnage";
    try {
        structurePrompt = await api("GET", `/api/prompt/listes?sujet=${encodeURIComponent(sujet)}`);
    } catch (err) {
        document.getElementById("blocs-prompt").innerHTML = `<p class="ko">${echapper(err.message)}</p>`;
        return;
    }
    definitionsPrompt = {};
    structurePrompt.blocs.forEach((b) => b.groupes.forEach((g) => g.sous_listes.forEach((sl) => {
        definitionsPrompt[cle(b.id, sl.nom)] = sl;
    })));
    incompatPrompt = new Map();
    structurePrompt.incompatibilites.forEach(([a, b]) => {
        [[a, b], [b, a]].forEach(([x, y]) => {
            if (!incompatPrompt.has(cle(...x))) incompatPrompt.set(cle(...x), []);
            incompatPrompt.get(cle(...x)).push(y);
        });
    });
    rendreBlocsPrompt();
    appliquerReglesPrompt();
    planifierAssemblage();
}

// Squelette : un accordéon par groupe ; les options sont remplies par appliquerReglesPrompt.
function rendreBlocsPrompt() {
    const premier = structurePrompt.blocs[0];
    if (![...groupesOuvertsPrompt].some((g) => g.startsWith(`${sujetPrompt}|`))) {
        groupesOuvertsPrompt.add(cle(sujetPrompt, premier.id, premier.groupes[0]?.nom));
    }
    let numero = 0;
    document.getElementById("blocs-prompt").innerHTML = structurePrompt.blocs.map((b) => `
        <div ${b.video ? `data-bloc-video data-clip-seul ${cibleVideo() ? "" : "hidden"}` : ""}>
        <h3 class="titre-bloc">${b.video ? "🎞️ " : ""}${echapper(b.titre)}</h3>
        ${b.groupes.map((g) => {
            const id = cle(sujetPrompt, b.id, g.nom);
            return `<details data-groupe="${echapper(id)}" ${groupesOuvertsPrompt.has(id) ? "open" : ""}>
                <summary>${echapper(g.nom)} <span class="discret compte"></span></summary>
                <div class="sous-listes">${g.sous_listes.map((sl) => `
                    <div class="sous-liste" data-bloc="${echapper(b.id)}" data-sl="${echapper(sl.nom)}">
                        <div class="entete-sl">
                            <label for="sl-${++numero}">${echapper(sl.nom)}</label>
                            <button type="button" class="de" title="Tirer au hasard (dans la catégorie choisie)">🎲</button>
                        </div>
                        ${sl.valeurs.some((v) => v.categorie)
                            ? `<select class="categorie-sl" aria-label="Catégorie : ${echapper(sl.nom)}"></select>` : ""}
                        <select id="sl-${numero}" class="valeur-sl"></select>
                        ${b.id === "personnage" && sl.nom === "Âge"
                            ? `<input id="age-prompt" class="age-precis" type="number" min="${structurePrompt.age_precis[0]}"
                                      max="${structurePrompt.age_precis[1]}" placeholder="âge précis (facultatif)">`
                            : ""}
                        ${b.id === "personnage" && sl.nom === "Origine"
                            ? `<label class="case discret" title="Sinon, le teint, les yeux et le nez suivent l'origine choisie">
                                   <input type="checkbox" id="toutes-options"> Toutes les options (teint, yeux, nez)</label>`
                            : ""}
                        <small class="raison"></small>
                    </div>`).join("")}
                </div>
            </details>`;
        }).join("")}
        </div>`).join("");
    const etat = etatPrompt();
    const age = document.getElementById("age-prompt");
    if (age) age.value = etat.age ?? "";
    const toutes = document.getElementById("toutes-options");
    if (toutes) toutes.checked = etat.toutes;
    document.getElementById("texte-prompt").value = etat.texte;
    document.getElementById("precisions-prompt").value = etat.precisions;
    document.getElementById("action-prompt").value = etat.action;
    document.getElementById("replique-prompt").value = etat.replique;
}

function contextePrompt(etat) {
    const perso = etat.choix.personnage ?? {};
    const genre = perso.Genre ? perso.Genre.toLowerCase() : null;
    const tranche = definitionsPrompt[cle("personnage", "Âge")]?.valeurs.find((v) => v.nom === perso["Âge"]);
    const mineur = Boolean(tranche?.mineur) || (etat.age !== null && etat.age < 18);
    const origine = definitionsPrompt[cle("personnage", "Origine")]?.valeurs.find((v) => v.nom === perso.Origine);
    const remplacees = {};
    Object.entries(etat.choix).forEach(([bloc, valeurs]) => Object.entries(valeurs).forEach(([nom, choisie]) => {
        const v = definitionsPrompt[cle(bloc, nom)]?.valeurs.find((x) => x.nom === choisie);
        v?.remplace.forEach((cible) => { remplacees[cle(bloc, cible)] = choisie; });
    }));
    return { genre, mineur, remplacees, familles: etat.toutes ? [] : (origine?.familles ?? []),
             choix: etat.choix };
}

// Origine : une aide au choix (teint, yeux, nez), pas une règle ; « Toutes les options » vide c.familles.
const selonOrigine = (v, c) => !c.familles.length || !v.origines?.length || v.origines.some((f) => c.familles.includes(f));

// « Avec <sous-liste> » : une règle (la poitrine selon la silhouette, la coiffure selon la longueur) ; « Toutes » ne la lève pas.
const coherente = (v, c, bloc) => Object.entries(v.avec ?? {})
    .every(([sl, noms]) => !c.choix[bloc]?.[sl] || noms.includes(c.choix[bloc][sl]));

const valeurPermise = (v, c, bloc) => !(c.mineur && v.adulte)
    && !(v.genre && ["femme", "homme"].includes(c.genre) && v.genre !== c.genre)
    && coherente(v, c, bloc)
    && selonOrigine(v, c);

// Catégorie affichée par sous-liste (sujet|bloc|sous-liste) : un filtre d'affichage, jamais envoyé au serveur.
const categoriesAffichees = {};

function categorieAffichee(bloc, sl, categories) {
    const choisie = categoriesAffichees[cle(sujetPrompt, bloc, sl)] ?? "";
    return categories.includes(choisie) ? choisie : "";  // une catégorie disparue (mineur…) revient à « Toutes »
}

const enConflit = (etat, bloc, sl, nom) => (incompatPrompt.get(cle(bloc, sl, nom)) ?? [])
    .find(([b, s, val]) => etat.choix[b]?.[s] === val);

function appliquerReglesPrompt() {
    const etat = etatPrompt();
    const divs = [...document.querySelectorAll("#blocs-prompt .sous-liste")];
    // Retirer une valeur peut en libérer ou en masquer d'autres (motif de tatouage…) : on recommence jusqu'à stabilité.
    for (let passe = 0, change = true; change && passe < 5; passe++) {
        change = false;
        const c = contextePrompt(etat);
        divs.forEach((div) => {
            const { bloc, sl } = div.dataset;
            const def = definitionsPrompt[cle(bloc, sl)];
            const choix = etat.choix[bloc] ?? {};
            const visibles = def.valeurs.filter((v) => valeurPermise(v, c, bloc));
            const conditionKo = def.condition
                && !(choix[def.condition.sous_liste] ?? "").toLowerCase().includes(def.condition.contient);
            const cachee = !visibles.length || (c.mineur && def.adulte) || conditionKo;
            div.hidden = cachee;
            const actuelle = choix[sl];
            if (actuelle && (cachee || !visibles.some((v) => v.nom === actuelle))) {
                delete choix[sl];
                retraitsPrompt.push(`${sl} : « ${actuelle} »`);
                change = true;
            }
        });
    }

    const c = contextePrompt(etat);
    divs.forEach((div) => {
        if (div.hidden) return;
        const { bloc, sl } = div.dataset;
        const def = definitionsPrompt[cle(bloc, sl)];
        const actuelle = etat.choix[bloc]?.[sl] ?? "";
        const select = div.querySelector(".valeur-sl");
        const option = (v) => {
            const conflit = enConflit(etat, bloc, sl, v.nom);
            return `<option value="${echapper(v.nom)}" title="${echapper(v.description || v.prompt)}"
                ${v.nom === actuelle ? "selected" : ""} ${conflit && v.nom !== actuelle ? "disabled" : ""}>
                ${echapper(v.nom)}${conflit ? ` ⛔ ${echapper(conflit[2])}` : ""}</option>`;
        };
        const permises = def.valeurs.filter((v) => valeurPermise(v, c, bloc));
        // Catégories (Pose, Action) : un premier menu choisit la catégorie, le second n'en montre que les valeurs.
        const categories = [...new Set(permises.map((v) => v.categorie).filter(Boolean))];
        const menuCategorie = div.querySelector(".categorie-sl");
        let affichees = permises;
        if (menuCategorie) {
            const choisie = categorieAffichee(bloc, sl, categories);
            menuCategorie.innerHTML = '<option value="">Toutes les catégories</option>' + categories.map((cat) =>
                `<option value="${echapper(cat)}" ${cat === choisie ? "selected" : ""}>${echapper(cat)} `
                + `(${permises.filter((v) => v.categorie === cat).length})</option>`).join("");
            if (choisie) {
                affichees = permises.filter((v) => v.categorie === choisie);
                const hors = permises.find((v) => v.nom === actuelle && v.categorie !== choisie);
                if (hors) affichees = [hors, ...affichees];  // le choix fait reste visible
            }
        }
        const groupesVisibles = [...new Set(affichees.map((v) => v.categorie).filter(Boolean))];
        select.innerHTML = '<option value="">—</option>' + (groupesVisibles.length > 1
            ? groupesVisibles.map((cat) => `<optgroup label="${echapper(cat)}">${affichees
                .filter((v) => v.categorie === cat).map(option).join("")}</optgroup>`).join("")
              + affichees.filter((v) => !v.categorie).map(option).join("")
            : affichees.map(option).join(""));
        const remplacant = c.remplacees[cle(bloc, sl)];
        select.disabled = Boolean(remplacant);
        if (menuCategorie) menuCategorie.disabled = Boolean(remplacant);
        div.querySelector(".de").disabled = Boolean(remplacant);
        div.classList.toggle("choisie", Boolean(actuelle) && !remplacant);
        div.querySelector(".raison").textContent = remplacant ? `remplacé par « ${remplacant} »`
            : (def.valeurs.find((v) => v.nom === actuelle)?.description ?? "");
    });

    document.querySelectorAll("#blocs-prompt details").forEach((d) => {
        const n = [...d.querySelectorAll(".sous-liste.choisie")].filter((x) => !x.hidden).length;
        d.querySelector(".compte").textContent = n ? `· ${n}` : "";
    });
    document.getElementById("mineur-prompt").hidden = !c.mineur;
    rendreEtiquettes();
}

function rendreEtiquettes() {
    const choix = etatPrompt().choix;
    const video = cibleVideo();
    const planVideo = new Set(structurePrompt?.blocs.filter((b) => b.video).map((b) => b.id));
    document.getElementById("etiquettes-prompt").innerHTML = Object.entries(choix)
        .filter(([bloc]) => video || !planVideo.has(bloc))  // le Plan reste en mémoire, mais ne compte qu'en vidéo
        .flatMap(([bloc, valeurs]) =>
        Object.entries(valeurs).map(([sl, v]) =>
            `<button type="button" class="etiquette" data-bloc="${echapper(bloc)}" data-sl="${echapper(sl)}"
                     title="${echapper(sl)} · retirer">${echapper(v)} ✕</button>`)).join("");
}

function poserChoix(bloc, sl, valeur) {
    const etat = etatPrompt();
    const choix = (etat.choix[bloc] ??= {});
    if (valeur) choix[sl] = valeur;
    else delete choix[sl];
    // Une tranche choisie à la main l'emporte sur un âge précis qui n'y entre pas.
    if (bloc === "personnage" && sl === "Âge" && etat.age !== null) {
        const t = definitionsPrompt[cle("personnage", "Âge")].valeurs.find((v) => v.nom === valeur);
        if (!t || etat.age < t.bornes[0] || etat.age > t.bornes[1]) {
            etat.age = null;
            document.getElementById("age-prompt").value = "";
        }
    }
    appliquerReglesPrompt();
}

function changerChoix(bloc, sl, valeur) {
    poserChoix(bloc, sl, valeur);
    planifierAssemblage();
}

// Tirage 🎲 : seulement parmi les valeurs proposées, compatibles, qui ajoutent quelque chose au prompt
// et qui n'écrasent pas un choix déjà fait (une tenue intime ne remplace pas un haut choisi).
function tirer(div) {
    const etat = etatPrompt();
    const { bloc, sl } = div.dataset;
    const c = contextePrompt(etat);
    if (div.hidden || c.remplacees[cle(bloc, sl)]) return false;
    const actuelle = etat.choix[bloc]?.[sl];
    const categorie = categoriesAffichees[cle(sujetPrompt, bloc, sl)];  // le dé reste dans la catégorie affichée
    const candidates = definitionsPrompt[cle(bloc, sl)].valeurs.filter((v) => (v.prompt || v.prompts_genre)
        && v.nom !== actuelle && (!categorie || v.categorie === categorie)
        && valeurPermise(v, c, bloc) && !enConflit(etat, bloc, sl, v.nom)
        && !v.remplace.some((cible) => etat.choix[bloc]?.[cible]));
    if (!candidates.length) return false;
    poserChoix(bloc, sl, candidates[Math.floor(Math.random() * candidates.length)].nom);
    return true;
}

function planifierAssemblage() {
    clearTimeout(minuteurPrompt);
    minuteurPrompt = setTimeout(assemblerPrompt, 200);
}

function majBoutonsPrompt() {
    const bloque = occupePrompt || !assemblagePrompt;
    ["peaufiner-prompt", "envoyer-prompt", "creer-prompt"].forEach((id) => {
        document.getElementById(id).disabled = bloque;
    });
}

function traductionValide(etat) {
    return etat.traduction && etat.traduction.source === etat.precisions.trim();
}

function traductionActionValide(etat) {
    return etat.traductionAction && etat.traductionAction.source === etat.action.trim();
}

async function assemblerPrompt() {
    const etat = etatPrompt();
    const numero = ++numeroAssemblage;
    const erreur = document.getElementById("erreur-prompt");
    let r;
    try {
        r = await api("POST", "/api/prompt/assembler", {
            sujet: sujetPrompt, choix: etat.choix, age: etat.age, texte: etat.texte,
            precisions: traductionValide(etat) ? etat.traduction.texte : etat.precisions,
            workflow: ciblePrompt(),
            action: traductionActionValide(etat) ? etat.traductionAction.texte : etat.action,
            replique: etat.replique,
        });
    } catch (err) {
        if (numero !== numeroAssemblage) return;
        assemblagePrompt = null;
        erreur.textContent = err.message;
        erreur.hidden = false;
        majBoutonsPrompt();
        return;
    }
    if (numero !== numeroAssemblage) return;  // une réponse plus récente est déjà partie
    assemblagePrompt = r;
    erreur.hidden = true;
    majBoutonsPrompt();
    document.getElementById("texte-apercu").textContent = r.prompt;
    const mots = document.getElementById("mots-prompt");
    mots.textContent = `${r.mots} mots${r.mots_max ? ` / ${r.mots_max} conseillés` : ""}`;
    mots.classList.toggle("ko", Boolean(r.mots_max) && r.mots > r.mots_max);
    const alertes = [...retraitsPrompt.map((x) => `Retiré : ${x}`), ...r.alertes];
    if (etat.precisions.trim() && !traductionValide(etat)) alertes.push("Précisions : traduites par Llama à l'envoi");
    if (r.video && etat.action.trim() && !traductionActionValide(etat)) alertes.push("Action : traduite par Llama à l'envoi");
    retraitsPrompt = [];
    document.getElementById("alertes-prompt").innerHTML = alertes.map((a) => `<li>${echapper(a)}</li>`).join("");
    // La version peaufinée ne vaut que pour le prompt dont elle est partie.
    if (peaufinage && peaufinage.source !== r.prompt) {
        peaufinage = null;
        document.getElementById("peaufine-prompt").hidden = true;
    }
}

// Llama : traduction des précisions, peaufinage

async function avecLlama(action) {
    const statut = document.getElementById("statut-prompt");
    const erreur = document.getElementById("erreur-prompt");
    occupePrompt = true;
    majBoutonsPrompt();
    erreur.hidden = true;
    try {
        await action((message) => {
            statut.textContent = message;
            statut.hidden = !message;
        });
    } catch (err) {
        statut.hidden = true;
        erreur.textContent = err.message;
        erreur.hidden = false;
    } finally {
        occupePrompt = false;
        majBoutonsPrompt();
    }
}

async function traduireSiBesoin(statut) {
    const etat = etatPrompt();
    const source = etat.precisions.trim();
    if (source && !traductionValide(etat)) {
        statut("Llama traduit les précisions… (le premier appel charge le modèle)");
        const r = await api("POST", "/api/prompt/traduire", { texte: source, llm: llmPrompt() });
        etat.traduction = { source, texte: r.texte };
        statut("");
    }
    const action = etat.action.trim();
    if (cibleVideo() && action && !traductionActionValide(etat)) {
        statut("Llama traduit l'action… (le premier appel charge le modèle)");
        const r = await api("POST", "/api/prompt/traduire", { texte: action, llm: llmPrompt() });
        etat.traductionAction = { source: action, texte: r.texte };
        statut("");
    }
    clearTimeout(minuteurPrompt);
    await assemblerPrompt();
    if (!assemblagePrompt) throw new Error(document.getElementById("erreur-prompt").textContent);
}

function promptAEnvoyer() {
    if (peaufinage && document.getElementById("utiliser-peaufine").checked) {
        const texte = document.getElementById("texte-peaufine").value.trim();
        if (texte) return texte;
    }
    return assemblagePrompt.prompt;
}

document.getElementById("peaufiner-prompt").addEventListener("click", () => avecLlama(async (statut) => {
    await traduireSiBesoin(statut);
    const select = document.getElementById("llm-prompt");
    const nom = select.selectedOptions[0]?.textContent.trim() ?? "Llama";
    statut(`${nom} peaufine… (le premier appel charge le modèle)`);
    const source = assemblagePrompt.prompt;
    const r = await api("POST", "/api/prompt/peaufiner", {
        prompt: source, llm: llmPrompt(), workflow: ciblePrompt(),
        duree: Number(document.getElementById("duree-prompt").value) || 5,
    });
    peaufinage = { source };
    document.getElementById("texte-peaufine").value = r.prompt;
    document.getElementById("utiliser-peaufine").checked = true;
    document.getElementById("peaufine-prompt").hidden = false;
    statut(`${nom} · ${r.duree_s} s · ${r.mots} mots`);
}));

// Envoi vers le Plateau : le prompt est prêt, il part sans réécriture, sur le modèle cible.
async function envoyerVersPlateau(creer) {
    await avecLlama(async (statut) => {
        await traduireSiBesoin(statut);
        const texte = promptAEnvoyer();
        await chargerOptionsPlateau();
        document.getElementById("idee-creation").value = texte;
        const sansReecriture = document.getElementById("sans-reecriture");
        sansReecriture.checked = true;
        sansReecriture.dispatchEvent(new Event("change"));
        const cible = document.getElementById("workflow-prompt").value;
        if (cible && choixModele("workflow-creation").workflow !== cible) {  // même workflow : on garde la variante
            const w = optionsPlateau.workflows.texte_vers_image.choix.find((x) => x.id === cible);
            const select = document.getElementById("workflow-creation");
            select.value = `${w.id}::${w.origine}`;
            select.dispatchEvent(new Event("change"));
        }
        statut("");
        document.querySelector('.onglet[data-vue="plateau"]').click();
        if (creer) document.getElementById("form-creation").requestSubmit();
        else document.getElementById("idee-creation").focus();
    });
}

// Envoi vers le Montage : Texte → vidéo, sans réécriture, avec la durée choisie ici.
async function envoyerVersMontage(tourner) {
    await avecLlama(async (statut) => {
        await traduireSiBesoin(statut);
        const texte = promptAEnvoyer();
        if (!optionsMontage) await preparerMontage();  // remplit les listes du Montage avant d'y poser nos valeurs
        document.querySelector('input[name="mode-montage"][value="texte"]').click();
        document.getElementById("idee-montage").value = texte;
        document.getElementById("sans-reecriture-montage").checked = true;
        const duree = document.getElementById("duree-montage");
        duree.value = document.getElementById("duree-prompt").value;
        duree.dispatchEvent(new Event("change"));
        statut("");
        document.querySelector('.onglet[data-vue="montage"]').click();
        window.scrollTo({ top: 0, behavior: "smooth" });
        if (tourner) document.getElementById("form-montage").requestSubmit();
        else document.getElementById("idee-montage").focus();
    });
}

document.getElementById("envoyer-prompt").addEventListener("click", () =>
    (cibleVideo() ? envoyerVersMontage(false) : envoyerVersPlateau(false)));
document.getElementById("creer-prompt").addEventListener("click", () =>
    (cibleVideo() ? envoyerVersMontage(true) : envoyerVersPlateau(true)));

// Personnages enregistrés : seulement le bloc Personnage (et l'âge précis)

async function chargerPersonnages(choisi) {
    personnagesEnregistres = await api("GET", "/api/prompt/personnages");
    const select = document.getElementById("personnage-prompt");
    const garde = choisi ?? select.value;
    select.innerHTML = '<option value="">—</option>' + personnagesEnregistres.map((p) =>
        `<option value="${echapper(p.nom)}" ${p.nom === garde ? "selected" : ""}>${echapper(p.nom)}</option>`).join("");
    document.getElementById("supprimer-personnage").disabled = !select.value;
}

document.getElementById("personnage-prompt").addEventListener("change", (e) => {
    document.getElementById("supprimer-personnage").disabled = !e.target.value;
    const p = personnagesEnregistres.find((x) => x.nom === e.target.value);
    if (!p) return;
    const etat = etatPrompt();
    etat.choix.personnage = { ...p.choix };
    etat.age = p.age;
    document.getElementById("age-prompt").value = p.age ?? "";
    // Un personnage enregistré garde son teint, ses yeux et son nez, même hors de ce que son origine propose.
    const c = contextePrompt({ ...etat, toutes: false });
    etat.toutes = Object.entries(etat.choix.personnage).some(([sl, nom]) => {
        const v = definitionsPrompt[cle("personnage", sl)]?.valeurs.find((x) => x.nom === nom);
        return v && !selonOrigine(v, c);
    });
    document.getElementById("toutes-options").checked = etat.toutes;
    appliquerReglesPrompt();  // une valeur disparue des listes est retirée, avec un message
    planifierAssemblage();
});

document.getElementById("enregistrer-personnage").addEventListener("click", async () => {
    const etat = etatPrompt();
    const nom = prompt("Nom du personnage :", document.getElementById("personnage-prompt").value);
    if (!nom?.trim()) return;
    if (personnagesEnregistres.some((p) => p.nom === nom.trim())
        && !confirm(`Remplacer le personnage « ${nom.trim()} » ?`)) return;
    try {
        const p = await api("PUT", `/api/prompt/personnages/${encodeURIComponent(nom.trim())}`,
            { choix: etat.choix.personnage ?? {}, age: etat.age });
        await chargerPersonnages(p.nom);
    } catch (err) {
        alert(err.message);
    }
});

document.getElementById("supprimer-personnage").addEventListener("click", async () => {
    const nom = document.getElementById("personnage-prompt").value;
    if (!nom || !confirm(`Supprimer le personnage « ${nom} » ?`)) return;
    try {
        await api("DELETE", `/api/prompt/personnages/${encodeURIComponent(nom)}`);
    } catch (err) {
        alert(err.message);
    }
    await chargerPersonnages("");
});

// Événements

document.getElementById("sujets-prompt").addEventListener("click", (e) => {
    const bouton = e.target.closest("button[data-sujet]");
    if (bouton && bouton.dataset.sujet !== sujetPrompt) {
        peaufinage = null;
        document.getElementById("peaufine-prompt").hidden = true;
        choisirSujet(bouton.dataset.sujet);
    }
});

document.getElementById("blocs-prompt").addEventListener("change", (e) => {
    if (e.target.id === "toutes-options") {
        etatPrompt().toutes = e.target.checked;
        appliquerReglesPrompt();
        planifierAssemblage();
        return;
    }
    const div = e.target.closest(".sous-liste");
    if (e.target.classList.contains("categorie-sl") && div) {  // filtre d'affichage : le choix fait ne bouge pas
        categoriesAffichees[cle(sujetPrompt, div.dataset.bloc, div.dataset.sl)] = e.target.value;
        appliquerReglesPrompt();
        return;
    }
    if (e.target.classList.contains("valeur-sl") && div) changerChoix(div.dataset.bloc, div.dataset.sl, e.target.value);
});

document.getElementById("blocs-prompt").addEventListener("click", (e) => {
    const de = e.target.closest(".de");
    if (de && tirer(de.closest(".sous-liste"))) planifierAssemblage();
});

document.getElementById("blocs-prompt").addEventListener("input", (e) => {
    if (e.target.id !== "age-prompt") return;
    const etat = etatPrompt();
    const age = e.target.value === "" ? null : Number(e.target.value);
    const [min, max] = structurePrompt.age_precis;
    if (age !== null && (!Number.isInteger(age) || age < min || age > max)) return;  // saisie en cours
    etat.age = age;
    // L'âge précis choisit la tranche, comme le fera le serveur.
    const tranche = age === null ? null : definitionsPrompt[cle("personnage", "Âge")].valeurs
        .find((v) => v.bornes.length && v.bornes[0] <= age && age <= v.bornes[1]);
    if (tranche) (etat.choix.personnage ??= {})["Âge"] = tranche.nom;
    appliquerReglesPrompt();
    planifierAssemblage();
});

// L'événement toggle ne remonte pas : on l'écoute en phase de capture.
document.getElementById("blocs-prompt").addEventListener("toggle", (e) => {
    if (!e.target.dataset?.groupe) return;
    if (e.target.open) groupesOuvertsPrompt.add(e.target.dataset.groupe);
    else groupesOuvertsPrompt.delete(e.target.dataset.groupe);
}, true);

document.getElementById("etiquettes-prompt").addEventListener("click", (e) => {
    const etiquette = e.target.closest(".etiquette");
    if (etiquette) changerChoix(etiquette.dataset.bloc, etiquette.dataset.sl, "");
});

[["texte-prompt", "texte"], ["precisions-prompt", "precisions"], ["action-prompt", "action"],
 ["replique-prompt", "replique"]].forEach(([id, champ]) => {
    document.getElementById(id).addEventListener("input", (e) => {
        etatPrompt()[champ] = e.target.value;
        planifierAssemblage();
    });
});

document.getElementById("workflow-prompt").addEventListener("change", () => {
    majCible();
    planifierAssemblage();
});

document.getElementById("effacer-prompt").addEventListener("click", () => {
    delete etatsPrompt[sujetPrompt];
    document.getElementById("personnage-prompt").value = "";
    document.getElementById("supprimer-personnage").disabled = true;
    rendreBlocsPrompt();
    appliquerReglesPrompt();
    planifierAssemblage();
});
