# Fiche projet : Aria Studio

Ce que cette appli a de propre. À relire avant toute modification, et à tenir à jour quand l'appli change. Relevée sur le
code de `refonte/v3` (la version 3, Rust et Tauri 2) le 2026-11-26 ; chaque ligne renvoie au fichier qui fait foi, et ce qui
n'a pas pu être lu est écrit « non vérifié ». La fiche de la v2 (Python) est celle de `main` : les écarts entre les deux sont
à la fin.

| | |
| --- | --- |
| Nom | Aria Studio (dépôt `ZedjeK/Aria-Studio`, exe `Aria-Studio.exe`, identifiant `io.github.zedjek.aria-studio`) : `coeur/src/identite.rs`, `appli/tauri.conf.json`. |
| Promesse | Un studio de génération d'images et de vidéos 100 % local : un LLM local écrit le prompt, ComfyUI le tourne, dans une fenêtre à lui, sans navigateur (`README.md`, `appli/tauri.conf.json`). |
| Élément orange du logo | Le clap de cinéma (`docs/logo.svg`, source unique : son commentaire d'en-tête ; `docs/logo.png` et `interface/src/lib/logo.svg` en sont tirés). L'icône Windows `appli/icons/icon.ico` est encore celle de la famille : `tools/icone.ps1` n'a pas tourné (voir les écarts). |
| Fond animé | Une grille de points que des gouttes, des cercles qui grandissent, allument au hasard en cyan, en bleu et en orange ; leur place, leur départ, leur taille et leur couleur sont tirés au hasard à chaque passage. Un passage dure 14 s (lente), 7 s (normale) ou 3,5 s (rapide) ; 2 à 5 gouttes par défaut, de 1 à 12 ; vitesse normale ; actif d'office. Zone des pages seulement, jamais sous le menu ; en pause quand la fenêtre est cachée, coupé si Windows demande moins d'animations. Réglages : Paramètres → Fond animé, gardés dans `reglages.json`, section `fond` (`actif`, `vitesse`, `minimum`, `maximum`). C'est le motif propre à Studio, qui remplace celui par défaut du modèle (`interface/src/lib/composants/FondGouttes.svelte`, calcul dans `interface/src/lib/gouttes.ts`, règles communes dans `interface/src/lib/fond.ts`, réglages dans `coeur/src/reglages.rs`). |
| Stack | Rust 1.100.0, édition 2024 (`rust-toolchain.toml`, `Cargo.toml`) ; fenêtre Tauri 2.12 sur WebView2 ; interface Svelte 5.59, TypeScript 6, Vite 8 (`interface/package.json`) ; HTTPS par `ureq` avec les certificats de Windows (`appli/Cargo.toml`), WebSocket par `tungstenite` (`coeur/Cargo.toml`) ; YAML par `serde-saphyr`. Moteurs téléchargés : ComfyUI portable v0.42.1, llama.cpp b11734 et le fork PrismML (`catalogue/moteurs.yaml`). Un seul exe, rien à extraire au lancement (`build.ps1`). |
| Droits administrateur | Non. Aucune élévation dans le code ni dans le build : `appli/build.rs` et `appli/Cargo.toml` ne demandent aucun niveau d'exécution (recherche de `requireAdministrator` sans résultat) ; le manifeste Windows que Tauri ajoute à l'exe n'a pas été relu : non vérifié. |
| Démarrage automatique | Désactivé par défaut. « Démarrer avec Windows » (menu de l'icône, ou Paramètres → Démarrage) écrit la valeur « Aria Studio » sous la clé `Run` de l'utilisateur (`HKCU`), vers l'exe en cours, avec `--silencieux` : l'icône seule, sans fenêtre. L'état est relu dans le registre à chaque affichage, une désactivation faite dans Windows est respectée ; après une installation, la valeur vise l'exe installé. Ni tâche planifiée, ni service (`coeur/src/demarrage.rs`, `appli/src/demarrage_auto.rs`, `interface/src/MenuIcone.svelte`). |
| Installation par l'exe | Oui. Lancé depuis un zip décompressé (avec son `fichiers.txt` à côté), l'exe propose « Installer » ou « Lancer sans installer » : installer copie ce que liste `fichiers.txt` (l'exe, `LICENCES-TIERS.txt`, `BUILDINFO.txt`) dans `%USERPROFILE%\.Aria-Studio\programme`, sans la marque « téléchargé depuis Internet », garde la version 3 d'avant dans `sauvegardes\versions\` (une v2 installée est gardée entière dans `programme.precedent\`), puis relance depuis là. Pas de proposition depuis `programme\`, avec `--portable`, avec un dossier `data\` à côté de l'exe, ni après « Ne plus me proposer » (`installation.proposee` dans `reglages.json`, réactivable dans Paramètres → Installation). Le raccourci du Bureau est une case décochée d'office (`coeur/src/installation.rs`, `appli/src/installation.rs`, `interface/src/Installation.svelte`). |
| Port local | Aucun port pour l'interface : la fenêtre parle à l'exe par le canal interne de Tauri, pas par HTTP (`appli/tauri.conf.json` : la CSP est `default-src 'self' ipc:`, sans `connect-src`). ComfyUI sur `127.0.0.1:8195` et llama-server sur `127.0.0.1:8095` (`coeur/src/comfy.rs`, `coeur/src/llama.rs`), sans `--enable-cors-header`. Le canal du MCP écoute sur `127.0.0.1`, sur un port choisi par Windows à chaque lancement (`coeur/src/mcp.rs`). `docs/PORTS.md` du dépôt modèle liste 8095 et 8195 pour Aria Studio, qui concordent, mais aussi 8075, que la v3 n'écoute plus : à retirer là-bas. |
| Données de l'utilisateur | `%USERPROFILE%\.Aria-Studio` : `reglages.json` (tous les réglages et choix), `journal.json` (les 500 dernières opérations), `sauvegardes\` (une copie de `reglages.json` avant chaque changement, les 20 dernières ; `versions\` : la version d'avant du programme), `cache\` (ce qui se recalcule, le cache de WebView2, `cache\comfyui\`, `cache\session.json` du MCP, `cache\mise-a-jour\`), `secrets\`, `data\` (rushes, clips, séquences, voix, personnages, castings, journaux des moteurs), `modeles\`, `moteurs\`, `programme\`. Mode portable : si `data\` existe à côté de l'exe (et que l'exe n'est pas dans `.Aria-Studio`), tout vit à côté de l'exe et les moteurs vont dans `services\` (`coeur/src/donnees.rs`, `coeur/src/emplacements.rs`). |
| Données secrètes | Les clés Civitai et Hugging Face, saisies dans Réglages → Emplacements et clés. Chiffrées par DPAPI pour le compte de l'utilisateur (avec le nom de l'exe comme entropie), un fichier `.dpapi` par secret dans `secrets\` ; jamais en clair dans `reglages.json`, le journal ni les sauvegardes, jamais renvoyées à l'interface (qui ne reçoit que l'origine de la clé) ni au LLM (`aucun_secret_ne_part`). Les clés de la v2 (Gestionnaire d'identification Windows, cibles `Aria-Studio/civitai` et `Aria-Studio/huggingface`) sont recopiées dans `secrets\` au lancement, sans rien retirer du coffre Windows. Repli : variables `CIVITAI_TOKEN` et `HF_TOKEN`. La protection ne joue pas contre un programme lancé sous le même compte Windows (`coeur/src/secrets.rs`, `coeur/src/cles.rs`, `coeur/src/expliquer.rs`). |
| Écritures hors du dossier de données | Voir la liste ci-dessous. |
| Connexions sortantes | Voir la liste ci-dessous. |
| Composants tiers embarqués | Dans l'exe : les polices Space Grotesk et IBM Plex Mono (SIL OFL 1.1), le chargeur WebView2 (`webview2-com`, licence Microsoft), les graphes de workflow dérivés des modèles de ComfyUI (MIT, Comfy Org), les crates compilées dans l'exe et les paquets npm de l'interface. Téléchargés à la demande : ComfyUI, llama.cpp, le fork PrismML de llama.cpp (épinglés dans `catalogue/moteurs.yaml`, avec leur SHA-256), les modèles et les LLM, chacun sous sa licence. Tout est dans `LICENCES-TIERS.txt`, que `tools/licences-tierces.ps1` régénère et que `tools/tester.ps1` contrôle. |
| Réussite de la v0.1 | À partir du zip, sur une machine propre : lancer `Aria-Studio.exe`, installer ComfyUI et un LLM depuis l'assistant, puis produire une image depuis le Scénario, sans toucher à un fichier ni à une ligne de commande. Critère fixé par l'auteur le 2026-11-13 pour la v2, repris pour la v3 ; pas encore mesuré sur une machine propre (`docs/migration/ESSAI-V3.md`, essai rapide). |

## Écritures hors du dossier de données

- **Registre Windows** : la valeur « Aria Studio » de `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, seulement si
  « Démarrer avec Windows » est coché (décoché d'office) ; décocher la retire (`coeur/src/demarrage.rs`).
- **Raccourci du Bureau** : « Aria Studio.lnk », seulement si la case est cochée à l'installation (décochée d'office).
  Bureau de l'utilisateur seulement, vers `programme\Aria-Studio.exe` ; un raccourci déjà là n'est mis à jour que si la case
  « Mettre à jour le raccourci du Bureau » est cochée (`appli/src/installation.rs`).
- **Dossier des modèles** choisi par l'utilisateur (Réglages → Emplacements et clés, `emplacements.modeles`). Sa corbeille
  vit dedans, dans `.corbeille\` : un modèle supprimé y est renommé (`coeur/src/collection.rs`). La v3 crée dans ce dossier les dossiers de
  famille qui manquent (`<nature>\<Famille>\modeles`, `loras`…, `coeur/src/modeles.rs`), y compris dans les modèles d'une
  v2 reprise pour un essai ; rien d'autre n'y est écrit par la reprise (`coeur/src/reprise.rs`).
- **Mise à jour intégrée** : elle remplace l'exe et ses fichiers là où il tourne. Installée, c'est `programme\` ; lancée
  depuis un zip décompressé ou en mode portable, c'est le dossier de l'exe, hors du dossier de données. La version
  d'avant est gardée dans `sauvegardes\versions\` (`coeur/src/mises_a_jour.rs`, `appli/src/mise_a_jour.rs`).
- **ComfyUI externe** (« J'ai déjà ComfyUI ») : Aria n'écrit rien dans son dossier. Elle le lance avec ses dossiers
  d'entrées, de sorties et de temporaires dans `cache\comfyui\` : les entrées et les temporaires y sont vidés avant chaque
  lancement, une sortie est effacée dès qu'Aria l'a récupérée (`coeur/src/comfy.rs`). ComfyUI peut encore écrire lui-même
  dans son dossier `user\`, et recréer `input\` s'il manque : non vérifié en détail.
- **Les moteurs eux-mêmes** (ComfyUI, llama-server, leur Python) peuvent écrire dans `%TEMP%` ou un cache : non vérifié.
- **Notifications Windows** (la fin d'une image, d'une retouche, d'un clip, d'une séquence ou d'un casting) : une
  notification locale de Windows (WinRT), qui ne passe par aucun réseau ; Aria n'écrit rien pour elle. Elle emprunte
  l'identité de PowerShell, que Windows connaît toujours : son en-tête porte ce nom, et Windows y range lui-même son
  réglage de notification, ce qui n'est pas du fait d'Aria. Les deux choix (`notifications.active`, `notifications.son`)
  vivent dans `reglages.json` (`appli/src/notifications.rs`).
- **L'Explorateur et le navigateur** sont lancés, sans rien écrire, pour ouvrir un dossier d'Aria ou la page d'une release
  ou d'un modèle (`appli/src/lieux.rs`, `appli/src/mise_a_jour.rs`).
- **Le cache de WebView2** est dans `cache\` du dossier de données (test `le_cache_de_webview2_est_dans_le_dossier_de_donnees`
  de `coeur/src/donnees.rs`) ; que WebView2 n'écrive rien d'autre ailleurs : non vérifié.

Rien dans `%APPDATA%`, `%LOCALAPPDATA%`, une tâche planifiée ou un service, dans le code d'`appli/` et de `coeur/` ; dans le
registre, seulement la valeur de démarrage ci-dessus.

## Connexions sortantes

Toutes en HTTPS, avec les certificats et le proxy de Windows (`appli/src/mise_a_jour.rs`, `appli/src/telechargements.rs`,
`appli/src/moteurs.rs`), sous l'agent `Aria-Studio/<version>`. Les redirections de ces hôtes vers leurs serveurs de fichiers
sont suivies, sans liste d'hôtes dans le code ; l'empreinte SHA-256 fait foi.

| Destination | Pourquoi | Quand |
| --- | --- | --- |
| `api.github.com` (`/repos/ZedjeK/Aria-Studio/releases/latest`) | Chercher une nouvelle version. Une requête anonyme, rien d'autre. Une préversion ou un brouillon n'est jamais retenu. | Au lancement si « Chercher une nouvelle version au lancement » est coché (Paramètres → Mises à jour, coché d'office) ; bouton « Vérifier maintenant ». |
| `github.com` (`/ZedjeK/Aria-Studio/releases/download/…`, puis le serveur de fichiers de GitHub) | Mise à jour intégrée : télécharger le zip de la dernière release ; sa taille et son SHA-256 sont ceux annoncés par GitHub. Adresse refusée si elle ne commence pas par ce préfixe. | Sur le clic « Installer la mise à jour », après confirmation. |
| `github.com` (releases de ComfyUI, de llama.cpp et de PrismML, adresses de `catalogue/moteurs.yaml`) | Télécharger les moteurs ; SHA-256 vérifié contre le catalogue. | Sur clic (assistant, Réglages → Moteurs). |
| `huggingface.co` (`/api/models/…`, `/<dépôt>/resolve/…`, `/<dépôt>/raw/…/README.md`, `/api/whoami-v2`) | Recherche de modèles, fichiers et README d'un dépôt, téléchargement des modèles et des LLM (SHA-256 des fiches), test de la clé. | Sur action de l'utilisateur (recherche, téléchargement, lien collé, « Tester »). |
| `civitai.com` (`/api/v1/…`, `/api/download/models/…`, `/api/v1/me`) | Recherche, fiches (mots déclencheurs, force des LoRA), téléchargement, test de la clé, « Retrouver les sources » par empreinte SHA-256. | Sur action de l'utilisateur. |
| `image.civitai.com` | Vignettes des résultats de recherche, lues par l'exe (la CSP de la fenêtre n'accepte que `'self'` et `blob:`), seulement si l'adresse commence par `https://image.civitai.com/`. | Quand l'utilisateur ouvre la recherche de modèles. |
| `127.0.0.1` | ComfyUI (HTTP et WebSocket), llama-server et le canal du MCP : pas une sortie du PC. | En marche. |

`civitai.red` n'est jamais appelé par Aria : seule la page d'un modèle à contenu adulte est ouverte dans le navigateur
(`coeur/src/decouvrir.rs`, `page_civitai`) ; un lien `civitai.red` collé est lu par l'API de `civitai.com`.

Aucune télémétrie, rien n'est envoyé à l'auteur. Ce qui n'est pas vérifié : les appels éventuels de ComfyUI et de
llama-server eux-mêmes ; ceux de WebView2 (le moteur web de Microsoft) vers Microsoft ; la façon dont `ureq` suit une
redirection vers un autre hôte au-delà de ce que dit le commentaire de `appli/src/telechargements.rs` (la clé n'est pas
renvoyée).

**Serveur MCP pour Claude** (`Aria-Studio.exe --mcp`, à déclarer soi-même dans Claude, commande dans Réglages → Claude
(MCP)) : pas une connexion d'Aria. Claude lance l'exe et lui parle sur l'entrée et la sortie standard ; le relais lit le port
et le jeton dans `cache\session.json` et n'appelle que le canal local d'Aria sur `127.0.0.1`. Aucun port ni secret en plus.
Ce que ses outils rendent (état, prompts écrits, fiches des images : idée, prompt, modèle, seed) part ensuite vers Claude,
donc vers le compte Claude de l'utilisateur ; les images elles-mêmes ne sont pas rendues, seulement leur chemin. Outils
limités à lire, écrire un prompt et lancer un rendu : rien ne supprime, ne déplace, ne règle ni n'installe
(`coeur/src/mcp.rs`, `appli/src/mcp.rs`).

## Écarts connus au contrat des applis Aria

Ils sont décrits ici pour que personne ne les découvre par accident ; chacun est traité par une PR à part ou fera l'objet
d'une dérogation écrite.

- **Fond animé** : le motif est celui de Studio (une grille de points allumée par des gouttes), qui remplace le motif par
  défaut du modèle : c'est une différence voulue, que le contrat prévoit (chaque appli dit ce qu'elle fait). Les quatre
  réglages obligatoires existent. Le motif n'a pas encore été vu dans l'exe : seul le calcul est testé (`gouttes.test.ts`).
- **Logo et icône** : `tools/icone.ps1` n'a pas tourné (il demande Microsoft Edge) : `appli/icons/icon.ico` est l'icône de
  la famille, pas celle d'Aria Studio avec son clap orange.
- **Tout se restaure** : une copie de `reglages.json` est faite avant chaque changement, mais aucune page ne la restaure
  (la v2 avait Réglages → Historique). Le journal ne se restaure pas non plus.
- **Clés de la v2** : elles sont recopiées dans `secrets\`, mais l'entrée du Gestionnaire d'identification Windows reste
  en place (hors du dossier de données) et aucune commande ne la retire encore ; elle ne devait l'être qu'avec l'accord
  de l'utilisateur.
- **Réseau local** : pas de serveur web, donc ni `Host`, ni `Origin`, ni CORS à contrôler pour l'interface. Le canal du
  MCP écoute sur la boucle locale seulement et exige un jeton neuf à chaque lancement, mais ce jeton est lisible dans
  `cache\session.json` par un programme du même compte. Dérogation technique, comme en v2 : ComfyUI n'a aucune option de clé
  d'accès ; il garde son refus des autres sites du navigateur (Aria ne lui passe jamais `--enable-cors-header`, un test le
  vérifie), mais un programme local peut lui parler. llama-server reçoit une clé neuve à chaque démarrage
  (`LLAMA_API_KEY`, jamais écrite).
- **Mises à jour** : l'exe vérifie la taille et le SHA-256 annoncés par GitHub, pas l'attestation de provenance (la
  provenance repose sur le compte de l'auteur, les releases immuables et l'attestation jointe ; le README le dit). Les
  releases immuables sont activées sur le dépôt (relu le 2026-11-26 : `enabled: true`), et le zip de la 2.4.0 porte deux
  attestations selon l'API de GitHub ; elles n'ont pas été vérifiées avec `gh attestation verify` ici.
- **Installation par l'exe** : le contrat veut qu'elle se teste par un vrai clic sur « Installer » avant la release : ce
  clic n'a pas été fait (les tests couvrent le noyau, sur des dossiers de test). Le raccourci du Bureau n'a aucun test. La
  vérification des droits du dossier n'est pas faite, et le contrat ne l'exige pas : l'appli n'est jamais élevée.
- **Pont de mise à jour depuis la v2** : jamais essayé sur une vraie v2 installée (`tools/essai-pont-v2.ps1` et
  `docs/migration/ESSAI-PONT.md` sont prêts, pas rejoués).
- **Fonctions jamais essayées en vrai** : celles qui passent par un vrai LLM (contrôle qualité, directions, vérifier,
  choisir, expliquer), le menu de l'icône et le serveur MCP branché à Claude Code (`docs/migration/PARITE.md`).

## Ce qui a changé depuis la fiche de la v2

- **Stack** : Python, FastAPI, interface web dans le navigateur et lanceur C# remplacés par un exe Rust (Tauri 2) avec une
  interface Svelte 5 dans sa propre fenêtre.
- **Port local** : le port 8075 de l'interface n'existe plus ; il reste ComfyUI (8195) et llama-server (8095), plus un
  canal de MCP à port aléatoire.
- **Secrets** : du Gestionnaire d'identification Windows vers DPAPI dans `secrets\`, dans le dossier de données.
- **Instance unique** : relancer l'exe ramène la fenêtre existante au lieu d'ouvrir un nouvel onglet.
- **Réseau** : plus de jeton, de `Host` ni d'`Origin` pour l'interface ; les vignettes de Civitai passent par l'exe, plus
  par le navigateur ; la mise à jour ne télécharge plus de `.sha256` mais compare l'empreinte annoncée par GitHub.
- **Mises à jour** : au lancement, quand le réglage est coché, plus « si la dernière vérification a plus d'un jour ».
- **Démarrage automatique** : plus réservé à Aria installée dans `programme\` ; il vise l'exe en cours.
- **Écrit en plus** : l'écriture de la mise à jour dans le dossier de l'exe quand il n'est pas installé.
- **Licences** : `LICENCES-TIERS.txt` est généré (crates, paquets npm) au lieu d'être gardé par un test Python.
