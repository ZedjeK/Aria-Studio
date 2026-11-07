// Scénario : la seule place où le LLM écrit. Quatre ateliers sur le même gabarit (Image, Retouche, Clip, Séquence) :
// 1 Départ (images de référence, references.js), 2 Ton idée, le prompteur libre (✨ Optimiser, 💬 Aide : libre.js),
// 3 Compléments (cases de l'atelier et listes à cliquer de prompt/listes), et à droite le prompt final, modifiable,
// qui part tel quel vers le Plateau, le panneau de retouche ou le Montage. Chaque atelier garde son brouillon.
// Les règles des listes (genre, âge, remplacements, incompatibilités) sont appliquées ici en direct pour guider,
// mais c'est le serveur qui fait foi : chaque changement repasse par /api/prompt/assembler.

const LIBELLES_SUJETS = { personnage: "👤 Personnage", creature: "🐉 Créature / animal", objet: "🏺 Objet",
                          lieu: "🚫 Aucun : un lieu, un paysage" };
const BROUILLON = "aria-scenario";  // brouillon gardé dans ce navigateur (une simple commodité)
let structurePrompt = null;
let sujetPrompt = "personnage";
let formeScenario = "image";  // l'atelier : image, retouche, clip ou sequence
let ongletListe = "sujet";
const ciblesParForme = { image: null, video: null, inspirer: null, variation: null, retouche: null };  // modèle cible
const ateliers = {};  // atelier → { idee, final (prompt final), echanges (chat d'aide), dernier (son dernier tour) }
const atelier = (nom = formeScenario) => (ateliers[nom] ??= { idee: "", final: "", echanges: [], dernier: null,
                                                            llm: null });  // llm : celui qui a écrit le prompt final

const IDEES = {
    image: ["2 · Ton idée", "Ton idée, en français, comme elle vient : une samouraï androïde sous la pluie à Tokyo, "
        + "ambiance néon…"],
    retouche: ["2 · Ta retouche", "Ce que tu veux changer sur l'image : remplace le ciel par un coucher de soleil, "
        + "garde tout le reste…"],
    clip: ["2 · Ton clip", "Ce qui se passe, en français : elle tourne lentement la tête vers la caméra et sourit…"],
    sequence: ["2 · Le fil de l'histoire (facultatif)", "En une à trois phrases, ce qui se passe du début à la fin : "
        + "une pilote rentre au hangar et retrouve son mécanicien… ✨ Tout écrire le répartit dans les plans."],
};
const ENVOIS = { image: ["→ Envoyer au Plateau", "Créer"], retouche: ["→ Panneau de retouche", "Retoucher"],
                 clip: ["→ Envoyer au Montage", "Tourner"] };

// Les modèles proposés : vidéo pour un clip ou une séquence, d'édition pour une retouche ; pour une image, d'édition
// quand on s'inspire d'images (references.js), qui savent varier une image pour une variation, sinon texte vers image.
function tacheCible() {
    const w = optionsPlateau.workflows;
    if (cibleVideo()) return ["video", w.texte_vers_video];
    if (formeScenario === "retouche") return ["retouche", w.retouche];
    const n = imagesReference().length;
    if (n && refsScenario.role === "inspirer") {
        return ["inspirer", { ...w.retouche, defaut: optionsPlateau.references_defaut,
                              choix: w.retouche.choix.filter((x) => x.references >= n) }];
    }
    if (n) return ["variation", { ...w.texte_vers_image, choix: w.texte_vers_image.choix.filter((x) => x.variation) }];
    return ["image", w.texte_vers_image];
}
// sujet -> { choix: {bloc: {sous-liste: valeur}}, age, texte, toutes: lève le filtre par origine,
//            replique, dialogue, avecMusique, musique, traductionMusique : les cases 🗣 et 🎵 d'un clip }
const etatsPrompt = {};
let definitionsPrompt = {};  // "bloc|sous-liste" -> sous-liste de la structure
let incompatPrompt = new Map();  // "bloc|sous-liste|valeur" -> [[bloc, sous-liste, valeur], …]
const groupesOuvertsPrompt = new Set();
let assemblagePrompt = null;
let occupePrompt = false;
let minuteurPrompt = null;
let numeroAssemblage = 0;
let retraitsPrompt = [];
let promptPret = false;
let personnagesEnregistres = [];

const etatPrompt = () => (etatsPrompt[sujetPrompt] ??=
    { choix: {}, age: null, texte: "", replique: "",
      toutes: false, dialogue: false, avecMusique: false, musique: "", traductionMusique: null });
const cle = (...morceaux) => morceaux.join("|");
// Un LLM par atelier ; par défaut celui du modèle cible (onglet Modèles), sinon le défaut. 👁 : il voit les images.
const llmsChoisis = {};  // atelier → GGUF choisi à la main (gardé dans le brouillon)
const llmAtelier = () => document.getElementById("llm-atelier").value || null;
const nomLlm = () => document.getElementById("llm-atelier").selectedOptions[0]?.textContent
    .replace(/ [★👁]/g, "").trim() || "Le LLM";

function remplirLlms() {
    if (!optionsPlateau) return;
    const voulu = llmsChoisis[formeScenario] ?? llmDuWorkflow(ciblePrompt());
    document.getElementById("llm-atelier").innerHTML = optionsPlateau.llm.choix.map((m) =>
        `<option value="${echapper(m.fichier)}" ${m.fichier === voulu ? "selected" : ""}>${echapper(m.nom)}`
        + `${m.favori ? " ★" : ""}${m.vision ? " 👁" : ""}</option>`).join("");
}
const ciblePrompt = () => document.getElementById("workflow-prompt").value || null;
const cibleVideo = () => formeScenario === "clip" || formeScenario === "sequence";

let preparationScenario = Promise.resolve();

