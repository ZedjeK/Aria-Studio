// Scénario, 🧭 Me guider (Image et Clip) : un déroulé fixe, en bulles, sans LLM. Aria Studio enchaîne les questions : le
// sujet, puis le décor et l'ambiance, puis la mise en scène (une question par liste), et range tes réponses dans le même
// état que les listes du Scénario (etatPrompt, scenario.js). À chaque étape : une puce, la liste complète, ta propre idée,
// Surprends-moi, Passer ou Revenir. À la fin, « Utiliser dans la conversation » les range dans le champ du chat.

const SUJETS_GUIDE = [["personnage", "Personnage"], ["creature", "Créature / animal"], ["objet", "Objet"],
    ["lieu", "Lieu et paysage"]];
const SECTIONS_GUIDE = { sujet: "👤 Sujet", decor: "🏞️ Décor et ambiance", scene: "🎥 Mise en scène" };
const RANG_SECTION = { sujet: 0, decor: 1, scene: 2 };
const PUCES_GUIDE_MAX = 8;  // les premières valeurs en puces, le reste dans la liste déroulante

// Où en est le déroulé de cet atelier : les réponses données, dans l'ordre.
// { id, onglet, titre, groupe, bloc, sl, genre: valeur | libre | passe | hasard, texte }
const guideDe = () => (atelier().guide ??= { fait: [], chemin: {}, toute: {} });

// Les listes rangées par catégorie (« Intérieur / Habitation ») se parcourent pas à pas : d'abord la catégorie, puis le
// niveau suivant, puis les valeurs. `chemin` : les catégories choisies pour cette étape ; `toute` : la liste entière.
const niveauxDe = (v) => (v.categorie ? v.categorie.split(" / ") : []);

function parcoursGuide(e) {
    const chemin = guideDe().chemin[e.id] ?? [];
    const restantes = e.valeurs.filter((v) => chemin.every((c, i) => niveauxDe(v)[i] === c));
    const cascade = e.valeurs.some((v) => v.categorie) && e.valeurs.length > PUCES_GUIDE_MAX && !guideDe().toute[e.id];
    const prochains = cascade ? [...new Set(restantes.map((v) => niveauxDe(v)[chemin.length]).filter(Boolean))] : [];
    return { chemin, restantes, prochains, cascade };
}

const oublierParcours = (id) => { delete guideDe().chemin[id]; delete guideDe().toute[id]; };

const valeursOffertes = (bloc, sl, etat, c) => sl.valeurs.filter((v) => (v.prompt || v.prompts_genre)
    && valeurPermise(v, c, bloc) && !enConflit(etat, bloc, sl.nom, v.nom)
    && !v.remplace.some((cible) => etat.choix[bloc]?.[cible]));

// Les questions encore possibles, dans l'ordre : elles se recalculent à chaque réponse (le genre, l'origine, la silhouette…
// changent les listes proposées). Le sujet vient des images (Mélanger, Redessiner) ou d'une idée libre : pas de questions
// du Sujet.
function etapesGuide() {
    const etapes = [];
    const fait = guideDe().fait;
    const sujetLibre = fait.some((f) => f.id === "sujet" && f.genre === "libre");
    const sansListesSujet = sansSujet() || sujetLibre;
    if (!sansSujet()) etapes.push({ id: "sujet", type: "sujet", onglet: "sujet", titre: "Qu'est-ce que tu veux créer ?",
                                    groupe: "" });
    if (!structurePrompt) return etapes;
    const etat = etatPrompt();
    const c = contextePrompt(etat);
    const candidates = [];
    structurePrompt.blocs.forEach((b) => {
        if (b.video && !cibleVideo()) return;
        b.groupes.forEach((g) => {
            if (sansListesSujet && g.onglet === "sujet") return;
            g.sous_listes.filter((sl) => !rubriquesIgnorees().has(sl.nom)).forEach((sl) => candidates.push({ bloc: b.id, groupe: g.nom, onglet: g.onglet, sl }));
        });
    });
    candidates.sort((x, y) => RANG_SECTION[x.onglet] - RANG_SECTION[y.onglet]);  // le tri garde l'ordre des accordéons
    candidates.forEach(({ bloc, groupe, onglet, sl }) => {
        const cond = sl.condition;
        if (cond && !(etat.choix[bloc]?.[cond.sous_liste] ?? "").toLowerCase().includes(cond.contient)) return;
        if (c.remplacees[cle(bloc, sl.nom)]) return;
        const valeurs = valeursOffertes(bloc, sl, etat, c);
        if (valeurs.length) etapes.push({ id: cle(bloc, sl.nom), type: "liste", bloc, groupe, onglet, sl, valeurs,
                                          titre: sl.nom });
    });
    if (formeScenario === "image") etapes.push({ id: "texte", type: "texte", onglet: "scene", groupe: "",
                                                 titre: "Un texte à écrire dans l'image ?" });
    return etapes;
}

