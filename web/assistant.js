// Assistant de premier lancement (app/assistant.py) : ta machine, les moteurs, les packs à cocher, leurs licences,
// puis le téléchargement par la file habituelle. Il s'ouvre tout seul tant qu'il manque de quoi générer et qu'il
// n'a pas été terminé ; « Plus tard » le ferme (les bandeaux prennent le relais), Réglages le relance.
// L'écran de fin récapitule les favoris réglés et propose une image d'essai (peu de steps).

const ASSISTANT = {
    etape: 0,             // 0 à 5 : Ta machine, Moteurs, Ce que tu veux faire, Licences, Téléchargement, C'est prêt
    packs: [],
    choisis: new Set(),   // packs à installer (ceux déjà installés n'y sont pas)
    carte: null,
    moteurs: [],
    plan: null,
    taches: [],           // identifiants des téléchargements lancés par l'assistant
    enAttente: [],        // fichiers que la file avait déjà
    fin: null,            // réponse de /api/assistant/terminer : favoris réglés, pas encore là, essai possible
};

const ecransAssistant = [...document.querySelectorAll("#assistant .ecran-assistant")];
const boutonSuivant = document.getElementById("suivant-assistant");
const etatAssistant = document.getElementById("etat-assistant");
const go = (octets) => `${Math.round(octets / 2 ** 30)} Go`;

async function ouvrirAssistant() {
    const e = await api("GET", "/api/assistant");
    ASSISTANT.packs = e.packs;
    ASSISTANT.choisis = new Set(e.packs.filter((p) => p.conseille && !p.installe).map((p) => p.id));
    ASSISTANT.etape = 0;
    ASSISTANT.taches = [];
    ASSISTANT.enAttente = [];
    ASSISTANT.fin = null;
    document.getElementById("assistant").hidden = false;
    await afficherEtapeAssistant();
}

function fermerAssistant() {
    document.getElementById("assistant").hidden = true;
    if (ASSISTANT.fin) {  // terminé : les listes du Plateau et les moteurs suivent les nouveaux favoris
        chargerOptionsPlateau();
        chargerMoteurs();
    }
}

async function afficherEtapeAssistant() {
    const n = ASSISTANT.etape;
    ecransAssistant.forEach((ecran, i) => { ecran.hidden = i !== n; });
    document.querySelectorAll(".etapes-assistant li").forEach((li, i) => {
        li.classList.toggle("actif", i === n);
        li.classList.toggle("fait", i < n);
    });
    document.getElementById("precedent-assistant").disabled = n === 0 || n >= 4;
    document.getElementById("plus-tard-assistant").hidden = n === 5;
    etatAssistant.textContent = "";
    boutonSuivant.textContent = n === 3 ? "J'accepte et je télécharge →" : n === 4 ? "Terminer" : n === 5 ? "C'est parti !" : "Suivant →";
    await [ecranMachine, ecranMoteurs, ecranPacks, ecranLicences, ecranTelechargement, ecranFin][n]();
}

// 1. Ta machine
async function ecranMachine() {
    const [m, lieux] = await Promise.all([api("GET", "/api/moteurs"), api("GET", "/api/emplacements")]);
    const c = m.carte;
    ASSISTANT.carte = c;
    document.getElementById("machine-assistant").innerHTML = `
        <dt>Carte graphique</dt><dd>${c.nvidia ? `${echapper(c.nom)} · ${c.vram_go} Go de VRAM` : "aucune carte NVIDIA détectée"}</dd>
        <dt>Pilote NVIDIA</dt><dd>${c.pilote ? `${echapper(c.pilote)}${c.ok ? " ✓" : ""}` : "—"}</dd>
        <dt>Mémoire vive</dt><dd>${c.ram_go} Go</dd>
        <dt>Testé sur</dt><dd>RTX 4080 SUPER 16 Go, 64 Go de RAM (pas encore d'autre carte vérifiée)</dd>`;
    // Un nouveau dossier des modèles ne vaut qu'au prochain démarrage : télécharger avant remplirait l'ancien.
    const message = c.message || (lieux.redemarrage
        ? `Le nouveau dossier des modèles (${lieux.modeles_prochain}) vaudra au prochain démarrage : relance Aria, `
          + "puis rouvre l'assistant (onglet Réglages) avant de télécharger." : "");
    const alerte = document.getElementById("alerte-machine");
    alerte.hidden = !message;
    alerte.textContent = message;
    document.getElementById("dossier-modeles-assistant").textContent = lieux.dossiers.modeles.chemin;
    document.getElementById("libre-assistant").textContent = `${lieux.dossiers.modeles.libre_go} Go`;
    boutonSuivant.disabled = !c.ok || lieux.redemarrage;
}

