// Onglets

document.querySelectorAll(".onglet").forEach((bouton) => {
    bouton.addEventListener("click", () => {
        document.querySelectorAll(".onglet").forEach((b) => b.classList.toggle("actif", b === bouton));
        document.querySelectorAll(".vue").forEach((v) => {
            v.hidden = v.id !== `vue-${bouton.dataset.vue}`;
        });
        if (bouton.dataset.vue === "reglages") {
            chargerReglages();
            chargerMoteurs();
            chargerCles();
        }
        if (bouton.dataset.vue === "montage") preparerMontage();
        if (bouton.dataset.vue === "scenario") preparerScenario();
        if (bouton.dataset.vue === "casting") chargerCasting();
        if (bouton.dataset.vue === "plateau") {
            chargerOptionsPlateau();
            chargerGalerie();
        }
        if (bouton.dataset.vue === "modeles") ouvrirModeles();  // web/modeles.js
        if (bouton.dataset.vue === "retouche") chargerOptionsPlateau();  // web/plateau.js : modèles de retouche et LoRAs
    });
});

// État en direct : pastilles des moteurs et VRAM, rafraîchies en continu dans la barre d'onglets

const ETATS_MOTEUR = { actif: "chargé", veille: "en veille", absent: "introuvable" };

// Une génération tourne (quel que soit l'onglet ouvert) : ce qu'elle fait, où elle en est, et ce qui attend derrière.
const NOMS_COMMANDES = { creation: "Image", retouche: "Retouche", agrandissement: "Agrandissement", clip: "Clip",
                         sequence: "Séquence", voix: "Essai de voix" };

function enCours(c) {
    if (!c) return "";
    const nom = NOMS_COMMANDES[c.type] ?? c.type;
    const avance = c.etat === "attente" ? "en file" : `${Math.round(100 * c.progression)} %`;
    const titre = `${nom} : ${c.etape || (c.etat === "attente" ? "en file d'attente" : "en cours")}`
        + (c.file > 1 ? ` · ${c.file - 1} autre${c.file > 2 ? "s" : ""} derrière` : "");
    return `<span class="pastille en-cours" title="${echapper(titre)}">⏳ ${nom} ${avance}${c.file > 1 ? ` (+${c.file - 1})` : ""}</span>`;
}

async function majEtatDirect() {
    if (document.hidden) return;  // onglet caché : la barre d'état n'interroge plus le serveur pour rien
    const zone = document.getElementById("etat-direct");
    let e;
    try {
        e = await api("GET", "/api/etat");
    } catch {
        zone.innerHTML = '<span class="pastille absent" title="Le serveur ne répond pas">● Serveur</span>';
        return;
    }
    // Un moteur : vert s'il tient la carte, éteint sinon (app/main.py : « actif » = celui de l'orchestrateur).
    const pastille = (nom, m, detail = "") => `<span class="pastille ${m.etat}"
        title="${nom} : ${ETATS_MOTEUR[m.etat]}${detail ? ` · ${echapper(detail)}` : ""}"><i class="point"></i><b>${nom}</b>`
        + `<span class="detail-pastille">${echapper(m.etat === "actif" && detail ? detail : ETATS_MOTEUR[m.etat])}</span></span>`;
    // Orange : une autre appli tient la carte (garde-VRAM), avec le coupable probable en infobulle.
    const alerte = e.garde?.attente ?? e.garde?.alerte;
    const titreVram = alerte
        ? `Carte occupée hors Aria${alerte.coupables.length ? ` : probablement ${alerte.coupables.map((c) => c.nom).join(", ")}` : ""}`
        : "VRAM occupée, tous programmes confondus";
    const remplissage = e.vram_totale_go ? Math.min(100, Math.round(100 * e.vram_utilisee_go / e.vram_totale_go)) : 0;
    const vram = e.vram_totale_go
        ? `<span class="vram-direct ${alerte ? "alerte-vram" : ""}" title="${echapper(titreVram)}">`
          + `<span>${alerte ? "⚠ " : ""}${e.vram_utilisee_go.toLocaleString("fr-FR")} / ${e.vram_totale_go.toLocaleString("fr-FR")} Go</span>`
          + `<span class="jauge-vram"><span style="width:${remplissage}%"></span></span></span>`
        : "";
    zone.innerHTML = enCours(e.commande) + pastille("ComfyUI", e.comfyui) + pastille("Llama", e.llama, e.llama.modele) + vram;
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
const suiviEtat = setInterval(majEtatDirect, 3000);
document.addEventListener("visibilitychange", () => { if (!document.hidden) majEtatDirect(); });  // à jour au retour

// ⏻ Quitter : arrête le serveur d'Aria, qui coupe ComfyUI et Llama en partant (app/main.py, /api/quitter).
document.getElementById("quitter-appli").addEventListener("click", async () => {
    const taches = await api("GET", "/api/telechargements").catch(() => []);
    const actifs = taches.filter((t) => ["attente", "en_cours", "pause", "verification"].includes(t.etat)).length;
    if (!confirm("Quitter Aria ? Son serveur, ComfyUI et Llama s'arrêtent, et une génération en cours est perdue."
        + (actifs ? `\n\n${actifs} téléchargement(s) en cours : ils reprendront au prochain lancement.` : ""))) return;
    try {
        await api("POST", "/api/quitter");
    } catch (err) {
        alert(err.message);
        return;
    }
    clearInterval(suiviEtat);
    document.getElementById("voile-quitte").hidden = false;
});

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
        afficherErreur(erreur, err.message);
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
            afficherErreur(erreur, err.message);
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
    const actifs = taches.filter((t) => ACTIFS.includes(t.etat)).length;
    const badge = document.getElementById("badge-telechargements");
    badge.querySelector("span").textContent = actifs ? `(${actifs})` : "";
    badge.classList.toggle("actif", actifs > 0);
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
        chargerModeles();  // la collection et les onglets d'usage voient les fichiers arrivés
        chargerUsages();
    }
}