function preparerScenario() {  // appelée à chaque ouverture de l'onglet
    preparationScenario = preparer();
    return preparationScenario;
}

// Ouvre le Scénario sur un atelier, avec une image : « ✍️ L'écrire au Scénario », « 🎞️ Animer », « 🖌️ Retoucher ».
async function ouvrirScenario(forme, { animer = null, retoucher = null } = {}) {
    document.querySelector('.onglet[data-vue="scenario"]').click();
    await preparationScenario;  // le brouillon est relu d'abord : il ne doit pas écraser ce choix
    if (animer) refsScenario.animer = animer;
    if (retoucher) refsScenario.retouche = retoucher;
    await choisirAtelier(forme);
}

async function preparer() {
    if (!promptPret) {  // posé à la fin : un échec (serveur occupé) laisse réessayer au prochain affichage
        const { sujets } = await api("GET", "/api/prompt/listes");
        document.getElementById("sujets-prompt").innerHTML = sujets.map((s) =>
            `<button type="button" data-sujet="${echapper(s.id)}">${echapper(LIBELLES_SUJETS[s.id] ?? s.nom)}</button>`).join("");
        if (!optionsPlateau) await chargerOptionsPlateau();
        relireBrouillon();
        promptPret = true;
    }
    await chargerModelesPrompt();
    await chargerPersonnages();
    await choisirSujet(sujetPrompt);  // relit les listes : une liste modifiée est prise en compte
}

async function chargerModelesPrompt() {
    if (!optionsPlateau) await chargerOptionsPlateau();
    const [cle_, tache] = tacheCible();
    const garde = ciblesParForme[cle_]
        ?? (cle_ === "image" ? choixModele("workflow-creation").workflow || tache?.defaut : tache?.defaut);
    remplirSelect("workflow-prompt", (tache?.choix ?? []).map((w) =>
        [w.id, `${w.nom}${w.id === tache.defaut ? " ★" : ""}`]), garde);
    ciblesParForme[cle_] = document.getElementById("workflow-prompt").value || null;
    remplirSelect("duree-prompt", optionsPlateau.video.durees.map((d) => [String(d), `${d} s`]),
                  document.getElementById("duree-prompt").value || "5");
    remplirLlms();
    majAtelier();
}

async function choisirAtelier(nom) {
    formeScenario = nom;
    await chargerModelesPrompt();  // ses modèles, son LLM, puis tout l'affichage de l'atelier
    planifierAssemblage();
}

// L'atelier en cours : ce qui s'affiche (data-atelier-seul, data-atelier-sauf), son idée, son prompt final, son chat.
function majAtelier() {
    const a = atelier();
    document.querySelectorAll(".onglet-scenario[data-atelier]").forEach((b) =>
        b.classList.toggle("actif", b.dataset.atelier === formeScenario));
    document.querySelectorAll("[data-atelier-seul]").forEach((e) => { e.hidden = e.dataset.atelierSeul !== formeScenario; });
    document.querySelectorAll("[data-atelier-sauf]").forEach((e) => { e.hidden = e.dataset.atelierSauf === formeScenario; });
    const [titre, exemple] = IDEES[formeScenario];
    document.getElementById("titre-idee").textContent = titre;
    const idee = document.getElementById("idee-scenario");
    idee.placeholder = exemple;
    idee.value = a.idee;
    document.getElementById("optimiser-scenario").textContent = formeScenario === "sequence" ? "✨ Tout écrire" : "✨ Optimiser";
    document.getElementById("numero-complements").textContent = formeScenario === "sequence" ? "4" : "3";
    document.getElementById("titre-final").textContent = formeScenario === "sequence" ? "Ta séquence" : "Prompt final";
    document.getElementById("tel-quel-scenario").hidden = formeScenario === "sequence";
    document.getElementById("facultatif-depart").textContent = formeScenario === "retouche"
        ? "(l'image à retoucher)" : "(facultatif)";
    if (ENVOIS[formeScenario]) {
        const [envoyer_, lancer] = ENVOIS[formeScenario];
        document.getElementById("envoyer-prompt").textContent = envoyer_;
        document.getElementById("creer-prompt").textContent = lancer;
    }
    document.getElementById("prompt-final").value = a.final;
    document.getElementById("chat-libre").hidden = !conversationActive().echanges.length;  // libre.js
    rendreLibre();
    rendreEtiquettes();
    rendreReferencesScenario();  // references.js
    majForme();  // Séquence (decoupage.js)
    majFinal();
}

function choisirListe(onglet) {
    ongletListe = onglet;
    document.querySelectorAll(".onglet-liste").forEach((b) => b.classList.toggle("actif", b.dataset.liste === onglet));
    document.querySelectorAll(".volet-liste").forEach((v) => { v.hidden = v.dataset.voletListe !== onglet; });
}

// Le prompt final : ce qui part, tel quel. ✨ Optimiser, 💬 Aide ou « Utiliser tel quel » le posent ; tu le retouches.
function poserFinal(texte, llm = null) {
    atelier().final = texte;
    atelier().llm = llm;
    document.getElementById("prompt-final").value = texte;
    majFinal();
    garderBrouillon();
}

// Ce que le Plateau, le panneau de retouche ou le Montage ajouteront au prompt (leurs propres réglages).
function majPartiraAvec() {
    const texteDe = (id) => document.getElementById(id)?.selectedOptions?.[0]?.textContent?.trim() ?? "";
    const actives = (id) => document.querySelectorAll(`#${id} .lora input[type=checkbox]:checked`).length;
    const loras = (id) => (actives(id) ? ` · ${actives(id)} LoRA` : "");
    const lignes = {
        image: () => `${texteDe("nombre-creation") || 1} image(s) · ${texteDe("format-creation") || "format du Plateau"}`
            + `${loras("loras-creation")}`,
        retouche: () => `le masque et le fondu se règlent au panneau de retouche${loras("loras-retouche")}`,
        clip: () => `${texteDe("nombre-montage") || 1} clip(s) · ${dureeClip()} s · ${texteDe("qualite-montage")
            .split(" · ")[0] || "qualité du Montage"}${loras("loras-montage")}`,
    };
    const ligne = lignes[formeScenario];
    document.getElementById("partira-avec").textContent = ligne
        ? `Partira avec : ${ligne()} (réglages du ${formeScenario === "clip" ? "Montage" : "Plateau"})` : "";
}

