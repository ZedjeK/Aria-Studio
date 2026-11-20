// Casting : lancement des bancs et comparaison à l'aveugle.

const NOMS_TACHES = { texte_vers_image: "Texte vers image", retouche: "Retouche", reecriture: "Écriture (LLM)",
                      image_vers_video: "Image vers vidéo", texte_vers_video: "Texte vers vidéo" };
const AVEC_SOURCE = ["retouche", "image_vers_video"];

// Une vidéo se lit en boucle, muette au départ (🔊 dans les contrôles) ; une image s'ouvre en grand.
const media = (url, titre) => url.endsWith(".mp4")
    ? `<video src="${url}" title="${titre}" loop muted autoplay playsinline controls preload="metadata"></video>`
    : `<a href="${url}" target="_blank"><img src="${url}" alt="${titre}" loading="lazy"></a>`;
const CONTROLES = { anglais: "anglais", longueur: "longueur", sans_preambule: "sans préambule", texte_conserve: "texte gardé" };
let bancAffiche = null;
let suiviBanc = null;
let tacheBanc = "texte_vers_image";  // la tâche du prochain casting (les cartes à onglet)
let preparation = null;  // /api/bancs/preparation : ce que ferait un casting de chaque tâche
let bancEnCours = false;
const UNITES_BANC = { reecriture: "étape", image_vers_video: "clip", texte_vers_video: "clip" };  // sinon : image
const nombreDe = (n, mot) => `${n} ${mot}${n > 1 ? "s" : ""}`;
const unitesNotees = (tache) => tache.endsWith("video") ? "clips notés" : "images notées";
// « 20 nov. · 10:15 », à partir de la date ISO d'un casting.
const dateBanc = (iso) => new Date(iso).toLocaleString("fr-FR", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })
    .replace(/,| à/, " ·");
// Sous la carte d'un casting : à l'aveugle ou révélé, et où en sont les notes (liste ou détail d'un casting).
const etatBanc = (b) => `${b.revele ? "révélé" : "à l'aveugle"} · ${b.notees} / ${b.total} ${b.tache.endsWith("video") ? "notés" : "notées"}`;
let sousOngletCasting = "lancer";

// Deux sous-onglets : lancer un casting, puis comparer ses résultats. L'encadré « À savoir » suit l'onglet actif
// (web/habillage.js).
function afficherSousOngletCasting(sous) {
    sousOngletCasting = sous;
    document.querySelectorAll("#vue-casting .sous-onglet").forEach((b) => b.classList.toggle("actif", b.dataset.sous === sous));
    document.querySelectorAll("#vue-casting [data-panneau-casting]").forEach((p) => { p.hidden = p.dataset.panneauCasting !== sous; });
}

document.querySelectorAll("#vue-casting .sous-onglet").forEach((b) => {
    b.addEventListener("click", () => afficherSousOngletCasting(b.dataset.sous));
});

async function chargerCasting() {
    const [{ actif, bancs }, prep] = await Promise.all([api("GET", "/api/bancs"), api("GET", "/api/bancs/preparation")]);
    preparation = prep;
    dessinerTaches();
    dessinerPreparation();
    afficherProgression(actif);

    // Sur le sous-onglet « Comparer » : combien de castings attendent encore leurs notes.
    const aNoter = bancs.filter((b) => !b.revele).length;
    const compte = document.getElementById("a-noter-casting");
    compte.hidden = !aNoter;
    compte.textContent = `${aNoter} à noter`;

    // Les castings : des cartes à onglet, un seul affiché à la fois. Sans aucun casting, une invitation à en lancer un.
    const vide = !bancs.length;
    document.getElementById("carte-castings").hidden = vide;
    document.getElementById("carte-banc").hidden = vide;
    document.getElementById("carte-vide-casting").hidden = !vide;
    if (vide) return;
    if (!bancs.some((b) => b.id === bancAffiche)) bancAffiche = bancs[0].id;
    document.getElementById("nb-castings").textContent = nombreDe(bancs.length, "casting");
    document.getElementById("historique-bancs").innerHTML = bancs.map((b) => `<label class="carte-mode">
        <input type="radio" name="banc-affiche" value="${echapper(b.id)}" ${b.id === bancAffiche ? "checked" : ""}>
        <strong>${NOMS_TACHES[b.tache] ?? echapper(b.tache)}</strong>
        <small>${dateBanc(b.date)} · ${nombreDe(b.nb_workflows, "modèle")}</small>
        <small class="${b.revele ? "etat-revele" : "etat-aveugle"}" data-etat-banc>${etatBanc(b)}</small></label>`).join("");
    chargerBanc();
}