// 2. Moteurs : la liste de la carte Moteurs (web/app.js), rendue aussi ici
async function ecranMoteurs() {
    await chargerMoteurs();
}

document.addEventListener("moteurs", (ev) => {
    ASSISTANT.moteurs = ev.detail.moteurs;
    if (ASSISTANT.etape !== 1 || document.getElementById("assistant").hidden) return;
    const absents = ev.detail.moteurs.filter((m) => m.statut === "absent" && !m.facultatif);
    const occupe = ev.detail.tache && !ev.detail.tache.fini;
    boutonSuivant.disabled = Boolean(absents.length || occupe);
    etatAssistant.textContent = absents.length ? `À installer : ${absents.map((m) => m.nom).join(", ")}` : "";
});

// 3. Ce que tu veux faire (rechargés : les moteurs ont pu s'installer depuis l'ouverture)
async function ecranPacks() {
    ASSISTANT.packs = (await api("GET", "/api/assistant")).packs;
    document.getElementById("packs-assistant").innerHTML = ASSISTANT.packs.map((p) => {
        const licences = p.licences.map((l) => l.nom).join(", ");
        return `<label class="pack-assistant">
            <input type="checkbox" data-pack="${p.id}" ${p.installe ? "checked disabled" : ASSISTANT.choisis.has(p.id) ? "checked" : ""}>
            <span>
                <strong>${echapper(p.nom)}</strong>
                ${p.conseille ? '<span class="badge">conseillé</span>' : ""}${p.facultatif ? '<span class="badge">facultatif</span>' : ""}
                <div class="discret">${echapper(p.usage)}</div>
                <div class="discret">Licence : ${echapper(licences)} · testé sur ${echapper(p.testee || "—")}</div>
                ${p.moteurs_absents.length ? `<div class="ko">Demande aussi : ${echapper(p.moteurs_absents.join(", "))}
                    (étape Moteurs)</div>` : ""}
            </span>
            <span class="discret">${p.installe ? "installé ✓" : p.a_telecharger < p.taille
                ? `${go(p.a_telecharger)} sur ${go(p.taille)}` : go(p.taille)}</span>
        </label>`;
    }).join("");
    return majPlanAssistant();
}

document.getElementById("packs-assistant").addEventListener("change", (ev) => {
    const id = ev.target.dataset.pack;
    if (!id) return;
    if (ev.target.checked) ASSISTANT.choisis.add(id);
    else ASSISTANT.choisis.delete(id);
    majPlanAssistant();
});

async function majPlanAssistant() {
    const total = document.getElementById("total-assistant");
    if (!ASSISTANT.choisis.size) {
        ASSISTANT.plan = null;
        total.textContent = ASSISTANT.packs.every((p) => p.installe) ? "Tout est déjà installé." : "Coche au moins un pack.";
        boutonSuivant.disabled = !ASSISTANT.packs.some((p) => p.installe);
        return;
    }
    const p = await api("POST", "/api/assistant/plan", { packs: [...ASSISTANT.choisis] });
    ASSISTANT.plan = p;
    const sansMoteur = ASSISTANT.packs.filter((x) => ASSISTANT.choisis.has(x.id) && x.moteurs_absents.length);
    total.textContent = `À télécharger : ${go(p.a_telecharger)} (fichiers communs comptés une fois) · ${go(p.libre)} libres`
        + (p.place_ok ? "" : " · pas assez de place : décoche un pack ou change le dossier des modèles (étape 1)");
    total.classList.toggle("ko", !p.place_ok);
    boutonSuivant.disabled = !p.place_ok || sansMoteur.length > 0;
    etatAssistant.textContent = sansMoteur.length ? "Un pack coché demande un moteur absent : installe-le à l'étape Moteurs" : "";
}

