// Scénario, atelier Séquence. Le fil de l'histoire (l'idée de l'atelier, facultatif) puis les plans, le cœur de
// l'atelier : chacun a son type, sa durée, « ce qui se passe », sa réplique (🗣, mot pour mot, au début, pendant
// l'action ou à la fin, voix off possible) et son prompt anglais, modifiable. ✨ d'un plan l'écrit seul (le LLM
// connaît le fil et les plans voisins), 💬 d'un plan ouvre le chat sous ce plan, ✨ Tout écrire écrit tous les plans
// d'un coup (app/decoupage.decouper). À droite, le résumé, puis la séquence créée dans le Montage, prête à tourner.

const TYPES_DEPART = [["texte", "✍️ Départ du texte"], ["rush", "🖼️ Départ de la référence"]];
const TYPES_SUITE = [["prolonger", "↪ Prolonger"], ["couper", "✂ Couper"], ["transition", "🌀 Transition"]];
const NOTES_TYPES = {
    texte: "Ce qui se passe : elle entre dans le hangar, casque sous le bras…",
    rush: "Ce qui bouge dans la référence : elle tourne la tête vers la porte…",
    prolonger: "La suite du même plan : elle pose son casque sur l'établi…",
    couper: "Le nouveau moment ou angle : gros plan sur ses mains…",
    transition: "Où la caméra arrive : la rue sous la pluie…",
};
const MOMENTS = [["debut", "au début"], ["pendant", "pendant l'action"], ["fin", "à la fin"]];
const plansOuverts = new WeakSet();  // plans prêts dépliés à la main (ou tout juste écrits) : rien n'est enregistré

const decoupage = { reference: null, contrainte: "", contrainteEn: null, plans: [], alertes: [],
                    avecMusique: false, musique: "", aidePlan: null,  // aidePlan : le plan dont le chat est ouvert
                    sequenceId: null };  // la séquence du Montage qu'un nouvel envoi mettra à jour
let decoupagePret = false;

const enSequence = () => formeScenario === "sequence";

// Un plan complet, quel que soit le brouillon d'où il vient (les anciens n'avaient ni réplique ni prompt).
const normaliserPlan = (p = {}) => ({
    enchainement: p.enchainement ?? "couper", duree: p.duree ?? 5, note: p.note ?? "", rush: p.rush ?? null,
    dialogue: Boolean(p.dialogue), replique: p.replique ?? "", moment: p.moment ?? "pendant", off: Boolean(p.off),
    prompt: p.prompt ?? "", idee: p.idee ?? "", ecrit: p.ecrit ?? null, echanges: p.echanges ?? [], dernier: p.dernier ?? null,
    idMontage: p.idMontage ?? null,  // le plan correspondant dans la séquence du Montage (mise à jour : garde son clip)
});

const nouveauPlanDecoupage = (k, enchainement) => normaliserPlan({
    enchainement: enchainement ?? (k === 0 ? (decoupage.reference ? "rush" : "texte") : "couper"),
});

// Ce que le prompt d'un plan a pris en compte : s'il change ensuite, le plan est à réécrire.
const signature = (p) => JSON.stringify([p.enchainement, p.duree, p.note.trim(), p.dialogue ? p.replique.trim() : "",
                                         p.moment, p.off, p.rush]);

function etatPlan(p) {
    if (!p.prompt.trim()) return ["a-ecrire", "✍️ à écrire"];
    if (p.ecrit && p.ecrit !== signature(p)) return ["perime", "⚠ changé : ✨ pour réécrire"];
    return ["pret", "✅ prompt prêt"];
}

// Une réplique trop longue pour son plan : le serveur prévient aussi (decoupage.MOTS_PAR_SECONDE).
function motsReplique(p) {
    const mots = compterMots(p.replique);
    const permis = Math.floor((optionsPlateau?.video.sequence.mots_par_seconde ?? 2.5) * p.duree);
    return { mots, permis, trop: mots > permis };
}