function majFinal() {
    majPartiraAvec();
    const texte = atelier().final.trim();
    const mots = compterMots(texte);
    const max = assemblagePrompt?.mots_max;
    const compte = document.getElementById("mots-final");
    compte.textContent = mots ? `${mots} mots${max ? ` / ${max} conseillés` : ""}` : "";
    compte.classList.toggle("ko", Boolean(max) && mots > max);
    majBoutonsPrompt();
}

// Ce que les compléments imposent (prompt assemblé des listes et des cases) : donné au LLM comme déjà fixé.
// Un clip le donne toujours (ses cases disent qui parle et s'il y a de la musique) ; sinon, s'il y a un choix.
function complements() {
    if (!assemblagePrompt || formeScenario === "retouche") return "";
    const e = etatPrompt();
    const choisi = Object.values(e.choix).some((v) => Object.keys(v ?? {}).length) || e.texte?.trim();
    return choisi || formeScenario === "clip" ? assemblagePrompt.prompt : "";
}

// Brouillon : l'atelier, les cibles, les choix de chaque sujet, chaque atelier (idée, chat, prompt final), l'histoire.
let minuteurBrouillon = null;

function garderBrouillon() {
    clearTimeout(minuteurBrouillon);
    minuteurBrouillon = setTimeout(() => {
        const etat = { forme: formeScenario, sujet: sujetPrompt, liste: ongletListe, cibles: ciblesParForme,
                       etats: etatsPrompt, llms: llmsChoisis, references: refsScenario, ateliers,
                       sequence: { reference: decoupage.reference, contrainte: decoupage.contrainte,
                                   contrainteEn: decoupage.contrainteEn, plans: decoupage.plans, alertes: decoupage.alertes,
                                   avecMusique: decoupage.avecMusique, musique: decoupage.musique,
                                   sequenceId: decoupage.sequenceId,
                                   titre: document.getElementById("titre-sequence-scenario").value } };
        try {
            localStorage.setItem(BROUILLON, JSON.stringify(etat));
            document.getElementById("brouillon-scenario").textContent = "✓ brouillon gardé";
        } catch {
            document.getElementById("brouillon-scenario").textContent = "";  // stockage refusé : on fait sans
        }
    }, 400);
}

function relireBrouillon() {
    let b = null;
    try {
        b = JSON.parse(localStorage.getItem(BROUILLON) ?? "null");
    } catch {
        return;
    }
    if (!b) return;
    if (ENVOIS[b.forme] || b.forme === "sequence") formeScenario = b.forme;
    if (b.sujet && LIBELLES_SUJETS[b.sujet]) sujetPrompt = b.sujet;
    Object.assign(ciblesParForme, b.cibles ?? {});
    Object.assign(etatsPrompt, b.etats ?? {});
    Object.assign(refsScenario, b.references ?? {});
    Object.assign(llmsChoisis, b.llms ?? {});
    Object.entries(b.ateliers ?? {}).forEach(([nom, a]) => { ateliers[nom] = { ...atelier(nom), ...a }; });
    const s = b.sequence ?? {};
    if (s.synopsis && !atelier("sequence").idee) atelier("sequence").idee = s.synopsis;  // avant : un champ à part
    decoupage.reference = s.reference ?? null;
    decoupage.contrainte = s.contrainte ?? "";
    decoupage.contrainteEn = s.contrainteEn ?? null;
    decoupage.alertes = Array.isArray(s.alertes) ? s.alertes : [];
    decoupage.sequenceId = s.sequenceId ?? null;
    document.getElementById("titre-sequence-scenario").value = s.titre ?? "";
    if (Array.isArray(s.plans) && s.plans.length >= 2) {
        decoupage.plans = s.plans.map(normaliserPlan);  // decoupage.js : les anciens plans n'avaient pas de réplique
        decoupagePret = false;  // preparerDecoupage remplit les listes sans écraser ces plans
    }
    // Avant : une case 🗣 pour tout le film et ses répliques ; elles rejoignent le fil, rien ne se perd.
    if (s.dialogue && (s.repliques ?? "").trim() && !decoupage.plans.some((p) => p.replique.trim())) {
        const lignes = s.repliques.split("\n").map((l) => l.trim()).filter(Boolean).map((l) => `« ${l} »`).join(" ");
        atelier("sequence").idee = `${atelier("sequence").idee.trim()}\nRépliques : ${lignes}`.trim();
        garderBrouillon();  // réécrit sans l'ancienne case : pas d'ajout en double au prochain chargement
    }
    Object.assign(decoupage, { avecMusique: Boolean(s.avecMusique), musique: s.musique ?? "" });
    document.getElementById("contrainte-scenario").value = decoupage.contrainte;
    document.getElementById("musique-active-sequence").checked = decoupage.avecMusique;
    document.getElementById("musique-sequence").value = decoupage.musique;
    majCases();
    choisirListe(b.liste ?? "sujet");
    document.getElementById("brouillon-scenario").textContent = "✓ brouillon repris";
}

