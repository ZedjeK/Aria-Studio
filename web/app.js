const echapper = (texte) => String(texte ?? "").replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[c]);

const taille = (octets) => {
    if (octets >= 2 ** 30) return `${(octets / 2 ** 30).toFixed(2)} Go`;
    if (octets >= 2 ** 20) return `${(octets / 2 ** 20).toFixed(1)} Mo`;
    return `${Math.round(octets / 2 ** 10)} Ko`;
};

async function api(methode, url, corps) {
    const reponse = await fetch(url, {
        method: methode,
        headers: corps ? { "Content-Type": "application/json" } : {},
        body: corps ? JSON.stringify(corps) : undefined,
    });
    const donnees = await reponse.json();
    if (!reponse.ok) throw new Error(donnees.detail ?? `HTTP ${reponse.status}`);
    return donnees;
}

// Onglets

document.querySelectorAll(".onglet").forEach((bouton) => {
    bouton.addEventListener("click", () => {
        document.querySelectorAll(".onglet").forEach((b) => b.classList.toggle("actif", b === bouton));
        document.querySelectorAll(".vue").forEach((v) => {
            v.hidden = v.id !== `vue-${bouton.dataset.vue}`;
        });
        if (bouton.dataset.vue === "reglages") chargerReglages();
        if (bouton.dataset.vue === "montage") preparerMontage();
        if (bouton.dataset.vue === "scenario") preparerScenario();
        if (bouton.dataset.vue === "casting") chargerCasting();
        if (bouton.dataset.vue === "plateau") {
            chargerOptionsPlateau();
            chargerGalerie();
        }
        if (bouton.dataset.vue === "modeles") {
            chargerWorkflows();
            chargerModeles();
            chargerTelechargements();
        }
    });
});

// Modèles : sous-onglets « Installés » et « Découvrir » (Civitai, Hugging Face)

document.querySelectorAll(".sous-onglet").forEach((bouton) => {
    bouton.addEventListener("click", () => {
        document.querySelectorAll(".sous-onglet").forEach((b) => b.classList.toggle("actif", b === bouton));
        document.getElementById("modeles-installes").hidden = bouton.dataset.sous !== "installes";
        document.getElementById("modeles-decouvrir").hidden = bouton.dataset.sous !== "decouvrir";
        if (bouton.dataset.sous === "decouvrir") preparerDecouvrir();
    });
});

// État en direct : pastilles des moteurs et VRAM, rafraîchies en continu dans la barre d'onglets

const ETATS_MOTEUR = { actif: "chargé", veille: "en veille", absent: "introuvable" };

async function majEtatDirect() {
    const zone = document.getElementById("etat-direct");
    let e;
    try {
        e = await api("GET", "/api/etat");
    } catch {
        zone.innerHTML = '<span class="pastille absent" title="Le serveur ne répond pas">● Serveur</span>';
        return;
    }
    const pastille = (nom, m, detail = "") => `<span class="pastille ${m.etat}"
        title="${nom} : ${ETATS_MOTEUR[m.etat]}${detail ? ` · ${echapper(detail)}` : ""}">● ${nom}</span>`;
    const vram = e.vram_totale_go
        ? `<span class="vram-direct" title="VRAM occupée, tous programmes confondus">${e.vram_utilisee_go.toLocaleString("fr-FR")} / ${e.vram_totale_go.toLocaleString("fr-FR")} Go</span>`
        : "";
    zone.innerHTML = pastille("ComfyUI", e.comfyui) + pastille("Llama", e.llama, e.llama.modele) + vram;
}

majEtatDirect();
setInterval(majEtatDirect, 3000);

// Modèles : fiche et confirmation

let dossiers = [];

document.getElementById("form-fiche").addEventListener("submit", async (e) => {
    e.preventDefault();
    const erreur = document.getElementById("erreur-fiche");
    const bloc = document.getElementById("fiche");
    erreur.hidden = true;
    bloc.hidden = true;
    try {
        const fiche = await api("POST", "/api/modeles/fiche", { url: document.getElementById("url-modele").value });
        afficherFiche(fiche);
    } catch (err) {
        erreur.textContent = err.message;
        erreur.hidden = false;
    }
});