document.getElementById("telechargements").addEventListener("click", async (e) => {
    const id = e.target.dataset.annuler;
    if (!id) return;
    await api("POST", `/api/telechargements/${id}/annuler`);
    chargerTelechargements();
});

// Licence d'une fiche (app/fiches.py) : ce qu'il faut savoir avant de télécharger le modèle (onglets d'usage).
function resumeLicence(l) {
    const usage = l.commercial === true ? "usage commercial permis" : l.commercial === false
        ? "non commercial" : "usage commercial : à lire";
    const pays = l.territoires_exclus === null ? " · pays exclus : à lire"
        : l.territoires_exclus.length ? ` · interdit : ${l.territoires_exclus.join(", ")}` : "";
    return `<a href="${echapper(l.url)}" target="_blank" rel="noopener">${echapper(l.nom)}</a> · ${usage}${echapper(pays)}
        <br>${echapper(l.resume)}`;
}

// Emplacements (app/emplacements.py) : données, modèles et moteurs, dans %USERPROFILE%\.Aria-Studio par défaut.
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

// Aussi appelé par l'assistant de premier lancement (écran « Ta machine »)
async function changerDossierModeles() {
    let chemin;
    try {
        chemin = (await api("POST", "/api/emplacements/modeles/parcourir")).chemin;
    } catch {
        chemin = prompt("Chemin complet du nouveau dossier des modèles (ex. D:\\IA\\Aria-modeles) :");
    }
    if (!chemin) return;
    const actuel = document.getElementById("chemin-modeles").textContent;
    if (!confirm(`Lire les modèles dans ${chemin} au prochain démarrage d'Aria ?\n\n`
        + `Rien n'est déplacé : tes modèles actuels restent dans ${actuel}.`)) return;
    try {
        afficherEmplacements(await api("PUT", "/api/emplacements/modeles", { chemin }));
    } catch (err) {
        alert(err.message);
    }
}

document.getElementById("changer-modeles").addEventListener("click", changerDossierModeles);

document.getElementById("garder-modeles").addEventListener("click", async () => {
    const e = await api("GET", "/api/emplacements");
    const actuel = e.dossiers.modeles.chemin;
    afficherEmplacements(await api("PUT", "/api/emplacements/modeles",
        { chemin: actuel === e.modeles_defaut ? null : actuel }));
});

