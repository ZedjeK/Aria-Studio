// Outils de base de toutes les pages (chargé en premier) : appels au serveur, échappement, tailles, suivi
// et annulation des commandes, décimales, mots, erreurs, listes déroulantes, adresse d'une image.

const echapper = (texte) => String(texte ?? "").replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[c]);

const taille = (octets) => {
    if (octets >= 2 ** 30) return `${(octets / 2 ** 30).toFixed(2)} Go`;
    if (octets >= 2 ** 20) return `${(octets / 2 ** 20).toFixed(1)} Mo`;
    return `${Math.round(octets / 2 ** 10)} Ko`;
};

// `signal` (AbortController) : ✕ Annuler coupe la connexion, et le serveur arrête le LLM (services.tant_que_connecte).
async function api(methode, url, corps, signal) {
    const reponse = await fetch(url, {
        method: methode,
        headers: corps ? { "Content-Type": "application/json" } : {},
        body: corps ? JSON.stringify(corps) : undefined,
        signal,
    });
    // Une erreur 500 brute arrive en texte, une erreur de validation avec une liste : le message reste lisible.
    const texte = await reponse.text();
    let donnees = null;
    try {
        donnees = texte ? JSON.parse(texte) : null;
    } catch {
        donnees = null;
    }
    if (!reponse.ok) {
        const detail = donnees?.detail;
        throw new Error(Array.isArray(detail) ? detail.map((d) => d.msg ?? String(d)).join(" ; ")
            : detail ?? (texte.trim().slice(0, 200) || `HTTP ${reponse.status}`));
    }
    return donnees;
}

// ✕ Annuler sous une barre de progression (Plateau, Montage, Séquence) : arrête la commande suivie par ce bloc.
function suivreAnnulable(bloc, idCommande) {
    bloc.dataset.commande = idCommande;
    const bouton = bloc.querySelector(".annuler-commande");
    bouton.disabled = false;
    bouton.textContent = "✕ Annuler";
}

document.addEventListener("click", async (e) => {
    const bouton = e.target.closest(".annuler-commande");
    const bloc = bouton?.closest("[data-commande]");
    if (!bloc) return;
    bouton.disabled = true;
    bouton.textContent = "Annulation…";
    try {
        await api("POST", `/api/commandes/${bloc.dataset.commande}/annuler`);
    } catch {
        suivreAnnulable(bloc, bloc.dataset.commande);  // déjà finie, ou serveur injoignable : le suivi le dira
    }
});

const attendre = (ms) => new Promise((fin) => setTimeout(fin, ms));

// Petits outils partagés par toutes les pages.
const virgule = (x, chiffres = 2) => Number(x).toFixed(chiffres).replace(".", ",");  // 0,80 : la décimale française
const compterMots = (texte) => {
    const t = (texte ?? "").trim();
    return t ? t.split(/\s+/).length : 0;
};

const estAnnulation = (err) => err?.name === "AbortError";

// Un champ « nombre » (Images du Plateau, Clips du Montage) : toujours un entier entre 1 et son max, celui du serveur
// (champ.max, posé au chargement des options) ; 1 si le champ est vide ou illisible. Ce qui est tapé hors limites
// est ramené dans les limites quand on quitte le champ.
function nombreDuChamp(id) {
    const champ = document.getElementById(id);
    const max = Number(champ.max) || 1;
    const n = Math.floor(Number(champ.value));
    return Number.isFinite(n) && champ.value !== "" ? Math.min(max, Math.max(1, n)) : 1;
}

function brancherNombre(id, quandChange = () => {}) {
    const champ = document.getElementById(id);
    champ.addEventListener("input", quandChange);  // l'estimation suit pendant la frappe
    champ.addEventListener("change", () => {
        champ.value = String(nombreDuChamp(id));
        quandChange();
    });
}

// Pose le max d'un champ nombre (celui du serveur) et le réaffiche dans les limites ; la valeur déjà tapée reste.
function poserMaxNombre(id, max) {
    const champ = document.getElementById(id);
    champ.max = String(max);
    champ.value = String(nombreDuChamp(id));
}

function afficherErreur(element, message) {
    element.textContent = message;
    element.hidden = false;
}

// Suit une commande jusqu'au bout et rend la commande finie. Chaque suivi garde son propre id : une variante lancée
// pendant une génération ne détourne plus le suivi de la première. Un relevé raté (serveur occupé, réseau) est
// réessayé ; au bout de `ratesMax` d'affilée, l'erreur remonte et l'appelant débloque son formulaire.
async function suivreJusquauBout(id, chaqueReleve, intervalle, ratesMax = 20) {
    let rates = 0;
    for (;;) {
        let c;
        try {
            c = await api("GET", `/api/commandes/${id}`);
            rates = 0;
        } catch (err) {
            if (++rates >= ratesMax) throw err;
            await attendre(intervalle);
            continue;
        }
        chaqueReleve(c);
        if (!["attente", "en_cours"].includes(c.etat)) return c;
        await attendre(intervalle);
    }
}

