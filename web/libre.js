// Scénario, 💬 M'aider à préciser : chaque atelier a sa conversation (POST /api/prompt/libre), et chaque plan d'une
// séquence la sienne (💬 d'un plan, decoupage.js : le chat descend sous ce plan). Le LLM pose les questions
// essentielles avec des réponses à cliquer et tient la fiche « Compris » (✕ retire une ligne fausse). ✍️ Écris-le :
// en Image, Retouche et Clip, le même rédacteur que ✨ Écrire le prompt (scenario.js) écrit avec ton idée et ce que le
// chat a compris ; en Séquence, le chat écrit le fil de l'histoire, et pour un plan, le prompt de ce plan (il sait tout
// ce que sait ✨ Tout écrire). Pendant qu'il réfléchit : une bulle qui compte les secondes et ✕ Annuler.

const ECRIRE_LIBRE = "ÉCRIS LE PROMPT";  // comme prompt.ECRIRE : ce tour-là réfléchit

// La conversation en cours : celle de l'atelier, ou celle du plan dont le chat est ouvert.
const planAide = () => (formeScenario === "sequence" ? decoupage.aidePlan : null);
const conversationActive = () => (planAide() !== null ? decoupage.plans[planAide()] : atelier());

// Les notes d'Aria (« ✍️ Prompt écrit… ») restent dans la page : le serveur ne reçoit que toi et le LLM.
const pourServeur = (echanges) => echanges.filter((e) => e.role !== "note");
const toursLibre = (a) => a.echanges.filter((e) => e.role === "user").length;
const questionCourte = (question) => (question ?? "").replace(/\s*\?\s*$/, "").trim();
const ligneChoix = (question, choix) => `${questionCourte(question)} : ${choix}`;

// Une question sur une rubrique du guide : toute la liste de la rubrique, en liste déroulante, à côté des propositions.
function valeursRubrique(nom) {
    for (const b of structurePrompt?.blocs ?? []) for (const g of b.groupes) {
        const sl = g.sous_listes.find((s) => s.nom === nom);
        if (sl) return sl.valeurs.filter((v) => !v.remplace?.length).map((v) => v.nom);
    }
    return [];
}

function listeDeroulante(q, i) {
    const valeurs = valeursRubrique(q.liste).filter((v) => !q.choix.includes(v));
    if (!valeurs.length) return "";
    return `<select class="liste-question" data-question="${i}" aria-label="${echapper(q.liste)}">
        <option value="" selected disabled>Toute la liste : ${echapper(q.liste)}…</option>
        ${valeurs.map((v) => `<option value="${echapper(v)}">${echapper(v)}</option>`).join("")}</select>`;
}

const lireTour = (contenu) => {
    try {
        return JSON.parse(contenu);
    } catch {
        return { message: contenu, fiche: {}, questions: [], prompt: null };
    }
};

// L'appel en cours : { conversation, texte, debut, controleur } (bulle d'attente, ✕ Annuler).
let attenteLibre = null;
let minuteurLibre = null;
const secondesAttente = () => `${Math.round((Date.now() - attenteLibre.debut) / 1000)} s`;

const bulle = (classe, auteur, corps) =>
    `<div class="bulle ${classe}"><span class="auteur">${echapper(auteur)}</span>${corps}</div>`;

function messageEcrit() {
    if (planAide() !== null) return "Prompt du plan écrit : il est dans le plan, modifiable.";
    return formeScenario === "sequence" ? "Fil écrit : il est au-dessus, ✨ Tout écrire le répartit dans les plans."
        : "Prompt écrit : il est à droite, modifiable.";
}

