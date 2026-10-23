// Montage, mode Séquence : un casting, des plans enchaînés (prolonger, couper, transition), tournés dans l'ordre
// et assemblés en un film par le serveur (app/sequences.py). La séquence se modifie ici, puis s'enregistre ;
// un plan modifié n'est plus « à jour » et sera retourné, avec les plans qui partent de lui.

const ENCHAINEMENTS_DEPART = [["rush", "🖼️ Depuis un rush"], ["texte", "✍️ Depuis le texte"]];
const ENCHAINEMENTS_SUITE = [["prolonger", "↪ Prolonger"], ["couper", "✂ Couper"], ["transition", "🌀 Transition"]];
const AIDE_ENCHAINEMENT = {
    rush: "Le plan anime ce rush.",
    texte: "Le plan naît du texte seul.",
    prolonger: "Même plan, sans coupure : la fin du plan précédent et son son continuent ici.",
    couper: "Nouveau plan : le personnage et la voix de référence gardent la ressemblance.",
    transition: "Le plan part de la dernière image du précédent et finit exactement sur ce rush.",
};
const REPLIQUE = /[«"“]/;

let sequenceCourante = null;
let sequencesConnues = [];
let commandeSequence = null;
let sequenceModifiee = false;

const nouveauPlan = (enchainement) => ({ idee: "", duree: 5, enchainement, rush: null, ambiance: true,
                                        clip: null, a_jour: false });

function sequenceVide() {
    // Rapide par défaut : choix de Jérémy pour les séquences (les plans qui parlent montent seuls à 12 steps).
    return { id: null, titre: "", qualite: "rapide", format: "16:9 (Widescreen)", llm: null, film: null,
             casting: { personnage: null, voix: "auto", ambiance: "" }, plans: [nouveauPlan("rush")] };
}

async function preparerSequence() {
    if (!optionsMontage) await preparerMontage();
    const v = optionsMontage.video;
    remplirSelect("qualite-sequence", v.qualites.map((q) => [q.cle, `${q.nom} · ${q.megapixels} MP`]), "rapide");
    remplirSelect("format-sequence", optionsMontage.formats.map((f) => [f, f]), "16:9 (Widescreen)");
    remplirSelect("llm-sequence", optionsMontage.llm.choix.map((m) => [m.fichier, m.favori ? `${m.nom} ★` : m.nom]),
                  optionsMontage.llm.defaut);
    const clips = await api("GET", "/api/creations?genre=clips");
    remplirSelect("voix-sequence", [["auto", "Auto : le premier plan qui parle"], ["", "Aucune référence"],
        ...clips.filter((f) => f.mode_video !== "film").map((f) => [f.id, `Clip : ${(f.idee || f.id).slice(0, 50)}`])], "auto");
    await chargerListeSequences();
    if (!sequenceCourante) afficherSequence(sequencesConnues[0] ?? sequenceVide());
}

async function chargerListeSequences() {
    sequencesConnues = await api("GET", "/api/sequences");
    const choix = document.getElementById("choix-sequence");
    const garde = sequenceCourante?.id ?? "";
    choix.innerHTML = '<option value="">＋ Nouvelle séquence</option>' + sequencesConnues.map((s) =>
        `<option value="${echapper(s.id)}" ${s.id === garde ? "selected" : ""}>${echapper(s.titre)}</option>`).join("");
}

async function ouvrirSequence(id) {  // depuis la fiche d'un plan ou d'un film des Bobines
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
    document.getElementById("ambiance-sequence").value = s.casting.ambiance ?? "";
    document.getElementById("voix-sequence").value = s.casting.voix ?? "";
    document.getElementById("qualite-sequence").value = s.qualite;
    document.getElementById("format-sequence").value = s.format;
    if (s.llm) document.getElementById("llm-sequence").value = s.llm;
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
                <label class="case discret" title="Décoche pour un plan qui se passe ailleurs">
                    <input type="checkbox" data-champ="ambiance" ${p.ambiance !== false ? "checked" : ""}> suit l'ambiance</label>
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
                <textarea data-champ="idee" rows="3" maxlength="1000"
                          placeholder="L'idée du plan, en français. Les répliques entre guillemets : elle dit « On décolle ce soir. »">${echapper(p.idee)}</textarea>
                ${p.clip ? `<video class="clip-plan" src="${urlVideo(p.clip)}" poster="${urlImage(p.clip)}" muted loop playsinline
                                   preload="none" title="Survole pour voir le plan tourné"></video>` : ""}
            </div>
            ${p.prompt ? `<details class="prompt-impose">
                <summary class="discret">📝 Prompt du Scénario : c'est lui qui est tourné, pas l'idée ci-dessus</summary>
                <textarea data-champ="prompt" rows="4">${echapper(p.prompt)}</textarea>
                <button type="button" class="petit" data-action="oublier-prompt">Réécrire depuis l'idée à la place</button>
            </details>` : ""}
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

function estimationSequence(indices) {
    const v = optionsMontage.video;
    const q = v.qualites.find((x) => x.cle === sequenceCourante.qualite);
    const cout = (p) => q.estimation_s * v.sequence.surcout * (p.duree / 5) ** 1.25
        * (REPLIQUE.test(p.idee) && q.steps < v.sequence.steps_parole ? v.sequence.surcout_parole : 1);
    return indices.reduce((total, k) => total + cout(sequenceCourante.plans[k]), indices.length ? 5 : 0);
}

function majEtat() {
    const indices = aTourner();
    document.querySelectorAll("#plans-sequence .plan-sequence").forEach((li) => {
        const k = Number(li.dataset.plan);
        const p = sequenceCourante.plans[k];
        const parle = REPLIQUE.test(p.idee) ? " · 🗣 voix soignée (12 steps)" : "";
        li.querySelector(".etat-plan").textContent = (indices.includes(k) ? "⏳ à tourner" : "✅ à jour") + parle;
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
document.getElementById("ambiance-sequence").addEventListener("input", (e) => {
    sequenceCourante.casting.ambiance = e.target.value;
    modifiee(true);
});
[["voix-sequence", (v) => { sequenceCourante.casting.voix = v || null; }],
 ["qualite-sequence", (v) => { sequenceCourante.qualite = v; }],
 ["format-sequence", (v) => { sequenceCourante.format = v; }]].forEach(([id, poser]) => {
    document.getElementById(id).addEventListener("change", (e) => {
        poser(e.target.value);
        modifiee(true);  // casting, qualité ou format : toute la séquence est à retourner (taille, voix)
    });
});
document.getElementById("llm-sequence").addEventListener("change", (e) => {
    sequenceCourante.llm = e.target.value;
    modifiee();
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
    modifiee();
});
plansEl.addEventListener("change", (e) => {
    const champ = e.target.dataset.champ;
    if (!champ || champ === "idee") return;
    const p = sequenceCourante.plans[planDe(e.target)];
    p[champ] = champ === "duree" ? Number(e.target.value) : champ === "ambiance" ? e.target.checked : e.target.value;
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
    if (action === "oublier-prompt") {  // Llama réécrira ce plan depuis son idée, au tournage
        plans[k].prompt = null;
        plans[k].a_jour = false;
        sequenceModifiee = true;
        rendrePlans();
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
// Survol : le plan tourné se lance.
plansEl.addEventListener("mouseover", (e) => { if (e.target.tagName === "VIDEO") e.target.play().catch(() => {}); });
plansEl.addEventListener("mouseout", (e) => { if (e.target.tagName === "VIDEO") e.target.pause(); });

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
             casting: { personnage: s.casting.personnage, voix: s.casting.voix, ambiance: s.casting.ambiance },
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
        erreur.textContent = err.message;
        erreur.hidden = false;
        return false;
    }
}

document.getElementById("enregistrer-sequence").addEventListener("click", enregistrerSequence);

document.getElementById("supprimer-sequence").addEventListener("click", async () => {
    if (!sequenceCourante.id || !confirm(`Supprimer la séquence « ${sequenceCourante.titre} » ? `
        + "Ses plans et son film restent dans les Bobines.")) return;
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
        erreur.textContent = "Donne un titre à la séquence : ses clips seront rangés dans un dossier à son nom.";
        erreur.hidden = false;
        return;
    }
    if ((sequenceModifiee || !sequenceCourante.id) && !(await enregistrerSequence())) return;
    const indices = plans ?? (aTourner().length ? null : sequenceCourante.plans.map((_, k) => k));
    const estimation = estimationSequence(aTourner(indices ?? []));
    try {
        commandeSequence = await api("POST", `/api/sequences/${encodeURIComponent(sequenceCourante.id)}/tourner`,
                                     { plans: indices });
    } catch (err) {
        const erreur = document.getElementById("erreur-sequence");
        erreur.textContent = err.message;
        erreur.hidden = false;
        return;
    }
    bloquerSequence(true);
    document.getElementById("progression-sequence").hidden = false;
    suivreSequence(Date.now(), estimation);
}

function bloquerSequence(bloque) {
    document.querySelectorAll("#sequence-montage button, #sequence-montage select, #sequence-montage input, "
        + "#sequence-montage textarea").forEach((el) => { el.disabled = bloque; });
    if (!bloque) rendrePlans();  // remet les boutons dans leur état normal (↑ du premier plan, etc.)
}

async function suivreSequence(debut, estimation) {
    const c = await api("GET", `/api/commandes/${commandeSequence.id}`);
    const bloc = document.getElementById("progression-sequence");
    const ecoule = (Date.now() - debut) / 1000;
    const reste = Math.max(0, estimation - ecoule);
    bloc.querySelector(".barre div").style.width = `${Math.round(100 * Math.min(1, ecoule / Math.max(estimation, 1)))}%`;
    bloc.querySelector("span").textContent = c.etat === "attente"
        ? `En file d'attente… ${enMinutes(ecoule)}`
        : `${c.etape || "Préparation"} · ${Math.round(100 * c.progression)} % du plan · ${enMinutes(ecoule)}`
          + (reste > 0 ? ` · encore ≈ ${enMinutes(reste)}` : "");
    if (["attente", "en_cours"].includes(c.etat)) {
        setTimeout(() => suivreSequence(debut, estimation), 1500);
        return;
    }
    bloc.hidden = true;
    const s = await api("GET", `/api/sequences/${encodeURIComponent(commandeSequence.sequence)}`);
    afficherSequence(s);  // les plans tournés avant une erreur sont gardés
    bloquerSequence(false);
    bobines.charger();
    if (c.etat === "erreur") {
        const erreur = document.getElementById("erreur-sequence");
        erreur.textContent = c.erreur;
        erreur.hidden = false;
    } else {
        document.getElementById("film-sequence").scrollIntoView({ behavior: "smooth", block: "center" });
    }
}

document.getElementById("tourner-sequence").addEventListener("click", () => tournerSequence());
