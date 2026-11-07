// Casting, onglet Voix : la voix de référence, les essais de voix et la bibliothèque (app/voix.py)

let voixConnues = [];
let brouillonVoix = null;  // essai tourné, pas encore gardé

async function chargerVoix() {
    voixConnues = await api("GET", "/api/voix");
    const clips = (await api("GET", "/api/creations?genre=clips")).filter((f) => f.mode_video !== "film");
    const select = document.getElementById("voix-sequence");
    const garde = sequenceCourante ? (sequenceCourante.casting.voix ?? "") : (select.value || "auto");
    const option = (valeur, texte) => `<option value="${echapper(valeur)}">${echapper(texte)}</option>`;
    select.innerHTML = option("auto", "Auto : le premier plan qui parle") + option("", "Aucune référence")
        + (voixConnues.length ? `<optgroup label="📚 Bibliothèque">${voixConnues.map((v) =>
            option(`voix:${v.id}`, `${v.source === "fichier" ? "📁" : "🎙️"} ${v.nom}`)).join("")}</optgroup>` : "")
        + (clips.length ? `<optgroup label="🎞️ Son d'un clip">${clips.map((f) =>
            option(f.id, (f.idee || f.id).slice(0, 50))).join("")}</optgroup>` : "");
    select.value = garde;
    rendreBibliotheque();
    majEcouteVoix();
}

function majEcouteVoix() {
    const valeur = document.getElementById("voix-sequence").value;
    const audio = document.getElementById("ecoute-voix-sequence");
    const src = valeur.startsWith("voix:") ? `/api/voix/${encodeURIComponent(valeur.slice(5))}/son`
        : valeur && valeur !== "auto" ? urlVideo(valeur) : null;  // le son d'un clip se lit aussi
    audio.hidden = !src;
    if (!src) audio.removeAttribute("src");
    else if (audio.getAttribute("src") !== src) audio.src = src;
}

function rendreBibliotheque() {
    document.querySelector("#bibliotheque-voix summary").textContent = `📚 Bibliothèque (${voixConnues.length})`;
    document.getElementById("liste-voix").innerHTML = voixConnues.length ? voixConnues.map((v) => `
        <li data-voix="${echapper(v.id)}">
            <span class="nom-voix">${v.source === "fichier" ? "📁" : "🎙️"} ${echapper(v.nom)}</span>
            <span class="discret">${v.duree_son ? `${Math.round(v.duree_son)} s · ` : ""}${
                v.source === "fichier" ? "fichier importé" : echapper(v.phrase ?? "")}</span>
            <audio controls preload="none" src="/api/voix/${encodeURIComponent(v.id)}/son"></audio>
            <span class="actions-plan">
                <button type="button" class="petit" data-action="choisir" title="Voix de cette séquence">Choisir</button>
                <button type="button" class="petit" data-action="renommer" title="Renommer">✏️</button>
                <button type="button" class="petit danger" data-action="supprimer" title="Supprimer">✕</button>
            </span>
        </li>`).join("")
        : "<li class=\"discret\">Aucune voix pour l'instant : tourne un essai, ou importe un fichier audio.</li>";
}

function erreurVoix(message) {
    const p = document.getElementById("erreur-voix");
    p.textContent = message ?? "";
    p.hidden = !message;
}

async function choisirVoix(id) {  // la voix devient celle de la séquence : tout est à retourner
    sequenceCourante.casting.voix = `voix:${id}`;
    await chargerVoix();
    modifiee(true);
}

// Essai de voix : une commande du Plateau, suivie comme un tournage.

async function lancerEssaiVoix() {
    erreurVoix(null);
    const premier = sequenceCourante.plans[0];
    const personnage = sequenceCourante.casting.personnage
        ?? (premier?.enchainement === "rush" ? premier.rush : null);  // comme le serveur (sequences.personnage)
    let commande;
    try {
        commande = await api("POST", "/api/voix/essais", {
            phrase: document.getElementById("phrase-voix").value.trim(),
            description: document.getElementById("description-voix").value.trim(), personnage });
    } catch (err) {
        erreurVoix(err.message);
        return;
    }
    brouillonVoix = null;
    document.getElementById("resultat-voix").hidden = true;
    document.getElementById("lancer-essai-voix").disabled = true;
    document.getElementById("progression-voix").hidden = false;
    suivreAnnulable(document.getElementById("progression-voix"), commande.id);
    suivreEssaiVoix(commande.id, Date.now());
}

