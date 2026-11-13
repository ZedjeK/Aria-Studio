// Montage, mode Séquence : des plans enchaînés (prolonger, couper, transition), tournés dans l'ordre et assemblés
// en un film par le serveur (app/sequences.py). Leurs prompts s'écrivent au Scénario (histoire, découpage) ; ici on
// les retouche, on règle le tournage (personnage, voix, modèle, résolution) et on tourne : chaque plan tourne son prompt
// tel quel. Un plan modifié n'est plus « à jour » et sera retourné, avec les plans qui partent de lui.
// La voix vient de la bibliothèque data/voix (essais tournés en cabine silencieuse, ou fichiers importés ; app/voix.py).

// Les mêmes mots qu'au Scénario (decoupage.js) : « Départ… », Prolonger, Couper, Transition.
const ENCHAINEMENTS_DEPART = [["rush", "🖼️ Départ d'une image"], ["texte", "✍️ Départ du texte"]];
const ENCHAINEMENTS_SUITE = [["prolonger", "↪ Prolonger"], ["couper", "✂ Couper"], ["transition", "🌀 Transition"]];
const AIDE_ENCHAINEMENT = {
    rush: "Le plan anime ce rush.",
    texte: "Le plan naît du texte seul.",
    prolonger: "Même plan, sans coupure : la fin du plan précédent et son son continuent ici.",
    couper: "Nouveau plan : le personnage et la voix de référence gardent la ressemblance.",
    transition: "Le plan part de la dernière image du précédent et finit exactement sur ce rush.",
};
const REPLIQUE = /<d>|[«"“]/i;  // comme sequences._REPLIQUE : la balise <d> de H3, ou des guillemets

let sequenceCourante = null;
let sequencesConnues = [];
let sequenceModifiee = false;

const nouveauPlan = (enchainement) => ({ idee: "", prompt: "", duree: 5, enchainement, rush: null, ambiance: true,
                                        clip: null, a_jour: false });
const parlePlan = (p) => REPLIQUE.test(`${p.idee} ${p.prompt ?? ""}`);

function sequenceVide() {
    // Rapide par défaut : choix de l'auteur pour les séquences (les plans qui parlent montent seuls à 12 steps).
    return { id: null, titre: "", qualite: "rapide", format: "16:9 (Widescreen)", llm: null, film: null,
             casting: { personnage: null, voix: "auto", ambiance: "", contrainte: "", reference_partout: true },
             loras: [], plans: [nouveauPlan("rush")] };
}

let preparationSequence = null;

function preparerSequence() {
    preparationSequence ??= preparerSequenceMaintenant().finally(() => { preparationSequence = null; });
    return preparationSequence;
}

async function preparerSequenceMaintenant() {
    if (!optionsMontage) await preparerMontage();
    const v = optionsMontage.video;
    remplirSelect("qualite-sequence", v.qualites.map((q) => [q.cle, `${q.nom} · ${q.aide}`]), "rapide");
    const ck = checkpointsVideo();
    remplirSelect("modele-sequence", ck.lignes, ck.defaut);
    remplirSelect("format-sequence", optionsMontage.formats.map((f) => [f, f]), "16:9 (Widescreen)");
    document.getElementById("phrase-voix").placeholder = v.voix.phrase_defaut;
    document.getElementById("lancer-essai-voix").textContent = `🎙️ Tourner l'essai (≈ ${enMinutes(v.voix.estimation_s)})`;
    await chargerVoix();
    await chargerListeSequences();
    if (!sequenceCourante) afficherSequence(sequencesConnues[0] ?? sequenceVide());
}

async function chargerListeSequences() {
    sequencesConnues = await api("GET", "/api/sequences");
    const choix = document.getElementById("choix-sequence");
    const garde = sequenceCourante?.id ?? "";
    choix.innerHTML = '<option value="">＋ Nouvelle séquence</option>' + sequencesConnues.map((s) =>
        `<option value="${echapper(s.id)}" ${s.id === garde ? "selected" : ""}>${echapper(s.titre)}</option>`).join("");
    await chargerCorbeilleSequences();
}

// Corbeille des séquences (app/sequences.py) : restaurer, ou vider pour de bon
async function chargerCorbeilleSequences() {
    const corbeille = await api("GET", "/api/sequences/corbeille");
    document.getElementById("corbeille-sequences").hidden = !corbeille.length;
    document.getElementById("nombre-corbeille-sequences").textContent = corbeille.length;
    document.getElementById("liste-corbeille-sequences").innerHTML = corbeille.map((s) => `<li class="ligne">
        <span>${echapper(s.titre)}</span>
        <span class="discret">${s.plans} plan${s.plans > 1 ? "s" : ""}${
            s.date ? ` · le ${new Date(s.date).toLocaleDateString("fr-FR")}` : ""}</span>
        <button type="button" class="petit" data-restaurer-sequence="${echapper(s.id)}">Restaurer</button>
    </li>`).join("");
}

document.getElementById("corbeille-sequences").addEventListener("click", async (e) => {
    const cible = e.target.closest("[data-restaurer-sequence], #vider-corbeille-sequences");
    if (!cible) return;
    try {
        if (cible.dataset.restaurerSequence) {
            if (sequenceModifiee && !confirm("La séquence ouverte a des changements non enregistrés : les abandonner ?")) return;
            cible.disabled = true;
            const s = await api("POST", `/api/sequences/corbeille/${encodeURIComponent(cible.dataset.restaurerSequence)}/restaurer`);
            sequenceCourante = null;
            await chargerListeSequences();
            afficherSequence(s);
            document.getElementById("choix-sequence").value = s.id;
        } else {
            const n = document.getElementById("nombre-corbeille-sequences").textContent;
            if (!confirm(`Vider la corbeille ? ${n} séquence(s) seront effacées définitivement. `
                + "Leurs clips et leurs films restent dans les Bobines.")) return;
            await api("DELETE", "/api/sequences/corbeille");
            await chargerCorbeilleSequences();
        }
    } catch (err) {
        cible.disabled = false;
        alert(err.message);
    }
});

async function ouvrirSequence(id) {  // depuis la fiche d'un plan ou d'un film des Bobines, ou depuis le Scénario
    if (sequenceModifiee && !confirm(`La séquence ouverte au Montage${sequenceCourante?.titre ? ` (« ${sequenceCourante.titre} »)` : ""} `
        + "a des changements non enregistrés : les abandonner ?")) return false;
    document.querySelector('.onglet[data-vue="montage"]').click();
    document.querySelector('input[name="mode-montage"][value="sequence"]').click();
    await preparerSequence();
    afficherSequence(await api("GET", `/api/sequences/${encodeURIComponent(id)}`));
    document.getElementById("choix-sequence").value = id;
    window.scrollTo({ top: 0, behavior: "smooth" });
}

function afficherSequence(s) {
    sequenceCourante = structuredClone(s);
    sequenceModifiee = false;
    document.getElementById("choix-sequence").value = s.id ?? "";
    document.getElementById("titre-sequence").value = s.titre;
    const contrainte = document.getElementById("contrainte-lue-sequence");  // écrite au Scénario, ajoutée à chaque plan
    contrainte.textContent = s.casting.contrainte ? `🔒 Contrainte de tous les plans : ${s.casting.contrainte}` : "";
    contrainte.hidden = !s.casting.contrainte;
    document.getElementById("reference-partout-sequence").checked = s.casting.reference_partout !== false;
    document.getElementById("voix-sequence").value = s.casting.voix ?? "";
    majEcouteVoix();
    const alerte = document.getElementById("alerte-sequence");  // ex. sa voix de la bibliothèque a été supprimée
    alerte.textContent = s.alerte ?? "";
    alerte.hidden = !s.alerte;
    document.getElementById("qualite-sequence").value = s.qualite;
    // Une nouvelle séquence prend le défaut des réglages ; une séquence existante garde le sien.
    document.getElementById("modele-sequence").value = s.modele ?? checkpointsVideo().defaut ?? "";
    document.getElementById("format-sequence").value = s.format;
    // Ses LoRA, cochées ; une LoRA supprimée du disque n'est plus proposée (elle partira au prochain enregistrement).
    rendreLoras("loras-sequence", workflowVideo(), s.loras ?? []);
    sequenceCourante.loras = lorasChoisies("loras-sequence");
    document.getElementById("supprimer-sequence").disabled = !s.id;
    document.getElementById("erreur-sequence").hidden = true;
    const film = document.getElementById("film-sequence");
    film.hidden = !s.film;
    if (s.film) {
        const video = film.querySelector("video");
        video.src = urlVideo(s.film);
        video.poster = urlImage(s.film);
    }
    rendreCasting();
    rendrePlans();
}

// Modifications : tout passe par ici pour tenir à jour « à tourner » et l'estimation.

function modifiee(toutRetourner = false) {
    sequenceModifiee = true;
    if (toutRetourner) sequenceCourante.plans.forEach((p) => { p.a_jour = false; });
    majEtat();
}

function rendreCasting() {
    const perso = sequenceCourante.casting.personnage;
    const caseRush = document.getElementById("personnage-sequence");
    caseRush.className = `emplacement ${perso ? "plein" : "vide"}`;
    caseRush.innerHTML = perso
        ? `<img src="${urlImage(perso)}" alt="" draggable="false">
           <button type="button" class="petit retirer" data-retirer="1" title="Retirer">✕</button>`
        : '<span class="numero">👤</span>';
    document.getElementById("bloc-format-sequence").hidden = sequenceCourante.plans[0]?.enchainement !== "texte";
}

function caseRush(id, titre) {
    return id
        ? `<div class="emplacement plein" data-rush-plan title="${echapper(titre)}"><img src="${urlImage(id)}" alt="" draggable="false">
             <button type="button" class="petit retirer" data-retirer="1" title="Retirer">✕</button></div>`
        : `<div class="emplacement vide" data-rush-plan title="${echapper(titre)}"><span class="numero">＋</span></div>`;
}

function rendrePlans() {
    const plans = sequenceCourante.plans;
    const durees = optionsMontage.video.durees;
    document.getElementById("plans-sequence").innerHTML = plans.map((p, k) => {
        const choix = k === 0 ? ENCHAINEMENTS_DEPART : ENCHAINEMENTS_SUITE;
        const avecRush = ["rush", "transition"].includes(p.enchainement);
        return `<li class="plan-sequence" data-plan="${k}">
            <div class="ligne entete-plan">
                <strong>Plan ${k + 1}</strong>
                <select data-champ="enchainement">${choix.map(([v, t]) =>
                    `<option value="${v}" ${v === p.enchainement ? "selected" : ""}>${t}</option>`).join("")}</select>
                <select data-champ="duree">${durees.map((d) =>
                    `<option value="${d}" ${d === p.duree ? "selected" : ""}>${d} s</option>`).join("")}</select>
                <span class="etat-plan discret"></span>
                <span class="actions-plan">
                    <button type="button" class="petit" data-action="monter" title="Monter" ${k === 0 ? "disabled" : ""}>↑</button>
                    <button type="button" class="petit" data-action="descendre" title="Descendre"
                            ${k === plans.length - 1 ? "disabled" : ""}>↓</button>
                    <button type="button" class="petit" data-action="retourner" title="Retourner ce plan (et ceux qui en partent)"
                            ${p.clip ? "" : "hidden"}>🎬</button>
                    <button type="button" class="petit danger" data-action="supprimer" title="Supprimer ce plan"
                            ${plans.length === 1 ? "disabled" : ""}>✕</button>
                </span>
            </div>
            <p class="discret aide-plan">${AIDE_ENCHAINEMENT[p.enchainement]}</p>
            <div class="ligne corps-plan">
                ${avecRush ? caseRush(p.rush, k === 0 ? "Rush de départ" : "Rush d'arrivée") : ""}
                <div class="textes-plan">
                    <input data-champ="idee" type="text" maxlength="1000" value="${echapper(p.idee)}"
                           placeholder="Ce qui se passe (un résumé, pour s'y retrouver ; facultatif)">
                    <textarea data-champ="prompt" rows="4" class="${p.prompt ? "" : "manquant"}"
                              placeholder="Prompt du plan, tourné tel quel : il arrive du ✍️ Scénario (découpage), ou écris-le ici (en anglais).">${echapper(p.prompt ?? "")}</textarea>
                </div>
                ${p.clip ? `<video class="clip-plan" src="${urlVideo(p.clip)}" poster="${urlImage(p.clip)}" controls muted loop playsinline
                                   preload="none" title="Le plan tourné"></video>` : ""}
            </div>
        </li>`;
    }).join("");
    document.getElementById("ajouter-plan").disabled = plans.length >= optionsMontage.video.sequence.plans_max;
    rendreCasting();
    majEtat();
}

// Plans à tourner et estimation, calculés comme le serveur (sequences.a_tourner, sequences.estimation).
function aTourner(demandes = []) {
    const plans = sequenceCourante.plans;
    const choisis = new Set(demandes);
    plans.forEach((p, k) => { if (!(p.clip && p.a_jour)) choisis.add(k); });
    for (let k = 1; k < plans.length; k++) {
        if (choisis.has(k - 1) && ["prolonger", "transition"].includes(plans[k].enchainement)) choisis.add(k);
    }
    return [...choisis].sort((a, b) => a - b);
}

// Comme sequences.estimation : le tableau, mesuré aux steps de sa résolution, suit au prorata ceux du checkpoint.
const stepsSequence = () => stepsCheckpoint(document.getElementById("modele-sequence").value);

function estimationSequence(indices) {
    const v = optionsMontage.video;
    const q = v.qualites.find((x) => x.cle === sequenceCourante.qualite);
    const base = stepsSequence() || q.steps;
    const cout = (p) => q.estimation_s * (base / q.steps) * v.sequence.surcout * (p.duree / 5) ** 1.25
        * (parlePlan(p) && base < v.sequence.steps_parole ? v.sequence.surcout_parole : 1);
    return indices.reduce((total, k) => total + cout(sequenceCourante.plans[k]), indices.length ? 5 : 0);
}

function majEtat() {
    const indices = aTourner();
    document.querySelectorAll("#plans-sequence .plan-sequence").forEach((li) => {
        const k = Number(li.dataset.plan);
        const p = sequenceCourante.plans[k];
        const parole = optionsMontage.video.sequence.steps_parole;
        const parle = parlePlan(p) && stepsSequence() < parole ? ` · 🗣 voix soignée (${parole} steps)` : "";
        const sansPrompt = !(p.prompt ?? "").trim() ? " · ✍️ prompt à écrire" : "";
        li.querySelector(".etat-plan").textContent = (indices.includes(k) ? "⏳ à tourner" : "✅ à jour") + parle + sansPrompt;
        li.classList.toggle("a-jour", !indices.includes(k));
    });
    const duree = sequenceCourante.plans.reduce((t, p) => t + p.duree, 0);
    document.getElementById("estimation-sequence").textContent = indices.length
        ? `${indices.length} plan${indices.length > 1 ? "s" : ""} à tourner · ≈ ${enMinutes(estimationSequence(indices))} `
          + `sur ta carte · film d'environ ${duree} s`
        : `Tous les plans sont à jour · film d'environ ${duree} s`;
    document.getElementById("etat-sequence").textContent = sequenceModifiee ? "● non enregistrée" : "";
    const bouton = document.getElementById("tourner-sequence");
    bouton.textContent = indices.length ? "🎬 Tourner la séquence" : "🎬 Tout retourner";
}

// Événements du panneau

document.getElementById("choix-sequence").addEventListener("change", async (e) => {
    if (sequenceModifiee && !confirm("Abandonner les modifications non enregistrées ?")) {
        e.target.value = sequenceCourante.id ?? "";
        return;
    }
    afficherSequence(e.target.value ? await api("GET", `/api/sequences/${encodeURIComponent(e.target.value)}`) : sequenceVide());
});

document.getElementById("titre-sequence").addEventListener("input", (e) => {
    sequenceCourante.titre = e.target.value;
    modifiee();
});
document.getElementById("reference-partout-sequence").addEventListener("change", (e) => {
    sequenceCourante.casting.reference_partout = e.target.checked;
    modifiee(true);
});
document.getElementById("vers-scenario-sequence").addEventListener("click", async () => {
    // Une séquence ouverte (et enregistrée) part au Scénario pour être retravaillée ; le prochain envoi la met à jour.
    if (sequenceCourante?.id) {
        if (sequenceModifiee && !(await enregistrerSequence())) return;
        if (!chargerSequenceAuScenario(sequenceCourante)) return;  // decoupage.js
    }
    ouvrirScenario("sequence");
});
groupeBoutons("qualite-sequence", "Résolution");  // web/commun.js, comme au Montage
[["voix-sequence", (v) => { sequenceCourante.casting.voix = v || null; majEcouteVoix(); }],
 ["modele-sequence", (v) => { sequenceCourante.modele = v || null; }],
 ["qualite-sequence", (v) => { sequenceCourante.qualite = v; }],
 ["format-sequence", (v) => { sequenceCourante.format = v; }]].forEach(([id, poser]) => {
    document.getElementById(id).addEventListener("change", (e) => {
        poser(e.target.value);
        modifiee(true);  // casting, résolution ou format : toute la séquence est à retourner (taille, voix)
    });
});

// LoRA de la séquence : les mêmes sur tous les plans ; en changer fait retourner toute la séquence.
brancherLoras("loras-sequence", () => {
    sequenceCourante.loras = lorasChoisies("loras-sequence");
    modifiee(true);
});

document.getElementById("personnage-sequence").addEventListener("click", (e) => {
    if (e.target.closest("[data-retirer]")) {
        sequenceCourante.casting.personnage = null;
        rendreCasting();
        modifiee(true);
        return;
    }
    ouvrirChoixRush((id) => {
        sequenceCourante.casting.personnage = id;
        rendreCasting();
        modifiee(true);
    }, "Quel personnage pour la séquence ?");
});

const planDe = (el) => Number(el.closest(".plan-sequence").dataset.plan);

const plansEl = document.getElementById("plans-sequence");
plansEl.addEventListener("input", (e) => {
    const champ = e.target.dataset.champ;
    if (champ !== "idee" && champ !== "prompt") return;
    const p = sequenceCourante.plans[planDe(e.target)];
    p[champ] = e.target.value;
    p.a_jour = false;
    if (champ === "prompt") e.target.classList.toggle("manquant", !e.target.value.trim());
    modifiee();
});
plansEl.addEventListener("change", (e) => {
    const champ = e.target.dataset.champ;
    if (!champ || champ === "idee") return;
    const p = sequenceCourante.plans[planDe(e.target)];
    p[champ] = champ === "duree" ? Number(e.target.value) : e.target.value;
    if (champ === "enchainement" && !["rush", "transition"].includes(p.enchainement)) p.rush = null;
    p.a_jour = false;
    sequenceModifiee = true;
    if (champ === "enchainement") rendrePlans();  // la case rush apparaît ou disparaît
    else majEtat();
});
plansEl.addEventListener("click", (e) => {
    const k = e.target.closest(".plan-sequence") ? planDe(e.target) : null;
    if (k === null) return;
    const plans = sequenceCourante.plans;
    const caseRushPlan = e.target.closest("[data-rush-plan]");
    if (caseRushPlan) {
        if (e.target.closest("[data-retirer]")) {
            plans[k].rush = null;
        } else {
            ouvrirChoixRush((id) => {
                plans[k].rush = id;
                plans[k].a_jour = false;
                sequenceModifiee = true;
                rendrePlans();
            }, k === 0 ? "Quel rush pour démarrer ?" : "Sur quel rush finit la transition ?");
            return;
        }
        plans[k].a_jour = false;
        sequenceModifiee = true;
        rendrePlans();
        return;
    }
    const action = e.target.closest("[data-action]")?.dataset.action;
    if (!action) return;
    if (action === "retourner") {
        tournerSequence([k]);
        return;
    }
    if (action === "supprimer") {
        if (!confirm(`Supprimer le plan ${k + 1} ? Son clip reste dans les Bobines.`)) return;
        plans.splice(k, 1);
    } else {
        const autre = action === "monter" ? k - 1 : k + 1;
        [plans[k], plans[autre]] = [plans[autre], plans[k]];
        plans[k].a_jour = plans[autre].a_jour = false;
    }
    normaliserEnchainements();
    sequenceModifiee = true;
    rendrePlans();
});

// Après un déplacement ou une suppression : le premier plan démarre, les suivants s'enchaînent.
function normaliserEnchainements() {
    sequenceCourante.plans.forEach((p, k) => {
        if (k === 0 && !["rush", "texte"].includes(p.enchainement)) {
            p.enchainement = p.rush ? "rush" : "texte";
            p.a_jour = false;
        } else if (k > 0 && ["rush", "texte"].includes(p.enchainement)) {
            p.enchainement = "couper";
            p.rush = null;
            p.a_jour = false;
        }
    });
}

document.getElementById("ajouter-plan").addEventListener("click", () => {
    sequenceCourante.plans.push(nouveauPlan("couper"));
    sequenceModifiee = true;
    rendrePlans();
    plansEl.lastElementChild?.querySelector("textarea").focus();
});

// Enregistrement et suppression

function corpsSequence() {
    const s = sequenceCourante;
    return { titre: s.titre.trim(), qualite: s.qualite, format: s.format, llm: s.llm || null,
             modele: s.modele ?? (document.getElementById("modele-sequence").value || null),
             casting: { personnage: s.casting.personnage, voix: s.casting.voix, ambiance: s.casting.ambiance,
                        contrainte: s.casting.contrainte ?? "", reference_partout: s.casting.reference_partout !== false },
             loras: s.loras ?? [],
             plans: s.plans.map((p) => ({ ...p })) };
}

async function enregistrerSequence() {
    const erreur = document.getElementById("erreur-sequence");
    erreur.hidden = true;
    try {
        const s = sequenceCourante.id
            ? await api("PUT", `/api/sequences/${encodeURIComponent(sequenceCourante.id)}`, corpsSequence())
            : await api("POST", "/api/sequences", corpsSequence());
        afficherSequence(s);
        await chargerListeSequences();
        return true;
    } catch (err) {
        afficherErreur(erreur, err.message);
        return false;
    }
}

document.getElementById("enregistrer-sequence").addEventListener("click", enregistrerSequence);

document.getElementById("supprimer-sequence").addEventListener("click", async () => {
    if (!sequenceCourante.id || !confirm(`Mettre la séquence « ${sequenceCourante.titre} » à la corbeille ? `
        + "Ses plans et son film restent dans les Bobines. Tu pourras la restaurer depuis « Séquences à la corbeille ».")) return;
    await api("DELETE", `/api/sequences/${encodeURIComponent(sequenceCourante.id)}`);
    sequenceCourante = null;
    await chargerListeSequences();
    afficherSequence(sequencesConnues[0] ?? sequenceVide());
});

// Tournage

async function tournerSequence(plans = null) {
    if (!sequenceCourante.titre.trim()) {
        document.getElementById("titre-sequence").focus();
        const erreur = document.getElementById("erreur-sequence");
        afficherErreur(erreur, "Donne un titre à la séquence : ses clips seront rangés dans un dossier à son nom.");
        return;
    }
    if ((sequenceModifiee || !sequenceCourante.id) && !(await enregistrerSequence())) return;
    const indices = plans ?? (aTourner().length ? null : sequenceCourante.plans.map((_, k) => k));
    const estimation = estimationSequence(aTourner(indices ?? []));
    let commande;
    try {
        commande = await api("POST", `/api/sequences/${encodeURIComponent(sequenceCourante.id)}/tourner`,
                             { plans: indices });
    } catch (err) {
        const erreur = document.getElementById("erreur-sequence");
        afficherErreur(erreur, err.message);
        return;
    }
    bloquerSequence(true);
    document.getElementById("erreur-sequence").hidden = true;  // message d'un tournage précédent (erreur, annulation)
    document.getElementById("progression-sequence").hidden = false;
    suivreAnnulable(document.getElementById("progression-sequence"), commande.id);  // après le blocage
    suivreSequence(commande, Date.now(), estimation);
}

function bloquerSequence(bloque) {
    document.querySelectorAll("#sequence-montage button, #sequence-montage select, #sequence-montage input, "
        + "#sequence-montage textarea").forEach((el) => { el.disabled = bloque; });
    if (!bloque) rendrePlans();  // remet les boutons dans leur état normal (↑ du premier plan, etc.)
}

async function suivreSequence(commande, debut, estimation) {
    const bloc = document.getElementById("progression-sequence");
    let c = null;
    let panne = null;
    try {
        c = await suivreJusquauBout(commande.id, (r) => {
            const ecoule = (Date.now() - debut) / 1000;
            const reste = Math.max(0, estimation - ecoule);
            bloc.querySelector(".barre div").style.width = `${Math.round(100 * Math.min(1, ecoule / Math.max(estimation, 1)))}%`;
            bloc.querySelector("span").textContent = r.etat === "attente"
                ? `En file d'attente… ${enMinutes(ecoule)}`
                : `${r.etape || "Préparation"} · ${Math.round(100 * r.progression)} % du plan · ${enMinutes(ecoule)}`
                  + (reste > 0 ? ` · encore ≈ ${enMinutes(reste)}` : "");
        }, 1500);
    } catch (err) {
        panne = err;  // le panneau se débloque quand même : avant, il restait figé jusqu'au rechargement
    }
    bloc.hidden = true;
    try {
        afficherSequence(await api("GET", `/api/sequences/${encodeURIComponent(commande.sequence)}`));  // plans déjà tournés
    } catch {
        // la séquence reste affichée telle quelle
    }
    bloquerSequence(false);
    bobines.charger();
    if (!panne) {
        if (c.etat === "erreur") notifierFin("Séquence en erreur", c.erreur || "Le tournage a échoué.", "montage", false);
        else if (c.etat === "annule") notifierFin("Séquence annulée", messageAnnulation(c, "plan"), "montage", false);
        else notifierFin("Séquence terminée", `Film prêt · ${enMinutes((Date.now() - debut) / 1000)}`, "montage");
    }
    if (panne || c.etat === "erreur" || c.etat === "annule") {
        const erreur = document.getElementById("erreur-sequence");
        afficherErreur(erreur, panne
            ? `Suivi interrompu (${panne.message}) : le tournage continue peut-être, rouvre la séquence dans un moment.`
            : c.etat === "annule" ? messageAnnulation(c, "plan") : c.erreur);
    } else {
        document.getElementById("film-sequence").scrollIntoView({ behavior: "smooth", block: "center" });
    }
}

document.getElementById("tourner-sequence").addEventListener("click", () => tournerSequence());