const etapeCourante = () => etapesGuide().find((e) => !guideDe().fait.some((f) => f.id === e.id));

// Ce que tu as répondu, une ligne « liste : valeur » chacune, comme les propositions du chat.
function lignesGuide() {
    const etat = etatPrompt();
    return guideDe().fait.flatMap((f) => {
        if (f.genre === "passe" || (sansSujet() && f.onglet === "sujet")) return [];
        if (f.id === "sujet") return f.genre === "libre" ? [`Sujet : ${f.texte}`]
            : [`Sujet : ${SUJETS_GUIDE.find(([id]) => id === f.valeur)?.[1] ?? f.valeur}`];
        if (f.id === "texte") return [`Texte dans l'image : ${f.texte}`];
        if (f.genre === "libre") return [`${f.titre} : ${f.texte}`];
        const valeur = etat.choix[f.bloc]?.[f.sl];
        return valeur ? [`${f.titre} : ${valeur}`] : [];
    });
}

// ── Affichage ──────────────────────────────────────────────────────────────────────────────────────────────────────

const questionGuide = (e) => `<p><strong>${echapper(e.titre)}</strong>${
    e.groupe && e.groupe !== e.titre ? ` <span class="discret">· ${echapper(e.groupe)}</span>` : ""}</p>`;

function reponseGuide(f) {
    if (f.genre === "passe") return "Passer";
    if (f.genre === "libre") return f.texte;
    if (f.genre === "hasard") return `🎲 ${f.valeur}`;
    return f.id === "sujet" ? SUJETS_GUIDE.find(([id]) => id === f.valeur)?.[1] ?? f.valeur : f.valeur;
}

function commandesGuide(e) {
    const etat = etatPrompt();
    let puces = "", liste = "", libre = "";
    if (e.type === "sujet") {
        puces = SUJETS_GUIDE.map(([id, nom]) => `<button type="button" class="petit" data-guide-valeur="${id}">${
            echapper(nom)}</button>`).join("");
        libre = "Ou décris-le : un dragon, une moto…";
    } else if (e.type === "liste") {
        const actuelle = etat.choix[e.bloc]?.[e.sl.nom];
        const { chemin, restantes, prochains } = parcoursGuide(e);
        const parCategorie = guideDe().toute[e.id] && e.valeurs.some((v) => v.categorie);
        const retour = chemin.length ? `<button type="button" class="petit" data-guide-action="categorie-retour">↩ ${
            chemin.length > 1 ? echapper(chemin[chemin.length - 2]) : "Autre catégorie"}</button>`
            : parCategorie ? `<button type="button" class="petit" data-guide-action="categorie-retour">↩ Par catégorie</button>` : "";
        const fil = chemin.length ? `<p class="discret">${chemin.map(echapper).join(" › ")}</p>` : "";
        const valeursVues = prochains.length ? [] : restantes;
        if (prochains.length) {
            puces = prochains.map((cat) => `<button type="button" class="petit" data-guide-categorie="${echapper(cat)}">${
                echapper(cat)} <span class="discret">${restantes.filter((v) => niveauxDe(v)[chemin.length] === cat).length}</span></button>`).join("")
                + `<button type="button" class="petit" data-guide-action="toute">Toute la liste</button>`;
        } else {
            puces = valeursVues.slice(0, PUCES_GUIDE_MAX).map((v) => `<button type="button" class="petit${
                v.nom === actuelle ? " choisi" : ""}" data-guide-valeur="${echapper(v.nom)}">${echapper(v.nom)}</button>`).join("");
        }
        puces = fil + puces + retour;
        const reste = valeursVues.slice(PUCES_GUIDE_MAX);
        if (reste.length) {
            const categories = [...new Set(reste.map((v) => v.categorie).filter(Boolean))];
            const options = (vs) => vs.map((v) => `<option value="${echapper(v.nom)}">${echapper(v.nom)}</option>`).join("");
            liste = `<select class="liste-guide" aria-label="${echapper(e.titre)}">
                <option value="" selected disabled>Toute la liste (${reste.length} de plus)…</option>${categories.length
                ? categories.map((cat) => `<optgroup label="${echapper(cat)}">${options(reste.filter((v) =>
                    v.categorie === cat))}</optgroup>`).join("") + options(reste.filter((v) => !v.categorie))
                : options(reste)}</select>`;
        }
        libre = e.sl.nom === "Âge" ? "Ou un âge précis : 34…" : "Ou écris ta propre idée…";
    } else {
        libre = "Ex. ARIA, écrit en néon";
    }
    const hasard = e.type === "liste" ? `<button type="button" class="petit" data-guide-action="hasard"
        title="Le dé choisit une valeur compatible avec tes réponses">🎲 Surprends-moi</button>` : "";
    return `<div class="choix-libre">${puces}${liste}</div>
        <form class="libre-guide"><input type="text" maxlength="200" placeholder="${echapper(libre)}"
            aria-label="Ta propre idée"><button type="submit" class="petit">Valider</button></form>
        <div class="actions-etape">
            ${guideDe().fait.length ? `<button type="button" class="petit" data-guide-action="revenir">← Revenir</button>` : ""}
            ${hasard}<button type="button" class="petit" data-guide-action="passer">Passer →</button></div>`;
}