async function suivreEssaiVoix(id, debut) {
    const estimation = optionsMontage.video.voix.estimation_s;
    const bloc = document.getElementById("progression-voix");
    let c;
    try {
        c = await suivreJusquauBout(id, (r) => {
            const ecoule = (Date.now() - debut) / 1000;
            bloc.querySelector(".barre div").style.width = `${Math.round(100 * Math.min(1, ecoule / estimation))}%`;
            bloc.querySelector("span").textContent = r.etat === "attente" ? `En file d'attente… ${enMinutes(ecoule)}`
                : `${r.etape || "Préparation"} · ${Math.round(100 * r.progression)} % · ${enMinutes(ecoule)}`;
        }, 1500);
    } catch (err) {
        c = { etat: "erreur", erreur: `Suivi interrompu (${err.message}) : l'essai continue peut-être.` };
    }
    bloc.hidden = true;
    document.getElementById("lancer-essai-voix").disabled = false;
    if (c.etat === "erreur" || c.etat === "annule") {
        erreurVoix(c.etat === "annule" ? "Essai annulé." : c.erreur);
        return;
    }
    brouillonVoix = c.voix;
    const resultat = document.getElementById("resultat-voix");
    resultat.querySelector("audio").src = `/api/voix/${encodeURIComponent(c.voix)}/son`;
    resultat.hidden = false;
    document.getElementById("nom-voix").focus();
}

document.getElementById("lancer-essai-voix").addEventListener("click", lancerEssaiVoix);
document.getElementById("autre-prise-voix").addEventListener("click", lancerEssaiVoix);  // nouvelle seed
document.getElementById("garder-voix").addEventListener("click", async () => {
    const nom = document.getElementById("nom-voix");
    if (!nom.value.trim()) {
        nom.focus();
        erreurVoix("Donne un nom à cette voix pour la garder.");
        return;
    }
    try {
        await api("PUT", `/api/voix/${encodeURIComponent(brouillonVoix)}`, { nom: nom.value.trim() });
    } catch (err) {
        erreurVoix(err.message);
        return;
    }
    erreurVoix(null);
    document.getElementById("resultat-voix").hidden = true;
    nom.value = "";
    await choisirVoix(brouillonVoix);
    brouillonVoix = null;
});

// Bibliothèque : choisir, renommer, supprimer, importer.

document.getElementById("liste-voix").addEventListener("click", async (e) => {
    const action = e.target.closest("[data-action]")?.dataset.action;
    const id = e.target.closest("[data-voix]")?.dataset.voix;
    if (!action || !id) return;
    const v = voixConnues.find((x) => x.id === id);
    erreurVoix(null);
    try {
        if (action === "choisir") {
            await choisirVoix(id);
        } else if (action === "renommer") {
            const nom = prompt("Nouveau nom de la voix :", v.nom);
            if (!nom?.trim()) return;
            await api("PUT", `/api/voix/${encodeURIComponent(id)}`, { nom: nom.trim() });
            await chargerVoix();
        } else if (action === "supprimer") {
            if (!confirm(`Supprimer la voix « ${v.nom} » ? Elle part dans la corbeille des voix ; `
                + "les séquences qui l'utilisent repassent en Auto (leurs plans seront à retourner).")) return;
            const r = await api("DELETE", `/api/voix/${encodeURIComponent(id)}`);
            if (sequenceCourante.casting.voix === `voix:${id}`) {
                if (sequenceCourante.id && r.sequences.includes(sequenceCourante.id) && !sequenceModifiee) {
                    afficherSequence(await api("GET", `/api/sequences/${encodeURIComponent(sequenceCourante.id)}`));
                } else {
                    sequenceCourante.casting.voix = "auto";
                    modifiee(true);
                }
            }
            await chargerVoix();
        }
    } catch (err) {
        erreurVoix(err.message);
    }
});

document.getElementById("importer-voix").addEventListener("click", () => document.getElementById("fichier-voix").click());
document.getElementById("fichier-voix").addEventListener("change", (e) => {
    const fichier = e.target.files[0];
    e.target.value = "";  // le même fichier pourra être repris
    if (!fichier) return;
    const nom = prompt("Nom de la voix :", fichier.name.replace(/\.[^.]+$/, "").slice(0, 40));
    if (!nom?.trim()) return;
    const lecteur = new FileReader();
    lecteur.onload = async () => {
        erreurVoix(null);
        try {
            const v = await api("POST", "/api/voix/import", { audio: lecteur.result, nom: nom.trim(), nom_origine: fichier.name });
            await choisirVoix(v.id);
        } catch (err) {
            erreurVoix(err.message);
        }
    };
    lecteur.readAsDataURL(fichier);
});