async function choisirSujet(sujet) {
    sujetPrompt = sujet;
    document.querySelectorAll("#sujets-prompt button").forEach((b) => b.classList.toggle("actif", b.dataset.sujet === sujet));
    document.getElementById("personnages-prompt").hidden = sujet !== "personnage";
    document.getElementById("sans-sujet").hidden = sujet !== "lieu";
    try {
        structurePrompt = await api("GET", `/api/prompt/listes?sujet=${encodeURIComponent(sujet)}`);
    } catch (err) {
        document.querySelector('[data-onglet-blocs="sujet"]').innerHTML = `<p class="ko">${echapper(err.message)}</p>`;
        return;
    }
    definitionsPrompt = {};
    structurePrompt.blocs.forEach((b) => b.groupes.forEach((g) => g.sous_listes.forEach((sl) => {
        definitionsPrompt[cle(b.id, sl.nom)] = sl;
    })));
    incompatPrompt = new Map();
    structurePrompt.incompatibilites.forEach(([a, b]) => {
        [[a, b], [b, a]].forEach(([x, y]) => {
            if (!incompatPrompt.has(cle(...x))) incompatPrompt.set(cle(...x), []);
            incompatPrompt.get(cle(...x)).push(y);
        });
    });
    rendreBlocsPrompt();
    appliquerReglesPrompt();
    planifierAssemblage();
}

// Squelette : un accordéon par groupe, rangé dans l'onglet que le serveur lui donne (sujet, decor, scene) ;
// les options sont remplies par appliquerReglesPrompt.
function rendreBlocsPrompt() {
    let numero = 0;
    const parOnglet = { sujet: [], decor: [], scene: [] };
    structurePrompt.blocs.forEach((b) => b.groupes.forEach((g) => (parOnglet[g.onglet] ?? parOnglet.sujet).push([b, g])));
    Object.entries(parOnglet).forEach(([onglet, groupes]) => {
        // Le premier accordéon de chaque onglet s'ouvre de lui-même, tant qu'aucun ne l'est.
        if (groupes.length && !groupes.some(([b, g]) => groupesOuvertsPrompt.has(cle(sujetPrompt, b.id, g.nom)))) {
            groupesOuvertsPrompt.add(cle(sujetPrompt, groupes[0][0].id, groupes[0][1].nom));
        }
    });
    Object.entries(parOnglet).forEach(([onglet, groupes]) => {
        document.querySelector(`[data-onglet-blocs="${onglet}"]`).innerHTML = groupes.map(([b, g]) => {
            const id = cle(sujetPrompt, b.id, g.nom);
            return `<details data-groupe="${echapper(id)}" ${groupesOuvertsPrompt.has(id) ? "open" : ""}
                     ${b.video ? `data-atelier-seul="clip" ${formeScenario === "clip" ? "" : "hidden"}` : ""}>
                <summary>${b.video ? "🎞️ " : ""}${echapper(g.nom)} <span class="discret compte"></span></summary>
                <div class="sous-listes">${g.sous_listes.map((sl) => `
                    <div class="sous-liste" data-bloc="${echapper(b.id)}" data-sl="${echapper(sl.nom)}">
                        <div class="entete-sl">
                            <label for="sl-${++numero}">${echapper(sl.nom)}</label>
                            <button type="button" class="de" title="Tirer au hasard (dans la catégorie choisie)">🎲</button>
                        </div>
                        ${sl.valeurs.some((v) => v.categorie)
                            ? `<select class="categorie-sl" aria-label="Catégorie : ${echapper(sl.nom)}"></select>` : ""}
                        <select id="sl-${numero}" class="valeur-sl"></select>
                        ${b.id === "personnage" && sl.nom === "Âge"
                            ? `<input id="age-prompt" class="age-precis" type="number" min="${structurePrompt.age_precis[0]}"
                                      max="${structurePrompt.age_precis[1]}" placeholder="âge précis (facultatif)">`
                            : ""}
                        ${b.id === "personnage" && sl.nom === "Origine"
                            ? `<label class="case discret" title="Sinon, le teint, les yeux et le nez suivent l'origine choisie">
                                   <input type="checkbox" id="toutes-options"> Toutes les options (teint, yeux, nez)</label>`
                            : ""}
                        <small class="raison"></small>
                    </div>`).join("")}
                </div>
            </details>`;
        }).join("");
    });
    const etat = etatPrompt();
    const age = document.getElementById("age-prompt");
    if (age) age.value = etat.age ?? "";
    const toutes = document.getElementById("toutes-options");
    if (toutes) toutes.checked = etat.toutes;
    document.getElementById("texte-prompt").value = etat.texte;
    document.getElementById("replique-prompt").value = etat.replique;
    document.getElementById("dialogue-prompt").checked = Boolean(etat.dialogue);
    document.getElementById("musique-active-prompt").checked = Boolean(etat.avecMusique);
    document.getElementById("musique-prompt").value = etat.musique ?? "";
    majCases();
}

// Cases 🗣 et 🎵 (clip, musique d'une séquence) : cochée, son champ apparaît ; décochée, la phrase qui dit ce qui se
// passe. Le dialogue d'une séquence se coche plan par plan (decoupage.js).
function majCases() {
    [["dialogue-prompt", "replique-prompt"], ["musique-active-prompt", "musique-prompt"],
     ["musique-active-sequence", "musique-sequence"]].forEach(([c, champ]) => {
        const coche = document.getElementById(c).checked;
        document.getElementById(champ).hidden = !coche;
        document.querySelector(`[data-sans="${c}"]`).hidden = coche;
    });
}

function contextePrompt(etat) {
    const perso = etat.choix.personnage ?? {};
    const genre = perso.Genre ? perso.Genre.toLowerCase() : null;
    const tranche = definitionsPrompt[cle("personnage", "Âge")]?.valeurs.find((v) => v.nom === perso["Âge"]);
    const mineur = Boolean(tranche?.mineur) || (etat.age !== null && etat.age < 18);
    const origine = definitionsPrompt[cle("personnage", "Origine")]?.valeurs.find((v) => v.nom === perso.Origine);
    const remplacees = {};
    Object.entries(etat.choix).forEach(([bloc, valeurs]) => Object.entries(valeurs).forEach(([nom, choisie]) => {
        const v = definitionsPrompt[cle(bloc, nom)]?.valeurs.find((x) => x.nom === choisie);
        v?.remplace.forEach((cible) => { remplacees[cle(bloc, cible)] = choisie; });
    }));
    return { genre, mineur, remplacees, familles: etat.toutes ? [] : (origine?.familles ?? []),
             choix: etat.choix };
}