chargerEmplacements();

// Clés d'accès (app/cles.py) : saisies ici, rangées dans le coffre Windows ; seule leur origine revient du serveur.

const ORIGINES_CLE = {
    aria: ["✓ enregistrée dans Aria (coffre Windows)", "ok"],
    variable: ["✓ lue dans la variable d'environnement", "ok"],
};

function afficherCles(etat) {
    document.getElementById("liste-cles").innerHTML = Object.entries(etat).map(([source, c]) => {
        const [texte, classe] = ORIGINES_CLE[c.origine] ?? ["aucune clé", "discret"];
        return `<li data-cle="${source}">
            <div class="ligne"><strong>${echapper(c.nom)}</strong>
                <span class="${classe}">${texte}${c.origine === "variable" ? ` ${echapper(c.variable)}` : ""}</span></div>
            <div class="ligne saisie-cle">
                <input type="password" autocomplete="new-password" spellcheck="false" placeholder="Colle ta clé ${echapper(c.nom)} ici">
                <button type="button" class="petit principal" data-action="enregistrer">Enregistrer</button>
                <button type="button" class="petit" data-action="tester" ${c.origine ? "" : "disabled"}>Tester</button>
                ${c.origine === "aria" ? '<button type="button" class="petit" data-action="supprimer">Supprimer</button>' : ""}
            </div>
            <p class="discret">Où la créer : ${echapper(c.aide)}. La variable ${echapper(c.variable)} sert si aucune clé n'est enregistrée ici.</p>
            <p class="resultat-cle" hidden></p>
        </li>`;
    }).join("");
}

async function chargerCles() {
    afficherCles(await api("GET", "/api/cles"));
}

document.getElementById("liste-cles").addEventListener("click", async (ev) => {
    const bouton = ev.target.closest("[data-action]");
    if (!bouton) return;
    const ligne = bouton.closest("[data-cle]");
    const source = ligne.dataset.cle;
    const dire = (texte, ok) => {  // la carte a pu être redessinée : on reprend la ligne affichée
        const resultat = document.querySelector(`#liste-cles [data-cle="${source}"] .resultat-cle`);
        resultat.textContent = texte;
        resultat.className = `resultat-cle ${ok ? "ok" : "ko"}`;
        resultat.hidden = false;
    };
    bouton.disabled = true;
    try {
        if (bouton.dataset.action === "enregistrer") {
            const saisie = ligne.querySelector("input");
            afficherCles(await api("PUT", `/api/cles/${source}`, { valeur: saisie.value }));
            saisie.value = "";
            const r = await api("POST", `/api/cles/${source}/tester`);  // enregistrée : on vérifie tout de suite
            dire(r.message, r.ok);
        } else if (bouton.dataset.action === "tester") {
            const r = await api("POST", `/api/cles/${source}/tester`);
            dire(r.message, r.ok);
        } else if (confirm("Supprimer la clé enregistrée dans Aria ? (Elle reste valable chez la source.)")) {
            afficherCles(await api("DELETE", `/api/cles/${source}`));
        }
        jetonsDecouvrir = await api("GET", "/api/jetons");  // web/decouvrir.js
    } catch (err) {
        dire(err.message, false);
    }
    bouton.disabled = false;
});

chargerCles();

// Moteurs (app/moteurs.py) : ComfyUI portable et llama-server aux versions épinglées de catalogue/moteurs.yaml.
// Rien ne se télécharge sans un clic ; le bandeau prévient quand un moteur obligatoire manque.

const STATUTS_MOTEUR = {
    absent: ["absent", "ko"], a_jour: ["à jour ✓", ""], different: ["autre version", "ko"], manuel: ["installé à la main", ""],
    externe: ["ton ComfyUI ✓", ""],
};
let moteursAffiches = [];
let verificationComfyui = null;  // « J'ai déjà ComfyUI » : le message pendant la vérification (jusqu'à une minute)

