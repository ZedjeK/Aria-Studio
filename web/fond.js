// Fond animé : des gouttes en cercles qui allument, à chaque passage, quelques points d'une grille. Elles tombent au
// hasard dans la zone des pages (jamais sous le menu), en cyan, orange et bleu.
//
// Les quatre réglages viennent du serveur (GET /api/fond) et se changent dans Réglages → Système → Fond animé.
// Le fond se met en pause quand l'onglet est caché, et ses gouttes disparaissent si Windows demande moins
// d'animations (la grille de points reste). Il ne touche à rien d'autre dans l'appli.

(() => {
    const fond = document.getElementById("fond");
    const gouttes = document.getElementById("gouttes");
    if (!fond || !gouttes) return;

    const VITESSES = { lente: 0.5, normale: 1, rapide: 2 };
    const PAS = 22;          // la grille de points : ses cases font 22 px
    const TAILLE = 91 * PAS; // 2002 px : un multiple du pas, pour que le centre reste sur la grille
    const reduit = window.matchMedia("(prefers-reduced-motion: reduce)");
    let reglage = { actif: true, vitesse: "normale", minimum: 2, maximum: 5 };
    let minuterie = null;

    const borne = (valeur) => Math.max(1, Math.min(12, Number.parseInt(valeur, 10) || 1));

    function vider() {
        clearTimeout(minuterie);
        gouttes.replaceChildren();
    }

    // Un passage : entre « minimum » et « maximum » gouttes, tirées au hasard, qui s'étalent puis s'effacent.
    function passage() {
        vider();
        fond.hidden = !reglage.actif;
        if (!reglage.actif || reduit.matches || document.hidden) return;
        const duree = 6 / (VITESSES[reglage.vitesse] ?? 1);
        const bas = Math.min(borne(reglage.minimum), borne(reglage.maximum));
        const haut = Math.max(borne(reglage.minimum), borne(reglage.maximum));
        const nombre = bas + Math.floor(Math.random() * (haut - bas + 1));
        const largeur = fond.clientWidth;
        const hauteur = fond.clientHeight;
        for (let i = 0; i < nombre; i++) {
            const x = 40 + Math.random() * Math.max(10, largeur - 80);
            const y = 40 + Math.random() * Math.max(10, hauteur - 80);
            const goutte = document.createElement("div");
            goutte.className = `goutte c${1 + Math.floor(Math.random() * 3)}`;
            // Calée sur la grille : les points allumés sont ceux du fond
            goutte.style.left = `${PAS * Math.round((x - TAILLE / 2) / PAS)}px`;
            goutte.style.top = `${PAS * Math.round((y - TAILLE / 2) / PAS)}px`;
            gouttes.append(goutte);
            const taille = `${TAILLE}px ${TAILLE}px`;
            goutte.animate([
                { maskSize: "0px 0px", webkitMaskSize: "0px 0px", opacity: 1 },
                { opacity: 1, offset: 0.75 },
                { maskSize: taille, webkitMaskSize: taille, opacity: 0 },
            ], { duration: duree * 1000, delay: Math.random() * 0.55 * duree * 1000, easing: "linear", fill: "both" })
                .onfinish = () => goutte.remove();
        }
        minuterie = setTimeout(passage, duree * 1600);
    }

    // Carte Réglages → Système → Fond animé
    const cases = {
        actif: document.getElementById("fond-actif"),
        vitesse: document.getElementById("fond-vitesse"),
        minimum: document.getElementById("fond-minimum"),
        maximum: document.getElementById("fond-maximum"),
    };
    const note = document.getElementById("fond-note");

    function afficher() {
        if (!cases.actif) return;
        cases.actif.checked = reglage.actif;
        cases.vitesse.value = reglage.vitesse;
        cases.minimum.value = reglage.minimum;
        cases.maximum.value = reglage.maximum;
        for (const nom of ["vitesse", "minimum", "maximum"]) cases[nom].disabled = !reglage.actif;
    }

    async function regler(changement) {
        if (note) note.hidden = true;
        try {
            reglage = { ...reglage, ...(await api("PUT", "/api/fond", changement)) };
        } catch (e) {
            if (note) {
                note.textContent = `Réglage non enregistré : ${e.message}`;
                note.hidden = false;
            }
        }
        afficher();
        passage();
    }

    if (cases.actif) {
        cases.actif.addEventListener("change", () => regler({ actif: cases.actif.checked }));
        cases.vitesse.addEventListener("change", () => regler({ vitesse: cases.vitesse.value }));
        cases.minimum.addEventListener("change", () => regler({ minimum: borne(cases.minimum.value) }));
        cases.maximum.addEventListener("change", () => regler({ maximum: borne(cases.maximum.value) }));
    }

    document.addEventListener("visibilitychange", passage);
    reduit.addEventListener("change", passage);
    let redimension = null;
    window.addEventListener("resize", () => {
        clearTimeout(redimension);
        redimension = setTimeout(passage, 250);
    });

    api("GET", "/api/fond")
        .then((p) => { reglage = { ...reglage, ...p }; })
        .catch(() => {})
        .finally(() => { afficher(); passage(); });
})();
