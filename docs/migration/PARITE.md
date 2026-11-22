# Migration vers Rust, Tauri 2 et Svelte 5 : table de parité

Aria Studio 2.x (Python, FastAPI, interface web servie sur `127.0.0.1:8075`, lanceur C#) passe sur le squelette
`rust-tauri` du dépôt modèle. Ce fichier dit ce que la version 3 doit refaire pour égaler la 2.4, et ce qui est fait.
Il vit sur la branche d'intégration et se coche PR par PR. Relevé sur `main` le 2026-11-22 (`d78ca07`).

Une ligne n'est cochée que si la fonction marche dans la v3 **et** que ses tests y sont portés.

Pour essayer la v3 sur un PC qui fait tourner les moteurs, sans toucher à une v2 installée : [ESSAI-V3.md](ESSAI-V3.md).

## Règles du chantier

- La branche par défaut reste la v2 en service : correctifs seulement, aucune fonction nouvelle.
- La v3 se construit sur la branche d'intégration, par petites PR, chacune avec la CI `tests` verte.
- Les tests Python de la v2 servent de cahier des charges : chaque module porté l'est avec ses tests.
- Les données d'un utilisateur v2 (`%USERPROFILE%\.Aria-Studio`) sont relues telles quelles ; rien n'est réécrit
  dans un format qu'une v2 ne relirait pas, tant que la v3 n'est pas publiée.
- L'interface reprend celle de la v2 à l'identique : toute différence voulue est nommée dans sa PR.
- Une version 3 et la v2 partagent le même dossier de données (`%USERPROFILE%\.Aria-Studio`). Tant que la partie 2
  ci-dessous n'est pas cochée, une build de la v3 ne se lance pas sur un PC qui a des données de la v2.
- README, CHANGELOG et `docs/FICHE.md` décrivent la v2 jusqu'à la parité : ils sont réécrits avant la première
  version 3 publiée.

## 1. Pont de mise à jour depuis la v2

La mise à jour intégrée de la v2 (`app/installation_maj.py`, `tools/launcher/Launcher.cs`) doit pouvoir installer
la v3. Ce qu'elle exige, et ce que la v3 doit donc fournir :

| Exigence de la v2 | Où | v3 |
| --- | --- | --- |
| Assets `Aria-Studio-X.Y.Z.zip` et `Aria-Studio-X.Y.Z.zip.sha256` (« empreinte  nom ») | `preparer()` | [ ] (déjà le format de `tools/empaqueter.ps1`) |
| Zip à racine unique `Aria-Studio-X.Y.Z/` | `_extraire()` | [ ] (déjà le format du modèle) |
| `Aria-Studio.exe` à la racine du zip | `_extraire()` | [ ] |
| `app/__init__.py` dans le zip, avec `VERSION = "X.Y.Z"` | `_extraire()` | [ ] témoin ajouté au zip, hors de `fichiers.txt` |
| L'exe accepte `--mise-a-jour <pid> <ancien lanceur>` | `_relayer()` | [ ] |
| Il attend la fin du processus `<pid>` (120 s au plus) | `MettreAJour()` | [ ] |
| Il attend que `programme\` se libère (l'icône v2 le tient, environ 12 s ; 15 s d'attente) | `Installer()` | [ ] |
| Il installe dans `programme\`, la v2 entière gardée (`programme.precedent\` en v2) | `Installer()` | [ ] la v2 n'a ni `fichiers.txt` ni `BUILDINFO.txt` : elle est gardée en bloc |
| Il retire la marque « téléchargé depuis Internet » | `RetirerMarque()` | [ ] |
| Il relance la v3 depuis `programme\`, ou l'ancien lanceur en cas d'échec | `MettreAJour()` | [ ] |
| La v3 lit `cache\mise-a-jour\relais.json` au démarrage et note le bilan au journal | `bilan_au_demarrage()` | [ ] |

Essai obligatoire avant la release : une v2.4.x réellement installée se met à jour vers une prerelease v3.

## 2. Données d'un utilisateur v2

| Donnée | v2 | v3 | Fait |
| --- | --- | --- | --- |
| Réglages | `reglages.json` (sections : emplacements, favoris, assistant, installation, fond, notifications, mises à jour, LoRA, LLM, workflows) | même format ; `fond`, `installation.proposee` et `mises_a_jour.active` lus et écrits, toutes les autres sections et clés gardées telles quelles (`coeur/src/reglages.rs`) ; chaque autre section se coche avec la page qui la gère | [ ] |
| Sauvegardes des réglages | `sauvegardes\reglages-AAAAMMJJ-HHMMSS-micro.json` (20 dernières), `reglages-illisible-…` | même nom, 20 gardées, fichier illisible mis de côté (`coeur/src/reglages.rs`) ; la restauration attend la page Réglages | [ ] |
| Journal | `journal.json` (500 dernières, la plus récente en premier : `date`, `action`, `objet`, `resultat`, `detail`), `journal-illisible-…` | même format, entrées v2 gardées telles quelles, journal illisible mis de côté, journal verrouillé jamais réécrit (`coeur/src/journal.rs`) ; page Journal de l'interface adaptée | [x] |
| Créations, rushes, clips, séquences, voix | `data\` | `data\creations\<id>.png` et `<id>.json` au format de la v2 (identifiant `AAAAMMJJ-HHMMSS-xxxxxx`, fiche indentée de quatre espaces, clés de la v2), écrits par le Plateau et relus tels quels (`coeur/src/creations.rs`) ; clips, séquences et voix attendent le Montage | [ ] |
| Cache | `cache\` (empreintes SHA-256, samplers, chemins ComfyUI) | recalculable | [ ] |
| Modèles et moteurs | `modeles\`, `moteurs\`, dossier des modèles choisi | mêmes chemins décidés au lancement, rien de déplacé (`coeur/src/emplacements.rs`) ; leur contenu se lit avec les lots des moteurs et des modèles | [ ] |
| Clés Civitai et Hugging Face | Gestionnaire d'identification Windows (`Aria-Studio/civitai`, `Aria-Studio/huggingface`), repli `CIVITAI_TOKEN` et `HF_TOKEN` | recopiées au lancement, chiffrées (DPAPI), dans `secrets\` si elles n'y sont pas déjà ; l'entrée du coffre reste en place pour une v2 restaurée et ne sera retirée qu'avec l'accord de l'utilisateur (`coeur/src/cles.rs`, `appli/src/main.rs`) ; saisie, test et retrait faits dans Réglages → Emplacements et clés | [x] |
| Démarrage avec Windows | valeur `Run` « Aria Studio » (HKCU) | même valeur, nouvelle cible | [ ] |
| Raccourci du Bureau | `Aria Studio.lnk` | proposé en mise à jour, jamais remplacé en silence | [ ] |
| Mode portable | `data\` à côté du code, sauf code dans `.Aria-Studio` ; moteurs dans `services\` ; pas de proposition d'installation | même règle (`coeur/src/emplacements.rs`, `appli/src/main.rs`) | [x] |
| Anciens fichiers de réglages (`aria.yaml`, `data\*.json`…) | versés dans `reglages.json` une fois (clé `migration`), laissés en place | laissés en place, pas relus : la v3 n'arrive que par la mise à jour d'une v2, qui a déjà fait cette migration à son lancement | [x] |

## 3. Pages

| Famille | Page | Sous-onglets et panneaux | Modules v2 | Fait |
| --- | --- | --- | --- | --- |
| — | Assistant de premier lancement | plan, licences, téléchargement, essai | `assistant.py`, `web/assistant.js`, `web/guide.js` | [ ] |
| Création | Scénario | idée, listes, personnages, prompt final, traduction, optimisation, découpage en séquences | `prompt.py`, `listes.py`, `personnages.py`, `optimiseur.py`, `decoupage.py`, `routes_scenario.py`, `web/scenario.js`, `web/libre.js`, `web/decoupage.js`, `web/references.js` | [ ] |
| Création | Plateau | créer, options, estimation, paliers, rushes, résultat, agrandissement | `plateau.py`, `paliers.py`, `estimation.py`, `rushes.py`, `orchestrateur.py`, `web/plateau.js`, `web/galerie.js` | [ ] page Plateau (`coeur/src/plateau.rs`, `appli/src/plateau.rs`, `interface/src/pages/Plateau.svelte`) : texte → image depuis une idée réécrite par le LLM (`coeur/src/prompt.rs` : consigne au gabarit de la v2, règles du modèle cible, consigne réécrite dans Réglages, spécialiste du workflow, nettoyage, AS-18 avant et après) ou depuis un prompt tel quel, LLM du workflow, workflow, checkpoint, format, nombre, seed, file et ✕ Annuler, dernières créations ; validation des steps d'un checkpoint « à valider » posée là en attendant Modèles → Ma collection. Rushes (`coeur/src/rushes.rs`, `interface/src/pages/PanneauRushes.svelte`) : vues Tous, Non rangés, Archives, Corbeille, dossiers dans `data\dossiers.json` (ancien format lu), sélection, actions, corbeille vidée seulement après confirmation. Départ depuis des rushes : S'inspirer (1 à 10 références sur l'encodeur d'édition, format de sortie, consigne « références » de la v2, images nommées comme le modèle cible) et Redessiner (img2img, liberté, format ou celui de l'image), le LLM voyant les images s'il a sa vision. Restent : LoRA, estimation, paliers, rangement d'une nouvelle création dans un dossier |
| Création | Retouche | rushes, retoucher | `retouche.py`, `web/retouche.js` | [ ] page Retouche (`coeur/src/retouche.rs`, `coeur/src/images.rs`, `interface/src/pages/Retouche.svelte`) : source prise dans les Rushes ou importée, retouche au pinceau (masque, gomme, fondu, format), agrandissement (taille, ancrage, précision), consigne telle quelle ou traduite par le LLM qui voit l'image, variantes, file, avant · après. Restent : LoRA, glisser-déposer, rangement du résultat ailleurs qu'avec sa source, passage par le Scénario |
| Création | Montage | tourner un clip, bobines, séquences, voix, corbeille | `sequences.py`, `voix.py`, `repliques.py`, `routes_montage.py`, `web/montage.js`, `web/sequence.js`, `web/voix.js` | [ ] |
| Préparation | Modèles | Rechercher, Ma collection, Texte → image, Image → image, Vidéo ; téléchargements, corbeille, moteurs | `modeles.py`, `collection.py`, `decouvrir.py`, `telechargement.py`, `sources/`, `fiches.py`, `gguf.py`, `migration_modeles.py`, `moteurs.py`, `web/modeles.js`, `web/decouvrir.js`, `web/loras.js` | [ ] page Modèles, première partie (`coeur/src/telechargement.rs`, `coeur/src/installations.rs`, `interface/src/pages/Modeles.svelte`) : workflows par tâche avec leurs fichiers et leur licence, LLM du catalogue, « Télécharger ce qui manque » et « Installer » après confirmation de la licence, file (reprise du `.part`, cinq essais, taille et SHA-256 vérifiés, registre `data\modeles.json` de la v2, journal). Ma collection (`coeur/src/collection.rs`, `interface/src/pages/ModelesCollection.svelte`) : fichiers rangés par groupe (image, vidéo, LoRA, LLM, fichiers de base), origine (Aria, source retrouvée, à la main), workflows qui les utilisent, steps à valider d'un checkpoint avec ce qu'en dit la fiche, mots déclencheurs et force d'une LoRA (section `loras` de `reglages.json`), corbeille des modèles (`<modèles>\.corbeille\`, `data\corbeille_modeles.json` de la v2, restauration, vidage confirmé, journal). Rechercher (`coeur/src/decouvrir.rs`, `coeur/src/suggestions.rs`, `interface/src/pages/ModelesRechercher.svelte`) : modèles image, vidéo, LoRA et LLM sur Civitai et Hugging Face filtrés sur les familles d'Aria (blocs `civitai` et `hugging_face` des fiches), tris, suite des résultats, modèles adultes sur demande et vignettes floutées (lues par Rust), fichiers d'un dépôt avec la VRAM libre pour un GGUF, fiche d'un lien collé (mots déclencheurs, force de l'auteur, steps et CFG lus sur la page et gardés au registre), .safetensors et .gguf seuls. Retrouver les sources (`coeur/src/retrouver.rs`) : empreinte des fichiers déposés à la main gardée dans `cache\empreintes.json` (format de la v2), cherchée dans le catalogue d'Aria puis sur Civitai, inscription au registre avec `retrouve`, journal ; le registre n'est modifié que par un fil à la fois. Restent : favoris des usages, onglets d'usage (Texte → image, Image → image, Vidéo) |
| Préparation | Casting | Nouveau casting, Comparer | `casting.py`, `web/casting.js` | [ ] |
| Préparation | Réglages | emplacements, clés, sécurité (protection des mineurs), mises à jour, démarrage, notifications, fond animé, Claude (MCP), sauvegardes, journal, LLM, modèles image | `reglages.py`, `fichier_reglages.py`, `emplacements.py`, `cles.py`, `securite.py`, `garde.py`, `notifications.py`, `demarrage.py`, `fond.py`, `journal.py`, `web/reglages.js` | [ ] |
| — | Quitter | arrêt d'Aria, de ComfyUI et de llama-server | `services.py` | [ ] |

## 4. Moteurs

| Moteur | v2 | Fait |
| --- | --- | --- |
| « J'ai déjà ComfyUI » | `moteurs.py::examiner_comfyui`, `verifier_comfyui` : ComfyUI portable de l'utilisateur (pas sur le réseau, version minimale, nœuds vérifiés par un lancement) | [ ] reporté : demande un sélecteur de dossier natif ; le chemin déjà choisi en v2 (`emplacements.comfyui`) est lu par l'appli |
| Filtre AS-18 (protection des mineurs) | `securite.py`, `securite/socle.yaml`, `securite/temoins.yaml` : mineur + terme sensible refusé, termes toujours bloqués, âges en chiffres et en lettres, deux lectures des tirets ; ajouts de l'utilisateur dans `reglages.json` | [ ] `coeur/src/securite.rs` fait (socle embarqué, ajouts lus, témoins rejoués), appliqué à tout prompt dans `Workflow::preparer` ; Réglages → Sécurité fait (`coeur/src/securite_reglages.rs`, `interface/src/pages/ReglagesSecurite.svelte`) : socle lu, essayer une phrase, ajouter et retirer ses termes (à confirmer s'ils font bloquer un témoin « doit passer »), phrases témoins ajoutées, retirées et rejouées ; la promotion d'un terme dans le socle n'existe plus (socle embarqué) |
| Préparation d'un workflow | `workflows.py::preparer`, `reglages.py` (section `workflows`) : défauts, réglages, appel ; checkpoint choisi ou par défaut ; steps à valider ; LoRA en chaîne ; LoRA du workflow retirée ; références ; variation | [ ] `coeur/src/workflows.rs` fait, références et variation comprises ; l'affiche vidéo et les balises de dialogue viendront avec le Montage |
| Dossiers des modèles et chemins pour ComfyUI | `modeles.py` : `<nature>/<Famille>/{modeles,loras}`, `<nature>/_base/<type>`, `llm` ; `cache/comfyui_chemins.yaml` réécrit avant chaque lancement | [x] `coeur/src/modeles.rs`, écrit par l'orchestrateur avant de lancer ComfyUI |
| Catalogue des moteurs | `catalogue/moteurs.yaml` lu par `app/moteurs.py` | [x] même fichier, embarqué dans l'exe et contrôlé à la lecture (`coeur/src/catalogue.rs`) ; YAML lu par `serde-saphyr` |
| ComfyUI portable | téléchargé (`catalogue/moteurs.yaml`, SHA-256), lancé sur `127.0.0.1:8195`, piloté en HTTP et WebSocket (`comfy.py`) ; mode externe « J'ai déjà ComfyUI » | [ ] `coeur/src/comfy.rs` : arguments sans CORS, dossiers de travail, préfixes uniques, événements, historique, effacement des sorties, orphelins, VRAM stable ; processus, HTTP, WebSocket et mode externe avec l'orchestrateur |
| llama-server | téléchargé, lancé sur `127.0.0.1:8095` avec une clé neuve à chaque démarrage (`llm.py`) ; fork PrismML | [ ] `coeur/src/llama.rs` : arguments, clé par `LLAMA_API_KEY`, corps des demandes, lecture du flux, messages ; processus et HTTP avec l'orchestrateur |
| Choix et lancement d'un LLM | `modeles.py::llm_disponibles`, `llm_defaut` ; `reglages.py` (section `llm`) : températures, échantillonnage par mode, longueur, contexte, réflexion, moteur, vision, consignes réécrites ; `gguf.py` (types des tenseurs : officiel ou PrismML) ; `fiches.py` (catalogue/llm.yaml, spécialistes) ; `orchestrateur.py::lancement_llm` | [ ] `coeur/src/llm.rs`, `coeur/src/gguf.rs`, fiches LLM dans `coeur/src/fiches.rs` : faits, réglages lus au format de la v2 ; Réglages → LLM et Modèles image et vidéo faits (`coeur/src/reglages_fins.rs`, `interface/src/pages/ReglagesLlm.svelte`, `ReglagesWorkflows.svelte`) : réglages contrôlés comme en v2, seuls les écarts enregistrés, consignes réécrites (réécriture, retouche, références), checkpoints de la famille ; samplers de ComfyUI retenus dans `cache\comfyui_samplers.json`. Restent : Recommander, Adapter à ma machine, résolutions du Montage, consignes des usages pas encore portés |
| Installation d'un moteur | `moteurs.py` : état (absent, manuel, différent, à jour, externe), archives dans `_archives\` avec reprise et SHA-256, zip et 7z (tar de Windows puis 7-Zip), `.new` puis échange sous verrou, place nécessaire | [ ] `coeur/src/moteurs.rs` fait ; branchement réseau, verrou et commandes avec l'onglet Moteurs |
| Retour à la version précédente d'un moteur | `moteurs.py` | [ ] `coeur/src/moteurs.rs::revenir` fait ; commande avec l'onglet Moteurs |
| Libérer la mémoire vidéo | `moteurs.py`, `mesures.py` | [ ] `coeur/src/orchestrateur.rs::liberer` et `comfy_serveur::liberer_vram` faits ; commande avec l'onglet Moteurs |
| Un seul moteur sur la carte | `orchestrateur.py` : ComfyUI vide sa VRAM sans s'arrêter quand le LLM prend la main ; moteur arrêté pendant un échange ; ComfyUI orphelin repris | [ ] `coeur/src/orchestrateur.rs` fait ; branché avec les pages qui génèrent |
| Garde VRAM | `garde.py` : attente sous 3,5 Gio, forcer, annuler ; processus coupables listés | [ ] `coeur/src/garde.rs` fait, sans la liste des coupables ; dialogue avec l'interface |
| Carte graphique | `moteurs.py::carte` (NVML) : nom, pilote minimum, VRAM, RAM | [ ] `coeur/src/carte.rs` fait ; affichage avec l'onglet Moteurs |

## 5. API v2 et son équivalent v3

La v2 expose 138 routes HTTP. En v3, l'interface appelle des commandes Tauri : plus de serveur web, plus de port
8075, plus de jeton ni de contrôle `Host` et `Origin` pour l'interface. Le tableau se remplit domaine par domaine.

| Domaine v2 | Routes | Commandes v3 | Fait |
| --- | --- | --- | --- |
| Assistant | `/api/assistant*` (6) | | [ ] |
| Casting | `/api/bancs*` (10) | | [ ] |
| Clés | `/api/cles*` (4) | `etat_cles`, `enregistrer_cle`, `supprimer_cle`, `tester_cle` ; rangées par DPAPI dans `secrets\` (l'entrée de la v2 dans le Gestionnaire d'identification reste) | [x] |
| Collection et LoRA | `/api/collection*` (6) | `collection_modeles`, `valider_steps_modele`, `regler_lora`, `oublier_reglage_lora`, `jeter_modele`, `corbeille_modeles`, `restaurer_modele`, `vider_corbeille_modeles`, `retrouver_sources`, `etat_sources` ; restent les usages et leurs favoris | [ ] |
| Commandes en cours | `/api/commandes*` (2) | `lire_commandes`, `annuler_commande` (images du Plateau) | [ ] |
| Créations | `/api/creations*`, `/api/agrandissements`, `/api/retouches` (9) | `commander_image`, `commander_retouche`, `commander_agrandissement`, `importer_image`, `options_retouche`, `lire_rushes`, `image_creation` | [ ] |
| Découvrir et télécharger | `/api/decouvrir*`, `/api/telechargements*`, `/api/workflows/*/telecharger`, `/api/llm/*/installer` (9) | `etat_modeles`, `lire_telechargements`, `telecharger_workflow`, `installer_llm`, `annuler_telechargement`, `familles_decouvrir`, `chercher_modeles`, `fichiers_depot`, `preparer_lien`, `telecharger_lien`, `vignette_modele` | [x] |
| Démarrage, installation, mise à jour | `/api/demarrage`, `/api/installation`, `/api/mise-a-jour*` (9) | squelette | [ ] |
| Emplacements | `/api/emplacements*` (4) | `etat_emplacements`, `ouvrir_dossier`, `parcourir_dossier` (sélecteur natif par `rfd`), `choisir_dossier_modeles` | [x] |
| État, santé, quitter, journal, jetons, usages | `/api/etat`, `/api/sante`, `/api/quitter`, `/api/journal`, `/api/jetons`, `/api/usages*` (8) | | [ ] |
| Fond, notifications, sauvegardes | `/api/fond`, `/api/notifications`, `/api/sauvegardes*` (6) | squelette en partie | [ ] |
| Garde | `/api/garde*` (2) | | [ ] |
| MCP | `/api/mcp` (1) | | [ ] |
| Modèles | `/api/modeles*` (8) | `valider_checkpoint` (`/api/modeles/valider`) | [ ] |
| Moteurs | `/api/moteurs*` (7) | | [ ] |
| Plateau | `/api/plateau*` (2) | `options_plateau` | [ ] |
| Prompt | `/api/prompt*` (8) | réécriture d'une idée par `commander_image` (`reecrire`) | [ ] |
| Réglages | `/api/reglages*` (6) | `etat_reglages`, `modifier_reglages_llm`, `modifier_reglages_workflow`, `modifier_reglages_variante` ; `optimiser` et `recommandations` à porter | [ ] |
| Rushes | `/api/rushes*` (6) | `etat_rushes`, `agir_rushes`, `vider_corbeille`, `creer_dossier`, `renommer_dossier`, `supprimer_dossier` (genre images ; les clips avec le Montage) | [ ] |
| Sécurité | `/api/securite*` (7) | `etat_securite`, `essayer_securite`, `ajouter_terme_securite`, `retirer_terme_securite`, `ajouter_temoin_securite`, `retirer_temoin_securite` ; `promouvoir` retirée (socle embarqué) | [x] |
| Séquences | `/api/sequences*`, `/api/clips` (11) | | [ ] |
| Sources | `/api/sources/*` (1) | | [ ] |
| Voix | `/api/voix*` (7) | | [ ] |

## 6. Hors des pages

| Fonction | v2 | Fait |
| --- | --- | --- |
| Serveur MCP pour Claude | `app/mcp.py`, lancé par Claude, parle à l'API d'Aria sur `127.0.0.1` avec le jeton de `cache\session.json` | [ ] `Aria-Studio.exe --mcp`, canal local à définir |
| Icône de notification et son menu | `tools/launcher/Icone.cs` | [ ] squelette |
| Instance unique | relancer ouvre un nouvel onglet (écart connu) | [ ] squelette : la fenêtre existante revient |
| Installation par l'exe | `tools/launcher/Launcher.cs` | [ ] squelette, plus le cas d'une v2 déjà installée |
| Données embarquées | `catalogue/`, `workflows/`, `prompt/listes/`, `securite/`, `casting/` | [ ] lues par la v3 (lecture YAML à choisir) |
| Licences tierces | `LICENCES-TIERS.txt`, `tests/test_licences_tiers.py` | [ ] |
| Logo et icône Windows avec le clap | `web/logo.svg`, `tools/launcher/aria-studio.ico` | [ ] `docs/logo.svg` et le logo du menu en place ; `appli/icons/icon.ico` reste celle de la famille tant que `tools/icone.ps1` n'a pas tourné (il demande Microsoft Edge) |
| Dependabot sur la v3 | — | [ ] Dependabot ne lit que la branche par défaut : les crates et paquets npm de la v3 ne sont suivis qu'après la fusion dans `main` |