function rendreLibre() {
    const a = conversationActive();
    const nom = nomLlm();
    const dernierLlm = a.echanges.findLastIndex((e) => e.role === "assistant");
    // Ses questions attendent ta réponse tant que rien d'autre que des notes n'est venu après.
    const enAttente = dernierLlm >= 0 && a.echanges.slice(dernierLlm + 1).every((e) => e.role === "note");
    let html = a.echanges.map((e, k) => {
        if (e.role === "note") return `<div class="bulle bulle-note">${echapper(e.content)}</div>`;
        if (e.role === "user") {
            const ecrire = e.content === ECRIRE_LIBRE;
            const texte = ecrire ? "✍️ Écris-le maintenant" : e.content;
            // Au survol : réessayer (le LLM répond de nouveau), modifier (le texte revient dans le champ), copier.
            const actions = [["reessayer", "↻ Réessayer", "Le LLM répond de nouveau à ce message"],
                             ...(ecrire ? [] : [["modifier", "✎ Modifier", "Remet ce message dans le champ ; ce qui le suit disparaît"],
                                               ["copier", "⧉ Copier", "Copie ce message"]])]
                .map(([id, nom, aide]) => `<button type="button" class="petit" data-action-bulle="${id}" data-echange="${k}"
                    title="${aide}">${nom}</button>`).join("");
            return `<div class="message-toi">${bulle("bulle-toi", k === 0 ? "💡 Ton idée" : "Toi", `<p>${echapper(texte)}</p>`)}
                <div class="actions-bulle">${actions}</div></div>`;
        }
        const t = lireTour(e.content);
        const questions = t.questions ?? [];
        const corps = k === dernierLlm && enAttente
            ? questions.map((q, i) => `<div class="question-libre"><strong>${echapper(q.question)}</strong>
                <div class="choix-libre">${[...q.choix, "Je te laisse choisir"].map((c) => `<button type="button"
                    class="petit" data-question="${i}" data-choix="${echapper(c)}">${echapper(c)}</button>`).join("")}${
                    listeDeroulante(q, i)}</div>
              </div>`).join("")
            : questions.length ? `<ul class="questions-passees">${questions.map((q) =>
                `<li>${echapper(q.question)}</li>`).join("")}</ul>` : "";
        const ecrit = t.prompt ? `<p class="discret">✍️ ${messageEcrit()}</p>` : "";
        return bulle("bulle-llm", nom, `<p>${echapper(t.message || "…")}</p>${corps}${ecrit}`);
    }).join("");
    if (attenteLibre?.conversation === a) {
        html += bulle("bulle-llm bulle-attente", nom, `<p><span id="texte-attente">${echapper(attenteLibre.texte)}</span>
            · <span id="temps-libre">${secondesAttente()}</span></p>
            <button type="button" id="annuler-libre" class="petit">✕ Annuler</button>`);
    }
    const conversation = document.getElementById("conversation-libre");
    conversation.innerHTML = html;
    suivreDerniereBulle(conversation);

    const fiche = a.dernier?.fiche ?? {};
    document.getElementById("fiche-libre").innerHTML = Object.entries(fiche).map(([k, v]) => `<li>
        <span class="cle">${echapper(k)}</span> ${echapper(v)}
        <button type="button" class="oublier" data-oublier="${echapper(k)}" title="Faux ? Retire-le : le LLM le reposera">✕</button></li>`).join("");
    document.getElementById("fiche-vide-libre").hidden = Object.keys(fiche).length > 0;

    document.getElementById("titre-chat").textContent = planAide() !== null ? `💬 Plan ${planAide() + 1} · ${nom}`
        : formeScenario === "sequence" ? `💬 Préciser avec ${nom}` : `Conversation · ${nom}`;
    // Image, Retouche, Clip : le chat est toujours ouvert, il n'y a rien à replier.
    document.getElementById("replier-libre").hidden = planAide() === null && formeScenario !== "sequence";
    const vide = !a.echanges.length;
    document.getElementById("retour-libre").disabled = occupePrompt || vide;
    document.getElementById("recommencer-libre").disabled = occupePrompt || vide;
    majChoixLibre();
    majBoutonAide();
}

// Une proposition est « choisie » quand sa ligne est dans le champ de réponse.
function majChoixLibre() {
    const lignes = new Set(document.getElementById("message-libre").value.split("\n").map((l) => l.trim()));
    const questions = conversationActive().dernier?.questions ?? [];
    document.querySelectorAll("#conversation-libre [data-choix]").forEach((b) => {
        b.classList.toggle("choisi", lignes.has(ligneChoix(questions[Number(b.dataset.question)]?.question,
                                                          b.dataset.choix)));
    });
}

// 💬 de l'atelier : ouvre ou replie le chat ; replié, le bouton dit combien de tours il garde.
function majBoutonAide() {
    const a = atelier();
    const n = toursLibre(a);
    const ouvert = !document.getElementById("chat-libre").hidden && planAide() === null;
    document.getElementById("aide-scenario").textContent = ouvert || !n ? "💬 M'aider à préciser"
        : `💬 Reprendre la conversation (${n} ${n > 1 ? "tours" : "tour"})`;
}

function replierAide() {
    if (planAide() !== null) {  // le chat d'un plan : il se referme comme en re-cliquant son 💬
        ouvrirAidePlan(planAide());
        return;
    }
    atelier().chatReplie = true;
    document.getElementById("chat-libre").hidden = true;
    majBoutonAide();
    garderBrouillon();
}