// Message d'une commande annulée : ce qui était déjà produit reste (galerie, bobines).
function messageAnnulation(c, nom, feminin = false) {
    const n = c.images.length;
    const accord = (feminin ? "e" : "") + (n > 1 ? "s" : "");
    return n ? `Annulé : ${n} ${nom}${n > 1 ? "s" : ""} déjà produit${accord}, gardé${accord}.` : "Annulé.";
}

const urlImage = (id) => `/api/creations/${encodeURIComponent(id)}/image`;

// Un select présenté en groupe de boutons (Montage, Scénario, Séquence, Modèles). Le select reste la source de vérité,
// caché : le reste du code continue de le lire, de le remplir (remplirSelect) et d'écouter « change ». Le groupe suit ses
// options (et leur « disabled »), un « select.value = … » posé par le code, et le « hidden » de son étiquette.
function groupeBoutons(idSelect, titre) {
    const select = document.getElementById(idSelect);
    const etiquette = select.closest("label");
    const groupe = document.createElement("div");
    groupe.className = "groupe-boutons";
    groupe.setAttribute("role", "radiogroup");
    groupe.setAttribute("aria-label", titre);
    etiquette.before(groupe);
    etiquette.style.display = "none";

    const rafraichir = () => groupe.querySelectorAll("button").forEach((b) => {
        const actif = b.dataset.valeur === select.value;
        b.classList.toggle("actif", actif);
        b.setAttribute("aria-checked", String(actif));
    });
    const construire = () => {
        groupe.innerHTML = `<span>${echapper(titre)}</span>`;
        for (const option of select.options) {
            const bouton = document.createElement("button");
            bouton.type = "button";
            bouton.className = "choix";  // pas un bouton nu : le style des segments ne le reprend pas
            bouton.setAttribute("role", "radio");
            bouton.dataset.valeur = option.value;
            bouton.disabled = option.disabled;
            bouton.title = option.textContent;  // le détail (MP, steps) en infobulle
            bouton.textContent = option.textContent.split(" · ")[0].replace(/^[^\wÀ-ÿ]+/, "").trim() || option.value;
            bouton.addEventListener("click", () => {
                select.value = option.value;
                select.dispatchEvent(new Event("change"));
            });
            groupe.append(bouton);
        }
        rafraichir();
    };
    const natif = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value");
    Object.defineProperty(select, "value", {
        configurable: true,
        get() { return natif.get.call(this); },
        set(v) { natif.set.call(this, v); rafraichir(); },
    });
    select.addEventListener("change", rafraichir);
    new MutationObserver(construire).observe(select, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled"] });
    const suivreEtiquette = () => { groupe.hidden = etiquette.hidden; };
    new MutationObserver(suivreEtiquette).observe(etiquette, { attributes: true, attributeFilter: ["hidden"] });
    suivreEtiquette();
    construire();
}

function remplirSelect(id, valeurs, defaut) {
    const select = document.getElementById(id);
    const garde = valeurs.some(([v]) => v === select.value) ? select.value : defaut;  // garde le choix en cours
    select.innerHTML = valeurs.map(([valeur, texte]) =>
        `<option value="${echapper(valeur)}" ${valeur === garde ? "selected" : ""}>${echapper(texte)}</option>`).join("");
}

// Fin d'un job (Plateau, Montage, Casting) : une notification Windows (API Notifications du navigateur), toujours, que la
// fenêtre d'Aria soit au premier plan ou non, et un son. Si le navigateur ne les autorise pas (pas encore accordées,
// refusées, absentes), un bandeau dans l'appli prend le relais pour ne rien rater. Choix dans Réglages → Système.
const prefsNotifications = { active: true, son: true };  // lus au démarrage par web/reglages.js

// Le navigateur ne demande l'autorisation qu'après un geste : au premier clic sur un bouton d'action (Créer, Tourner…).
document.addEventListener("click", (e) => {
    if (prefsNotifications.active && e.target.closest?.("button.principal")
        && "Notification" in window && Notification.permission === "default") Notification.requestPermission();
}, true);

let sonNotifications = null;
function jouerSonFin(reussi) {
    try {
        sonNotifications ??= new AudioContext();
        const t = sonNotifications.currentTime;
        // deux notes montantes si c'est réussi, descendantes sinon
        (reussi ? [660, 990] : [520, 350]).forEach((frequence, i) => {
            const osc = sonNotifications.createOscillator();
            const gain = sonNotifications.createGain();
            osc.type = "sine";
            osc.frequency.value = frequence;
            gain.gain.setValueAtTime(0.0001, t + i * 0.16);
            gain.gain.exponentialRampToValueAtTime(0.18, t + i * 0.16 + 0.02);
            gain.gain.exponentialRampToValueAtTime(0.0001, t + i * 0.16 + 0.3);
            osc.connect(gain).connect(sonNotifications.destination);
            osc.start(t + i * 0.16);
            osc.stop(t + i * 0.16 + 0.32);
        });
    } catch {
        // pas d'audio (navigateur, réglages) : le bandeau suffit
    }
}

// titre : « Image terminée » ; texte : le détail ; vue : l'onglet à ouvrir au clic ; reussi : false pour une erreur
// ou une annulation.
function notifierFin(titre, texte, vue, reussi = true) {
    if (!prefsNotifications.active) return;
    const ouvrir = () => { window.focus(); document.querySelector(`.onglet[data-vue="${vue}"]`)?.click(); };
    if (prefsNotifications.son) jouerSonFin(reussi);
    if ("Notification" in window && Notification.permission === "granted") {
        try {
            // silent : le son est celui d'Aria (réglable), pas un second celui de Windows.
            // Un tag propre à chaque job : avec un tag commun à l'onglet, une notification restée au centre de notifications
            // est remplacée sans reparaître en bannière.
            const n = new Notification(`Aria Studio · ${titre}`, { body: texte, icon: "/static/favicon.png",
                                                                  tag: `aria-${vue}-${Date.now()}`, silent: true });
            n.onclick = () => { n.close(); ouvrir(); };
            n.onerror = () => afficherBandeauFin(titre, texte, reussi, ouvrir);  // le navigateur n'a pas pu l'afficher
            return;
        } catch {
            // constructeur refusé : le bandeau prend le relais
        }
    }
    afficherBandeauFin(titre, texte, reussi, ouvrir);
}

// Le bandeau dans l'appli : le repli quand la notification Windows n'est pas autorisée, absente ou en échec.
function afficherBandeauFin(titre, texte, reussi, ouvrir) {
    let bandeau = document.getElementById("notification-fin");
    if (!bandeau) {
        bandeau = document.createElement("button");
        bandeau.type = "button";
        bandeau.id = "notification-fin";
        document.body.append(bandeau);
    }
    bandeau.className = reussi ? "ok" : "ko";
    bandeau.innerHTML = `<strong>${echapper(titre)}</strong><span>${echapper(texte)}</span>`;
    bandeau.onclick = () => { bandeau.hidden = true; ouvrir(); };
    bandeau.hidden = false;
    clearTimeout(bandeau.minuteur);
    bandeau.minuteur = setTimeout(() => { bandeau.hidden = true; }, 10000);
}

// Zones de texte : elles grandissent vers le bas avec leur contenu (plafond à 70 % de l'écran, puis défilement).
// `rows` donne la hauteur minimale. Les remplissages par le code (prompt écrit par le LLM, valeur rechargée) et les
// zones masquées puis affichées sont suivis aussi.
(() => {
    const ajuster = (zone) => {
        if (!zone.offsetParent) return;                       // masquée : IntersectionObserver reprendra à l'affichage
        zone.style.height = "auto";
        const bords = zone.offsetHeight - zone.clientHeight;
        zone.style.height = `${zone.scrollHeight + bords}px`;
    };
    const vue = new IntersectionObserver((entrees) => {
        for (const e of entrees) if (e.isIntersecting) ajuster(e.target);
    });
    const suivre = (zone) => {
        if (zone.dataset.autoTaille) return;
        zone.dataset.autoTaille = "1";
        vue.observe(zone);
        ajuster(zone);
    };

    const reglage = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value");
    Object.defineProperty(HTMLTextAreaElement.prototype, "value", {
        ...reglage,
        set(texte) { reglage.set.call(this, texte); suivre(this); ajuster(this); },
    });

    document.addEventListener("input", (e) => {
        if (e.target instanceof HTMLTextAreaElement) ajuster(e.target);
    });
    window.addEventListener("resize", () => {
        document.querySelectorAll("textarea").forEach(ajuster);
    });
    new MutationObserver((mutations) => {
        for (const m of mutations) for (const n of m.addedNodes) {
            if (n instanceof HTMLTextAreaElement) suivre(n);
            else if (n.querySelectorAll) n.querySelectorAll("textarea").forEach(suivre);
        }
    }).observe(document.documentElement, { childList: true, subtree: true });
    document.addEventListener("DOMContentLoaded", () => document.querySelectorAll("textarea").forEach(suivre));
})();
