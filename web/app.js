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
    // Orange : une autre appli tient la carte (garde-VRAM), avec le coupable probable en infobulle.
    const alerte = e.garde?.attente ?? e.garde?.alerte;
    const titreVram = alerte
        ? `Carte occupée hors Aria${alerte.coupables.length ? ` : probablement ${alerte.coupables.map((c) => c.nom).join(", ")}` : ""}`
        : "VRAM occupée, tous programmes confondus";
    const vram = e.vram_totale_go
        ? `<span class="vram-direct ${alerte ? "alerte-vram" : ""}" title="${echapper(titreVram)}">${alerte ? "⚠ " : ""}${e.vram_utilisee_go.toLocaleString("fr-FR")} / ${e.vram_totale_go.toLocaleString("fr-FR")} Go</span>`
        : "";
    zone.innerHTML = pastille("ComfyUI", e.comfyui) + pastille("Llama", e.llama, e.llama.modele) + vram;
    majBandeauGarde(e.garde?.attente);
}

// Garde-VRAM : bandeau d'une demande qui attend la carte. Aria ne ferme jamais l'appli qui la tient : c'est toi.
function majBandeauGarde(attente) {
    const bandeau = document.getElementById("bandeau-garde");
    bandeau.hidden = !attente;
    if (!attente) return;
    const qui = attente.coupables.length ? ` par ${attente.coupables[0].nom}, probablement` : " par une autre appli";
    bandeau.querySelector("span").textContent = `⏸ ${attente.moteur} attend la carte graphique : `
        + `${attente.vram_go.toLocaleString("fr-FR")} Go occupés${qui} (seuil ${attente.seuil_go.toLocaleString("fr-FR")} Go). `
        + `Ça repart tout seul dès qu'elle est libérée · ${enDuree(attente.depuis_s)}`;
}

const enDuree = (s) => (s >= 60 ? `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")}` : `${s} s`);

["forcer", "annuler"].forEach((action) => {
    document.getElementById(`${action}-garde`).addEventListener("click", async () => {
        await api("POST", `/api/garde/${action}`);
        majEtatDirect();
    });
});

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

// Licence d'une fiche (app/fiches.py) : ce qu'il faut savoir avant de télécharger le modèle.
function resumeLicence(l) {
    const usage = l.commercial === true ? "usage commercial permis" : l.commercial === false
        ? "non commercial" : "usage commercial : à lire";
    const pays = l.territoires_exclus === null ? " · pays exclus : à lire"
        : l.territoires_exclus.length ? ` · interdit : ${l.territoires_exclus.join(", ")}` : "";
    return `<a href="${echapper(l.url)}" target="_blank" rel="noopener">${echapper(l.nom)}</a> · ${usage}${echapper(pays)}
        <br>${echapper(l.resume)}`;
}

