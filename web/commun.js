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

function remplirSelect(id, valeurs, defaut) {
    const select = document.getElementById(id);
    const garde = valeurs.some(([v]) => v === select.value) ? select.value : defaut;  // garde le choix en cours
    select.innerHTML = valeurs.map(([valeur, texte]) =>
        `<option value="${echapper(valeur)}" ${valeur === garde ? "selected" : ""}>${echapper(texte)}</option>`).join("");
}
