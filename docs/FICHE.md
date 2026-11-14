# Fiche projet : Aria Studio

Ce que cette appli a de propre. À relire avant toute modification, et à tenir à
jour quand l'appli change. Relevée sur le code de `main` le 2026-11-13 ; la
colonne « Preuve » renvoie au fichier qui fait foi.

| | |
| --- | --- |
| Nom | Aria Studio (dépôt `ZedjeK/Aria-Studio`, exe `Aria-Studio.exe`) |
| Promesse | Un studio de génération d'images et de vidéos 100 % local, piloté depuis le navigateur : un LLM local écrit le prompt, ComfyUI le tourne (`README.md`). |
| Élément orange du logo | Le clap de cinéma (`docs/logo.png`, `web/logo.svg`). |
| Fond animé | Une grille de points que des gouttes en cercles allument au hasard, en cyan, orange et bleu. Position et couleur tirées à chaque passage ; 2 à 5 gouttes par défaut (de 1 à 12), vitesse normale. Zone des pages seulement, jamais sous le menu. En pause quand la page est cachée, sans gouttes si Windows demande moins d'animations. Réglages : Réglages → Système → Fond animé, gardés dans `reglages.json`, section `fond` (`app/fond.py`, `web/fond.js`). |
| Stack | Python 3.11, FastAPI et Uvicorn, interface en HTML/CSS/JS sans framework, ComfyUI portable (images, vidéos), llama.cpp (LLM, GGUF), lanceur C# (`tools/launcher/Launcher.cs`). |
| Droits administrateur | Non. Aucune élévation dans le code, le lanceur ni le build. |
| Démarrage automatique | Aucun : rien n'est installé, ni clé `Run`, ni tâche planifiée, ni service. |
| Installation par l'exe | Oui. Lancé depuis un zip décompressé, le lanceur propose « Installer » ou « Lancer sans installer » : installer copie tout le dossier du zip (lanceur, `python\`, `app\`, `web\`, licences ; environ 185 Mo) dans `%USERPROFILE%\.Aria-Studio\programme`, sans la marque « téléchargé depuis Internet », garde la version d'avant dans `programme.precedent\`, puis relance depuis là (`tools/launcher/Launcher.cs`). Pas de proposition depuis `programme\`, en mode portable, avec `--portable` ou après « Ne plus me proposer » (`installation.proposee` dans `reglages.json`, réactivable dans Réglages → Système). `tools/build_portable.py` installe au même endroit. |
| Port local (interface web) | 8075 sur `127.0.0.1` (`config.yaml`). ComfyUI 8195 et llama-server 8095, aussi sur `127.0.0.1`. Conflit avec `docs/PORTS.md` du dépôt modèle : non vérifié. |
| Données de l'utilisateur | `%USERPROFILE%\.Aria-Studio` : `reglages.json` (tous les réglages et choix : emplacements, favoris, assistant, installation, fond, notifications, mises à jour, LoRA, LLM, workflows ; `app/fichier_reglages.py`), `sauvegardes\` (une copie de `reglages.json` avant chaque changement, et les anciens fichiers de réglages copiés à la migration), `journal.json` (les 500 dernières opérations), `data\` (rushes, clips, séquences, voix, journaux), `cache\` (ce qui se recalcule : empreintes SHA-256, samplers et chemins de ComfyUI), `modeles\`, `moteurs\`, et le programme installé dans `programme\` (version d'avant dans `programme.precedent\`). Mode portable : si `data\` existe à côté du code, tout vit à côté du code (`app/emplacements.py`). |
| Données secrètes | Les clés Civitai et Hugging Face, saisies dans Réglages. Rangées dans le Gestionnaire d'identification Windows (cibles `Aria-Studio/civitai` et `Aria-Studio/huggingface`), jamais dans un fichier d'Aria, jamais renvoyées à l'interface (`app/cles.py`). Repli : variables `CIVITAI_TOKEN` et `HF_TOKEN`. |
| Écritures hors du dossier de données | Voir la liste ci-dessous. |
| Connexions sortantes | Voir la liste ci-dessous. |
| Composants tiers embarqués | Dans le zip : CPython (python-build-standalone), FastAPI, Uvicorn, PyYAML, httpx, websockets, nvidia-ml-py, psutil, Pillow et leurs dépendances, polices Space Grotesk et IBM Plex Mono (OFL, textes dans `web/polices/`). Téléchargés à la demande : ComfyUI et llama.cpp (épinglés dans `catalogue/moteurs.yaml`, avec leur SHA-256), le fork PrismML de llama.cpp, et les modèles, chacun sous sa licence. Les licences de chacun sont dans `LICENCES-TIERS.txt`, que `tests/test_licences_tiers.py` garde à jour. |
| Réussite de la v0.1 | À partir du zip, sur une machine propre : lancer `Aria-Studio.exe`, installer ComfyUI et un LLM depuis l'assistant, puis produire une image depuis le Scénario, sans toucher à un fichier ni à une ligne de commande. Critère fixé par l'auteur le 2026-11-13 ; pas encore mesuré sur une machine propre. |

## Écritures hors du dossier de données

- **Coffre Windows** : les deux clés d'accès (Gestionnaire d'identification, persistance sur la machine).
- **Mode portable** : si `data\` existe à côté du code, reglages.json, data, modèles et services y restent.
- **Dossier des modèles choisi** par l'utilisateur dans l'onglet Modèles (`reglages.json`, `emplacements.modeles`). Sa corbeille vit dedans, dans `.corbeille\` (`app/modeles.py`) : un modèle supprimé y est renommé, sans copie entre disques.
- **ComfyUI externe** (« J'ai déjà ComfyUI ») : Aria y téléverse des fichiers dans `input` (`app/comfy.py`), et ComfyUI y écrit ses propres dossiers. Ces écritures sont décidées par l'utilisateur ; Aria ne les nettoie pas.
- **Les moteurs eux-mêmes** (ComfyUI, llama-server, Python) peuvent écrire dans `%TEMP%` ou un cache : non vérifié.
- **Raccourci du Bureau** : « Aria Studio.lnk », seulement si la case est cochée à l'installation (décochée d'office). Bureau de l'utilisateur seulement (jamais le Bureau public), vers `programme\Aria-Studio.exe` ; un raccourci déjà là n'est mis à jour qu'après une question.

Rien dans `%APPDATA%`, `%LOCALAPPDATA%`, le registre, une tâche planifiée ou un service, dans le code d'`app/`.

## Connexions sortantes

| Destination | Pourquoi | Quand |
| --- | --- | --- |
| `api.github.com` (`/repos/ZedjeK/Aria-Studio/releases/latest`) | Chercher une nouvelle version. Une requête anonyme, rien d'autre. | Au démarrage si la dernière vérification a plus d'un jour (coupable dans Réglages) ; bouton « Vérifier maintenant ». |
| `github.com` (`/<dépôt>/releases/download/…`) | Télécharger ComfyUI et llama.cpp ; SHA-256 vérifié contre `catalogue/moteurs.yaml`. | Sur clic (assistant, onglet Moteurs). |
| `huggingface.co` | Recherche, fiches, README de dépôt, téléchargement des modèles et des LLM, test de clé. | Sur action de l'utilisateur. |
| `civitai.com` et `civitai.red` | Recherche, fiches, téléchargement, test de clé, « Retrouver les sources » par empreinte SHA-256. | Sur action de l'utilisateur. |
| CDN d'images de Civitai (adresse venue de l'API) | Vignettes des résultats, chargées par le navigateur, sans `Referer`. | Quand l'utilisateur ouvre la recherche. |

Au build seulement : `github.com/astral-sh/python-build-standalone` (Python portable, SHA-256 vérifié).

Aucune télémétrie, rien n'est envoyé à l'auteur. Les appels éventuels de ComfyUI et de llama-server eux-mêmes : non vérifiés.

## Écarts connus au contrat des applis Aria

Ils sont décrits ici pour que personne ne les découvre par accident ; chacun est
traité par une PR à part ou fera l'objet d'une dérogation écrite.

- **Fichiers du dossier de données** : `reglages.json` et `cache\` sont en place ; `sauvegardes\` reçoit une copie de `reglages.json` avant chaque changement (les 20 dernières, restaurables dans Réglages → Système) ; `journal.json` garde les 500 dernières opérations (téléchargements, moteurs, corbeilles, migrations, réglages). Les anciens fichiers de réglages (`aria.yaml`, `data/reglages.yaml`, `data/fond.json`, `data/notifications.json`, `data/mise_a_jour.json`, `data/loras.json`) restent à leur place après la migration, sans être lus, pour qu'une version d'avant d'Aria tourne encore.
- **Clés d'accès** : dans le coffre Windows, donc chiffrées au niveau du compte, mais hors du dossier de données.
- **Réseau local** : contrôle de `Host` et d'`Origin`, aucun CORS, actions en POST/PUT/DELETE ; mais pas de jeton par lancement, et llama-server et ComfyUI n'ont pas de clé d'accès.
- **Mises à jour** : l'appli signale la version et le lien ; elle ne télécharge ni n'installe rien.
- **Icône de notification** : aucune ; le serveur reste actif en fond après l'ouverture du navigateur, et le bouton « Quitter » de la page l'arrête.
- **Installation par l'exe, vérification des droits** (dérogation, décision de l'auteur, 2026-11-13) : le contrat demande de refuser l'installation si le dossier de données est modifiable par un programme sans droits. Cette règle protège un exe lancé en administrateur ; Studio n'en demande jamais, et son dossier doit rester modifiable par l'utilisateur pour que modèles et créations s'y écrivent. La vérification n'est donc pas faite.
- **Releases** : le workflow atteste la provenance du zip, mais l'attestation n'a pas encore été éprouvée sur un tag (l'essai à blanc a validé la construction seule) ; les releases immuables sont désactivées sur le dépôt.