function basculerAide() {
    const ouvert = !document.getElementById("chat-libre").hidden && planAide() === null;
    if (ouvert) replierAide();
    else ouvrirAide();
}

// 💬 de l'atelier : la conversation s'ouvre sous l'idée ; la première fois, elle part de ton idée.
function ouvrirAide() {
    if (formeScenario === "sequence" && decoupage.aidePlan !== null) {
        decoupage.aidePlan = null;
        rendrePlansDecoupage();
    }
    const a = atelier();
    a.chatReplie = false;
    placerChat();
    document.getElementById("chat-libre").hidden = false;
    rendreLibre();
    if (!a.echanges.length && a.idee.trim()) envoyerLibre(a.idee);
    else document.getElementById("message-libre").focus();
}

// Un appel au LLM depuis le chat : bulle d'attente qui compte les secondes, ✕ Annuler (le serveur arrête le LLM),
// boutons bloqués. Échec ou annulation : `defaire` remet la conversation comme avant. Rend la réponse, ou null.
async function tourLibre(a, texte, appel, defaire) {
    const erreur = document.getElementById("erreur-libre");
    const statut = document.getElementById("statut-libre");
    erreur.hidden = true;
    statut.textContent = "";
    attenteLibre = { conversation: a, texte, debut: Date.now(), controleur: new AbortController() };
    occupePrompt = true;
    majBoutonsPrompt();
    bloquerChat(true);
    rendreLibre();
    minuteurLibre = setInterval(() => {
        const temps = document.getElementById("temps-libre");
        if (temps && attenteLibre) temps.textContent = secondesAttente();
    }, 1000);
    const changerTexte = (m) => {
        if (!m || !attenteLibre) return;
        attenteLibre.texte = m;
        const zone = document.getElementById("texte-attente");
        if (zone) zone.textContent = m;
    };
    try {
        return await appel(attenteLibre.controleur.signal, changerTexte);
    } catch (err) {
        defaire();
        if (estAnnulation(err)) statut.textContent = "Annulé.";
        else afficherErreur(erreur, err.message);
        return null;
    } finally {
        clearInterval(minuteurLibre);
        attenteLibre = null;
        occupePrompt = false;
        majBoutonsPrompt();
        bloquerChat(false);
        rendreLibre();
        garderBrouillon();
    }
}

function bloquerChat(bloque) {
    document.querySelectorAll("#form-libre button, .actions-chat button:not(#replier-libre)")
        .forEach((b) => { b.disabled = bloque; });
}

async function envoyerLibre(texte) {
    texte = texte.trim();
    if (!texte || occupePrompt) return;
    const a = conversationActive();
    const plan = planAide();  // un plan de la séquence, gardé même si on change de plan pendant que le LLM écrit
    const llm = llmAtelier();
    const forme = plan !== null ? "plan" : formeScenario;
    const champ = document.getElementById("message-libre");
    a.echanges.push({ role: "user", content: texte });
    if (plan === null && formeScenario !== "sequence" && a.echanges.length === 1 && texte !== ECRIRE_LIBRE) {
        a.idee = texte;  // Image, Retouche, Clip : ton premier message est ton idée (✍️ Écris-le et l'envoi la relisent)
        document.getElementById("idee-scenario").value = texte;
    }
    if (texte !== ECRIRE_LIBRE) champ.value = "";
    const premier = a.echanges.length === 1;
    const r = await tourLibre(a, texte === ECRIRE_LIBRE ? "Il écrit… (il réfléchit)"
        : premier ? "Il lit ton idée… (le premier appel charge le modèle)" : "Il lit ta réponse…",
        async (signal) => {
            if (forme === "clip") await traduireSiBesoin(() => {});  // la musique d'un clip, en anglais
            return api("POST", "/api/prompt/libre", forme === "plan"
                ? { echanges: pourServeur(a.echanges), forme, workflow: ciblePrompt(), llm,
                    sequence: { ...demandeSequence(), plan } }
                : { echanges: pourServeur(a.echanges), forme, workflow: ciblePrompt(), contexte: complements(),
                    images: imagesReference(), role: roleReference(), llm, duree: dureeClip(),
                    format: formatScenario(), sujet: sujetPrompt, sans_sujet: sansSujet() }, signal);
        },
        () => {
            a.echanges.pop();  // ta réponse revient dans le champ : renvoie-la
            if (texte !== ECRIRE_LIBRE && !champ.value.trim()) champ.value = texte;
        });
    if (!r) return;
    a.echanges.push({ role: "assistant", content: r.brut });
    a.dernier = r;
    if (r.prompt && forme === "plan") {  // le prompt du plan, contrôlé comme ✨ (réplique à son moment)
        poserPlan(plan, { prompt: r.prompt });
        majAlertes(r.alertes ?? [], plan);
        majCartePlan(plan);
    } else if (r.prompt && forme === "sequence") {  // le fil rejoint l'idée, que ✨ Tout écrire répartit
        a.idee = r.prompt;
        if (formeScenario === forme) document.getElementById("idee-scenario").value = r.prompt;
    } else if (r.prompt && formeScenario === forme) {
        poserFinal(r.prompt, llm);
    } else if (r.prompt) {
        a.final = r.prompt;
        a.llm = llm;
    }
    document.getElementById("statut-libre").textContent = `${virgule(r.duree_s, 1)} s`;
    rendreLibre();
    garderBrouillon();
}