// 4. Licences
function ecranLicences() {
    const choisis = ASSISTANT.packs.filter((p) => ASSISTANT.choisis.has(p.id));
    document.getElementById("licences-assistant").innerHTML = choisis.map((p) => `
        <div class="licence-assistant">
            <strong>${echapper(p.nom)}</strong>
            ${p.licences.map((l) => `<div>
                <a href="${echapper(l.url)}" target="_blank" rel="noopener">${echapper(l.nom)}</a>
                <div class="discret">${echapper(l.resume)}</div>
                ${l.territoires_exclus && l.territoires_exclus.length
                    ? `<div class="ko">Utilisation non autorisée : ${echapper(l.territoires_exclus.join(", "))}</div>` : ""}
                ${l.composants.length ? `<ul class="discret">${l.composants.map((c) => `<li>${echapper(c)}</li>`).join("")}</ul>` : ""}
            </div>`).join("")}
            <label class="case"><input type="checkbox" data-licence="${p.id}" ${p.licence_acceptee ? "checked" : ""}>
                J'ai lu ${p.licences.length > 1 ? "ces licences" : "cette licence"} et je l'accepte</label>
        </div>`).join("") || '<p class="discret">Rien à télécharger : aucune licence à accepter.</p>';
    majLicencesAssistant();
}

function majLicencesAssistant() {
    const cases = [...document.querySelectorAll("#licences-assistant [data-licence]")];
    boutonSuivant.disabled = cases.some((c) => !c.checked);
}

document.getElementById("licences-assistant").addEventListener("change", majLicencesAssistant);

// 5. Téléchargement : la file habituelle (app/telechargement.py), suivie ici
async function ecranTelechargement() {
    const intro = document.getElementById("intro-telechargement-assistant");
    const fichiers = ASSISTANT.plan ? ASSISTANT.plan.fichiers.length : 0;
    intro.textContent = fichiers
        ? `${fichiers} fichier${fichiers > 1 ? "s" : ""} en file : chacun est vérifié par son empreinte SHA-256 à l'arrivée.`
        : "Rien à télécharger : tout est déjà là.";
    await suivreTelechargements();
}

async function suivreTelechargements() {
    if (ASSISTANT.etape !== 4 || document.getElementById("assistant").hidden) return;
    const taches = (await api("GET", "/api/telechargements"))
        .filter((t) => ASSISTANT.taches.includes(t.id) || ASSISTANT.enAttente.includes(t.nom));
    const ETATS = { attente: "en attente", en_cours: "en cours", pause: "en pause", verification: "vérification",
        termine: "✓", erreur: "erreur", annule: "annulé" };
    document.getElementById("telechargements-assistant").innerHTML = taches.map((t) => {
        const pourcent = t.taille ? Math.floor((t.recu * 100) / t.taille) : 0;
        return `<li>
            <div class="ligne"><span>${echapper(t.nom)}</span><span class="discret">${ETATS[t.etat] || t.etat}${
                t.etat === "en_cours" ? ` · ${pourcent} % · ${taille(t.debit)}/s` : ""}</span></div>
            ${["en_cours", "pause"].includes(t.etat) ? `<div class="barre"><div style="width:${pourcent}%"></div></div>` : ""}
            ${t.erreur ? `<div class="ko">${echapper(t.erreur)}</div>` : ""}
        </li>`;
    }).join("");
    const finis = taches.filter((t) => ["termine", "erreur", "annule"].includes(t.etat));
    const recu = taches.reduce((s, t) => s + (t.etat === "termine" ? t.taille : t.recu), 0);
    const tout = taches.reduce((s, t) => s + t.taille, 0);
    etatAssistant.textContent = tout ? `${go(recu)} sur ${go(tout)}` : "";
    boutonSuivant.disabled = finis.length < taches.length;
    if (finis.length < taches.length) setTimeout(suivreTelechargements, 1000);
}

// 6. C'est prêt : les favoris réglés, ce qui n'est pas encore là, et l'essai
function ecranFin() {
    const f = ASSISTANT.fin;
    document.getElementById("recap-assistant").innerHTML = f.regles.map((r) =>
        `<dt>${echapper(r.usage)}</dt><dd>${echapper(r.nom)} ✓</dd>`).join("")
        || "<dt>Rien de réglé</dt><dd>aucun pack n'est complet pour l'instant</dd>";
    const manquants = document.getElementById("manquants-assistant");
    manquants.hidden = !f.manquants.length;
    manquants.textContent = `Pas encore là (téléchargement pas fini ou en erreur) : ${f.manquants.map((m) =>
        `${m.usage} avec ${m.nom}`).join(", ")}. Relance l'assistant une fois les fichiers arrivés.`;
    document.getElementById("bloc-essai-assistant").hidden = !f.essai;
    for (const id of ["image-essai-assistant", "erreur-essai-assistant", "barre-essai-assistant"]) {
        document.getElementById(id).hidden = true;
    }
    document.getElementById("suivi-essai-assistant").textContent = "";
    document.getElementById("essai-assistant").disabled = false;
    boutonSuivant.disabled = false;
}