function recapGuide() {
    const lignes = lignesGuide();
    return `<p>${lignes.length ? "Voilà ce que j'ai noté :" : "Rien de noté pour l'instant."}</p>
        ${lignes.length ? `<ul class="recap-guide">${lignes.map((l) => `<li>${echapper(l)}</li>`).join("")}</ul>` : ""}
        <div class="actions-etape"><button type="button" class="petit" data-guide-action="revenir">← Revenir</button>
            <button type="button" class="petit" data-guide-action="refaire">↺ Tout refaire</button>
            <button type="button" class="principal" data-guide-action="utiliser"${lignes.length ? "" : " disabled"}
                title="Range tes réponses dans le champ de la conversation : le LLM écrira le prompt avec">Utiliser dans la conversation</button></div>`;
}

function rendreGuide() {
    const zone = document.getElementById("conversation-guide");
    if (!zone || !guidable()) return;
    const fait = guideDe().fait;
    const aide = AIDES_GUIDE[formeScenario === "clip" ? "clip" : refsScenario.role];
    let html = bulle("bulle-llm", "Guide", `<p>${echapper(aide)}</p><p class="discret">Je te pose les questions une à une. À chaque étape : une réponse de la liste, ta propre idée, ou Passer.</p>`);
    let section = null;
    const entete = (onglet) => (onglet === section ? "" : `<div class="section-guide">${SECTIONS_GUIDE[section = onglet]}</div>`);
    fait.forEach((f) => {
        html += entete(f.onglet) + bulle("bulle-llm", "Guide", questionGuide(f))
            + `<div class="message-toi">${bulle("bulle-toi", "Toi", `<p>${echapper(reponseGuide(f) ?? "")}</p>`)}</div>`;
    });
    const e = etapeCourante();
    html += e ? entete(e.onglet) + bulle("bulle-llm", "Guide", questionGuide(e) + (e.sl?.note ? `<p class="discret">${
        echapper(e.sl.note)}</p>` : "") + commandesGuide(e)) : bulle("bulle-llm", "Guide", recapGuide());
    zone.innerHTML = html;
    zone.scrollTop = zone.scrollHeight;
}

// ── Réponses ───────────────────────────────────────────────────────────────────────────────────────────────────────

const effacerChoixGuide = (f) => { if (f.bloc && f.sl) poserChoix(f.bloc, f.sl, ""); };

