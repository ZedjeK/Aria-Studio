// Galeries rangées : les Rushes du Plateau (images) et les Bobines du Montage (clips).
// Mêmes vues (Tous, Non rangés, dossiers, Archives, Corbeille), mêmes dossiers, même sélection.
// Ranger ne déplace aucun fichier : la fiche note son dossier, son archivage ou sa mise à la corbeille.
//
// Le balisage d'une galerie porte des data-role : compte, vues, selectionner, barre, compte-selection,
// ranger-dans, vignettes (voir index.html). « Vider la corbeille » est dans la rangée des vues, vue Corbeille.

const ACTIONS_DEPOT = { non_ranges: ["ranger", null], archives: ["archiver"], corbeille: ["corbeille"] };
const actionDepot = (cle) => (cle?.startsWith("dossier:") ? ["ranger", cle.slice(8)] : ACTIONS_DEPOT[cle]);

// `actions(fiche)` : actions propres au genre du menu clic droit, [[libellé, fonction]] (variante, animer…).
function creerGalerie({ genre, racine, mot, vides, vignette, ouvrir, actions = () => [] }) {
    const $ = (role) => racine.querySelector(`[data-role="${role}"]`);
    const g = { fiches: [], etat: null, vue: { vue: "tous", dossier: null }, selection: new Set(), modeSelection: false };
    let dernierClic = null;  // pour Maj + clic
    const pluriel = (n) => `${n} ${mot}${n > 1 ? "s" : ""}`;
    const cleVue = () => (g.vue.dossier ? `dossier:${g.vue.dossier}` : g.vue.vue);

    g.dossierOuvert = () => g.vue.dossier;  // une création lancée depuis un dossier ouvert y est rangée

    g.charger = async () => {
        const params = new URLSearchParams({ vue: g.vue.dossier ? "tous" : g.vue.vue, genre });
        if (g.vue.dossier) params.set("dossier", g.vue.dossier);
        [g.fiches, g.etat] = await Promise.all([api("GET", `/api/creations?${params}`), api("GET", `/api/rushes?genre=${genre}`)]);
        if (g.vue.dossier && !g.etat.dossiers.some((d) => d.nom === g.vue.dossier)) {
            g.vue = { vue: "tous", dossier: null };  // dossier supprimé ou renommé entre-temps
            return g.charger();
        }
        [...g.selection].filter((id) => !g.fiches.some((f) => f.id === id)).forEach((id) => g.selection.delete(id));
        rendreVues();
        rendre();
    };

    function rendreVues() {
        const e = g.etat;
        const actif = cleVue();
        const pastille = (cle, libelle, nombre) => `<button type="button" class="vue-rush ${cle === actif ? "actif" : ""}"
            data-vue-rush="${echapper(cle)}">${libelle} <span class="nombre">${nombre}</span></button>`;
        $("vues").innerHTML = pastille("tous", "Tous", e.tous)
            + pastille("non_ranges", "Non rangés", e.non_ranges)
            + e.dossiers.map((d) => pastille(`dossier:${d.nom}`, `📁 ${echapper(d.nom)}`, d.nombre)).join("")
            + '<button type="button" class="vue-rush nouveau" data-nouveau-dossier="1">＋ Dossier</button>'
            + (g.vue.dossier
                ? `<span class="outils-dossier"><button type="button" class="petit" data-renommer-dossier="1">✎ Renommer</button>
                   <button type="button" class="petit" data-supprimer-dossier="1">✕ Supprimer le dossier</button></span>` : "")
            + '<span class="separateur"></span>'
            + pastille("archives", "📦 Archives", e.archives) + pastille("corbeille", "🗑 Corbeille", e.corbeille)
            // Dans la corbeille, le bouton qui la vide est ici, avec les autres : plus besoin de tout dérouler.
            + (actif === "corbeille" && e.corbeille
                ? `<button type="button" class="petit danger" data-vider-corbeille="1"
                    title="Ce qui est dans la corbeille reste restaurable jusqu'à ce que tu la vides">Vider la corbeille</button>` : "");
    }

    function rendre() {
        $("compte").textContent = `· ${pluriel(g.fiches.length)}`;
        $("vignettes").innerHTML = g.fiches.length
            ? g.fiches.map((f) => vignette(f, g.selection.has(f.id))).join("")
            : `<p class="discret">${vides[g.vue.dossier ? "dossier" : g.vue.vue]}</p>`;
        $("vignettes").classList.toggle("en-selection", g.modeSelection);
        majBarre();
    }

    function majBarre() {
        const barre = $("barre");
        barre.hidden = !g.modeSelection;
        $("selectionner").hidden = g.modeSelection;
        if (!g.modeSelection) return;
        const n = g.selection.size;
        $("compte-selection").textContent = `${n} sélectionné${n > 1 ? "s" : ""}`;
        const vue = g.vue.dossier ? "dossier" : g.vue.vue;
        const visibles = {  // actions qui ont un sens dans cette vue
            archiver: !["archives", "corbeille"].includes(vue), desarchiver: vue === "archives",
            restaurer: vue === "corbeille", corbeille: vue !== "corbeille",
        };
        barre.querySelectorAll("[data-action]").forEach((b) => {
            b.hidden = !visibles[b.dataset.action];
            b.disabled = !n;
        });
        const ranger = $("ranger-dans");
        ranger.closest("label").hidden = vue === "corbeille";
        ranger.disabled = !n;
        ranger.innerHTML = '<option value="">— choisir —</option><option value="__aucun__">Non rangés</option>'
            + g.etat.dossiers.map((d) => `<option value="${echapper(d.nom)}">📁 ${echapper(d.nom)}</option>`).join("");
    }

    async function agir(ids, action, dossier = null) {
        if (!ids.length) return;
        try {
            await api("POST", "/api/rushes/lot", { ids, action, dossier });
        } catch (err) {
            alert(err.message);
        }
        ids.forEach((id) => g.selection.delete(id));
        await g.charger();
    }

    function changerVue(cle) {
        g.vue = cle.startsWith("dossier:") ? { vue: "tous", dossier: cle.slice(8) } : { vue: cle, dossier: null };
        g.selection.clear();
        g.charger();
    }

    $("selectionner").addEventListener("click", () => {
        g.modeSelection = true;
        rendre();
    });

    $("barre").addEventListener("click", (e) => {
        const bouton = e.target.closest("button");
        if (!bouton) return;
        if (bouton.dataset.selection === "tout") g.fiches.forEach((f) => g.selection.add(f.id));
        if (bouton.dataset.selection === "aucun") g.selection.clear();
        if (bouton.dataset.selection === "fermer") {
            g.modeSelection = false;
            g.selection.clear();
        }
        if (bouton.dataset.action) {
            agir([...g.selection], bouton.dataset.action);
            return;
        }
        rendre();
    });

    $("ranger-dans").addEventListener("change", (e) => {
        if (!e.target.value) return;
        agir([...g.selection], "ranger", e.target.value === "__aucun__" ? null : e.target.value);
    });

    $("vues").addEventListener("click", async (e) => {
        const bouton = e.target.closest("button");
        if (!bouton) return;
        try {
            if (bouton.dataset.vueRush) changerVue(bouton.dataset.vueRush);
            if (bouton.dataset.nouveauDossier) {
                const nom = prompt("Nom du nouveau dossier :");
                if (!nom?.trim()) return;
                const d = await api("POST", "/api/rushes/dossiers", { nom: nom.trim() });
                changerVue(`dossier:${d.nom}`);
            }
            if (bouton.dataset.renommerDossier) {
                const nom = prompt("Nouveau nom du dossier :", g.vue.dossier);
                if (!nom?.trim() || nom.trim() === g.vue.dossier) return;
                const d = await api("PUT", `/api/rushes/dossiers/${encodeURIComponent(g.vue.dossier)}`, { nom: nom.trim() });
                changerVue(`dossier:${d.nom}`);
            }
            if (bouton.dataset.viderCorbeille) {
                const n = g.etat.corbeille;
                if (!confirm(`Supprimer définitivement ${pluriel(n)} de la corbeille ? C'est sans retour.`)) return;
                await api("DELETE", `/api/rushes/corbeille?genre=${genre}`);
                await g.charger();
            }
            if (bouton.dataset.supprimerDossier
                && confirm(`Supprimer le dossier « ${g.vue.dossier} » ? Ses images et ses clips ne sont pas supprimés : `
                    + "ils retournent dans « Non rangés ».")) {
                await api("DELETE", `/api/rushes/dossiers/${encodeURIComponent(g.vue.dossier)}`);
                changerVue("tous");
            }
        } catch (err) {
            alert(err.message);
        }
    });

    // Glisser des vignettes sur une pastille : la sélection entière si la vignette en fait partie.
    $("vues").addEventListener("dragover", (e) => {
        const pastille = e.target.closest("[data-vue-rush]");
        if (!pastille || !e.dataTransfer.types.includes("text/aria-rush") || !actionDepot(pastille.dataset.vueRush)) return;
        e.preventDefault();
        pastille.classList.add("survol");
    });
    $("vues").addEventListener("dragleave", (e) => e.target.closest("[data-vue-rush]")?.classList.remove("survol"));
    $("vues").addEventListener("drop", (e) => {
        const pastille = e.target.closest("[data-vue-rush]");
        const action = actionDepot(pastille?.dataset.vueRush);
        if (!action) return;
        e.preventDefault();
        pastille.classList.remove("survol");
        const id = e.dataTransfer.getData("text/aria-rush");
        agir(g.selection.has(id) ? [...g.selection] : [id], ...action);
    });

    $("vignettes").addEventListener("dragstart", (e) => {
        const id = e.target.dataset?.creation;
        if (!id) return;
        e.dataTransfer.setData("text/aria-rush", id);
        e.dataTransfer.effectAllowed = "copy";
    });

    $("vignettes").addEventListener("click", async (e) => {
        const id = e.target.closest("[data-creation]")?.dataset.creation;
        if (!id) return;
        if (g.modeSelection) {
            const ids = g.fiches.map((f) => f.id);
            if (e.shiftKey && dernierClic && ids.includes(dernierClic)) {  // Maj + clic : toute la plage
                const [a, b] = [ids.indexOf(dernierClic), ids.indexOf(id)].sort((x, y) => x - y);
                ids.slice(a, b + 1).forEach((i) => g.selection.add(i));
            } else if (g.selection.has(id)) {
                g.selection.delete(id);
            } else {
                g.selection.add(id);
            }
            dernierClic = id;
            rendre();
            return;
        }
        ouvrir(await api("GET", `/api/creations/${encodeURIComponent(id)}`));
    });

    // Clic droit sur une vignette : les mêmes gestes que la fiche et la barre de sélection, sans ouvrir la fiche.
    // Si la vignette fait partie de la sélection, les gestes de rangement valent pour toute la sélection.
    let menu = null;
    const fermerMenu = () => {
        menu?.remove();
        menu = null;
    };
    document.addEventListener("click", fermerMenu);
    document.addEventListener("scroll", fermerMenu, true);
    window.addEventListener("blur", fermerMenu);
    document.addEventListener("keydown", (e) => e.key === "Escape" && fermerMenu());

    $("vignettes").addEventListener("contextmenu", (e) => {
        const id = e.target.closest("[data-creation]")?.dataset.creation;
        if (!id) return;
        e.preventDefault();
        fermerMenu();
        const f = g.fiches.find((x) => x.id === id);
        if (!f) return;
        const lot = g.selection.has(id) && g.selection.size > 1 ? [...g.selection] : [id];
        const vue = g.vue.dossier ? "dossier" : g.vue.vue;
        const suffixe = lot.length > 1 ? ` (${lot.length})` : "";
        const lignes = [["Ouvrir la fiche", async () => ouvrir(await api("GET", `/api/creations/${encodeURIComponent(id)}`))]];
        if (vue !== "corbeille") lignes.push(...actions(f).map(([l, fn]) => [l, () => fn(f)]));
        lignes.push(["—"]);
        if (vue !== "corbeille") {
            lignes.push([g.selection.has(id) ? "Désélectionner" : "Sélectionner", () => {
                g.modeSelection = true;
                g.selection.has(id) ? g.selection.delete(id) : g.selection.add(id);
                rendre();
            }]);
            lignes.push(["Ranger dans…", null, [["Non rangés", () => agir(lot, "ranger", null)],
                ...g.etat.dossiers.map((d) => [`📁 ${d.nom}`, () => agir(lot, "ranger", d.nom)])]]);
            lignes.push([vue === "archives" ? `Sortir des archives${suffixe}` : `Archiver${suffixe}`,
                () => agir(lot, vue === "archives" ? "desarchiver" : "archiver")]);
            lignes.push([`🗑 Mettre à la corbeille${suffixe}`, () => agir(lot, "corbeille"), "danger"]);
        } else {
            lignes.push([`Restaurer${suffixe}`, () => agir(lot, "restaurer")]);
        }
        menu = document.createElement("div");
        menu.className = "menu-contexte";
        menu.setAttribute("role", "menu");
        const remplir = (conteneur, items) => items.forEach(([libelle, fn, sous]) => {
            if (libelle === "—") {
                conteneur.appendChild(Object.assign(document.createElement("hr"), { className: "sep" }));
                return;
            }
            const b = document.createElement("button");
            b.type = "button";
            b.setAttribute("role", "menuitem");
            b.textContent = libelle;
            if (sous === "danger") b.classList.add("danger");
            if (Array.isArray(sous)) {
                b.classList.add("avec-sous");
                const bloc = document.createElement("div");
                bloc.className = "menu-contexte sous-menu";
                remplir(bloc, sous);
                const wrap = document.createElement("div");
                wrap.className = "entree-sous";
                wrap.append(b, bloc);
                conteneur.appendChild(wrap);
                return;
            }
            b.addEventListener("click", (ev) => {
                ev.stopPropagation();
                fermerMenu();
                fn();
            });
            conteneur.appendChild(b);
        });
        remplir(menu, lignes);
        document.body.appendChild(menu);
        const { innerWidth: L, innerHeight: H } = window;
        const r = menu.getBoundingClientRect();
        menu.style.left = `${Math.max(4, Math.min(e.clientX, L - r.width - 4))}px`;
        menu.style.top = `${Math.max(4, Math.min(e.clientY, H - r.height - 4))}px`;
        if (e.clientX + r.width + 220 > L) menu.classList.add("sous-a-gauche");
        menu.querySelector("button")?.focus({ preventScroll: true });
    });

    return g;
}
