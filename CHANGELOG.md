# Journal des versions

Un bloc par version, le plus récent en haut. Les notes complètes d'une version
sont dans `docs/releases/` (à partir de la 2.0.0) et dans la page de sa release sur
GitHub (pour les précédentes).

## 3.0.0 (à publier)

Aria Studio est réécrite : une vraie appli Windows en un seul exe, avec sa propre fenêtre, sans navigateur ni Python. Elle relit tes réglages, ton journal et tes données de la 2.4 telles quelles, et la mise à jour intégrée de la 2.4 sait l'installer. Les versions `3.0.0-essai.N` servent d'abord à l'essayer ; elles ne sont jamais proposées en mise à jour. Ce qui n'a jamais tourné en vrai est listé dans `docs/migration/PARITE.md`.

### Ajouté

- Le LLM en fait plus. Chaque prompt qu'il écrit est contrôlé (langue, longueur, texte que tu avais cité) et redemandé une fois s'il manque son but ; s'il tombe en panne, ton idée part telle quelle au lieu d'un échec. Au Scénario, il propose trois directions à partir d'une envie vague. Il vérifie une image d'après ce que tu avais demandé, propose un prompt corrigé et choisit les meilleures d'un lot (il lui faut un LLM qui voit). Un bouton « 🩺 Expliquer » dit la cause probable d'une erreur et quoi faire.
- Une icône près de l'horloge plus complète : l'état de ComfyUI, du LLM et de la mémoire de la carte graphique, annuler la génération en cours ou libérer la carte sans ouvrir la fenêtre, aller à une page, démarrer avec Windows, quitter.
- Windows te prévient quand une image, une retouche, un clip, une séquence ou un casting se termine, si la fenêtre d'Aria n'est pas devant toi (Paramètres → Notifications).
- Relancer l'exe quand Aria tourne déjà ramène sa fenêtre, au lieu d'en ouvrir une seconde.
- Paramètres : « Vérifier maintenant » cherche une nouvelle version à la demande, et « Voir la mise à jour » ouvre ses notes.
- Au Scénario : un bouton Créer qui envoie le prompt au Plateau et lance la création ; un champ Format dans l'atelier Image ; le dialogue et la musique d'ambiance d'un clip dans son idée.
- Au Plateau, on voit quels LLM regardent les images ; la fiche d'une création garde ton idée, ce que le LLM a écrit et ce qui est vraiment parti au modèle.
- Dans les Rushes et les Bobines : la fiche d'un fichier, sa sélection, le menu du clic droit, et depuis cette fiche : réutiliser, prendre en référence, animer, retoucher, ouvrir le Scénario.
- Dans Modèles : la corbeille prend tous les modèles, déposés à la main compris ; un bouton dit pourquoi un workflow n'est pas prêt ; les LLM spécialisés de Qwen-Image 2.1 sont présentés comme facultatifs.
- Dans Réglages : « Chercher un réglage » et « Modifiés seulement », « Ouvrir dans Modèles » à côté du checkpoint par défaut. En haut à droite, les pastilles d'état de ComfyUI et du LLM montrent en fond la part de mémoire de la carte graphique utilisée.
- Une v2 déjà installée se met à jour vers la 3 par son propre bouton « Installer » : elle est gardée entière dans `programme.precedent\`, et tu peux la relancer de là.
- Pour un essai en mode portable sur un PC qui a une v2 installée, la 3 reprend d'elle-même ses modèles, son ComfyUI et ses llama-server, sans rien copier ni toucher.
- `LICENCES-TIERS.txt` est complet : chaque composant embarqué et chaque modèle proposé, avec sa licence.

### Modifié

- Aria Studio a sa propre fenêtre et son menu en trois familles (Création, Préparation, Appli) ; il n'y a plus de page web sur `127.0.0.1:8075`, plus de jeton à ouvrir et plus d'onglet qui se rouvre. Les pages Plateau, Retouche, Montage, Scénario, Modèles, Casting et Réglages sont refaites sur la même maquette ; le Journal et les Paramètres (démarrage, installation, mises à jour, notifications, fond animé) ont chacun leur page, à la place de Réglages → Application.
- Tes clés Civitai et Hugging Face sont rangées dans `secrets\`, chiffrées par Windows pour ton compte, dans le dossier de données. Celles que la 2.4 avait mises dans le Gestionnaire d'identification de Windows y sont recopiées au lancement ; l'entrée d'origine reste en place.
- Claude se branche avec une nouvelle commande, `Aria-Studio.exe --mcp` : refais le branchement depuis Réglages → Claude (MCP). L'outil `generer_image` n'a plus de paramètre `idee` (le prompt est noté comme idée), et `etat` dit quel moteur tient la carte graphique.
- La mise à jour intégrée vérifie la taille et l'empreinte SHA-256 annoncées par GitHub pour le zip de la dernière release ; la version d'avant est gardée dans `sauvegardes\versions\`.
- Redessiner une image et la Retouche font écrire au LLM ce qui change, sans décrire l'image.
- Casting : « Nouveau casting » et « Comparer », les tâches et les candidats en cartes, « par défaut » à la place de « favori », la révélation des modèles demande confirmation.
- Casting : les images de départ des castings de retouche et d'image vers vidéo sont désormais les tiennes (figées depuis un casting, importées ou prises dans les Rushes), pas celles que livrait l'exe.
- Réglages : le checkpoint par défaut se choisit dans Modèles ; les réglages du LLM sont en quatre groupes.
- Installation : une v2 déjà installée est gardée entière ; un raccourci du Bureau déjà présent est proposé en mise à jour, jamais remplacé en silence ; l'entrée « Démarrer avec Windows » garde le nom « Aria Studio » et vise l'exe installé.

### Corrigé

- Casting LLM : « Sans LLM (témoin) » ne peut plus être choisi comme LLM par défaut. Il n'en est pas un, et le choisir faisait retomber Aria sur un autre LLM sans le dire.
- Scénario, conversation : un refus du LLM est dit comme tel, et sa réponse s'affiche.
- Casting : la préparation ne montre plus le texte d'une erreur technique.
- Relancer l'exe n'ouvre plus un nouvel onglet : il ramène la fenêtre.

### Retiré

- La page web, le serveur local et Python : tout tient dans l'exe.
- La restauration des sauvegardes de réglages (Réglages → Historique) : les copies sont toujours faites dans `sauvegardes\`, mais aucune page ne les restaure encore.
- Le glisser-déposer d'une image dans la Retouche : on passe par les Rushes ou l'import.
- Le dialogue de la garde de la carte graphique (forcer ou annuler l'attente) ; l'attente, elle, existe toujours.
- Au Casting : le pic de mémoire de ComfyUI, la pause des téléchargements pendant un casting et « Ouvrir dans Modèles » ne sont pas encore repris.
- La promotion d'un terme dans le socle du filtre AS-18 : le socle est embarqué dans l'exe, tes ajouts restent dans `reglages.json`.

## 2.4.0 (2026-11-19)

Écriture des prompts par le LLM, après une revue de ses consignes et de ses réglages ; un serveur MCP pour piloter Aria depuis Claude ; ComfyUI s'installe sous Windows 10 avec 7-Zip. Notes complètes : `docs/releases/v2.4.0.md`.

### Ajouté

- Un serveur MCP (`app/mcp.py`) : Claude Code peut voir l'état d'Aria, écrire un prompt avec le LLM local, lancer un rendu d'image, le suivre et lire la fiche d'une image. Il passe par l'API d'Aria avec le jeton du lancement en cours, sans port ni secret en plus ; rien ne supprime, ne règle ni n'installe. Réglages → Système, carte « Claude (MCP) » : la commande pour le déclarer (#130).
- Réglages, onglet LLM : un jeu d'échantillonnage « avec réflexion » (température, top-p, top-k, min-p, pénalité de présence) à côté du jeu existant, qui devient « sans réflexion ». Chaque demande prend le jeu de son mode ; « Adapter à ma machine » propose celui de la fiche de Bonsai 2 (#124).
- Adapter à ma machine : un GGUF dérivé d'un modèle du catalogue (même début de nom, comme Bonsai 2 Heretic) reprend ses recommandations, et la raison affichée le dit (#127).

### Modifié

- Scénario, conversation (💬 M'aider à préciser) : elle écrit ses prompts avec les mêmes règles que ✨ Écrire le prompt, et les heures de coupe d'un clip qu'elle écrit sont contrôlées et affichées (#125).
- Séquence, découpage : chaque plan a une longueur conseillée selon sa durée, et la réponse du LLM est bornée pour contenir tous les plans demandés (#126).
- Casting LLM : les candidats écrivent avec les règles et la longueur du modèle de rendu, et le contrôle de longueur suit cette longueur (#127).

### Corrigé

- Scénario, conversation : une conversation longue n'est plus refusée par le LLM ; elle garde le premier message et les derniers échanges qui tiennent dans son contexte (#127).
- MiniMax H3 : en trois champs, les sons étaient demandés deux fois, en fin de description et dans `overall_soundscape` ; ils vont maintenant dans `overall_soundscape` seulement (#127).
- Moteurs : sous Windows 10, l'installation de ComfyUI échouait sur « Error exit delayed from previous errors » (le `tar` de Windows 10 ne lit pas son archive 7z). 7-Zip prend maintenant le relais, la vraie erreur de chaque outil s'affiche, et un PC sans outil capable est prévenu avant le téléchargement (#129).

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