document.getElementById("essai-assistant").addEventListener("click", async (ev) => {
    const erreur = document.getElementById("erreur-essai-assistant");
    ev.target.disabled = true;
    erreur.hidden = true;
    document.getElementById("image-essai-assistant").hidden = true;
    try {
        suivreEssai((await api("POST", "/api/assistant/essai")).id, Date.now());
    } catch (err) {
        afficherErreur(erreur, err.message);
        ev.target.disabled = false;
    }
});

async function suivreEssai(id, debut) {
    const suivi = document.getElementById("suivi-essai-assistant");
    const barre = document.getElementById("barre-essai-assistant");
    let c;
    try {
        c = await suivreJusquauBout(id, (r) => {
            if (!["attente", "en_cours"].includes(r.etat)) return;
            const secondes = Math.round((Date.now() - debut) / 1000);
            barre.hidden = false;
            barre.firstElementChild.style.width = `${Math.round(100 * r.progression)}%`;
            suivi.textContent = r.etat === "attente" ? `En file d'attente… ${secondes} s`
                : `${r.etape || "Finalisation"} · ${Math.round(100 * r.progression)} % · ${secondes} s`;
        }, 700);
    } catch (err) {
        c = { etat: "erreur", erreur: `Suivi interrompu (${err.message}) : l'essai continue peut-être, regarde les rushes.` };
    }
    const secondes = Math.round((Date.now() - debut) / 1000);
    barre.hidden = true;
    document.getElementById("essai-assistant").disabled = false;
    if (c.etat === "erreur") {
        suivi.textContent = "";
        const erreur = document.getElementById("erreur-essai-assistant");
        afficherErreur(erreur, c.erreur);
        return;
    }
    suivi.textContent = `Tout tourne ✓ (${secondes} s). L'image est aussi dans tes rushes.`;
    const image = document.getElementById("image-essai-assistant");
    image.src = urlImage(c.images[0]);
    image.hidden = false;
}

// Navigation
boutonSuivant.addEventListener("click", async () => {
    boutonSuivant.disabled = true;
    try {
        if (ASSISTANT.etape === 3) {
            const packs = [...ASSISTANT.choisis];
            if (packs.length) {
                await api("POST", "/api/assistant/licences", { packs });
                const r = await api("POST", "/api/assistant/telecharger", { packs });
                ASSISTANT.taches = r.ajoutes;
                ASSISTANT.enAttente = r.deja_en_file;
            }
        } else if (ASSISTANT.etape === 4) {
            const installes = ASSISTANT.packs.filter((p) => p.installe).map((p) => p.id);
            ASSISTANT.fin = await api("POST", "/api/assistant/terminer",
                { packs: [...new Set([...installes, ...ASSISTANT.choisis])] });
        } else if (ASSISTANT.etape === 5) {
            fermerAssistant();
            return;
        }
        ASSISTANT.etape += 1;
        await afficherEtapeAssistant();
    } catch (err) {
        etatAssistant.textContent = err.message;
        boutonSuivant.disabled = false;
    }
});

document.getElementById("precedent-assistant").addEventListener("click", async () => {
    ASSISTANT.etape = Math.max(0, ASSISTANT.etape - 1);
    await afficherEtapeAssistant();
});

document.getElementById("plus-tard-assistant").addEventListener("click", fermerAssistant);
document.getElementById("ouvrir-assistant").addEventListener("click", ouvrirAssistant);
document.getElementById("changer-modeles-assistant").addEventListener("click", async () => {
    await changerDossierModeles();
    await ecranMachine();
});

(async () => {
    try {
        if ((await api("GET", "/api/assistant")).ouvrir) await ouvrirAssistant();
    } catch {
        // pas de serveur : la barre d'état le dit déjà
    }
})();