// Origine : une aide au choix (teint, yeux, nez), pas une règle ; « Toutes les options » vide c.familles.
const selonOrigine = (v, c) => !c.familles.length || !v.origines?.length || v.origines.some((f) => c.familles.includes(f));

// « Avec <sous-liste> » : une règle (la poitrine selon la silhouette, la coiffure selon la longueur) ; « Toutes » ne la lève pas.
const coherente = (v, c, bloc) => Object.entries(v.avec ?? {})
    .every(([sl, noms]) => !c.choix[bloc]?.[sl] || noms.includes(c.choix[bloc][sl]));

const valeurPermise = (v, c, bloc) => !(c.mineur && v.adulte)
    && !(v.genre && ["femme", "homme"].includes(c.genre) && v.genre !== c.genre)
    && coherente(v, c, bloc)
    && selonOrigine(v, c);

// Catégorie affichée par sous-liste (sujet|bloc|sous-liste) : un filtre d'affichage, jamais envoyé au serveur.
const categoriesAffichees = {};

function categorieAffichee(bloc, sl, categories) {
    const choisie = categoriesAffichees[cle(sujetPrompt, bloc, sl)] ?? "";
    return categories.includes(choisie) ? choisie : "";  // une catégorie disparue (mineur…) revient à « Toutes »
}

const enConflit = (etat, bloc, sl, nom) => (incompatPrompt.get(cle(bloc, sl, nom)) ?? [])
    .find(([b, s, val]) => etat.choix[b]?.[s] === val);

function appliquerReglesPrompt() {
    const etat = etatPrompt();
    const divs = [...document.querySelectorAll("#blocs-prompt .sous-liste[data-bloc]")];
    // Retirer une valeur peut en libérer ou en masquer d'autres (motif de tatouage…) : on recommence jusqu'à stabilité.
    for (let passe = 0, change = true; change && passe < 5; passe++) {
        change = false;
        const c = contextePrompt(etat);
        divs.forEach((div) => {
            const { bloc, sl } = div.dataset;
            const def = definitionsPrompt[cle(bloc, sl)];
            const choix = etat.choix[bloc] ?? {};
            const visibles = def.valeurs.filter((v) => valeurPermise(v, c, bloc));
            const conditionKo = def.condition
                && !(choix[def.condition.sous_liste] ?? "").toLowerCase().includes(def.condition.contient);
            const cachee = !visibles.length || (c.mineur && def.adulte) || conditionKo;
            div.hidden = cachee;
            const actuelle = choix[sl];
            if (actuelle && (cachee || !visibles.some((v) => v.nom === actuelle))) {
                delete choix[sl];
                retraitsPrompt.push(`${sl} : « ${actuelle} »`);
                change = true;
            }
        });
    }

    const c = contextePrompt(etat);
    divs.forEach((div) => {
        if (div.hidden) return;
        const { bloc, sl } = div.dataset;
        const def = definitionsPrompt[cle(bloc, sl)];
        const actuelle = etat.choix[bloc]?.[sl] ?? "";
        const select = div.querySelector(".valeur-sl");
        const option = (v) => {
            const conflit = enConflit(etat, bloc, sl, v.nom);
            return `<option value="${echapper(v.nom)}" title="${echapper(v.description || v.prompt)}"
                ${v.nom === actuelle ? "selected" : ""} ${conflit && v.nom !== actuelle ? "disabled" : ""}>
                ${echapper(v.nom)}${conflit ? ` ⛔ ${echapper(conflit[2])}` : ""}</option>`;
        };
        const permises = def.valeurs.filter((v) => valeurPermise(v, c, bloc));
        // Catégories (Pose, Action) : un premier menu choisit la catégorie, le second n'en montre que les valeurs.
        const categories = [...new Set(permises.map((v) => v.categorie).filter(Boolean))];
        const menuCategorie = div.querySelector(".categorie-sl");
        let affichees = permises;
        if (menuCategorie) {
            const choisie = categorieAffichee(bloc, sl, categories);
            menuCategorie.innerHTML = '<option value="">Toutes les catégories</option>' + categories.map((cat) =>
                `<option value="${echapper(cat)}" ${cat === choisie ? "selected" : ""}>${echapper(cat)} `
                + `(${permises.filter((v) => v.categorie === cat).length})</option>`).join("");
            if (choisie) {
                affichees = permises.filter((v) => v.categorie === choisie);
                const hors = permises.find((v) => v.nom === actuelle && v.categorie !== choisie);
                if (hors) affichees = [hors, ...affichees];  // le choix fait reste visible
            }
        }
        const groupesVisibles = [...new Set(affichees.map((v) => v.categorie).filter(Boolean))];
        select.innerHTML = '<option value="">—</option>' + (groupesVisibles.length > 1
            ? groupesVisibles.map((cat) => `<optgroup label="${echapper(cat)}">${affichees
                .filter((v) => v.categorie === cat).map(option).join("")}</optgroup>`).join("")
              + affichees.filter((v) => !v.categorie).map(option).join("")
            : affichees.map(option).join(""));
        const remplacant = c.remplacees[cle(bloc, sl)];
        select.disabled = Boolean(remplacant);
        if (menuCategorie) menuCategorie.disabled = Boolean(remplacant);
        div.querySelector(".de").disabled = Boolean(remplacant);
        div.classList.toggle("choisie", Boolean(actuelle) && !remplacant);
        div.querySelector(".raison").textContent = remplacant ? `remplacé par « ${remplacant} »`
            : (def.valeurs.find((v) => v.nom === actuelle)?.description ?? "");
    });

    document.querySelectorAll("#blocs-prompt details").forEach((d) => {
        const n = [...d.querySelectorAll(".sous-liste.choisie")].filter((x) => !x.hidden).length;
        d.querySelector(".compte").textContent = n ? `· ${n}` : "";
    });
    document.getElementById("mineur-prompt").hidden = !c.mineur;
    rendreEtiquettes();
}