function suiviMoteur(t) {
    const enCours = !t.fini;
    const pourcent = t.total ? Math.floor((t.fait * 100) / t.total) : null;
    return `<div class="suivi-moteur">
        <span class="discret">${echapper(t.etape)}${pourcent !== null ? ` · ${pourcent} %` : ""}</span>
        ${pourcent !== null && enCours ? `<div class="barre"><div style="width:${pourcent}%"></div></div>` : ""}
        ${t.erreur ? `<span class="ko">${echapper(t.erreur)}</span>` : ""}
        ${enCours && t.etape.startsWith("Téléchargement") ? '<button type="button" class="petit" data-annuler-moteur>✕ Annuler</button>' : ""}
    </div>`;
}

function afficherMoteurs(e) {
    const c = e.carte;
    const ligneCarte = document.getElementById("carte-graphique");
    ligneCarte.textContent = c.nvidia ? `Carte : ${c.nom} · pilote ${c.pilote}${c.ok ? " ✓" : ""}` : "";
    if (!c.ok) ligneCarte.textContent += (ligneCarte.textContent ? " · " : "") + c.message;
    ligneCarte.classList.toggle("ko", !c.ok);

    const t = e.tache;
    const occupe = t && !t.fini;
    moteursAffiches = e.moteurs;
    // La même liste dans Réglages et dans l'assistant de premier lancement (web/assistant.js)
    const html = e.moteurs.map((m) => {
        const [libelle, classe] = STATUTS_MOTEUR[m.statut];
        const sansV = (v) => String(v).replace(/^v/, "");  // « v0.42.1 » (tag) et « 0.42.1 » (ComfyUI) sont la même
        const autre = m.version_installee && sansV(m.version_installee) !== sansV(m.version)
            ? ` · installé : ${echapper(m.version_installee)}` : "";
        const installer = ["a_jour", "externe"].includes(m.statut) ? ""
            : `<button type="button" class="petit ${m.statut === "absent" && !m.facultatif ? "principal" : ""}"
                data-installer="${m.id}" ${occupe || verificationComfyui ? "disabled" : ""}>${m.statut === "absent" ? "Installer" : "Réinstaller"}${
                m.a_telecharger ? ` (${taille(m.a_telecharger)})` : ""}</button>`;
        const sien = m.id !== "comfyui" ? ""
            : m.externe ? `<button type="button" class="petit" data-comfyui-aria ${occupe || verificationComfyui ? "disabled" : ""}
                title="Reprendre le ComfyUI installé par Aria">Revenir à celui d'Aria</button>`
            : `<button type="button" class="petit" data-comfyui-externe ${occupe || verificationComfyui ? "disabled" : ""}
                title="Un ComfyUI portable déjà installé (0.42.1 ou plus récent) : pas de 4 Go en double">Utiliser mon ComfyUI…</button>`;
        return `<li>
            <div>
                <strong>${echapper(m.nom)}</strong> <span class="badge ${classe}">${libelle}</span>
                ${m.facultatif ? '<span class="badge">facultatif</span>' : ""}${m.occupe ? '<span class="badge">en marche</span>' : ""}
                <div class="discret">${echapper(m.role)} · épinglé ${echapper(m.version)}${autre}</div>
                ${m.externe ? `<div class="discret">Dossier : <code>${echapper(m.chemin)}</code></div>` : ""}
            </div>
            <div class="actions-moteur">
                ${installer}
                ${m.precedente && !m.externe ? `<button type="button" class="petit" data-revenir="${m.id}" ${occupe || verificationComfyui ? "disabled" : ""}
                    title="Remet la version précédente (un second clic redonne celle-ci)">↺ Revenir</button>` : ""}
                ${sien}
                ${!["absent", "externe"].includes(m.statut) ? `<button type="button" class="petit" data-ouvrir="moteurs"
                    data-sous="${echapper(m.dossier)}" title="Ouvrir le dossier">📂</button>` : ""}
            </div>
            ${m.id === "comfyui" && verificationComfyui ? `<div class="suivi-moteur"><span class="discret">${echapper(verificationComfyui)}</span></div>` : ""}
            ${t && t.id === m.id ? suiviMoteur(t) : ""}
        </li>`;
    }).join("");
    document.querySelectorAll(".liste-moteurs").forEach((liste) => { liste.innerHTML = html; });
    document.dispatchEvent(new CustomEvent("moteurs", { detail: e }));

    const manquants = e.moteurs.filter((m) => m.statut === "absent" && !m.facultatif);
    const bandeau = document.getElementById("bandeau-moteurs");
    bandeau.hidden = !manquants.length;
    if (manquants.length) document.getElementById("systeme-reglages").open = true;  // à installer : visible d'emblée
    bandeau.querySelector("span").textContent = manquants.length
        ? `⚠ ${manquants.map((m) => m.nom).join(" et ")} : absent${manquants.length > 1 ? "s" : ""}, Aria ne peut pas générer sans.`
        : "";
    clearTimeout(minuteurMoteurs);  // chaque appel relançait sa propre boucle d'une seconde : elles s'empilaient
    if (occupe) minuteurMoteurs = setTimeout(chargerMoteurs, 1000);
}