function afficherFiche(fiche) {
    const bloc = document.getElementById("fiche");
    const apres = fiche.espace_libre - fiche.taille;
    const alertes = [];
    if (fiche.jeton_requis) alertes.push("Modèle protégé : un jeton d'accès est nécessaire.");
    if (apres < 2 ** 30) alertes.push("Espace disque insuffisant.");
    if (!fiche.sha256) alertes.push("Aucune empreinte publiée : seule la taille sera vérifiée.");
    if (!fiche.dossier) alertes.push("Dossier non reconnu : choisis-le ci-dessous.");

    const options = dossiers.map((d) =>
        `<option value="${echapper(d)}" ${d === fiche.dossier ? "selected" : ""}>${echapper(d)}</option>`).join("");

    bloc.innerHTML = `
        <dl>
            <dt>Fichier</dt><dd>${echapper(fiche.nom)}</dd>
            <dt>Source</dt><dd><a href="${echapper(fiche.page)}" target="_blank" rel="noopener">${echapper(fiche.source)}</a></dd>
            <dt>Taille</dt><dd>${taille(fiche.taille)}</dd>
            <dt>SHA-256</dt><dd>${fiche.sha256 ? echapper(fiche.sha256.slice(0, 16)) + "…" : "—"}</dd>
            <dt>Licence</dt><dd>${echapper(fiche.licence ?? "voir la page du modèle")}</dd>
            <dt>Espace libre</dt><dd>${taille(fiche.espace_libre)} → ${taille(Math.max(apres, 0))}</dd>
        </dl>
        ${alertes.map((a) => `<p class="alerte">${a}</p>`).join("")}
        <div class="ligne">
            <select id="dossier-fiche">${fiche.dossier ? "" : '<option value="">— dossier —</option>'}${options}</select>
            <button id="confirmer" class="principal">Télécharger</button>
        </div>`;
    bloc.hidden = false;

    const select = document.getElementById("dossier-fiche");
    const bouton = document.getElementById("confirmer");
    const bloque = fiche.jeton_requis || apres < 2 ** 30;
    const majBouton = () => { bouton.disabled = bloque || !select.value; };
    select.addEventListener("change", majBouton);
    majBouton();

    bouton.addEventListener("click", async () => {
        bouton.disabled = true;
        try {
            await api("POST", "/api/telechargements", { fiche_id: fiche.id, dossier: select.value });
            bloc.hidden = true;
            document.getElementById("url-modele").value = "";
            chargerTelechargements();
        } catch (err) {
            const erreur = document.getElementById("erreur-fiche");
            erreur.textContent = err.message;
            erreur.hidden = false;
            majBouton();
        }
    });
}

// Modèles : file de téléchargement

const ETATS = {
    attente: "en attente",
    en_cours: "en cours",
    pause: "en pause",
    verification: "vérification",
    termine: "terminé",
    erreur: "erreur",
    annule: "annulé",
};
const ACTIFS = ["attente", "en_cours", "pause", "verification"];
let suivi = null;