// Après une note : la carte du casting affiché dit où il en est, sans relire toute la liste.
function majCarteBanc(b) {
    const etat = document.querySelector("#historique-bancs input:checked")?.closest("label").querySelector("[data-etat-banc]");
    if (etat) etat.textContent = etatBanc(b);
}

// Nouveau casting : ce que le serveur annonce pour chaque tâche (app/casting.py, preparation)

// « Lancer le casting » s'éteint pendant un casting, et quand celui de la tâche choisie ne peut pas partir.
function majLancer() {
    document.getElementById("lancer-banc").disabled = bancEnCours || Boolean(preparation?.taches[tacheBanc]?.blocage);
}

// Sous le nom de chaque carte : combien de modèles, et ce que le casting produirait ou ce qui l'en empêche.
function dessinerTaches() {
    for (const [tache, p] of Object.entries(preparation.taches)) {
        const carte = document.querySelector(`#taches-banc input[value="${tache}"]`)?.closest("label");
        if (!carte) continue;
        const n = p.candidats.length;
        const incomplets = p.candidats.filter((c) => c.manquants).length;
        const mot = tache === "reecriture" ? "candidat" : "modèle";
        const qui = nombreDe(n, mot);
        carte.classList.toggle("seul", Boolean(p.blocage));
        carte.querySelector("small").textContent = !p.blocage ? `${qui} · ${nombreDe(p.total, UNITES_BANC[tache] ?? "image")} à produire`
            : n === 0 ? `aucun ${mot}` : n === 1 ? `1 seul ${mot} · rien à comparer`
            : incomplets ? `${qui} · ${incomplets} incomplet${incomplets > 1 ? "s" : ""}` : "ne peut pas partir";
    }
}

