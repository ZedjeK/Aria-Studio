// Scénario, 💬 Aide : chaque atelier a sa conversation (POST /api/prompt/libre). Le LLM pose les questions
// essentielles avec des réponses à cliquer, tient la fiche « Ce que j'ai compris », puis écrit : le prompt final
// (Image, Retouche, Clip) ou l'histoire (Séquence, que ✂ Découper découpe ensuite). Il connaît les compléments
// de l'atelier et voit ses images de référence.

const ECRIRE_LIBRE = "ÉCRIS LE PROMPT";  // comme prompt.ECRIRE : ce tour-là réfléchit

const lireTour = (contenu) => {
    try {
        return JSON.parse(contenu);
    } catch {
        return { message: contenu, fiche: {}, questions: [], prompt: null };
    }
};

function rendreLibre() {
    const a = atelier();
    const conversation = document.getElementById("conversation-libre");
    conversation.innerHTML = a.echanges.map((e, k) => {
        if (e.role === "user") {
            const texte = e.content === ECRIRE_LIBRE ? "✍️ Écris-le maintenant" : e.content;
            return `<div class="bulle bulle-toi">${echapper(texte)}</div>`;
        }
        const t = lireTour(e.content);
        const dernier = k === a.echanges.length - 1;
        return `<div class="bulle bulle-llm">
            <p>${echapper(t.message || "…")}</p>
            ${(t.questions ?? []).map((q, i) => `<div class="question-libre">
                <strong>${echapper(q.question)}</strong>
                ${dernier ? `<div class="choix-libre">${[...q.choix, "Je te laisse choisir"].map((c) =>
                    `<button type="button" class="petit" data-question="${i}" data-choix="${echapper(c)}">${echapper(c)}</button>`).join("")}</div>`
                : ""}
            </div>`).join("")}
            ${t.prompt ? `<p class="discret">✍️ ${formeScenario === "sequence" ? "Histoire écrite : elle est dans ton idée."
                : "Prompt écrit : il est à droite, modifiable."}</p>` : ""}
        </div>`;
    }).join("");
    conversation.scrollTop = conversation.scrollHeight;
    const fiche = a.dernier?.fiche ?? {};
    document.getElementById("fiche-libre").innerHTML = Object.entries(fiche).map(([k, v]) =>
        `<dt>${echapper(k)}</dt><dd>${echapper(v)}</dd>`).join("");
    document.getElementById("fiche-vide-libre").hidden = Object.keys(fiche).length > 0;
}

// 💬 Aide : la conversation s'ouvre ; la première fois, elle part de ton idée.
function ouvrirAide() {
    document.getElementById("chat-libre").hidden = false;
    const a = atelier();
    if (!a.echanges.length && a.idee.trim()) envoyerLibre(a.idee);
    else document.getElementById("message-libre").focus();
}

async function envoyerLibre(texte) {
    texte = texte.trim();
    if (!texte || occupePrompt) return;
    const a = atelier();
    const forme = formeScenario;
    const erreur = document.getElementById("erreur-libre");
    const statut = document.getElementById("statut-libre");
    erreur.hidden = true;
    a.echanges.push({ role: "user", content: texte });
    rendreLibre();
    occupePrompt = true;
    majBoutonsPrompt();
    document.querySelectorAll("#form-libre button, .actions-libre button").forEach((b) => { b.disabled = true; });
    statut.textContent = texte === ECRIRE_LIBRE ? `${nomLlm()} écrit… (il réfléchit)` : `${nomLlm()} lit ta réponse…`;
    try {
        if (forme === "clip") await traduireSiBesoin(() => {});  // la musique d'un clip, en anglais
        const r = await api("POST", "/api/prompt/libre", {
            echanges: a.echanges, forme, workflow: ciblePrompt(), contexte: complements(),
            images: imagesReference(), role: roleReference(), llm: llmAtelier(), duree: dureeClip(),
        });
        a.echanges.push({ role: "assistant", content: r.brut });
        a.dernier = r;
        if (r.prompt && forme === "sequence") {  // l'histoire rejoint l'idée, que ✂ Découper découpe
            a.idee = r.prompt;
            if (formeScenario === forme) document.getElementById("idee-scenario").value = r.prompt;
            histoireModifiee(r.prompt);
        } else if (r.prompt && formeScenario === forme) {
            poserFinal(r.prompt);
        } else if (r.prompt) {
            a.final = r.prompt;
        }
        statut.textContent = `${r.duree_s} s`;
        document.getElementById("message-libre").value = "";
    } catch (err) {
        a.echanges.pop();  // ta réponse reste dans le champ : renvoie-la
        if (texte !== ECRIRE_LIBRE) document.getElementById("message-libre").value = texte;
        erreur.textContent = err.message;
        erreur.hidden = false;
        statut.textContent = "";
    } finally {
        occupePrompt = false;
        majBoutonsPrompt();
        document.querySelectorAll("#form-libre button, .actions-libre button").forEach((b) => { b.disabled = false; });
        rendreLibre();
        garderBrouillon();
    }
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
document.getElementById("ecrire-libre").addEventListener("click", () => envoyerLibre(ECRIRE_LIBRE));
document.getElementById("recommencer-libre").addEventListener("click", () => {
    const a = atelier();
    if (a.echanges.length && !confirm("Recommencer la conversation de cet atelier ?")) return;
    a.echanges = [];
    a.dernier = null;
    document.getElementById("erreur-libre").hidden = true;
    document.getElementById("statut-libre").textContent = "";
    rendreLibre();
    garderBrouillon();
});

// Une proposition cliquée s'ajoute au champ (« question : réponse »), à compléter ou envoyer telle quelle.
document.getElementById("conversation-libre").addEventListener("click", (e) => {
    const bouton = e.target.closest("[data-choix]");
    if (!bouton) return;
    const q = atelier().dernier?.questions?.[Number(bouton.dataset.question)];
    const champ = document.getElementById("message-libre");
    const ligne = `${(q?.question ?? "").replace(/\s*\?$/, "")} : ${bouton.dataset.choix}`;
    champ.value = champ.value.trim() ? `${champ.value.trim()}\n${ligne}` : ligne;
    bouton.classList.add("choisi");
    champ.focus();
});