async function chargerTelechargements() {
    const taches = await api("GET", "/api/telechargements");
    const liste = document.getElementById("telechargements");
    if (!taches.length) {
        liste.innerHTML = '<li class="discret">Aucun téléchargement.</li>';
    } else {
        liste.innerHTML = taches.slice().reverse().map((t) => {
            const pourcent = t.taille ? (100 * t.recu / t.taille) : 0;
            const reste = t.debit > 0 ? Math.round((t.taille - t.recu) / t.debit) : null;
            const details = t.etat === "en_cours"
                ? ` · ${taille(t.debit)}/s${reste !== null ? ` · ${Math.floor(reste / 60)} min ${reste % 60} s` : ""}`
                : "";
            const classe = t.etat === "termine" ? "ok" : t.etat === "erreur" ? "ko" : "";
            return `<li>
                <div class="ligne"><span style="flex:1">${echapper(t.dossier)}/${echapper(t.nom)}</span>
                ${["attente", "en_cours", "pause"].includes(t.etat)
                    ? `<button class="petit" data-annuler="${t.id}">Annuler</button>` : ""}</div>
                <div class="barre"><div style="width:${pourcent.toFixed(1)}%"></div></div>
                <span class="discret ${classe}">${ETATS[t.etat]} · ${taille(t.recu)} / ${taille(t.taille)}${details}</span>
                ${t.erreur ? `<p class="alerte">${echapper(t.erreur)}</p>` : ""}
            </li>`;
        }).join("");
    }

    const enCours = taches.some((t) => ACTIFS.includes(t.etat));
    if (enCours && !suivi) {
        suivi = setInterval(chargerTelechargements, 1000);
    } else if (!enCours && suivi) {
        clearInterval(suivi);
        suivi = null;
        chargerModeles();
        chargerWorkflows();
    }
}

document.getElementById("telechargements").addEventListener("click", async (e) => {
    const id = e.target.dataset.annuler;
    if (!id) return;
    await api("POST", `/api/telechargements/${id}/annuler`);
    chargerTelechargements();
});

// Modèles : workflows du catalogue

const TACHES = { texte_vers_image: "texte vers image", retouche: "retouche",
                 image_vers_video: "image vers vidéo", texte_vers_video: "texte vers vidéo" };

async function chargerWorkflows() {
    const liste = await api("GET", "/api/workflows");
    document.getElementById("workflows").innerHTML = liste.map((wf) => `<li>
        <div class="ligne"><strong style="flex:1">${echapper(wf.nom)}</strong>
        ${wf.pret
            ? '<span class="ok">prêt</span>'
            : `<span class="ko">il manque ${taille(wf.a_telecharger)}</span>`}</div>
        <span class="discret">${TACHES[wf.tache] ?? echapper(wf.tache)} · ${echapper(wf.licence)}
            · VRAM estimée ${wf.vram_estimee_go} Go</span>
        ${wf.manquants.length
            ? `<span class="discret"><br>${wf.manquants.map((f) => `${echapper(f.dossier)}/${echapper(f.nom)}`).join("<br>")}</span>`
            : ""}
    </li>`).join("");
}

// Modèles : inventaire

async function chargerModeles() {
    const donnees = await api("GET", "/api/modeles");
    dossiers = donnees.dossiers;
    document.getElementById("espace").textContent = `· ${taille(donnees.espace_libre)} libres`;
    const corps = document.querySelector("#inventaire tbody");
    if (!donnees.modeles.length) {
        corps.innerHTML = '<tr><td colspan="5" class="discret">Aucun modèle pour l\'instant.</td></tr>';
        return;
    }
    corps.innerHTML = donnees.modeles.map((m) => `<tr>
        <td>${echapper(m.dossier)}</td>
        <td>${echapper(m.nom)}</td>
        <td>${taille(m.taille)}</td>
        <td>${m.suivi
            ? `<a href="${echapper(m.infos.page)}" target="_blank" rel="noopener">${echapper(m.infos.source)}</a>`
            : '<span class="discret">non suivi</span>'}</td>
        <td>${m.suivi
            ? `<button class="petit" data-supprimer="${echapper(m.cle)}" data-taille="${m.taille}">Supprimer</button>`
            : ""}</td>
    </tr>`).join("");
}

document.querySelector("#inventaire tbody").addEventListener("click", async (e) => {
    const cle = e.target.dataset.supprimer;
    if (!cle) return;
    if (!confirm(`Supprimer définitivement ${cle} (${taille(Number(e.target.dataset.taille))}) ?`)) return;
    try {
        await api("DELETE", `/api/modeles/${cle.split("/").map(encodeURIComponent).join("/")}`);
    } catch (err) {
        alert(err.message);
    }
    chargerModeles();
});

chargerModeles();