function dessinerPreparation() {
    const p = preparation?.taches[tacheBanc];
    if (!p) return;
    const llm = tacheBanc === "reecriture";
    const manque = (c) => c.manquants > 1 ? `${c.manquants} fichiers manquent` : "1 fichier manque";
    // Le bouton ne porte que l'identifiant du modèle : sa tâche est celle de la page, ou Texte vers image pour le
    // modèle de rendu du Casting LLM. Rien de ce qui est lu dans la page n'est réécrit dans le HTML.
    const versModeles = (c, rendu = false) => c.manquants
        ? `<button type="button" class="petit" data-ouvrir-modele="${echapper(c.id)}"${rendu ? " data-rendu" : ""}>Ouvrir dans Modèles</button>` : "";

    document.getElementById("nom-tache-banc").textContent = NOMS_TACHES[tacheBanc];
    document.getElementById("titre-candidats-banc").textContent = llm ? "Candidats comparés" : "Modèles comparés";
    document.getElementById("candidats-banc").innerHTML = p.candidats.map((c) => `<li><strong>${echapper(c.nom)}</strong>
        <span class="badge ${c.manquants ? "manque" : "ok"}">${c.manquants ? `incomplet · ${manque(c)}` : "prêt"}</span>
        ${versModeles(c)}</li>`).join("") || '<li class="discret">Aucun candidat pour l\'instant.</li>';
    document.getElementById("note-candidats-banc").innerHTML = !llm
        ? "Tous les modèles de la tâche passent, dans un ordre tiré au hasard que tu ne vois pas."
        : p.rendu ? `Chaque candidat réécrit les idées, puis ${echapper(p.rendu.nom)} les rend en image : c'est le modèle par défaut
            de Texte vers image. ${versModeles(p.rendu, true)}` : "";

    document.getElementById("titre-essais-banc").textContent = llm ? "Idées à réécrire" : "Jeu d'essai";
    document.getElementById("essais-banc").innerHTML = p.essais.map((e) =>
        `<span class="essai-banc" title="${echapper(e.prompt)}"><b>${echapper(e.id)}</b> <i>· ${echapper(e.teste)}</i></span>`).join("");
    document.getElementById("note-essais-banc").textContent = llm ? `Une seed de rendu : ${p.seeds[0]}.`
        : `${nombreDe(p.seeds.length, "seed")} par prompt : ${p.seeds.join(" et ")}. Les mêmes pour tous les modèles.`;
    document.getElementById("bloc-sources-banc").hidden = !p.sources.length;
    document.getElementById("sources-banc").innerHTML = p.sources.map((s) => `<figure>${s.presente
        ? `<img src="/api/sources/${encodeURIComponent(s.nom)}" alt="" loading="lazy">` : '<span class="ko">absente</span>'}
        <figcaption>${echapper(s.nom.replace(/\.png$/, ""))}</figcaption></figure>`).join("");

    const n = p.candidats.length;
    const compte = document.getElementById("compte-rendu-banc");
    compte.hidden = n < 2;
    compte.innerHTML = `<strong>${nombreDe(p.total, UNITES_BANC[tacheBanc] ?? "image")}</strong>
        <span>${llm ? `${nombreDe(n, "candidat")} × ${nombreDe(p.essais.length, "idée")} : une réécriture, puis un rendu`
            : `${nombreDe(n, "modèle")} × ${nombreDe(p.essais.length, "prompt")} × ${nombreDe(p.seeds.length, "seed")}`}</span>
        <span class="discret">Les téléchargements sont suspendus pendant le casting.</span>`;
    const blocage = document.getElementById("blocage-banc");
    blocage.hidden = !p.blocage;
    blocage.querySelector("span").textContent = p.blocage ? `${p.blocage}.` : "";
    document.getElementById("etat-lancer-banc").textContent = p.blocage ? ""
        : `Le casting démarre si moins de ${preparation.vram_max_occupee_go.toLocaleString("fr-FR")} Go de VRAM sont occupés.`;
    majLancer();
}

// « Ouvrir dans Modèles » sur un modèle incomplet : sa carte, dans l'onglet d'usage de sa tâche (web/modeles.js).
document.getElementById("carte-preparation").addEventListener("click", (e) => {
    const b = e.target.closest("[data-ouvrir-modele]");
    if (b) montrerWorkflow(b.dataset.ouvrirModele, "rendu" in b.dataset ? "texte_vers_image" : tacheBanc);
});

function afficherProgression(actif) {
    const bloc = document.getElementById("progression-banc");
    const erreur = document.getElementById("erreur-banc");
    const enCours = actif && ["attente", "en_cours"].includes(actif.etat);
    bloc.hidden = !enCours;
    bancEnCours = Boolean(enCours);
    majLancer();
    erreur.hidden = !(actif && actif.etat === "erreur");
    if (actif && actif.etat === "erreur") erreur.textContent = actif.erreur;
    if (enCours) {
        bloc.querySelector(".barre div").style.width = `${(100 * actif.fait / actif.total).toFixed(1)}%`;
        bloc.querySelector("span").textContent = `${actif.fait} / ${actif.total} · ${actif.courant || "préparation…"}`;
        if (!suiviBanc) suiviBanc = setInterval(suivreBanc, 2000);
    } else if (suiviBanc) {
        clearInterval(suiviBanc);
        suiviBanc = null;
        const nom = NOMS_TACHES[actif?.tache] ?? "Casting";
        if (actif?.etat === "erreur") notifierFin("Casting en erreur", actif.erreur || nom, "casting", false);
        else {
            notifierFin("Casting terminé", `${nom} · résultats à comparer`, "casting");
            afficherSousOngletCasting("comparer");  // les résultats sont là : c'est le moment de noter
        }
        chargerCasting();
    }
}

