// Habillage d'Aria Studio : ce que le menu latéral et les sous-onglets ont besoin de JavaScript.
// Le reste de l'habillage est dans web/habillage.css. Ce fichier ne change aucun comportement de l'appli.

// Chemin de la page affichée, en haut de la zone : « CRÉATION · PLATEAU ».
(() => {
    const chemin = document.getElementById("chemin");
    if (!chemin) return;
    const afficher = (bouton) => {
        const famille = bouton.closest(".famille-onglets")?.querySelector(".etiquette-famille")?.textContent.trim() ?? "";
        const page = document.createElement("b");
        page.textContent = bouton.textContent.trim().toUpperCase();
        chemin.replaceChildren(`${famille.toUpperCase()} · `, page);
    };
    document.querySelectorAll(".onglet").forEach((bouton) => bouton.addEventListener("click", () => afficher(bouton)));
    const actif = document.querySelector(".onglet.actif");
    if (actif) afficher(actif);
})();

// Commentaire de l'onglet actif : un encadré « À savoir » sous les sous-onglets. Le texte est dans l'attribut
// data-savoir de chaque onglet ; l'encadré le reprend quand l'appli change l'onglet actif (classe « actif »).
document.querySelectorAll(".savoir[data-pour]").forEach((encadre) => {
    const groupe = document.querySelector(encadre.dataset.pour);
    const texte = encadre.querySelector("span");
    if (!groupe || !texte) return;
    const afficher = () => {
        const actif = groupe.querySelector("button.actif[data-savoir]");
        texte.textContent = actif ? actif.dataset.savoir : "";
        encadre.hidden = !actif;
    };
    new MutationObserver(afficher).observe(groupe, { subtree: true, attributes: true, attributeFilter: ["class"] });
    afficher();
});