function rendreEtiquettes() {
    const choix = etatPrompt().choix;
    const video = cibleVideo();
    const planVideo = new Set(structurePrompt?.blocs.filter((b) => b.video).map((b) => b.id));
    document.getElementById("etiquettes-prompt").innerHTML = Object.entries(choix)
        .filter(([bloc]) => video || !planVideo.has(bloc))  // le Plan reste en mémoire, mais ne compte qu'en vidéo
        .flatMap(([bloc, valeurs]) =>
        Object.entries(valeurs).map(([sl, v]) =>
            `<button type="button" class="etiquette" data-bloc="${echapper(bloc)}" data-sl="${echapper(sl)}"
                     title="${echapper(sl)} · retirer">${echapper(v)} ✕</button>`)).join("");
}

function poserChoix(bloc, sl, valeur) {
    const etat = etatPrompt();
    const choix = (etat.choix[bloc] ??= {});
    if (valeur) choix[sl] = valeur;
    else delete choix[sl];
    // Une tranche choisie à la main l'emporte sur un âge précis qui n'y entre pas.
    if (bloc === "personnage" && sl === "Âge" && etat.age !== null) {
        const t = definitionsPrompt[cle("personnage", "Âge")].valeurs.find((v) => v.nom === valeur);
        if (!t || etat.age < t.bornes[0] || etat.age > t.bornes[1]) {
            etat.age = null;
            document.getElementById("age-prompt").value = "";
        }
    }
    appliquerReglesPrompt();
}

function changerChoix(bloc, sl, valeur) {
    poserChoix(bloc, sl, valeur);
    planifierAssemblage();
}

// Tirage 🎲 : seulement parmi les valeurs proposées, compatibles, qui ajoutent quelque chose au prompt
// et qui n'écrasent pas un choix déjà fait (une tenue intime ne remplace pas un haut choisi).
function tirer(div) {
    const etat = etatPrompt();
    const { bloc, sl } = div.dataset;
    const c = contextePrompt(etat);
    if (div.hidden || c.remplacees[cle(bloc, sl)]) return false;
    const actuelle = etat.choix[bloc]?.[sl];
    const categorie = categoriesAffichees[cle(sujetPrompt, bloc, sl)];  // le dé reste dans la catégorie affichée
    const candidates = definitionsPrompt[cle(bloc, sl)].valeurs.filter((v) => (v.prompt || v.prompts_genre)
        && v.nom !== actuelle && (!categorie || v.categorie === categorie)
        && valeurPermise(v, c, bloc) && !enConflit(etat, bloc, sl, v.nom)
        && !v.remplace.some((cible) => etat.choix[bloc]?.[cible]));
    if (!candidates.length) return false;
    poserChoix(bloc, sl, candidates[Math.floor(Math.random() * candidates.length)].nom);
    return true;
}

function planifierAssemblage() {
    garderBrouillon();
    clearTimeout(minuteurPrompt);
    minuteurPrompt = setTimeout(assemblerPrompt, 200);
}

function majBoutonsPrompt() {
    const final = Boolean(atelier().final.trim());
    ["optimiser-scenario", "aide-scenario", "tel-quel-scenario"].forEach((id) => {
        document.getElementById(id).disabled = occupePrompt;
    });
    document.querySelectorAll("[data-optimiser-plan], [data-aide-plan]").forEach((b) => { b.disabled = occupePrompt; });
    ["envoyer-prompt", "creer-prompt"].forEach((id) => { document.getElementById(id).disabled = occupePrompt || !final; });
}

function traductionMusiqueValide(etat) {
    return etat.traductionMusique && etat.traductionMusique.source === (etat.musique ?? "").trim();
}

async function assemblerPrompt() {
    const etat = etatPrompt();
    const numero = ++numeroAssemblage;
    const erreur = document.getElementById("erreur-prompt");
    let r;
    try {
        r = await api("POST", "/api/prompt/assembler", {
            sujet: sujetPrompt, choix: etat.choix, age: etat.age, texte: etat.texte, workflow: ciblePrompt(),
            replique: etat.replique, dialogue: Boolean(etat.dialogue), avec_musique: Boolean(etat.avecMusique),
            musique: traductionMusiqueValide(etat) ? etat.traductionMusique.texte : (etat.musique ?? ""),
        });
    } catch (err) {
        if (numero !== numeroAssemblage) return;
        assemblagePrompt = null;
        afficherErreur(erreur, err.message);
        majBoutonsPrompt();
        return;
    }
    if (numero !== numeroAssemblage) return;  // une réponse plus récente est déjà partie
    assemblagePrompt = r;
    erreur.hidden = true;
    document.getElementById("texte-apercu").textContent = complements() || "rien pour l'instant";
    const alertes = [...retraitsPrompt.map((x) => `Retiré : ${x}`), ...r.alertes];
    if (r.video && etat.avecMusique && (etat.musique ?? "").trim() && !traductionMusiqueValide(etat)) {
        alertes.push("Musique : traduite par le LLM à l’envoi");
    }
    retraitsPrompt = [];
    document.getElementById("alertes-prompt").innerHTML = alertes.map((a) => `<li>${echapper(a)}</li>`).join("");
    majFinal();
}

// Le LLM : optimiser, aider (libre.js), traduire la musique d'un clip