async function suivreBanc() {
    const { actif } = await api("GET", "/api/bancs?liste=false");  // le banc en cours seulement
    afficherProgression(actif);
}

document.getElementById("lancer-banc").addEventListener("click", async () => {
    const erreur = document.getElementById("erreur-banc");
    erreur.hidden = true;
    try {
        const actif = await api("POST", "/api/bancs", { tache: tacheBanc });
        bancAffiche = actif.id;
        afficherProgression(actif);
    } catch (err) {
        afficherErreur(erreur, err.message);
    }
});

async function chargerBanc() {
    afficherBanc(await api("GET", `/api/bancs/${bancAffiche}`));
}

// Affichage d'un banc

function etoiles(modele, g) {
    const boutons = [1, 2, 3, 4, 5].map((n) =>
        `<button class="${g.note >= n ? "allumee" : ""}" title="${n}/5"
            data-noter="${n}" data-cle="${echapper(modele.cle)}" data-fichier="${echapper(g.fichier)}">★</button>`).join("");
    const source = g.peut_servir_de_source
        ? (g.est_la_source
            ? '<span class="petit ok">source ✓</span>'
            : `<button class="petit" data-source="1" data-cle="${echapper(modele.cle)}"
                  data-fichier="${echapper(g.fichier)}">Utiliser comme source</button>`)
        : "";
    return `<div class="etoiles">${boutons}${source}</div>`;
}

// Après la révélation : les mesures de chaque modèle, et lequel sert par défaut. « Par défaut » est le mot de la page
// Modèles, et c'est le même réglage : `b.par_defaut` dit le défaut du moment, `b.favori` le choix fait dans ce casting
// (il a pu changer depuis).
function tableauResume(b) {
    const llm = b.tache === "reecriture";
    const moyennes = b.modeles.map((m) => {
        const notes = m.generations.map((g) => g.note).filter(Boolean);
        return notes.length ? notes.reduce((a, n) => a + n, 0) / notes.length : null;
    });
    const meilleure = Math.max(...moyennes.filter((v) => v !== null));
    const ligne = (m, i) => {
        const moyenne = moyennes[i] === null ? "—" : moyennes[i].toFixed(2);
        const r = m.resume ?? {};
        const l = m.liberation ?? {};
        const defaut = b.par_defaut === m.cle
            ? '<span class="ok">★ par défaut</span>'
            : `<button class="petit" data-favori="${echapper(m.cle)}">Choisir par défaut</button>`
              + (b.favori === m.cle ? ' <span class="discret">choisi ici, changé depuis</span>' : "");
        const mesures = llm
            ? `<td>${r.chargement_s ?? "—"} s</td><td>${r.ecriture_tps_moyen ?? "—"} tok/s</td>
               <td>${r.duree_moyenne_s ?? "—"} s</td><td>${r.pic_vram_go ?? "—"} Go</td><td>${r.controles_reussis ?? "—"}</td>`
            : `<td>${r.temps_froid_s ?? "—"} s</td><td>${r.temps_chaud_moyen_s ?? "—"} s</td>
               <td>${r.pic_ram_go ?? "—"} Go</td><td>${l.vram_avant_go ?? "—"} → ${l.vram_apres_go ?? "—"} Go</td>`;
        return `<tr><td>${echapper(m.nom)}</td><td class="${moyennes[i] === meilleure ? "meilleure-note" : ""}">${moyenne}</td>
            ${mesures}<td>${defaut}</td></tr>`;
    };
    const titres = llm
        ? "<th>Chargement</th><th>Écriture</th><th>Durée moyenne</th><th>Pic de VRAM</th><th>Contrôles</th>"
        : "<th>À froid</th><th>À chaud</th><th>Pic de RAM</th><th>VRAM avant → après libération</th>";
    return `<table class="resume">
        <thead><tr><th>Modèle</th><th>Note moyenne</th>${titres}<th>Par défaut</th></tr></thead>
        <tbody>${b.modeles.map(ligne).join("")}</tbody>
    </table>
    <p class="discret">${llm ? "Le LLM par défaut sert aux modèles qui n'ont pas le leur, au Scénario et aux traductions."
        : "Le modèle par défaut est celui que prennent le Plateau, le Montage et le Scénario pour cette tâche. Il se change aussi dans Modèles."}</p>`;
}