async function chargerWorkflows() {
    const [liste, erreurs] = await Promise.all([api("GET", "/api/workflows"), api("GET", "/api/workflows/erreurs")]);
    const vram = (wf) => wf.vram_min_go
        ? `VRAM mesurée : ${wf.vram_min_go} Go minimum, ${wf.vram_confort_go} Go de confort`
        : `VRAM estimée ${wf.vram_estimee_go} Go (pas encore mesurée)`;
    document.getElementById("workflows").innerHTML = liste.map((wf) => `<li>
        <div class="ligne"><strong style="flex:1">${echapper(wf.nom)}</strong>
        ${wf.pret
            ? '<span class="ok">prêt</span>'
            : `<span class="ko">il manque ${taille(wf.a_telecharger)}</span>`}</div>
        <span class="discret">${TACHES[wf.tache] ?? echapper(wf.tache)} · ${taille(wf.taille)} · ${vram(wf)}</span>
        <span class="discret licence-fiche"><br>${resumeLicence(wf.licence_fiche)}</span>
        ${wf.manquants.length
            ? `<span class="discret"><br>${wf.manquants.map((f) => `${echapper(f.dossier)}/${echapper(f.nom)}`).join("<br>")}</span>`
            : ""}
    </li>`).join("") + erreurs.map((e) => `<li class="ko">Fiche écartée : ${echapper(e)}</li>`).join("");
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

// Emplacements (app/emplacements.py) : données, modèles et moteurs, dans %USERPROFILE%\.aria-studio par défaut.
// 📂 ouvre un dossier dans l'Explorateur ; « Changer… » choisit le dossier des modèles, valable au prochain démarrage.

function afficherEmplacements(e) {
    const m = e.dossiers.modeles;
    document.getElementById("chemin-modeles").textContent = m.chemin;
    const prochain = document.getElementById("prochain-modeles");
    prochain.hidden = !e.redemarrage;
    prochain.querySelector("span").textContent = `Au prochain démarrage d'Aria, les modèles seront lus dans ${e.modeles_prochain}`
        + ` (rien n'a été déplacé : tes fichiers actuels restent dans ${m.chemin}).`;
    document.getElementById("mode-emplacements").textContent = e.portable
        ? "Mode portable : tout reste à côté du programme (un dossier data se trouve à côté de lui)."
        : "Tout est rangé dans ton dossier d'utilisateur ; seul le dossier des modèles peut aller ailleurs (onglet Modèles).";
    document.getElementById("liste-emplacements").innerHTML = Object.entries(e.dossiers).map(([cle, d]) => `<li>
        <strong>${echapper(d.nom)}</strong>
        <code>${echapper(d.chemin)}</code>
        <span class="discret">${d.libre_go !== null ? `${d.libre_go} Go libres` : ""}</span>
        <button type="button" class="petit" data-ouvrir="${cle}">📂 Ouvrir</button>
    </li>`).join("");
}

async function chargerEmplacements() {
    afficherEmplacements(await api("GET", "/api/emplacements"));
}

document.addEventListener("click", async (ev) => {
    const bouton = ev.target.closest("[data-ouvrir]");
    if (!bouton) return;
    try {
        await api("POST", "/api/emplacements/ouvrir", { dossier: bouton.dataset.ouvrir, sous: bouton.dataset.sous || null });
    } catch (err) {
        alert(err.message);
    }
});

document.getElementById("changer-modeles").addEventListener("click", async () => {
    let chemin;
    try {
        chemin = (await api("POST", "/api/emplacements/modeles/parcourir")).chemin;
    } catch {
        chemin = prompt("Chemin complet du nouveau dossier des modèles (ex. D:\IA\Aria-modeles) :");
    }
    if (!chemin) return;
    const actuel = document.getElementById("chemin-modeles").textContent;
    if (!confirm(`Lire les modèles dans ${chemin} au prochain démarrage d'Aria ?

`
        + `Rien n'est déplacé : tes modèles actuels restent dans ${actuel}.`)) return;
    try {
        afficherEmplacements(await api("PUT", "/api/emplacements/modeles", { chemin }));
    } catch (err) {
        alert(err.message);
    }
});

document.getElementById("garder-modeles").addEventListener("click", async () => {
    const e = await api("GET", "/api/emplacements");
    const actuel = e.dossiers.modeles.chemin;
    afficherEmplacements(await api("PUT", "/api/emplacements/modeles",
        { chemin: actuel === e.modeles_defaut ? null : actuel }));
});

chargerEmplacements();

// Mise à jour : version installée, et bandeau quand une release plus récente existe sur GitHub (app/mises_a_jour.py).
// Rien ne se télécharge : le lien mène à la release. « Plus tard » cache le bandeau jusqu'à la version suivante.

const MAJ_IGNOREE = "aria-maj-ignoree";

function majIgnoree() {
    try { return localStorage.getItem(MAJ_IGNOREE); } catch { return null; }
}

function afficherMiseAJour(e) {
    document.getElementById("version-appli").textContent = `· v${e.version}`;
    const bandeau = document.getElementById("bandeau-maj");
    const d = e.derniere;
    bandeau.hidden = !(e.disponible && d && majIgnoree() !== d.version);
    if (!bandeau.hidden) {
        bandeau.querySelector("span").textContent = `✨ Aria - Studio ${d.version} est disponible (tu as la ${e.version})`
            + (d.date ? ` · publiée le ${new Date(d.date).toLocaleDateString("fr-FR")}` : "");
        document.getElementById("lien-maj").href = d.url;
        const notes = document.getElementById("notes-maj");
        notes.hidden = !d.notes;
        notes.querySelector("pre").textContent = d.notes;
    }
    // Onglet Réglages
    document.getElementById("active-maj").checked = e.active;
    const quand = e.verifie_le ? new Date(e.verifie_le).toLocaleString("fr-FR") : "jamais";
    document.getElementById("etat-maj").textContent = `Version installée : ${e.version} · dépôt ${e.depot} · `
        + (e.disponible ? `nouvelle version ${d.version} disponible` : e.erreur ? e.erreur
            : d ? `à jour (dernière release : ${d.version})` : "pas encore vérifié") + ` · vérifié : ${quand}`;
}

async function chargerMiseAJour(force = false) {
    try {
        afficherMiseAJour(await api(force ? "POST" : "GET", force ? "/api/mise-a-jour/verifier" : "/api/mise-a-jour"));
    } catch {
        // pas de serveur : la barre d'état le dit déjà
    }
}

document.getElementById("plus-tard-maj").addEventListener("click", async () => {
    const e = await api("GET", "/api/mise-a-jour");
    try { localStorage.setItem(MAJ_IGNOREE, e.derniere?.version ?? ""); } catch { /* navigation privée */ }
    document.getElementById("bandeau-maj").hidden = true;
});
document.getElementById("verifier-maj").addEventListener("click", () => chargerMiseAJour(true));
document.getElementById("active-maj").addEventListener("change", async (ev) => {
    afficherMiseAJour(await api("PUT", "/api/mise-a-jour", { active: ev.target.checked }));
});

chargerMiseAJour();