async function repondreGuide(e, genre, valeur = "", texte = "") {
    const etat = etatPrompt();
    const f = { id: e.id, onglet: e.onglet, titre: e.titre, groupe: e.groupe, bloc: e.bloc, sl: e.sl?.nom, genre,
                valeur, texte };
    if (e.type === "sujet") {
        if (genre === "valeur") await choisirSujet(valeur);
    } else if (e.type === "texte") {
        etat.texte = texte;
        document.getElementById("texte-prompt").value = texte;
    } else {
        if (genre === "valeur" || genre === "hasard") poserChoix(e.bloc, e.sl.nom, valeur);
        else poserChoix(e.bloc, e.sl.nom, "");
        const age = Number(texte);
        if (genre === "libre" && e.bloc === "personnage" && e.sl.nom === "Âge" && Number.isInteger(age)
            && age >= structurePrompt.age_precis[0] && age <= structurePrompt.age_precis[1]) {
            etat.age = age;  // un âge précis : il remplace la tranche
            const champ = document.getElementById("age-prompt");
            if (champ) champ.value = age;
            appliquerReglesPrompt();
        }
    }
    guideDe().fait.push(f);
    oublierParcours(e.id);
    planifierAssemblage();
    rendreGuide();
}

function revenirGuide() {
    const f = guideDe().fait.pop();
    if (!f) return;
    if (f.id === "texte") { etatPrompt().texte = ""; document.getElementById("texte-prompt").value = ""; }
    else effacerChoixGuide(f);
    oublierParcours(f.id);
    planifierAssemblage();
    rendreGuide();
}

function refaireGuide() {
    while (guideDe().fait.length) {
        const f = guideDe().fait.pop();
        if (f.id === "texte") { etatPrompt().texte = ""; document.getElementById("texte-prompt").value = ""; }
        else effacerChoixGuide(f);
    }
    guideDe().chemin = {};
    guideDe().toute = {};
    planifierAssemblage();
    rendreGuide();
}

function hasardGuide(e) {
    const etat = etatPrompt();
    const c = contextePrompt(etat);
    const dansLeParcours = new Set(parcoursGuide(e).restantes.map((v) => v.nom));  // reste dans la catégorie choisie
    const candidates = valeursOffertes(e.bloc, e.sl, etat, c)
        .filter((v) => v.nom !== etat.choix[e.bloc]?.[e.sl.nom] && dansLeParcours.has(v.nom));
    if (!candidates.length) return repondreGuide(e, "passe");
    const v = candidates[Math.floor(Math.random() * candidates.length)].nom;
    return repondreGuide(e, "hasard", v);
}

function utiliserGuide() {
    const champ = document.getElementById("message-libre");
    const lignes = lignesGuide();
    if (!lignes.length) return;
    const deja = champ.value.split("\n").map((l) => l.trim()).filter(Boolean);
    champ.value = [...deja, ...lignes.filter((l) => !deja.includes(l))].join("\n");
    document.getElementById("erreur-libre").hidden = true;
    choisirOngletIdee("conversation");
}

const zoneGuide = document.getElementById("conversation-guide");

zoneGuide.addEventListener("click", (ev) => {
    const e = etapeCourante();
    const valeur = ev.target.closest("[data-guide-valeur]");
    if (valeur && e) return repondreGuide(e, "valeur", valeur.dataset.guideValeur);
    const categorie = ev.target.closest("[data-guide-categorie]");
    if (categorie && e) {
        (guideDe().chemin[e.id] ??= []).push(categorie.dataset.guideCategorie);
        return rendreGuide();
    }
    const action = ev.target.closest("[data-guide-action]")?.dataset.guideAction;
    if (action === "passer" && e) return repondreGuide(e, "passe");
    if (action === "hasard" && e) return hasardGuide(e);
    if (action === "categorie-retour" && e) {
        if (guideDe().chemin[e.id]?.length) guideDe().chemin[e.id].pop();
        else delete guideDe().toute[e.id];
        return rendreGuide();
    }
    if (action === "toute" && e) { oublierParcours(e.id); guideDe().toute[e.id] = true; return rendreGuide(); }
    if (action === "revenir") return revenirGuide();
    if (action === "refaire") return refaireGuide();
    if (action === "utiliser") return utiliserGuide();
});

zoneGuide.addEventListener("change", (ev) => {
    const liste = ev.target.closest("select.liste-guide");
    const e = etapeCourante();
    if (liste?.value && e) repondreGuide(e, "valeur", liste.value);
});

zoneGuide.addEventListener("submit", (ev) => {
    ev.preventDefault();
    const e = etapeCourante();
    const texte = ev.target.querySelector("input").value.trim();
    if (e && texte) repondreGuide(e, "libre", "", texte);
});
