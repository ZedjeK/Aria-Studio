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