async function avecLlama(action) {
    const statut = document.getElementById("statut-prompt");
    const erreur = document.getElementById("erreur-prompt");
    occupePrompt = true;
    majBoutonsPrompt();
    erreur.hidden = true;
    // Pendant que le LLM écrit, le message compte les secondes ; le message final (durée, mots) reste tel quel.
    let message = "";
    const debut = Date.now();
    const afficher = (enCours) => {
        statut.textContent = enCours && message ? `${message} · ${Math.round((Date.now() - debut) / 1000)} s` : message;
        statut.hidden = !message;
    };
    const minuteur = setInterval(() => afficher(true), 1000);
    try {
        await action((m) => {
            message = m;
            afficher(true);
        });
        clearInterval(minuteur);
        afficher(false);
    } catch (err) {
        clearInterval(minuteur);
        statut.hidden = true;
        afficherErreur(erreur, err.message);
    } finally {
        occupePrompt = false;
        majBoutonsPrompt();
    }
}

async function traduireSiBesoin(statut) {
    const etat = etatPrompt();
    const musique = (etat.musique ?? "").trim();
    if (formeScenario === "clip" && etat.avecMusique && musique && !traductionMusiqueValide(etat)) {
        statut("Le LLM traduit la musique…");
        const r = await api("POST", "/api/prompt/traduire", { texte: musique, llm: llmAtelier() });
        etat.traductionMusique = { source: musique, texte: r.texte };
        statut("");
    }
    clearTimeout(minuteurPrompt);
    await assemblerPrompt();
    if (!assemblagePrompt) throw new Error(document.getElementById("erreur-prompt").textContent);
}

const dureeClip = () => Number(document.getElementById("duree-prompt").value) || 5;

// ✨ Optimiser : le LLM écrit le meilleur prompt pour le modèle cible, d'après ton idée, tes compléments et tes
// images (il les voit). En séquence, c'est le découpage en plans (decoupage.js).
document.getElementById("optimiser-scenario").addEventListener("click", () => {
    if (formeScenario === "sequence") {
        decouperSequence();
        return;
    }
    avecLlama(async (statut) => {
        await traduireSiBesoin(statut);
        const idee = atelier().idee.trim();
        const comp = complements();
        if (!idee && !comp) {
            throw new Error(formeScenario === "retouche" ? "Écris ce que tu veux changer"
                : "Écris ton idée (ou choisis des compléments)");
        }
        const images = imagesReference();
        statut(`${nomLlm()} ${images.length ? "regarde l'image et écrit" : "écrit"}… (le premier appel charge le modèle)`);
        const r = await api("POST", "/api/prompt/optimiser", {
            atelier: formeScenario, idee, complements: comp, images, role: roleReference(), workflow: ciblePrompt(),
            llm: llmAtelier(), duree: dureeClip(),
        });
        poserFinal(r.prompt, llmAtelier());
        statut(`${nomLlm()} · ${r.duree_s} s · ${r.mots} mots`);
    });
});

document.getElementById("aide-scenario").addEventListener("click", () => ouvrirAide());  // libre.js

document.getElementById("tel-quel-scenario").addEventListener("click", () => avecLlama(async (statut) => {
    await traduireSiBesoin(statut);
    const texte = [atelier().idee.trim(), complements()].filter(Boolean).join(" ");
    if (!texte) throw new Error("Écris ton idée");
    poserFinal(texte);
}));

// Envoi : le prompt final part tel quel, avec le modèle cible et les images de référence.
async function envoyer(creer) {
    const prompt = atelier().final.trim();
    if (!prompt) return;
    const envoi = { prompt, workflow: ciblePrompt(), images: imagesReference(), role: roleReference(),
                    duree: dureeClip(), lancer: creer, idee: atelier().idee.trim(), llm: atelier().llm };
    if (formeScenario === "retouche") await recevoirRetouche(envoi);  // plateau.js
    else await (cibleVideo() ? recevoirAuMontage(envoi) : recevoirAuPlateau(envoi));
}

document.getElementById("envoyer-prompt").addEventListener("click", () => envoyer(false));
document.getElementById("creer-prompt").addEventListener("click", () => envoyer(true));

// Personnages enregistrés : seulement le bloc Personnage (et l'âge précis)

async function chargerPersonnages(choisi) {
    personnagesEnregistres = await api("GET", "/api/prompt/personnages");
    const select = document.getElementById("personnage-prompt");
    const garde = choisi ?? select.value;
    select.innerHTML = '<option value="">—</option>' + personnagesEnregistres.map((p) =>
        `<option value="${echapper(p.nom)}" ${p.nom === garde ? "selected" : ""}>${echapper(p.nom)}</option>`).join("");
    document.getElementById("supprimer-personnage").disabled = !select.value;
}

document.getElementById("personnage-prompt").addEventListener("change", (e) => {
    document.getElementById("supprimer-personnage").disabled = !e.target.value;
    const p = personnagesEnregistres.find((x) => x.nom === e.target.value);
    if (!p) return;
    const etat = etatPrompt();
    etat.choix.personnage = { ...p.choix };
    etat.age = p.age;
    document.getElementById("age-prompt").value = p.age ?? "";
    // Un personnage enregistré garde son teint, ses yeux et son nez, même hors de ce que son origine propose.
    const c = contextePrompt({ ...etat, toutes: false });
    etat.toutes = Object.entries(etat.choix.personnage).some(([sl, nom]) => {
        const v = definitionsPrompt[cle("personnage", sl)]?.valeurs.find((x) => x.nom === nom);
        return v && !selonOrigine(v, c);
    });
    document.getElementById("toutes-options").checked = etat.toutes;
    appliquerReglesPrompt();  // une valeur disparue des listes est retirée, avec un message
    planifierAssemblage();
});