// Ce que le rédacteur reçoit de ✍️ Écris-le : ton idée, ce que le chat a compris, et ta réponse pas encore envoyée.
function ideeComprise(a, enPlus) {
    const idee = a.idee.trim() || (a.echanges.find((e) => e.role === "user")?.content ?? "").trim();
    const lignes = [...Object.entries(a.dernier?.fiche ?? {}).map(([k, v]) => `- ${k} : ${v}`),
                    ...enPlus.split("\n").map((l) => l.trim()).filter(Boolean).map((l) => `- ${l}`)];
    return (lignes.length ? `${idee}\n\nPrécisé ensemble :\n${lignes.join("\n")}` : idee).slice(0, 2000);
}

// ✍️ Écris-le en Image, Retouche ou Clip : le rédacteur de ✨ Écrire le prompt (un LLM spécialisé du modèle cible y
// écrit avec sa propre consigne), puis une note dans la conversation. Tu peux continuer à préciser ensuite.
async function ecrireDepuisChat() {
    if (occupePrompt) return;
    const a = atelier();
    const forme = formeScenario;
    const llm = llmAtelier();
    const champ = document.getElementById("message-libre");
    const enPlus = champ.value;
    const idee = ideeComprise(a, enPlus);
    champ.value = "";
    const r = await tourLibre(a, "Il écrit le prompt…", (signal, statut) => ecrirePrompt(idee, statut, signal),
        () => { if (!champ.value.trim()) champ.value = enPlus; });
    if (!r) return;
    if (formeScenario === forme) poserFinal(r.prompt, llm);
    else {
        a.final = r.prompt;
        a.llm = llm;
    }
    a.echanges.push({ role: "note", content: `✍️ Prompt écrit par ${nomLlm()} (${r.mots} mots, ${virgule(r.duree_s, 1)} s) : `
        + "il est à droite, modifiable. Continue à préciser si besoin, puis ✍️ Écris-le à nouveau." });
    document.getElementById("statut-libre").textContent = `${virgule(r.duree_s, 1)} s`;
    rendreLibre();
    garderBrouillon();
}

// ↩️ Un tour en arrière : ta dernière réponse et ce que le LLM y a répondu disparaissent, ta réponse revient dans le
// champ. Une note « prompt écrit » s'en va seule (le prompt, lui, reste).
function tourEnArriere() {
    const a = conversationActive();
    const e = a.echanges;
    if (!e.length || occupePrompt) return;
    if (e[e.length - 1].role === "note") e.pop();
    else {
        while (e.length && e[e.length - 1].role !== "user") e.pop();
        const retiree = e.pop();
        const tonIdee = planAide() === null && !e.length;  // le premier message de l'atelier : ton idée, déjà au-dessus
        if (retiree && retiree.content !== ECRIRE_LIBRE && !tonIdee) {
            document.getElementById("message-libre").value = retiree.content;
        }
    }
    const k = e.findLastIndex((x) => x.role === "assistant");
    a.dernier = k >= 0 ? lireTour(e[k].content) : null;
    document.getElementById("erreur-libre").hidden = true;
    document.getElementById("statut-libre").textContent = "";
    rendreLibre();
    garderBrouillon();
}