let minuteurMoteurs = null;

async function chargerMoteurs() {
    try {
        afficherMoteurs(await api("GET", "/api/moteurs"));
    } catch {
        // pas de serveur : la barre d'état le dit déjà
    }
}

document.addEventListener("click", async (ev) => {
    const bouton = ev.target.closest("[data-installer], [data-revenir], [data-annuler-moteur], [data-comfyui-externe], [data-comfyui-aria]");
    if (!bouton || !bouton.closest(".liste-moteurs")) return;
    try {
        if ("comfyuiExterne" in bouton.dataset || "comfyuiAria" in bouton.dataset) {
            await choisirComfyui("comfyuiExterne" in bouton.dataset);
        } else if (bouton.dataset.installer) {
            const m = moteursAffiches.find((x) => x.id === bouton.dataset.installer);
            const source = m.a_telecharger
                ? `Téléchargement : ${taille(m.a_telecharger)} depuis sa release GitHub officielle (empreinte SHA-256 vérifiée).`
                : "Les archives sont déjà là : rien à télécharger.";
            if (!confirm(`Installer ${m.nom} ${m.version} ?\n\n${source}\n`
                + "Le moteur est arrêté le temps de l'échange ; la version actuelle reste à côté (↺ Revenir).")) return;
            await api("POST", `/api/moteurs/${m.id}/installer`);
        } else if (bouton.dataset.revenir) {
            if (!confirm("Remettre la version précédente de ce moteur ?")) return;
            await api("POST", `/api/moteurs/${bouton.dataset.revenir}/revenir`);
        } else {
            await api("POST", "/api/moteurs/annuler");
        }
    } catch (err) {
        alert(err.message);
    }
    chargerMoteurs();
});

// « J'ai déjà ComfyUI » : un ComfyUI portable choisi par l'utilisateur, lancé une fois pour vérifier ses nœuds.
async function choisirComfyui(sien) {
    let chemin = null;
    if (sien) {
        try {
            chemin = (await api("POST", "/api/moteurs/comfyui/parcourir")).chemin;
        } catch {
            chemin = prompt("Chemin complet du dossier de ton ComfyUI portable (celui qui contient python_embeded) :");
        }
        if (!chemin) return;
        if (!confirm(`Utiliser le ComfyUI de ${chemin} ?\n\nAria va le lancer une fois pour vérifier qu'il a tous les nœuds `
            + "dont elle se sert (jusqu'à une minute). Le ComfyUI d'Aria est arrêté pendant ce temps.")) return;
    } else if (!confirm("Reprendre le ComfyUI installé par Aria ?")) {
        return;
    }
    verificationComfyui = sien ? "Vérification de ton ComfyUI (jusqu'à une minute)…" : "Bascule…";
    await chargerMoteurs();
    try {
        await api("PUT", "/api/moteurs/comfyui/externe", { chemin });
    } finally {
        verificationComfyui = null;
    }
}

document.getElementById("voir-moteurs").addEventListener("click", () => {
    document.querySelector('.onglet[data-vue="reglages"]').click();
    document.getElementById("systeme-reglages").open = true;
    document.getElementById("carte-moteurs").scrollIntoView({ behavior: "smooth", block: "start" });
});

chargerMoteurs();

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
        bandeau.querySelector("span").textContent = `✨ Aria Studio ${d.version} est disponible (tu as la ${e.version})`
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
