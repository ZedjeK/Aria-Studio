# Fiche projet : Aria Studio

Ce que cette appli a de propre. À relire avant toute modification, et à tenir à
jour quand l'appli change. Relevée sur le code de `main` le 2026-11-13 ; la
colonne « Preuve » renvoie au fichier qui fait foi.

| | |
| --- | --- |
| Nom | Aria Studio (dépôt `ZedjeK/Aria-Studio`, exe `Aria-Studio.exe`) |
| Promesse | Un studio de génération d'images et de vidéos 100 % local, piloté depuis le navigateur : un LLM local écrit le prompt, ComfyUI le tourne (`README.md`). |
| Élément orange du logo | Le clap de cinéma (`docs/logo.png`, `web/logo.svg`). |
| Fond animé | Une grille de points que des gouttes en cercles allument au hasard, en cyan, orange et bleu. Position et couleur tirées à chaque passage ; 2 à 5 gouttes par défaut (de 1 à 12), vitesse normale. Zone des pages seulement, jamais sous le menu. En pause quand la page est cachée, sans gouttes si Windows demande moins d'animations. Réglages : Réglages → Système → Fond animé, gardés dans `data/fond.json` (`app/fond.py`, `web/fond.js`). |
| Stack | Python 3.11, FastAPI et Uvicorn, interface en HTML/CSS/JS sans framework, ComfyUI portable (images, vidéos), llama.cpp (LLM, GGUF), lanceur C# (`tools/launcher/Launcher.cs`). |
| Droits administrateur | Non. Aucune élévation dans le code, le lanceur ni le build. |
| Démarrage automatique | Aucun : rien n'est installé, ni clé `Run`, ni tâche planifiée, ni service. |
| Port local (interface web) | 8075 sur `127.0.0.1` (`config.yaml`). ComfyUI 8195 et llama-server 8095, aussi sur `127.0.0.1`. Conflit avec `docs/PORTS.md` du dépôt modèle : non vérifié. |
| Données de l'utilisateur | `%USERPROFILE%\.Aria-Studio` : `aria.yaml` (choix, favoris, licences acceptées), `data\` (rushes, clips, séquences, voix, réglages, journaux), `modeles\`, `moteurs\`. Mode portable : si `data\` existe à côté du code, tout vit à côté du code (`app/emplacements.py`). |
| Données secrètes | Les clés Civitai et Hugging Face, saisies dans Réglages. Rangées dans le Gestionnaire d'identification Windows (cibles `Aria-Studio/civitai` et `Aria-Studio/huggingface`), jamais dans un fichier d'Aria, jamais renvoyées à l'interface (`app/cles.py`). Repli : variables `CIVITAI_TOKEN` et `HF_TOKEN`. |
| Écritures hors du dossier de données | Voir la liste ci-dessous. |
| Connexions sortantes | Voir la liste ci-dessous. |
| Composants tiers embarqués | Dans le zip : CPython (python-build-standalone), FastAPI, Uvicorn, PyYAML, httpx, websockets, nvidia-ml-py, psutil, Pillow et leurs dépendances, polices Space Grotesk et IBM Plex Mono (OFL, textes dans `web/polices/`). Téléchargés à la demande : ComfyUI et llama.cpp (épinglés dans `catalogue/moteurs.yaml`, avec leur SHA-256), le fork PrismML de llama.cpp, et les modèles, chacun sous sa licence. `LICENCES-TIERS.txt` : à venir (PR suivante). |
| Réussite de la v0.1 | À partir du zip, sur une machine propre : lancer `Aria-Studio.exe`, installer ComfyUI et un LLM depuis l'assistant, puis produire une image depuis le Scénario, sans toucher à un fichier ni à une ligne de commande. Critère fixé par l'auteur le 2026-11-13 ; pas encore mesuré sur une machine propre. |

## Écritures hors du dossier de données

- **Coffre Windows** : les deux clés d'accès (Gestionnaire d'identification, persistance sur la machine).
- **Mode portable** : si `data\` existe à côté du code, aria.yaml, data, modèles et services y restent.
- **Dossier des modèles choisi** par l'utilisateur dans l'onglet Modèles (`aria.yaml`, clé `modeles`).
- **ComfyUI externe** (« J'ai déjà ComfyUI ») : Aria y téléverse des fichiers dans `input` (`app/comfy.py`), et ComfyUI y écrit ses propres dossiers. Ces écritures sont décidées par l'utilisateur ; Aria ne les nettoie pas.
- **Les moteurs eux-mêmes** (ComfyUI, llama-server, Python) peuvent écrire dans `%TEMP%` ou un cache : non vérifié.

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

- **Fichiers du dossier de données** : réglages en `data/reglages.yaml`, choix en `aria.yaml` à la racine ; pas de `journal.json`, de `sauvegardes\` ni de `cache\`. Le fond animé est dans `data/fond.json`, sur le modèle de `data/notifications.json`.
- **Sauvegarde avant modification** : seule la migration des modèles garde des `.bak`. `data/logs/aria.log` n'est pas borné.
- **Clés d'accès** : dans le coffre Windows, donc chiffrées au niveau du compte, mais hors du dossier de données.
- **Réseau local** : contrôle de `Host` et d'`Origin`, aucun CORS, actions en POST/PUT/DELETE ; mais pas de jeton par lancement, et llama-server et ComfyUI n'ont pas de clé d'accès.
- **Mises à jour** : l'appli signale la version et le lien ; elle ne télécharge ni n'installe rien.
- **Suppressions** : un modèle téléchargé et une séquence s'effacent sans copie préalable.
- **Icône de notification** : aucune ; le serveur reste actif en fond après l'ouverture du navigateur, et le bouton « Quitter » de la page l'arrête.
- **Releases** : pas d'attestation de provenance jointe pour l'instant.