// Actions au survol de ta bulle : ↻ le LLM répond de nouveau, ✎ le texte revient dans le champ (ce qui suivait
// disparaît), ⧉ copie. Réessayer et Modifier reviennent à ce message : tout ce qui le suivait est retiré.
async function actionBulle(action, k) {
    const a = conversationActive();
    const message = a.echanges[k];
    if (message?.role !== "user") return;
    const statut = document.getElementById("statut-libre");
    if (action === "copier") {
        try {
            await navigator.clipboard.writeText(message.content);
            statut.textContent = "Copié.";
        } catch {
            statut.textContent = "Copie refusée par le navigateur.";
        }
        return;
    }
    if (occupePrompt) return;
    const texte = message.content;
    a.echanges = a.echanges.slice(0, k);
    const dernier = a.echanges.findLastIndex((x) => x.role === "assistant");
    a.dernier = dernier >= 0 ? lireTour(a.echanges[dernier].content) : null;
    document.getElementById("erreur-libre").hidden = true;
    statut.textContent = "";
    if (action === "reessayer") {
        await envoyerLibre(texte);
        return;
    }
    const champ = document.getElementById("message-libre");
    champ.value = texte;
    rendreLibre();
    garderBrouillon();
    champ.focus();
}

// ✕ sur une ligne de « Compris » : elle quitte la fiche, ici et dans le dernier tour du LLM, qu'il relit.
function oublierLibre(cle) {
    const a = conversationActive();
    if (!a.dernier?.fiche || !(cle in a.dernier.fiche)) return;
    delete a.dernier.fiche[cle];
    const k = a.echanges.findLastIndex((e) => e.role === "assistant");
    const tour = k >= 0 ? lireTour(a.echanges[k].content) : null;
    if (tour?.fiche && cle in tour.fiche) {
        delete tour.fiche[cle];
        a.echanges[k].content = JSON.stringify(tour);
    }
    rendreLibre();
    garderBrouillon();
}

document.getElementById("form-libre").addEventListener("submit", (e) => {
    e.preventDefault();
    envoyerLibre(document.getElementById("message-libre").value);
});
document.getElementById("message-libre").addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        envoyerLibre(e.target.value);
    }
});
document.getElementById("message-libre").addEventListener("input", majChoixLibre);
document.getElementById("ecrire-libre").addEventListener("click", () => {
    if (planAide() !== null || formeScenario === "sequence") envoyerLibre(ECRIRE_LIBRE);
    else ecrireDepuisChat();
});
document.getElementById("retour-libre").addEventListener("click", tourEnArriere);
document.getElementById("replier-libre").addEventListener("click", replierAide);
document.getElementById("recommencer-libre").addEventListener("click", () => {
    const a = conversationActive();
    if (a.echanges.length && !confirm(planAide() !== null ? "Recommencer la conversation de ce plan ?"
        : "Recommencer la conversation de cet atelier ?")) return;
    a.echanges = [];
    a.dernier = null;
    document.getElementById("erreur-libre").hidden = true;
    document.getElementById("statut-libre").textContent = "";
    rendreLibre();
    garderBrouillon();
});
document.getElementById("fiche-libre").addEventListener("click", (e) => {
    const bouton = e.target.closest("[data-oublier]");
    if (bouton) oublierLibre(bouton.dataset.oublier);
});

// Une proposition cliquée s'écrit dans le champ (« question : réponse ») ; une autre de la même question la remplace,
// la même une seconde fois l'enlève. ✕ Annuler arrête le LLM en cours.
document.getElementById("conversation-libre").addEventListener("click", (e) => {
    if (e.target.closest("#annuler-libre")) {
        attenteLibre?.controleur.abort();
        return;
    }
    const action = e.target.closest("[data-action-bulle]");
    if (action) {
        actionBulle(action.dataset.actionBulle, Number(action.dataset.echange));
        return;
    }
    const bouton = e.target.closest("[data-choix]");
    if (bouton) repondreQuestion(Number(bouton.dataset.question), bouton.dataset.choix);
});

// La réponse à une question (proposition ou valeur de la liste) devient une ligne du champ, qui remplace l'ancienne.
function repondreQuestion(i, choix) {
    const question = conversationActive().dernier?.questions?.[i]?.question;
    const ligne = ligneChoix(question, choix);
    const champ = document.getElementById("message-libre");
    const lignes = champ.value.split("\n").map((l) => l.trim()).filter(Boolean);
    const restantes = lignes.filter((l) => !l.startsWith(`${questionCourte(question)} : `));
    champ.value = [...restantes, ...(lignes.includes(ligne) ? [] : [ligne])].join("\n");
    majChoixLibre();
    champ.focus();
}

document.addEventListener("change", (e) => {
    const liste = e.target.closest?.("select.liste-question");
    if (liste?.value) repondreQuestion(Number(liste.dataset.question), liste.value);
});