function majForme() {
    if (enSequence()) preparerDecoupage();
    placerChat();
}

function preparerDecoupage() {
    if (!decoupagePret) {
        decoupagePret = true;
        const max = optionsPlateau.video.sequence.plans_max;
        remplirSelect("nombre-plans-scenario", Array.from({ length: max - 1 }, (_, i) => [String(i + 2), String(i + 2)]), "3");
        if (decoupage.plans.length < 2) {  // sinon : les plans du brouillon repris
            decoupage.plans = [nouveauPlanDecoupage(0), nouveauPlanDecoupage(1, "prolonger"), nouveauPlanDecoupage(2)];
        }
    }
    rendreReference();
    rendrePlansDecoupage();
    majBoutonsEnvoi();
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

const planOuvert = (p, k) => etatPlan(p)[0] !== "pret" || k === decoupage.aidePlan || plansOuverts.has(p);

function cartePlan(p, k, durees) {
    const types = k === 0 ? TYPES_DEPART : TYPES_SUITE;
    const [classe, etat] = etatPlan(p);
    const ouvert = planOuvert(p, k);
    const resume = p.note.trim() || p.idee.trim() || p.prompt.trim();
    const { mots, permis, trop } = motsReplique(p);
    return `<li class="plan-scenario ${k === decoupage.aidePlan ? "aide-ouverte" : ""} ${ouvert ? "" : "repliee"}" data-plan="${k}">
        <div class="ligne entete-plan-scenario">
            ${classe === "pret" ? `<button type="button" class="petit replier-plan" data-replier
                title="${ouvert ? "Replier ce plan" : "Déplier ce plan"}">${ouvert ? "▾" : "▸"}</button>` : ""}
            <strong>Plan ${k + 1}</strong>
            <select data-champ="enchainement">${types.map(([v, t]) => `<option value="${v}"
                ${v === p.enchainement ? "selected" : ""} ${v === "rush" && !decoupage.reference ? "disabled" : ""}>${t}</option>`).join("")}</select>
            <select data-champ="duree">${durees.map((d) =>
                `<option value="${d}" ${d === p.duree ? "selected" : ""}>${d} s</option>`).join("")}</select>
            <span class="etat-plan ${classe}">${etat}</span>
            <span class="actions-plan">
                <button type="button" class="petit" data-optimiser-plan title="Le LLM écrit le prompt de ce plan">✨ Écrire</button>
                <button type="button" class="petit" data-aide-plan title="Le LLM te pose ses questions sur ce plan">💬 Préciser</button>
            </span>
        </div>
        <p class="resume-plan discret">${echapper(resume.length > 120 ? `${resume.slice(0, 120)}…` : resume)}</p>
        <div class="corps-plan-scenario">
            ${p.enchainement === "transition"
                ? caseImage(p.rush, "data-arrivee", "Rush d'arrivée (facultatif : sinon, à choisir dans le Montage)") : ""}
            <textarea data-champ="note" rows="2" maxlength="500"
                      placeholder="${echapper(NOTES_TYPES[p.enchainement])}">${echapper(p.note)}</textarea>
        </div>
        <div class="dialogue-plan">
            <label class="case"><input type="checkbox" data-champ="dialogue" ${p.dialogue ? "checked" : ""}> 🗣 Dialogue</label>
            ${p.dialogue ? `<div class="replique-plan">
                <input data-champ="replique" type="text" maxlength="300" value="${echapper(p.replique)}"
                       placeholder="Ce que dit le personnage, mot pour mot : « Tu m'as encore attendu ? »">
                <div class="ligne moment-plan">
                    <span class="discret">Quand :</span>
                    ${MOMENTS.map(([v, t]) => `<label class="case"><input type="radio" name="moment-plan-${k}"
                        data-champ="moment" value="${v}" ${p.moment === v ? "checked" : ""}> ${t}</label>`).join("")}
                    <label class="case" title="On entend la voix, ses lèvres restent fermées">
                        <input type="checkbox" data-champ="off" ${p.off ? "checked" : ""}> voix off</label>
                    <small class="discret mots-replique ${trop ? "ko" : ""}">${mots ? `${mots} mots · ${permis} au plus pour ${p.duree} s` : ""}</small>
                </div>
            </div>` : ""}
        </div>
        <label class="prompt-plan"><span class="discret">Prompt du plan, en anglais, tourné tel quel
            <small class="mots-plan">${compterMots(p.prompt) ? `· ${compterMots(p.prompt)} mots` : ""}</small></span>
            <textarea data-champ="prompt" rows="3"
                      placeholder="✨ ou 💬 l'écrivent ici ; tu peux le retoucher, ou l'écrire toi-même.">${echapper(p.prompt)}</textarea>
        </label>
        <div class="place-chat-plan"></div>
    </li>`;
}

function rendrePlansDecoupage() {
    rangerChat();  // le chat d'un plan vit dans sa carte : il sort avant qu'on la refasse
    const durees = optionsPlateau.video.durees;
    document.getElementById("nombre-plans-scenario").value = String(decoupage.plans.length);
    document.getElementById("plans-scenario").innerHTML = decoupage.plans.map((p, k) => cartePlan(p, k, durees)).join("");
    majBoutonsPrompt();
    placerChat();
    rendreResume();
}

// Sans refaire la carte (la saisie garde le focus) : l'état, les compteurs.
function majCartePlan(k) {
    const li = document.querySelector(`.plan-scenario[data-plan="${k}"]`);
    const p = decoupage.plans[k];
    if (!li || !p) return;
    const [classe, etat] = etatPlan(p);
    const badge = li.querySelector(".etat-plan");
    badge.className = `etat-plan ${classe}`;
    badge.textContent = etat;
    li.querySelector(".mots-plan").textContent = compterMots(p.prompt) ? `· ${compterMots(p.prompt)} mots` : "";
    const compteur = li.querySelector(".mots-replique");
    if (compteur) {
        const { mots, permis, trop } = motsReplique(p);
        compteur.textContent = mots ? `${mots} mots · ${permis} au plus pour ${p.duree} s` : "";
        compteur.classList.toggle("ko", trop);
    }
    const prompt = li.querySelector('[data-champ="prompt"]');
    if (prompt.value !== p.prompt) prompt.value = p.prompt;
    rendreResume();
}

// À droite : un plan par ligne (clic : on y va), la durée, les alertes, puis la création dans le Montage.
function rendreResume() {
    const total = decoupage.plans.reduce((t, p) => t + p.duree, 0);
    document.getElementById("duree-sequence-scenario").textContent = `film d'environ ${total} s`;
    document.getElementById("resume-sequence").innerHTML = decoupage.plans.map((p, k) => {
        const [classe, etat] = etatPlan(p);
        const type = [...TYPES_DEPART, ...TYPES_SUITE].find(([v]) => v === p.enchainement)?.[1] ?? "";
        const texte = p.note.trim() || p.idee.trim() || "—";
        return `<li data-aller="${k}" class="${classe}" title="${echapper(etat)}">
            <strong>${k + 1}</strong> <span class="discret">${echapper(type)} · ${p.duree} s${p.dialogue && p.replique.trim() ? " · 🗣" : ""}</span>
            <span class="texte-resume">${echapper(texte.length > 70 ? `${texte.slice(0, 70)}…` : texte)}</span>
            <span class="etat-resume">${etat.split(" ")[0]}</span></li>`;
    }).join("");
    document.getElementById("alertes-decoupage").innerHTML =
        decoupage.alertes.map((a) => `<li>${echapper(a)}</li>`).join("");
}

function planChange(k) {
    majCartePlan(k);
    garderBrouillon();
}

// Un plan écrit par le LLM (✨, ✨ Tout écrire ou 💬) : son prompt, son résumé ; « ce qui se passe » vide est rempli.
function poserPlan(k, ecrit) {
    const p = decoupage.plans[k];
    if (!p) return;
    plansOuverts.add(p);  // tout juste écrit : il reste ouvert, pour être relu
    p.prompt = ecrit.prompt;
    if (ecrit.idee) p.idee = ecrit.idee;
    if (!p.note.trim() && ecrit.idee) p.note = ecrit.idee;
    p.ecrit = signature(p);
}

// Les alertes d'un plan réécrit remplacent les siennes ; celles des autres restent.
function majAlertes(alertes, k = null) {
    decoupage.alertes = k === null ? alertes
        : [...decoupage.alertes.filter((a) => !a.startsWith(`Plan ${k + 1} :`)), ...alertes];
}

// Ce que le serveur reçoit pour écrire (✨, ✨ Tout écrire, 💬 d'un plan) : le fil, le personnage et le décor des
// compléments, chaque plan avec sa réplique et son prompt déjà écrit (le contexte de ses voisins).
function demandeSequence() {
    const etat = etatPrompt();
    const choix = Object.fromEntries(Object.entries(etat.choix).filter(([bloc]) =>
        !structurePrompt.blocs.find((b) => b.id === bloc)?.video));  // le Plan d'un clip seul ne compte pas ici
    return {
        synopsis: atelier("sequence").idee, sujet: sujetPrompt, choix, age: etat.age,
        plans: decoupage.plans.map((p) => ({
            enchainement: p.enchainement, duree: p.duree, note: p.note, rush: p.rush,
            replique: p.dialogue ? p.replique : "", moment: p.moment, off: p.dialogue && p.off, prompt: p.prompt,
        })),
        reference: decoupage.reference, contrainte: decoupage.contrainte, contrainte_en: decoupage.contrainteEn,
        musique: decoupage.avecMusique ? (decoupage.musique.trim() || "music that fits the story, your choice") : null,
    };
}

// ✨ Tout écrire, le bouton ✨ de l'atelier : tous les plans d'un coup, d'après le fil et ce que disent les plans.
function decouperSequence() {
    const ecrits = decoupage.plans.filter((p) => p.prompt.trim()).length;
    if (ecrits && !confirm(`✨ Tout écrire réécrit aussi ${ecrits > 1 ? `les ${ecrits} plans` : "le plan"} qui `
        + `${ecrits > 1 ? "ont" : "a"} déjà un prompt, tes retouches à la main comprises. Continuer ?`)) return;
    avecLlama(async (statut, signal) => {
        statut(`${nomLlm()} écrit tous les plans… (le premier appel charge le modèle)`);
        const r = await api("POST", "/api/sequences/decouper", { ...demandeSequence(), llm: llmAtelier() }, signal);
        r.plans.forEach((ecrit, k) => poserPlan(k, ecrit));
        majAlertes(r.alertes);
        decoupage.contrainteEn = r.contrainte_en;
        rendrePlansDecoupage();
        garderBrouillon();
        statut(`${nomLlm()} · tous les plans écrits en ${r.duree_s} s · relis-les, retouche si besoin`);
    });
}

// ✨ d'un plan : ce plan seul, en continuité avec ses voisins.
function ecrirePlan(k) {
    avecLlama(async (statut, signal) => {
        statut(`${nomLlm()} écrit le plan ${k + 1}… (le premier appel charge le modèle)`);
        const r = await api("POST", "/api/sequences/decouper", { ...demandeSequence(), plan: k, llm: llmAtelier() },
            signal);
        poserPlan(k, r.plans[0]);
        majAlertes(r.alertes, k);
        decoupage.contrainteEn = r.contrainte_en;
        rendrePlansDecoupage();
        garderBrouillon();
        statut(`${nomLlm()} · plan ${k + 1} écrit en ${r.duree_s} s`);
    });
}

// 💬 d'un plan : le chat (libre.js) descend sous ce plan, avec sa propre conversation. Re-cliquer le referme.
function rangerChat() {
    document.getElementById("place-chat-atelier").appendChild(document.getElementById("chat-libre"));
}

function placerChat() {
    const k = enSequence() ? decoupage.aidePlan : null;
    const place = k !== null ? document.querySelector(`.plan-scenario[data-plan="${k}"] .place-chat-plan`) : null;
    (place ?? document.getElementById("place-chat-atelier")).appendChild(document.getElementById("chat-libre"));
}

function ouvrirAidePlan(k) {
    const chat = document.getElementById("chat-libre");
    if (decoupage.aidePlan === k && !chat.hidden) {
        decoupage.aidePlan = null;
        chat.hidden = true;
        rendrePlansDecoupage();
        return;
    }
    decoupage.aidePlan = k;
    rendrePlansDecoupage();
    chat.hidden = false;
    rendreLibre();
    const p = decoupage.plans[k];
    if (!p.echanges.length) envoyerLibre(p.note.trim() || `Aide-moi à écrire le plan ${k + 1}.`);
    else document.getElementById("message-libre").focus();
}

// Événements : référence, plans, musique, contraintes

document.getElementById("reference-scenario").addEventListener("click", (e) => {
    if (e.target.closest("[data-retirer]")) {
        decoupage.reference = null;
        if (decoupage.plans[0]?.enchainement === "rush") decoupage.plans[0].enchainement = "texte";
        rendreReference();
        rendrePlansDecoupage();
        garderBrouillon();
        return;
    }
    ouvrirChoixRush((id) => {
        decoupage.reference = id;
        rendreReference();
        rendrePlansDecoupage();
        garderBrouillon();
    }, "Quelle image de référence pour le personnage ?");
});

document.getElementById("musique-active-sequence").addEventListener("change", (e) => {
    decoupage.avecMusique = e.target.checked;
    majCases();
    garderBrouillon();
});
document.getElementById("musique-sequence").addEventListener("input", (e) => {
    decoupage.musique = e.target.value;
    garderBrouillon();
});

document.getElementById("contrainte-scenario").addEventListener("input", (e) => {
    decoupage.contrainte = e.target.value;  // donnée au LLM, puis gardée au casting de la séquence créée
    garderBrouillon();
});

document.getElementById("nombre-plans-scenario").addEventListener("change", (e) => {
    const n = Number(e.target.value);
    const remplis = decoupage.plans.slice(n).filter((p) => p.note.trim() || p.replique.trim() || p.prompt.trim()).length;
    if (remplis && !confirm(`Passer à ${n} plans supprime ${remplis > 1 ? `${remplis} plans déjà remplis`
        : "un plan déjà rempli"} (ce qui s'y passe, réplique, prompt). Continuer ?`)) {
        e.target.value = String(decoupage.plans.length);
        return;
    }
    decoupage.plans = decoupage.plans.slice(0, n);
    while (decoupage.plans.length < n) decoupage.plans.push(nouveauPlanDecoupage(decoupage.plans.length));
    if (decoupage.aidePlan !== null && decoupage.aidePlan >= n) {
        decoupage.aidePlan = null;
        document.getElementById("chat-libre").hidden = true;
    }
    decoupage.alertes = decoupage.alertes.filter((a) => !/^Plan (\d+) :/.test(a) || Number(a.match(/^Plan (\d+)/)[1]) <= n);
    rendrePlansDecoupage();
    garderBrouillon();
});

const plansScenario = document.getElementById("plans-scenario");
const indexPlan = (e) => Number(e.target.closest(".plan-scenario")?.dataset.plan);

plansScenario.addEventListener("change", (e) => {
    const champ = e.target.dataset.champ;
    const k = indexPlan(e);
    const p = decoupage.plans[k];
    if (!p || !["enchainement", "duree", "dialogue", "moment", "off"].includes(champ)) return;
    if (champ === "duree") p.duree = Number(e.target.value);
    else if (champ === "dialogue" || champ === "off") p[champ] = e.target.checked;
    else p[champ] = e.target.value;
    if (champ === "enchainement" && p.enchainement !== "transition") p.rush = null;
    if (champ === "moment" || champ === "off") planChange(k);
    else {
        rendrePlansDecoupage();
        garderBrouillon();
        if (champ === "dialogue" && p.dialogue) {
            document.querySelector(`.plan-scenario[data-plan="${k}"] [data-champ="replique"]`)?.focus();
        }
    }
});
plansScenario.addEventListener("input", (e) => {
    const champ = e.target.dataset.champ;
    const k = indexPlan(e);
    if (!decoupage.plans[k] || !["note", "replique", "prompt"].includes(champ)) return;
    decoupage.plans[k][champ] = e.target.value;
    planChange(k);
});
plansScenario.addEventListener("click", (e) => {
    const k = indexPlan(e);
    const p = decoupage.plans[k];
    if (!p) return;
    if (e.target.closest("[data-replier]")) {
        if (planOuvert(p, k)) plansOuverts.delete(p);
        else plansOuverts.add(p);
        return rendrePlansDecoupage();
    }
    if (e.target.closest("[data-optimiser-plan]")) return ecrirePlan(k);
    if (e.target.closest("[data-aide-plan]")) return ouvrirAidePlan(k);
    if (!e.target.closest("[data-arrivee]")) return;
    if (e.target.closest("[data-retirer]")) {
        p.rush = null;
        rendrePlansDecoupage();
        garderBrouillon();
        return;
    }
    ouvrirChoixRush((id) => {
        p.rush = id;
        rendrePlansDecoupage();
        garderBrouillon();
    }, "Sur quel rush finit la transition ?");
});

document.getElementById("resume-sequence").addEventListener("click", (e) => {
    const ligne = e.target.closest("[data-aller]");
    const carte = ligne && document.querySelector(`.plan-scenario[data-plan="${ligne.dataset.aller}"]`);
    if (!carte) return;
    carte.scrollIntoView({ behavior: "smooth", block: "center" });
    carte.querySelector('[data-champ="note"]').focus({ preventScroll: true });
});

// Envoi au Montage : la première fois, la séquence est créée ; ensuite, elle est mise à jour (ses réglages du Montage,
// voix, qualité, LoRA, restent, et un plan dont le prompt n'a pas changé garde son clip). Les prompts sont imposés.

async function envoyerSequence(mettreAJour) {
    const titre = document.getElementById("titre-sequence-scenario");
    const erreur = document.getElementById("erreur-sequence-scenario");
    erreur.hidden = true;
    const manquant = decoupage.plans.findIndex((p) => !p.prompt.trim());
    const probleme = !titre.value.trim()
        ? "Donne un titre à la séquence : ses plans seront rangés dans un dossier à son nom."
        : manquant >= 0 ? `Plan ${manquant + 1} : pas encore de prompt (✨ Écrire, 💬 Préciser, ou écris-le).` : "";
    if (probleme) {
        afficherErreur(erreur, probleme);
        if (!titre.value.trim()) titre.focus();
        return;
    }
    const enValide = decoupage.contrainteEn?.fr === decoupage.contrainte.trim().replace(/\s+/g, " ");
    try {
        let existante = null;
        if (mettreAJour && decoupage.sequenceId) {
            existante = await api("GET", `/api/sequences/${encodeURIComponent(decoupage.sequenceId)}`)
                .catch(() => null);  // supprimée depuis : une nouvelle est créée
        }
        const avant = Object.fromEntries((existante?.plans ?? []).map((p) => [p.id, p]));
        const plans = decoupage.plans.map((p, k) => ({
            ...(avant[p.idMontage] ? { id: p.idMontage, seed: avant[p.idMontage].seed } : {}),
            idee: p.idee || p.note || `Plan ${k + 1}`, prompt: p.prompt, duree: p.duree,
            // Départ de la référence : l'image du personnage est le rush du plan 1 (le serveur l'exige).
            enchainement: p.enchainement, rush: p.enchainement === "rush" ? decoupage.reference : p.rush,
            ambiance: true,
        }));
        const contrainteEn = enValide ? decoupage.contrainteEn : null;  // traduite à l'écriture : le tournage s'en sert
        const s = existante
            ? await api("PUT", `/api/sequences/${encodeURIComponent(existante.id)}`, {
                titre: titre.value.trim(), qualite: existante.qualite, format: existante.format, llm: llmAtelier(),
                modele: existante.modele, loras: existante.loras ?? [], contrainte_en: contrainteEn, plans,
                casting: { ...existante.casting, personnage: decoupage.reference, contrainte: decoupage.contrainte.trim() },
            })
            : await api("POST", "/api/sequences", {
                titre: titre.value.trim(), qualite: "rapide", format: "16:9 (Widescreen)", llm: llmAtelier(),
                casting: { personnage: decoupage.reference, voix: "auto", ambiance: "",
                           contrainte: decoupage.contrainte.trim(), reference_partout: true },
                contrainte_en: contrainteEn, plans,
            });
        decoupage.sequenceId = s.id;  // la prochaine fois : mise à jour
        s.plans.forEach((plan, k) => { if (decoupage.plans[k]) decoupage.plans[k].idMontage = plan.id; });
        majBoutonsEnvoi();
        garderBrouillon();
        await ouvrirSequence(s.id);
    } catch (err) {
        afficherErreur(erreur, err.message);
    }
}

function majBoutonsEnvoi() {
    const titre = document.getElementById("titre-sequence-scenario").value.trim();
    document.getElementById("creer-sequence-scenario").textContent = decoupage.sequenceId
        ? `→ Mettre à jour${titre ? ` « ${titre} »` : ""}` : "→ Créer dans le Montage";
    document.getElementById("nouvelle-sequence-scenario").hidden = !decoupage.sequenceId;
}

document.getElementById("creer-sequence-scenario").addEventListener("click", () => envoyerSequence(true));
document.getElementById("nouvelle-sequence-scenario").addEventListener("click", () => envoyerSequence(false));
document.getElementById("titre-sequence-scenario").addEventListener("input", () => {
    majBoutonsEnvoi();
    garderBrouillon();
});

// ✍️ du Montage : la séquence ouverte là-bas revient au Scénario pour être retravaillée (ses plans, leurs prompts,
// la référence et la contrainte ; les répliques sont dans les prompts). Le prochain envoi la met à jour.
function chargerSequenceAuScenario(s) {
    const remplis = decoupage.plans.some((p) => p.note.trim() || p.prompt.trim());
    if (remplis && decoupage.sequenceId !== s.id
        && !confirm(`Le Scénario a déjà une séquence en cours d'écriture : la remplacer par « ${s.titre} » ?`)) return false;
    const premier = s.plans[0];
    decoupage.sequenceId = s.id;
    decoupage.reference = s.casting.personnage ?? (premier?.enchainement === "rush" ? premier.rush : null);
    decoupage.contrainte = s.casting.contrainte ?? "";
    decoupage.contrainteEn = s.contrainte_en ?? null;
    decoupage.alertes = [];
    decoupage.aidePlan = null;
    decoupage.plans = s.plans.map((p, k) => {
        const plan = normaliserPlan({
            enchainement: k === 0 && p.enchainement === "rush" && !decoupage.reference ? "texte" : p.enchainement,
            duree: p.duree, note: p.idee ?? "", idee: p.idee ?? "", prompt: p.prompt ?? "", idMontage: p.id,
            rush: p.enchainement === "transition" ? p.rush : null,
        });
        plan.ecrit = signature(plan);  // ses prompts sont prêts tels quels
        return plan;
    });
    // Le Montage accepte un seul plan, le Scénario en veut deux au moins : un plan vide complète.
    while (decoupage.plans.length < 2) decoupage.plans.push(nouveauPlanDecoupage(decoupage.plans.length));
    document.getElementById("titre-sequence-scenario").value = s.titre;
    document.getElementById("contrainte-scenario").value = decoupage.contrainte;
    decoupagePret = false;  // les listes (nombre de plans) se refont à l'ouverture de l'atelier
    garderBrouillon();
    return true;
}
