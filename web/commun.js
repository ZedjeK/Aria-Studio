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
