// Prépare la compilation Tauri : icône et version dans l'exe, et une autorisation par commande de l'appli.
// Une commande absente de cette liste n'est pas appelable depuis l'interface, même si le code Rust l'expose.
fn main() {
    let commandes = tauri_build::AppManifest::new().commands(&[
        "identite",
        "lire_reglages",
        "enregistrer_reglages",
        "mesurer_disque",
        "lire_journal",
        "quitter",
        "lire_demarrage_auto",
        "regler_demarrage_auto",
        "etat_menu",
        "action_menu",
        "taille_menu",
        "infos_installation",
        "installer",
        "lancer_sans_installer",
        "mise_a_jour_disponible",
        "voir_notes",
        "installer_mise_a_jour",
    ]);
    let attributs = tauri_build::Attributes::new().app_manifest(commandes);
    if let Err(erreur) = tauri_build::try_build(attributs) {
        // Un script de build signale son échec en s'arrêtant : cargo affiche le message.
        panic!("préparation de Tauri impossible : {erreur:#}");
    }
}
