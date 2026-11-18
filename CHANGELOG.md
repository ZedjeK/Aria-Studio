# Journal des versions

Un bloc par version, le plus récent en haut. Les notes complètes d'une version
sont dans `docs/releases/` (à partir de la 2.0.0) et dans la page de sa release sur
GitHub (pour les précédentes).

## 2.3.1 (2026-11-18)

Correctif d'interface. Notes complètes : `docs/releases/v2.3.1.md`.

### Corrigé

- Les fenêtres (choix d'une image, fiches, « Aria est fermée »…) passent au-dessus du menu latéral : une fenêtre large n'a plus son bord gauche caché dessous, et le menu est assombri avec le reste (#122).

## 2.3.0 (2026-11-16)

Retouches de l'interface. Notes complètes : `docs/releases/v2.3.0.md`.

### Ajouté

- Réglages : un onglet Modèles vidéo (Texte vers vidéo, Image vers vidéo), et un bouton « Recommander » pour chaque modèle d'image ou de vidéo, qui remplit ses réglages avec les valeurs de sa fiche, avec leur raison et leur source ; rien n'est enregistré avant « Enregistrer » (#119).
- Galeries (Rushes, Bobines) : un volet à gauche des vignettes pour les vues et les dossiers, sans limite de nombre, avec des noms de dossier en majuscules partout (#118).
- Montage : deux groupes de cartes, « Avec des images » et « Sans image » (#120).

### Modifié

- Réglages : le bouton « Tout » disparaît, un onglet par famille ; « Adapter à ma machine » passe dans l'onglet LLM et ne propose plus que des réglages de LLM ; la résolution des modèles d'image n'est plus jamais proposée, elle reste en note (#119).
- Menu : titres Création et Préparation en cyan avec un filet ; Quitter remonté sous Réglages, en rouge (#120).
- Scénario : chaque atelier a ses compléments, qui ne se partagent plus ; un brouillon d'avant est repris (#117).

### Corrigé

- Scénario : « Tout réinitialiser » ne vide plus les images de départ et les compléments des autres ateliers, et remet à zéro le personnage de référence d'une Séquence (#117).

## 2.2.3 (2026-11-16)

Correctif de la mise à jour intégrée. Notes complètes : `docs/releases/v2.2.3.md`.

### Corrigé

- « Installer la mise à jour » installe maintenant la nouvelle version : le lanceur de mise à jour réessayait une seule fois si `programme\` était encore ouvert, alors que l'icône de la barre des tâches met une dizaine de secondes à se fermer après le serveur ; il abandonnait et relançait l'ancienne version. Il réessaie maintenant pendant 15 s avant de renoncer (#115).

## 2.2.2 (2026-11-16)

Correctif de génération. Notes complètes : `docs/releases/v2.2.2.md`.

### Corrigé

- Rejouer une seed (même prompt, même modèle, même format) ne fait plus échouer la génération avec « 404 Not Found » : ComfyUI servait le workflow depuis son cache et annonçait des fichiers qu'Aria avait déjà effacés. Chaque envoi reçoit maintenant des noms de fichiers neufs, seul l'enregistrement est refait (#113).

## 2.2.1 (2026-11-15)

Correctif du filtre de protection des mineurs. Notes complètes : `docs/releases/v2.2.1.md`.

### Corrigé

- Un âge écrit au pluriel est lu : « 12-year-olds », « 12 year olds », « 14 yos », « 15 y/os » passaient, le « s » final empêchant la reconnaissance (#111).
- Un tiret typographique (insécable, demi-cadratin, cadratin, signe moins...) est lu comme un trait d'union : « dix‑sept ans » et « 10–12 ans » étaient collés en « dixsept » et « 1012 » et passaient. Le texte est lu sous l'ancienne et la nouvelle lecture des tirets et bloque si l'une des deux bloque, donc un tiret glissé dans un mot du filtre ne le cache pas (#111).

## 2.2.0 (2026-11-15)

Notes complètes : `docs/releases/v2.2.0.md`.

### Ajouté

- Les listes du filtre de protection des mineurs se pilotent depuis Réglages → Système (« Sécurité · Protection des mineurs ») : ajouter et retirer tes termes, promouvoir un ajout dans le socle, essayer une phrase, phrases témoins revérifiées à chaque changement. Le socle vit dans `securite/socle.yaml` et il est complété ; sans socle lisible, Aria refuse de générer (#100).
- Les clips MiniMax H3 : la consigne dit la durée du clip au LLM, les heures de coupe sont contrôlées, un plan avec référence passe aux six champs du guide et garde le visage dans le cadre jusqu'à la fin du plan qui le précède (#97, #99).
- SECURITY.md (#92), `tools/proteger-depot.ps1` (#91), `.gitattributes` (#93), CodeQL sur Python, JavaScript, C# et les workflows (#94), `tools/verifier-release.ps1` lancé avant le build d'une release (#95).

### Modifié

- Les consignes envoyées au LLM suivent un gabarit par sections (réécriture, retouche, références, vidéo), avec les règles du modèle cible rangées dedans (#62).
- Le conseil de la page d'un checkpoint (steps et CFG) reste lisible après la validation (#98).
- « J'ai déjà ComfyUI » refuse un dossier sur le réseau ; l'estimation refuse un checkpoint qui n'est pas un fichier déposé ; un GGUF se désigne par un nom de fichier ; les identifiants sont contrôlés en entier ; le nettoyage des balises des pages de modèles et la lecture des répliques ne gèlent plus sur un texte piégé, une réplique de plus de 1 000 caractères n'étant plus reconnue comme telle (#101, #102, #103, #104, #106, #107).
- Les fiches Krea 2 et Qwen-Image 2.1 gardent les résultats du banc du 2026-11-15 (#109). FastAPI passe de 0.162.0 à 0.162.5 (#105).
- README : décompresse d'abord le zip, et pourquoi (#96).

### Corrigé

- Un âge adulte écrit en lettres n'est plus lu comme sa dernière unité : « vingt-neuf ans », « dix-huit ans », « vingt et un ans », « twenty-nine year old ». Les âges de 18 à 99 ans, en français et en anglais, sont lus à leur vraie valeur ; les mineurs restent bloqués et le plancher de 18 ans ne change pas (#108).

## 2.1.0 (2026-11-14)

Première version publiée depuis la 1.3.0 : elle contient aussi la 2.0.0, jamais publiée. Notes complètes : `docs/releases/v2.1.0.md`.

### Ajouté

- L'icône d'Aria près de l'horloge : délavée ou pleine selon que la carte graphique sert, avec une pastille (orange : une génération attend la carte ; cyan : génération ou téléchargement ; bleue : mise à jour). Son menu donne l'état, annule la génération, libère la carte graphique, ouvre une page, et quitte Aria. Le lanceur reste ouvert pour la tenir ; relancer l'exe ne crée pas de seconde icône.
- « Démarrer avec Windows », décoché d'office, dans le menu de l'icône et dans Réglages → Système : une valeur sous la clé `Run` du compte, lancement avec `--silencieux` (sans navigateur), seulement pour Aria installée.
- La mise à jour intégrée : « Installer la mise à jour » (bandeau et Réglages → Système) télécharge la dernière release depuis GitHub, vérifie son empreinte SHA-256 deux fois (celle de GitHub et le `.sha256` publié), puis le nouveau lanceur installe la version dans `programme\` en gardant l'ancienne dans `programme.precedent\` et relance Aria. En mode portable, elle est décompressée à côté, sans rien déplacer.
- `journal.json`, les 500 dernières opérations d'Aria sur tes affaires (téléchargements, moteurs, corbeilles, migrations, réglages), et une carte « Journal » dans Réglages → Système. `data\logs\aria.log` est borné à 5 Mo, le précédent gardé en `aria.log.1`.
- Une sauvegarde de `reglages.json` avant chaque changement, dans `sauvegardes\` (les 20 dernières), et « Restaurer » dans Réglages → Système, qui sauvegarde d'abord les réglages actuels.
- Une corbeille pour les modèles téléchargés et les séquences : supprimer les met de côté (les modèles dans `.corbeille` de leur dossier, les séquences dans `data/sequences/corbeille`), « Restaurer » les remet en service, et seul « Vider la corbeille » efface pour de bon.
- Habillage selon le contrat visuel des applis Aria : menu latéral, panneaux à coins coupés, sous-onglets en encoche avec leur encadré « À savoir », cartes de départ à onglet, boutons en biais, liseré accentué sur le panneau que touche le bouton principal (#63).
- Fond animé en gouttes, avec ses réglages (activé, vitesse, nombre de gouttes) dans Réglages → Système, gardés dans `data/fond.json` (#63).
- Le format se choisit pour « Redessiner une image » (#58) et au Scénario pour la séquence, et le LLM connaît le cadre (#60, #61).
- `docs/FICHE.md`, la fiche projet (#64), et les scripts `build.ps1`, `tools/tester.ps1` et `tools/version.ps1` (#65).
- L'installation par l'exe : lancé depuis un zip décompressé, `Aria-Studio.exe` propose « Installer » (dans `%USERPROFILE%\.Aria-Studio\programme`, version d'avant gardée, raccourci du Bureau facultatif) ou « Lancer sans installer ». La proposition se coupe, et se réactive dans Réglages → Système.

### Modifié

- L'assistant de premier lancement prend l'habillage des applis Aria, sur le modèle des dialogues d'Aria-Modele : cadre à coins coupés, étapes en encoches, encadré « À savoir », un panneau accentué par étape, boutons en biais à droite avec un seul orange (à l'étape Moteurs, sur le premier « Installer » tant qu'un moteur manque). « Plus tard » passe dans la rangée du bas.
- ComfyUI (celui d'Aria comme « J'ai déjà ComfyUI ») travaille dans `cache\comfyui\` : les fichiers envoyés et les résultats ne s'accumulent plus dans ses dossiers `input` et `output`. Les entrées et les temporaires sont vidés avant chaque lancement, un résultat est effacé dès qu'Aria l'a récupéré. Les anciens fichiers restent en place.
- llama-server reçoit une clé d'accès neuve à chaque démarrage (variable d'environnement `LLAMA_API_KEY`, jamais sur la ligne de commande ni dans un fichier) et refuse qui ne la présente pas. ComfyUI n'a pas d'option de clé : sa protection contre les autres sites du navigateur est gardée, et la limite est dite dans le README.
- L'interface ne s'ouvre plus que par le lanceur : un jeton tiré à chaque lancement (écrit dans `cache\session.json`) est exigé à chaque requête, sauf `/api/sante`. Un ancien onglet ou un favori affiche « Aria Studio s'ouvre par son lanceur ». Aria refuse de démarrer si `config.yaml` demande d'écouter ailleurs que sur `127.0.0.1`, et `GET /api/mise-a-jour` n'interroge plus GitHub (le démarrage et « Vérifier maintenant » le font).
- Tous les réglages et choix de l'utilisateur sont réunis dans `reglages.json`, à la racine du dossier d'Aria : `aria.yaml`, `data/reglages.yaml`, `data/fond.json`, `data/notifications.json`, `data/mise_a_jour.json` et `data/loras.json` y sont versés au premier démarrage, copiés dans `sauvegardes\` et laissés en place pour une version d'avant. Le lanceur y lit et y écrit la proposition d'installer.
- `tools/build_portable.py` installe dans `programme\` ; une installation de l'ancienne disposition (programme à la racine de `.Aria-Studio`) y est rangée dans `programme.precedent\`.
- Bonsai 2 devient le LLM par défaut (#54).
- Les modèles d'image et de vidéo sont rangés chacun de leur côté, avec une migration au démarrage (#57).
- La réponse du LLM est assez longue pour les mots visés, et le clip H3 du chat se remplit en trois champs (#53).
- La notification de fin de job a un tag unique par job et un bandeau si elle échoue (#55).
- Ce qui se recalcule (empreintes SHA-256 des modèles, samplers et chemins de ComfyUI) quitte `data\` pour `cache\`, rangé au premier démarrage.
- La molette fait défiler la page, plus un cadre interne (#59).

### Retiré

- Les LLM Ministral, Qwen3.5 et Gemma (#54).

### Corrigé

- Dans un profil neuf, le dossier des modèles n'existe pas encore : l'assistant échouait à l'étape « Ce que tu veux faire » (erreur au calcul de la place libre). La place se lit maintenant sur le premier dossier parent qui existe (`app/modeles.py`).
- Sans modèle vidéo installé, le Montage ne chargeait pas ses Bobines, et le mode Séquence ni sa liste ni ses voix quand on l'ouvrait depuis le Scénario ou les Bobines (`web/montage.js`).
- `catalogue/moteurs.yaml` est de nouveau lisible (#56).

## 2.0.0 (tag posé le 2026-11-09, jamais publiée : son contenu est livré avec la 2.1.0)

### Ajouté

- Un onglet Retouche : retoucher, agrandir et comparer à l'original.
- Des dossiers séparés pour les rushes et les bobines, des galeries masquées par défaut, un menu clic droit et « Vider la corbeille ».
- Des modèles que tu valides : un checkpoint téléchargé attend ta validation, les steps se règlent par checkpoint, une collection par modèle dans Modèles.
- Une estimation du Montage calculée à partir du checkpoint, de la VRAM libre et du nombre de clips.
- Une notification à la fin d'un job (Plateau, Montage, Casting), envoyée par Windows.
- Un Scénario en quatre ateliers (Image, Retouche, Clip, Séquence), un choix de LLM par onglet, « Me guider » en bulles, jusqu'à 10 images pour s'inspirer.
- Les LoRA se rangent par usage et servent aussi aux clips et aux séquences.
- Une refonte visuelle (palette Synthwave, chat à bulles, pastilles des moteurs).

### Modifié

- Des consignes alignées sur les guides officiels des modèles, et un garde-fou sur les sorties du LLM en caractères asiatiques.
- « Qualité » devient « Résolution ».
- ComfyUI ne tourne jamais en double et s'arrête en quittant, même orphelin.

### Retiré

- Qwen-Image-Edit 2511 : Qwen-Image 2.1 Edit fait le même travail, sous licence Qwen Research (non commerciale) au lieu d'Apache 2.0. Les fichiers déjà téléchargés restent sur le disque.

## 1.3.0 (2026-11-06)

### Ajouté

- Un Scénario en onglets, avec images de référence, sujet, décor, mise en scène, histoire et un chat « Libre » où le LLM pose les questions qui manquent.
- « Peaufiner » avec le LLM spécialisé d'un modèle quand il existe.

### Modifié

- Le LLM ne travaille plus qu'au Scénario : le Plateau et le Montage tournent exactement le prompt affiché.
- Des consignes fidèles aux guides officiels de Krea 2, Qwen-Image 2.1 et MiniMax H3 ; le contenu entre adultes n'est plus adouci (la clause sur les mineurs et le filtre AS-18 restent).
- La réflexion passe à un budget de 8 192 tokens, pour ne plus rendre de réponse vide.

## 1.2.0 (2026-11-03)

### Ajouté

- La vision des LLM, avec leur projecteur d'images (`mmproj`), et la réflexion activée par défaut.
- Un LLM par workflow, et des LLM spécialisés proposés quand l'auteur d'un modèle en publie un.
- « Annuler » sous les barres de progression, et « Quitter » qui arrête Aria, ComfyUI et llama.cpp.
- Les clés d'accès Civitai et Hugging Face, rangées dans le Gestionnaire d'identification Windows.

### Modifié

- Le moteur llama.cpp est choisi d'après le GGUF (PrismML pour les formats ternaires).

## 1.1.0 (2026-11-02)

### Modifié

- L'onglet Modèles est refait en cinq sous-onglets (Rechercher, Ma collection, Texte → image, Image → image, Vidéo).

### Ajouté

- « Retrouver les sources » : un fichier déposé à la main est reconnu par son empreinte SHA-256, sur ton clic seulement.
- Un onglet par usage, avec le favori de chaque usage et un bouton pour télécharger ce qui manque à un workflow.

## 1.0.0 (2026-11-02)

### Ajouté

- Un assistant de premier lancement : vérification de la machine, installation des moteurs, packs de modèles, licences, téléchargements, image d'essai.
- L'installateur des moteurs (ComfyUI portable, llama.cpp, PrismML en option), versions épinglées et SHA-256 vérifié.
- « J'ai déjà ComfyUI » : Aria utilise ton ComfyUI portable après avoir vérifié ses nœuds.
- Le dossier `%USERPROFILE%\.Aria-Studio` pour tout ce qui t'appartient, le README et la licence MIT.

## 0.9.0 (2026-11-01, préversion)

### Ajouté

- Première version publique : Plateau, Montage, Scénario et séquences, Casting, Réglages, Modèles, garde-VRAM, vérification quotidienne des mises à jour.
