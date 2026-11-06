// Scénario, onglet Libre : le prompt s'écrit en conversation (POST /api/prompt/libre). Le LLM pose les questions
// essentielles avec des réponses à cliquer, tient la fiche « Ce que j'ai compris », écrit le prompt sur demande puis
// l'ajuste. Il connaît ce que les autres onglets fixent et voit l'image de référence. Le prompt écrit rejoint la
// version modifiable de l'aperçu, d'où il part vers le Plateau ou le Montage.

const ECRIRE_LIBRE = "ÉCRIS LE PROMPT";  // comme prompt.ECRIRE : ce tour-là réfléchit
const libre = { echanges: [], dernier: null };  // échanges envoyés au LLM ; dernier tour rendu (fiche, questions)

const lireTour = (contenu) => {
    try {
        return JSON.parse(contenu);
    } catch {
        return { message: contenu, fiche: {}, questions: [], prompt: null };
    }
};

// Ce que les autres onglets fixent déjà : le prompt assemblé, s'il y a des choix (sinon « A person. » ne dit rien).
function contexteLibre() {
    const e = etatPrompt();
    const choisi = Object.values(e.choix).some((v) => Object.keys(v ?? {}).length)
        || [e.texte, e.precisions, e.action, e.replique].some((x) => x?.trim());
    return choisi && assemblagePrompt ? assemblagePrompt.prompt : "";
}

function rendreLibre() {
    const conversation = document.getElementById("conversation-libre");
    conversation.innerHTML = libre.echanges.map((e, k) => {
        if (e.role === "user") {
            const texte = e.content === ECRIRE_LIBRE ? "✍️ Écris le prompt" : e.content;
            return `<div class="bulle bulle-toi">${echapper(texte)}</div>`;
        }
        const t = lireTour(e.content);
        const dernier = k === libre.echanges.length - 1;
        return `<div class="bulle bulle-llm">
            <p>${echapper(t.message || "…")}</p>
            ${(t.questions ?? []).map((q, i) => `<div class="question-libre">
                <strong>${echapper(q.question)}</strong>
                ${dernier ? `<div class="choix-libre">${[...q.choix, "Je te laisse choisir"].map((c) =>
                    `<button type="button" class="petit" data-question="${i}" data-choix="${echapper(c)}">${echapper(c)}</button>`).join("")}</div>`
                : ""}
            </div>`).join("")}
            ${t.prompt ? '<p class="discret">✍️ Prompt écrit : il est à droite, modifiable.</p>' : ""}
        </div>`;
    }).join("");
    conversation.scrollTop = conversation.scrollHeight;
    const fiche = libre.dernier?.fiche ?? {};
    document.getElementById("fiche-libre").innerHTML = Object.entries(fiche).map(([k, v]) =>
        `<dt>${echapper(k)}</dt><dd>${echapper(v)}</dd>`).join("");
    document.getElementById("fiche-vide-libre").hidden = Object.keys(fiche).length > 0;
}

function poserPromptLibre(texte) {  // la version modifiable de l'aperçu, celle qui part
    peaufinage = { source: assemblagePrompt?.prompt ?? "" };
    document.getElementById("texte-peaufine").value = texte;
    document.getElementById("utiliser-peaufine").checked = true;
    document.getElementById("peaufine-prompt").hidden = false;
}

async function envoyerLibre(texte) {
    texte = texte.trim();
    if (!texte || occupePrompt) return;
    const erreur = document.getElementById("erreur-libre");
    const statut = document.getElementById("statut-libre");
    erreur.hidden = true;
    libre.echanges.push({ role: "user", content: texte });
    rendreLibre();
    occupePrompt = true;
    majBoutonsPrompt();
    document.querySelectorAll("#form-libre button, .actions-libre button").forEach((b) => { b.disabled = true; });
    statut.textContent = texte === ECRIRE_LIBRE ? "Le LLM écrit le prompt… (il réfléchit)" : "Le LLM lit ta réponse…";
    try {
        const r = await api("POST", "/api/prompt/libre", {
            echanges: libre.echanges, forme: formeScenario, workflow: ciblePrompt(), contexte: contexteLibre(),
            images: imagesReference(), role: roleReference(), llm: llmPrompt(),
            duree: Number(document.getElementById("duree-prompt").value) || 5,
        });
        libre.echanges.push({ role: "assistant", content: r.brut });
        libre.dernier = r;
        if (r.prompt) poserPromptLibre(r.prompt);
        statut.textContent = `${r.duree_s} s`;
        document.getElementById("message-libre").value = "";
    } catch (err) {
        libre.echanges.pop();  // ta réponse reste dans le champ : renvoie-la
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
    if (libre.echanges.length && !confirm("Recommencer la conversation ?")) return;
    libre.echanges = [];
    libre.dernier = null;
    document.getElementById("erreur-libre").hidden = true;
    document.getElementById("statut-libre").textContent = "";
    rendreLibre();
    garderBrouillon();
});

// Une proposition cliquée s'ajoute au champ (« question : réponse »), à compléter ou envoyer telle quelle.
document.getElementById("conversation-libre").addEventListener("click", (e) => {
    const bouton = e.target.closest("[data-choix]");
    if (!bouton) return;
    const q = libre.dernier?.questions?.[Number(bouton.dataset.question)];
    const champ = document.getElementById("message-libre");
    const ligne = `${(q?.question ?? "").replace(/\s*\?$/, "")} : ${bouton.dataset.choix}`;
    champ.value = champ.value.trim() ? `${champ.value.trim()}\n${ligne}` : ligne;
    bouton.classList.add("choisi");
    champ.focus();
});
