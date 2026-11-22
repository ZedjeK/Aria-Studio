# Migration vers Rust, Tauri 2 et Svelte 5 : table de parité

Aria Studio 2.x (Python, FastAPI, interface web servie sur `127.0.0.1:8075`, lanceur C#) passe sur le squelette
`rust-tauri` du dépôt modèle. Ce fichier dit ce que la version 3 doit refaire pour égaler la 2.4, et ce qui est fait.
Il vit sur la branche d'intégration et se coche PR par PR. Relevé sur `main` le 2026-11-22 (`d78ca07`).

Une ligne n'est cochée que si la fonction marche dans la v3 **et** que ses tests y sont portés.

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
| Créations, rushes, clips, séquences, voix | `data\` | relus | [ ] |
| Cache | `cache\` (empreintes SHA-256, samplers, chemins ComfyUI) | recalculable | [ ] |
| Modèles et moteurs | `modeles\`, `moteurs\`, dossier des modèles choisi | mêmes chemins décidés au lancement, rien de déplacé (`coeur/src/emplacements.rs`) ; leur contenu se lit avec les lots des moteurs et des modèles | [ ] |
| Clés Civitai et Hugging Face | Gestionnaire d'identification Windows (`Aria-Studio/civitai`, `Aria-Studio/huggingface`), repli `CIVITAI_TOKEN` et `HF_TOKEN` | recopiées au lancement, chiffrées (DPAPI), dans `secrets\` si elles n'y sont pas déjà ; l'entrée du coffre reste en place pour une v2 restaurée et ne sera retirée qu'avec l'accord de l'utilisateur (`coeur/src/cles.rs`, `appli/src/main.rs`) ; saisie, test et retrait attendent la page Réglages | [ ] |
| Démarrage avec Windows | valeur `Run` « Aria Studio » (HKCU) | même valeur, nouvelle cible | [ ] |
| Raccourci du Bureau | `Aria Studio.lnk` | proposé en mise à jour, jamais remplacé en silence | [ ] |
| Mode portable | `data\` à côté du code, sauf code dans `.Aria-Studio` ; moteurs dans `services\` ; pas de proposition d'installation | même règle (`coeur/src/emplacements.rs`, `appli/src/main.rs`) | [x] |
| Anciens fichiers de réglages (`aria.yaml`, `data\*.json`…) | versés dans `reglages.json` une fois (clé `migration`), laissés en place | laissés en place, pas relus : la v3 n'arrive que par la mise à jour d'une v2, qui a déjà fait cette migration à son lancement | [x] |

## 3. Pages

| Famille | Page | Sous-onglets et panneaux | Modules v2 | Fait |
| --- | --- | --- | --- | --- |
| — | Assistant de premier lancement | plan, licences, téléchargement, essai | `assistant.py`, `web/assistant.js`, `web/guide.js` | [ ] |
| Création | Scénario | idée, listes, personnages, prompt final, traduction, optimisation, découpage en séquences | `prompt.py`, `listes.py`, `personnages.py`, `optimiseur.py`, `decoupage.py`, `routes_scenario.py`, `web/scenario.js`, `web/libre.js`, `web/decoupage.js`, `web/references.js` | [ ] |
| Création | Plateau | créer, options, estimation, paliers, rushes, résultat, agrandissement | `plateau.py`, `paliers.py`, `estimation.py`, `rushes.py`, `orchestrateur.py`, `web/plateau.js`, `web/galerie.js` | [ ] |
| Création | Retouche | rushes, retoucher | `retouche.py`, `web/retouche.js` | [ ] |
| Création | Montage | tourner un clip, bobines, séquences, voix, corbeille | `sequences.py`, `voix.py`, `repliques.py`, `routes_montage.py`, `web/montage.js`, `web/sequence.js`, `web/voix.js` | [ ] |
| Préparation | Modèles | Rechercher, Ma collection, Texte → image, Image → image, Vidéo ; téléchargements, corbeille, moteurs | `modeles.py`, `collection.py`, `decouvrir.py`, `telechargement.py`, `sources/`, `fiches.py`, `gguf.py`, `migration_modeles.py`, `moteurs.py`, `web/modeles.js`, `web/decouvrir.js`, `web/loras.js` | [ ] |
| Préparation | Casting | Nouveau casting, Comparer | `casting.py`, `web/casting.js` | [ ] |
| Préparation | Réglages | emplacements, clés, sécurité (protection des mineurs), mises à jour, démarrage, notifications, fond animé, Claude (MCP), sauvegardes, journal, LLM, modèles image | `reglages.py`, `fichier_reglages.py`, `emplacements.py`, `cles.py`, `securite.py`, `garde.py`, `notifications.py`, `demarrage.py`, `fond.py`, `journal.py`, `web/reglages.js` | [ ] |
| — | Quitter | arrêt d'Aria, de ComfyUI et de llama-server | `services.py` | [ ] |

## 4. Moteurs

| Moteur | v2 | Fait |
| --- | --- | --- |
| Catalogue des moteurs | `catalogue/moteurs.yaml` lu par `app/moteurs.py` | [x] même fichier, embarqué dans l'exe et contrôlé à la lecture (`coeur/src/catalogue.rs`) ; YAML lu par `serde-saphyr` |
| ComfyUI portable | téléchargé (`catalogue/moteurs.yaml`, SHA-256), lancé sur `127.0.0.1:8195`, piloté en HTTP et WebSocket (`comfy.py`) ; mode externe « J'ai déjà ComfyUI » | [ ] `coeur/src/comfy.rs` : arguments sans CORS, dossiers de travail, préfixes uniques, événements, historique, effacement des sorties, orphelins, VRAM stable ; processus, HTTP, WebSocket et mode externe avec l'orchestrateur |
| llama-server | téléchargé, lancé sur `127.0.0.1:8095` avec une clé neuve à chaque démarrage (`llm.py`) ; fork PrismML | [ ] `coeur/src/llama.rs` : arguments, clé par `LLAMA_API_KEY`, corps des demandes, lecture du flux, messages ; processus et HTTP avec l'orchestrateur |
| Installation d'un moteur | `moteurs.py` : état (absent, manuel, différent, à jour, externe), archives dans `_archives\` avec reprise et SHA-256, zip et 7z (tar de Windows puis 7-Zip), `.new` puis échange sous verrou, place nécessaire | [ ] `coeur/src/moteurs.rs` fait ; branchement réseau, verrou et commandes avec l'onglet Moteurs |
| Retour à la version précédente d'un moteur | `moteurs.py` | [ ] `coeur/src/moteurs.rs::revenir` fait ; commande avec l'onglet Moteurs |
| Libérer la mémoire vidéo | `moteurs.py`, `mesures.py` | [ ] |

## 5. API v2 et son équivalent v3

La v2 expose 138 routes HTTP. En v3, l'interface appelle des commandes Tauri : plus de serveur web, plus de port
8075, plus de jeton ni de contrôle `Host` et `Origin` pour l'interface. Le tableau se remplit domaine par domaine.

| Domaine v2 | Routes | Commandes v3 | Fait |
| --- | --- | --- | --- |
| Assistant | `/api/assistant*` (6) | | [ ] |
| Casting | `/api/bancs*` (10) | | [ ] |
| Clés | `/api/cles*` (4) | | [ ] |
| Collection et LoRA | `/api/collection*` (6) | | [ ] |
| Commandes en cours | `/api/commandes*` (2) | | [ ] |
| Créations | `/api/creations*`, `/api/agrandissements`, `/api/retouches` (9) | | [ ] |
| Découvrir et télécharger | `/api/decouvrir*`, `/api/telechargements*`, `/api/workflows/*/telecharger`, `/api/llm/*/installer` (9) | | [ ] |
| Démarrage, installation, mise à jour | `/api/demarrage`, `/api/installation`, `/api/mise-a-jour*` (9) | squelette | [ ] |
| Emplacements | `/api/emplacements*` (4) | | [ ] |
| État, santé, quitter, journal, jetons, usages | `/api/etat`, `/api/sante`, `/api/quitter`, `/api/journal`, `/api/jetons`, `/api/usages*` (8) | | [ ] |
| Fond, notifications, sauvegardes | `/api/fond`, `/api/notifications`, `/api/sauvegardes*` (6) | squelette en partie | [ ] |
| Garde | `/api/garde*` (2) | | [ ] |
| MCP | `/api/mcp` (1) | | [ ] |
| Modèles | `/api/modeles*` (8) | | [ ] |
| Moteurs | `/api/moteurs*` (7) | | [ ] |
| Plateau | `/api/plateau*` (2) | | [ ] |
| Prompt | `/api/prompt*` (8) | | [ ] |
| Réglages | `/api/reglages*` (6) | | [ ] |
| Rushes | `/api/rushes*` (6) | | [ ] |
| Sécurité | `/api/securite*` (7) | | [ ] |
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