function detailsPrompt(g) {
    if (!g.texte) return "";
    const controles = Object.entries(CONTROLES)
        .filter(([cle]) => g.controles && g.controles[cle] !== null && g.controles[cle] !== undefined)
        .map(([cle, nom]) => `<span class="${g.controles[cle] ? "ok" : "ko"}">${g.controles[cle] ? "✓" : "✗"} ${nom}</span>`)
        .join(" · ");
    return `<p class="texte-prompt discret">${echapper(g.texte)}</p><p class="discret">${controles} · ${g.controles.mots} mots</p>`;
}

function afficherBanc(b) {
    const conteneur = document.getElementById("banc");
    // « Révéler les modèles » est l'action de l'onglet : le panneau qu'elle révèle est accentué tant qu'elle est là.
    document.getElementById("carte-banc").classList.toggle("accent", !b.revele);
    // L'en-tête reste visible pendant qu'on descend dans les résultats : où en sont les notes, un saut par prompt,
    // et « Révéler les modèles » sous la main.
    const duPrompt = (id) => b.modeles.flatMap((m) => m.generations.filter((g) => g.prompt === id));
    const sauts = b.prompts.map((p) => {
        const generations = duPrompt(p.id);
        const notees = generations.filter((g) => g.note).length;
        return `<button type="button" class="petit" data-saut="${echapper(p.id)}" title="Aller à ce prompt">${echapper(p.id)}
            ${notees === generations.length ? "✓" : `${notees}/${generations.length}`}</button>`;
    }).join("");
    const entete = `<div class="tete-banc">
        <div class="entete-banc">
            <h2>${NOMS_TACHES[b.tache] ?? echapper(b.tache)} <span class="discret">${dateBanc(b.date)}${b.revele ? "" : " · à l'aveugle"}${b.en_cours ? " · casting en cours…" : ""}</span></h2>
            ${b.revele ? "" : `<button id="reveler" class="principal" ${b.en_cours ? "disabled" : ""}>Révéler les modèles</button>`}
        </div>
        <div class="avancement-banc">
            <span>${b.notees} / ${b.total} ${unitesNotees(b.tache)}</span>
            <span class="jauge-notes"><span style="width:${b.total ? (100 * b.notees / b.total).toFixed(1) : 0}%"></span></span>
            <span class="discret">VRAM au repos : ${b.vram_base_go ?? "—"} Go</span>
        </div>
        <div class="sauts-banc">${sauts}</div>
    </div>
    ${b.revele ? tableauResume(b) : ""}`;

    const colonnes = b.modeles.length + (AVEC_SOURCE.includes(b.tache) ? 1 : 0);
    const blocs = b.prompts.map((p) => {
        const source = AVEC_SOURCE.includes(b.tache)
            ? `<div class="colonne"><h4>Source</h4><div class="vignette">
                <a href="/api/sources/${encodeURIComponent(p.source)}" target="_blank">
                <img src="/api/sources/${encodeURIComponent(p.source)}" alt="source" loading="lazy"></a></div></div>`
            : "";
        const modeles = b.modeles.map((m) => `<div class="colonne">
            <h4>${echapper(m.nom)} <span class="discret">${b.seeds.length > 1 ? "seeds" : "seed"} ${b.seeds.join(" · ")}</span></h4>
            <div class="seeds" style="grid-template-columns: repeat(${b.seeds.length}, 1fr)">
            ${m.generations.filter((g) => g.prompt === p.id).map((g) => `
                <div class="vignette ${g.est_la_source ? "source" : ""}">
                    ${media(g.url, `seed ${g.seed}`)}
                    ${etoiles(m, g)}
                    ${detailsPrompt(g)}
                </div>`).join("")}
            </div>
        </div>`).join("");
        const sert = duPrompt(p.id).some((g) => g.peut_servir_de_source)
            ? '<span class="sert-banc">sert d\'image de départ à la Retouche</span>' : "";
        return `<div class="bloc-prompt" data-prompt-banc="${echapper(p.id)}">
            <h3>${echapper(p.id)} <span class="discret">· ${echapper(p.teste)}</span>${sert}</h3>
            <p class="discret">${echapper(p.prompt)}</p>
            <div class="grille" style="grid-template-columns: repeat(${colonnes}, 1fr)">${source}${modeles}</div>
        </div>`;
    }).join("");

    conteneur.innerHTML = entete + blocs;
}