document.getElementById("enregistrer-personnage").addEventListener("click", async () => {
    const etat = etatPrompt();
    const nom = prompt("Nom du personnage :", document.getElementById("personnage-prompt").value);
    if (!nom?.trim()) return;
    if (personnagesEnregistres.some((p) => p.nom === nom.trim())
        && !confirm(`Remplacer le personnage « ${nom.trim()} » ?`)) return;
    try {
        const p = await api("PUT", `/api/prompt/personnages/${encodeURIComponent(nom.trim())}`,
            { choix: etat.choix.personnage ?? {}, age: etat.age });
        await chargerPersonnages(p.nom);
    } catch (err) {
        alert(err.message);
    }
});

document.getElementById("supprimer-personnage").addEventListener("click", async () => {
    const nom = document.getElementById("personnage-prompt").value;
    if (!nom || !confirm(`Supprimer le personnage « ${nom} » ?`)) return;
    try {
        await api("DELETE", `/api/prompt/personnages/${encodeURIComponent(nom)}`);
    } catch (err) {
        alert(err.message);
    }
    await chargerPersonnages("");
});

// Événements

document.getElementById("sujets-prompt").addEventListener("click", (e) => {
    const bouton = e.target.closest("button[data-sujet]");
    if (bouton && bouton.dataset.sujet !== sujetPrompt) choisirSujet(bouton.dataset.sujet);
});

document.getElementById("blocs-prompt").addEventListener("change", (e) => {
    if (e.target.id === "toutes-options") {
        etatPrompt().toutes = e.target.checked;
        appliquerReglesPrompt();
        planifierAssemblage();
        return;
    }
    const div = e.target.closest(".sous-liste");
    if (e.target.classList.contains("categorie-sl") && div) {  // filtre d'affichage : le choix fait ne bouge pas
        categoriesAffichees[cle(sujetPrompt, div.dataset.bloc, div.dataset.sl)] = e.target.value;
        appliquerReglesPrompt();
        return;
    }
    if (e.target.classList.contains("valeur-sl") && div) changerChoix(div.dataset.bloc, div.dataset.sl, e.target.value);
});

document.getElementById("blocs-prompt").addEventListener("click", (e) => {
    const de = e.target.closest(".de");
    if (de && tirer(de.closest(".sous-liste"))) planifierAssemblage();
});

document.getElementById("blocs-prompt").addEventListener("input", (e) => {
    if (e.target.id !== "age-prompt") return;
    const etat = etatPrompt();
    const age = e.target.value === "" ? null : Number(e.target.value);
    const [min, max] = structurePrompt.age_precis;
    if (age !== null && (!Number.isInteger(age) || age < min || age > max)) return;  // saisie en cours
    etat.age = age;
    // L'âge précis choisit la tranche, comme le fera le serveur.
    const tranche = age === null ? null : definitionsPrompt[cle("personnage", "Âge")].valeurs
        .find((v) => v.bornes.length && v.bornes[0] <= age && age <= v.bornes[1]);
    if (tranche) (etat.choix.personnage ??= {})["Âge"] = tranche.nom;
    appliquerReglesPrompt();
    planifierAssemblage();
});

// L'événement toggle ne remonte pas : on l'écoute en phase de capture.
document.getElementById("blocs-prompt").addEventListener("toggle", (e) => {
    if (!e.target.dataset?.groupe) return;
    if (e.target.open) groupesOuvertsPrompt.add(e.target.dataset.groupe);
    else groupesOuvertsPrompt.delete(e.target.dataset.groupe);
}, true);

document.getElementById("etiquettes-prompt").addEventListener("click", (e) => {
    const etiquette = e.target.closest(".etiquette");
    if (etiquette) changerChoix(etiquette.dataset.bloc, etiquette.dataset.sl, "");
});

[["dialogue-prompt", "dialogue"], ["musique-active-prompt", "avecMusique"]].forEach(([id, champ]) => {
    document.getElementById(id).addEventListener("change", (e) => {
        etatPrompt()[champ] = e.target.checked;
        majCases();
        planifierAssemblage();
    });
});

[["texte-prompt", "texte"], ["replique-prompt", "replique"], ["musique-prompt", "musique"]].forEach(([id, champ]) => {
    document.getElementById(id).addEventListener("input", (e) => {
        etatPrompt()[champ] = e.target.value;
        planifierAssemblage();
    });
});

document.getElementById("workflow-prompt").addEventListener("change", () => {
    ciblesParForme[tacheCible()[0]] = ciblePrompt();
    remplirLlms();  // s'il n'a pas été choisi à la main, le LLM suit celui du modèle cible
    planifierAssemblage();
});

document.querySelector(".onglets-scenario").addEventListener("click", (e) => {
    const bouton = e.target.closest("[data-atelier]");
    if (bouton && bouton.dataset.atelier !== formeScenario) choisirAtelier(bouton.dataset.atelier);
});

document.querySelector(".onglets-listes").addEventListener("click", (e) => {
    const bouton = e.target.closest("[data-liste]");
    if (!bouton) return;
    choisirListe(bouton.dataset.liste);
    garderBrouillon();
});

document.getElementById("llm-atelier").addEventListener("change", (e) => {
    llmsChoisis[formeScenario] = e.target.value;
    garderBrouillon();
});

document.getElementById("idee-scenario").addEventListener("input", (e) => {
    atelier().idee = e.target.value;
    garderBrouillon();
});

document.getElementById("prompt-final").addEventListener("input", (e) => {
    atelier().final = e.target.value;
    majFinal();
    garderBrouillon();
});

document.getElementById("effacer-prompt").addEventListener("click", () => {
    if (!confirm(`Vider l'atelier ${document.querySelector(".onglet-scenario.actif").textContent.trim()} `
        + "(idée, conversation, prompt final) ? Les listes et les images restent.")) return;
    ateliers[formeScenario] = { idee: "", final: "", echanges: [], dernier: null };
    majAtelier();
    garderBrouillon();
});