// Actions : notes, révélation, modèle par défaut, source

document.getElementById("banc").addEventListener("click", async (e) => {
    const cible = e.target.closest("button");
    if (!cible) return;
    if (cible.dataset.saut) {
        document.querySelector(`#banc [data-prompt-banc="${CSS.escape(cible.dataset.saut)}"]`)
            ?.scrollIntoView({ block: "start", behavior: "smooth" });
        return;
    }
    const url = `/api/bancs/${bancAffiche}`;
    try {
        if (cible.dataset.noter) {
            const b = await api("POST", `${url}/notes`, {
                cle: cible.dataset.cle, fichier: cible.dataset.fichier, note: Number(cible.dataset.noter),
            });
            const hauteur = window.scrollY;  // la page ne doit pas bouger sous le doigt pendant qu'on note
            afficherBanc(b);
            window.scrollTo(0, hauteur);
            majCarteBanc(b);
        } else if (cible.id === "reveler") {
            const b = await api("GET", url);
            // Révéler ne se défait pas : avant d'avoir tout noté, la fenêtre de l'appli le rappelle (web/commun.js).
            if (b.notees < b.total && !await demander({
                titre: "Révéler les modèles",
                texte: `Seulement ${b.notees} ${unitesNotees(b.tache)} sur ${b.total}.`,
                detail: "Une fois les noms affichés, ce casting n'est plus à l'aveugle : ça ne se défait pas. "
                    + "Tes notes restent, et tu peux encore noter après.",
                valider: "Révéler quand même",
                annuler: "Continuer à noter",
            })) return;
            afficherBanc(await api("POST", `${url}/reveler`));
            chargerCasting();
        } else if (cible.dataset.favori) {
            afficherBanc(await api("POST", `${url}/favori`, { workflow: cible.dataset.favori }));
        } else if (cible.dataset.source) {
            afficherBanc(await api("POST", `${url}/source`, { cle: cible.dataset.cle, fichier: cible.dataset.fichier }));
        }
    } catch (err) {
        alert(err.message);
    }
});

// Cartes à onglet : la tâche du prochain casting.
document.getElementById("taches-banc").addEventListener("change", (e) => {
    tacheBanc = e.target.value;
    document.getElementById("erreur-banc").hidden = true;
    dessinerPreparation();
});

// Cartes à onglet : le casting affiché.
document.getElementById("historique-bancs").addEventListener("change", (e) => {
    bancAffiche = e.target.value;
    chargerBanc();
});

document.getElementById("vers-nouveau-casting").addEventListener("click", () => afficherSousOngletCasting("lancer"));
