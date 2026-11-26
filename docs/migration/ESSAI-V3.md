# Essayer la v3 sur un autre PC

La v3 (Rust et Tauri) n'est pas encore publiée. Ce guide décrit comment l'essayer sur un PC capable de faire tourner
les moteurs, sans toucher à une v2 installée. Il y a trois essais, qui se font dans cet ordre :

1. **L'essai portable** (sections 1 à 6) : tout se passe dans un dossier d'essai, et se retire en supprimant ce dossier.
2. **L'installation par l'exe** (section 7) : l'exe se range dans `%USERPROFILE%\.Aria-Studio\programme`.
3. **Le pont depuis la v2** (section 8) : une v2 installée se met à jour vers la v3 par son propre bouton, sans rien
   publier sur GitHub.

Ce qui n'a jamais tourné en vrai (un vrai LLM, un vrai ComfyUI, une vraie carte graphique, une vraie v2) est listé dans
[PARITE.md](PARITE.md) : c'est ce que cet essai doit dire.

## 1. Ce qu'il faut

- Windows 10 ou 11, 64 bits, avec WebView2 (présent d'office sur Windows 11).
- Une carte NVIDIA avec un pilote de la branche 580 ou plus récente (CUDA 13).
- La v2 fermée pendant l'essai : la v3 utilise les mêmes ports (ComfyUI sur 8195, llama-server sur 8095).
- Le zip de la v3. Le plus simple : la dernière release d'essai de la copie de test, `ZedjeK/Aria-Studio_refonte`,
  téléchargée depuis la page de ses [releases](https://github.com/ZedjeK/Aria-Studio_refonte/releases), sur le PC de
  l'essai. Ses versions sont numérotées `3.0.0-essai.1`, `3.0.0-essai.2`, et ainsi de suite : le zip s'appelle
  `Aria-Studio-3.0.0-essai.N.zip`, avec son `Aria-Studio-3.0.0-essai.N.zip.sha256`. Ce sont des préversions : jamais
  proposées en mise à jour, et le vrai `v3.0.0` reste pour la sortie dans `ZedjeK/Aria-Studio`. Aucune n'était publiée
  quand ce guide a été relu (le 2026-11-26).

  Sans release d'essai à jour, l'essai à blanc du workflow Release produit le même zip en artefact, gardé 7 jours :

```powershell
gh workflow run Release --repo ZedjeK/Aria-Studio --ref refonte/v3
$id = gh run list --repo ZedjeK/Aria-Studio --workflow Release --limit 1 --json databaseId --jq '.[0].databaseId'
gh run watch $id --repo ZedjeK/Aria-Studio --exit-status
gh run download $id --repo ZedjeK/Aria-Studio --name release --dir "$env:USERPROFILE\Desktop\essai-v3"
```

  Une release d'essai se fait comme une vraie : la version passe à `X.Y.Z-essai.N` dans `Cargo.toml`, ses notes vont
  dans `docs/releases/vX.Y.Z-essai.N.md`, puis le tag `vX.Y.Z-essai.N` poussé crée la release en brouillon, en
  préversion.

Le zip contient un dossier `Aria-Studio-3.0.0-essai.N\` avec `Aria-Studio.exe`, `fichiers.txt` (la liste de ce que
livre la version), `BUILDINFO.txt` et `LICENCES-TIERS.txt`.

## 2. Préparer le dossier d'essai

**Décompresse d'abord le zip en entier** (clic droit, « Extraire tout », ou la commande ci-dessous). Lancé depuis
l'aperçu du zip dans l'Explorateur, l'exe démarre sans les fichiers livrés à côté de lui : il ne propose pas de
s'installer, et une installation tentée sans `fichiers.txt` est refusée avec ce même conseil.

Le dossier `data\` à côté de l'exe met la v3 en mode portable : réglages, journal, créations et moteurs (dans
`services\`) restent dans ce dossier, et la v3 ne propose pas de s'installer.

```powershell
$essai = "$env:USERPROFILE\Desktop\Essai-Aria-v3"
Expand-Archive -Path "$env:USERPROFILE\Downloads\Aria-Studio-3.0.0-essai.1.zip" -DestinationPath $essai
Set-Location "$essai\Aria-Studio-3.0.0-essai.1"
New-Item -ItemType Directory -Path data | Out-Null
```

Vérifier l'empreinte du zip, avant de le décompresser :

```powershell
(Get-FileHash "$env:USERPROFILE\Downloads\Aria-Studio-3.0.0-essai.1.zip" -Algorithm SHA256).Hash
```

Elle doit être celle de la ligne du fichier `.sha256`. L'exe n'est pas signé : Windows SmartScreen peut demander une
confirmation au premier lancement (« Informations complémentaires », puis « Exécuter quand même »).

## 3. Les modèles et les moteurs

Deux façons de faire, selon ce que le PC a déjà.

### Le PC a déjà la v2, avec ses modèles et ses moteurs

C'est automatique. Au premier lancement de l'essai, la v3 cherche la v2 installée au chemin standard
(`%USERPROFILE%\.Aria-Studio`) et pointe vers ses dossiers, sans rien copier ni déplacer :

- **ses modèles** : le dossier `modeles` de la v2 (ou celui qu'elle avait choisi ailleurs), qui contient `image\`,
  `video\` et `llm\` ;
- **son ComfyUI** (`moteurs\comfyui`, ou celui qu'elle avait choisi) : traité comme « ton ComfyUI », lancé mais
  jamais mis à jour ;
- **ses llama-server** (`moteurs\llama` et `moteurs\llama-prism`) : pris là où ils sont, jamais mis à jour.

Un bandeau le dit au premier lancement, le Journal note les chemins repris, et Réglages → Emplacements et clés les
montre dans le panneau « Repris de la version 2 », avec « Ne plus utiliser la version 2 » pour revenir aux dossiers de
l'essai (au lancement suivant). La reprise ne se fait qu'une fois : un emplacement changé ensuite, ou une reprise
annulée, n'est jamais réécrit.

La v2 n'est que lue : rien n'est écrit dans ses réglages, son journal ni ses créations. Seule exception, comme
avant : la v3 peut créer dans ses modèles un dossier de famille vide qui manque. Tes créations, rushes,
personnages et voix de la v2 ne sont pas repris : l'essai part de zéro dans son propre `data\`.

Si la v2 tourne au lancement de l'essai (ses ports sont pris), rien n'est repris : un bandeau demande de la fermer
puis de relancer l'essai, et le panneau « Version 2 » de Réglages → Emplacements et clés le redit.

**À la main**, si la v2 est ailleurs qu'au chemin standard : dans le dossier de l'exe, créer `reglages.json`
(chemins complets) :

```json
{
    "emplacements": {
        "modeles": "C:\\Users\\<nom>\\.Aria-Studio\\modeles",
        "comfyui": "C:\\Users\\<nom>\\.Aria-Studio\\moteurs\\comfyui",
        "llama": "C:\\Users\\<nom>\\.Aria-Studio\\moteurs\\llama",
        "llama-prism": "C:\\Users\\<nom>\\.Aria-Studio\\moteurs\\llama-prism"
    }
}
```

Un emplacement réglé à la main empêche la reprise automatique.

### Le PC n'a pas la v2

- Moteurs : les installer depuis Réglages → Moteurs, bouton « Installer » de chaque moteur (téléchargés depuis leurs
  releases GitHub officielles, empreinte SHA-256 vérifiée ; environ 2 Go pour ComfyUI, 0,6 Go pour llama.cpp).
  « Réinstaller » et « ↺ Revenir » gardent la version précédente.
- Modèles : l'assistant de premier lancement propose les packs de la v2 (licences à accepter, place vérifiée), et
  Modèles (onglet « Workflows et LLM ») les télécharge aussi un par un. On peut encore les déposer à la main dans
  `modeles\` à côté de l'exe, rangés comme en v2, ou copier ceux d'une autre machine.

## 4. L'essai rapide

Le premier passage, pour voir si l'essentiel tient debout avant le parcours complet. Noter l'heure de ce qui ne va
pas.

1. **Lancement** : la fenêtre s'ouvre sur le Plateau ; s'il manque de quoi générer, l'assistant de premier lancement
   s'ouvre aussi (« Plus tard » le ferme ; Réglages → Moteurs, « 🧭 Relancer l'assistant », le rouvre).
2. **Réglages → Moteurs** : le panneau « Carte graphique » donne la carte et sa VRAM, le panneau « Moteurs » montre
   ComfyUI et llama.cpp prêts (ou s'installent).
3. **Une image sans réécriture** : au Plateau, panneau « Créer une image », décocher « Réécrire mon idée », un prompt
   anglais, « Créer ». L'image arrive dans le panneau Rushes.
4. **Une image avec réécriture** : une idée en français, « Réécrire mon idée » cochée ; la réécriture passe, puis
   l'image. La fiche porte l'idée, le prompt du LLM et le prompt envoyé.
5. **Le Scénario** : atelier « 🖼️ Image », onglet « ✨ Écrire », ✨ Écrire le prompt, puis « → Envoyer au Plateau » (ou
   « Créer », qui l'envoie et lance la création).
6. **Un clip** : valider d'abord les steps du checkpoint MiniMax H3 (Modèles → Ma collection), puis au Montage, onglet
   « 🎬 Tourner », « 🎬 Animer un rush » de 5 s en résolution 0,4 MP (la carte « rapide »), « Tourner ». Le clip arrive dans
   l'onglet « 🎞️ Bobines », avec le son.
7. **Fermer** : « Quitter » (menu de l'icône de notification) arrête ComfyUI et llama-server.

Si ces sept points passent, enchaîner sur le parcours complet.

## 5. Le parcours complet

Cocher au fur et à mesure ; noter l'heure de ce qui ne va pas. Les pages sont rangées en trois familles dans le menu
de gauche : Création (Scénario, Plateau, Retouche, Montage), Préparation (Modèles, Casting, Réglages) et Appli
(Journal, Paramètres).

1. **Lancement** : la fenêtre s'ouvre, sans proposition d'installation ; Journal → « Aria lancée … (portable) ».
2. **Réglages → Moteurs** : la carte, son pilote et sa VRAM sont lus ; l'état de ComfyUI, de llama.cpp et de PrismML
   est juste (« ton ComfyUI » si `emplacements.comfyui` est donné). Les pastilles d'état en haut à droite se remplissent
   en orangé selon la VRAM utilisée.
3. **Plateau → options** : les workflows dont les fichiers sont là sont proposés ; les autres sont nommés comme
   incomplets.
4. **Steps à valider** : pour Qwen-Image 2.1, valider 25 steps (bouton « Valider » au Plateau, ou Modèles → Ma
   collection) ; il devient utilisable, et `reglages.json` contient `workflows.qwen_image_2_1.variantes.<checkpoint>.steps`.
5. **Créer sans réécriture** : décocher « Réécrire mon idée », écrire un prompt anglais, « Créer ». Suivre l'étape et
   la progression dans le panneau « Rendu » ; l'image arrive dans les Rushes ; `data\creations\` contient le `.png` et
   le `.json` (fiche indentée, clés de la v2).
6. **Créer avec réécriture** : cocher la réécriture, choisir le LLM, écrire une idée en français. L'étape « Réécriture
   du prompt » passe, puis la génération ; la fiche porte l'idée, le prompt du LLM et le prompt envoyé. Un seul moteur
   tient la carte à la fois (llama-server s'arrête quand ComfyUI démarre).
7. **Plusieurs images et seed** : 3 images avec une seed donnée ; les seeds se suivent dans les fiches.
8. **Annuler** : lancer 3 images, « ✕ Annuler » pendant la génération ; la commande passe « annulée », ce qui est déjà
   enregistré reste.
9. **Filtre AS-18** : une idée qui associe un âge mineur à un terme sensible est refusée avant d'entrer dans la file,
   avec le message « ERREUR AS-18 · Génération bloquée : … ». Réglages → Sécurité permet d'essayer une phrase.
10. **Rushes** : créer un dossier, y ranger des images, le renommer, archiver, désarchiver, mettre à la corbeille,
    restaurer, puis vider la corbeille (dialogue « Vider la corbeille » ; image et fiche supprimées).
11. **Libérer la carte** (Réglages → Moteurs, « Libérer la carte graphique », ou le menu de l'icône) : la VRAM
    redescend.
12. **Fermer** : « Quitter » arrête ComfyUI et llama-server lancés par la v3 ; Journal → « Aria fermée ».
13. **Retouche** : « 🖌 Retoucher » avec un masque peint (« Choisir une image » dans les Rushes), puis « ↔ Agrandir » ;
    « Ranger dans » range le résultat. Le glisser-déposer d'une image n'existe pas : c'est connu, pas à rapporter.
14. **Scénario** : les quatre ateliers (🖼️ Image, 🖌️ Retouche, 🎬 Clip, 📽️ Séquence), l'onglet « 💬 M'aider à préciser »,
    l'onglet « 🧭 Me guider » (Image et Clip), « ✕ Annuler » pendant que le LLM écrit.
15. **Séquence** : découper une histoire de 3 plans au Scénario (atelier Séquence), l'envoyer au Montage, la tourner
    (onglet « 📽️ Séquences ») ; chaque plan arrive dans les Bobines, puis le film assemblé.
16. **Voix** : onglet « 🎙️ Voix » du Montage, un essai de voix, le garder sous un nom, s'en servir comme voix d'une
    séquence.
17. **Casting** : onglet « Nouveau casting », un casting d'images de deux modèles ; puis « Comparer », à l'aveugle.
18. **Claude (MCP)** : Réglages → Claude (MCP), panneau « Brancher Claude Code » : copier la commande, brancher Claude,
    puis lui demander l'état d'Aria.
19. **Notifications** : Paramètres → Notifications, « Essayer » ; puis lancer une image fenêtre réduite : Windows le
    dit (l'en-tête porte « Windows PowerShell », c'est voulu).
20. **Menu de l'icône** : clic droit sur l'icône de notification. Le menu montre « Ouvrir Aria Studio », le bloc
    « État », « Agir sans ouvrir » (annuler la génération en cours, libérer la carte graphique), « Aller à »
    (Plateau, Montage, Modèles, Réglages, Paramètres), « Démarrer avec Windows » et « Quitter Aria Studio ». Ce menu
    n'a jamais été vu à l'écran : noter tout ce qui jure.
21. **Instance unique** : relancer l'exe quand l'appli tourne déjà (fenêtre ouverte, puis rangée dans l'icône) ramène la
    fenêtre existante et n'ouvre pas une seconde appli ; Claude (MCP) reste branché.

### Les nouveautés du LLM

Elles n'existaient pas en v2 : ce sont elles qui ont le plus besoin d'un vrai LLM pour être jugées.

22. **Recommander et Adapter à ma machine** (Réglages → Modèles image et vidéo, et LLM) : les propositions
    s'affichent avant d'être appliquées, et rien ne change sans confirmation.
23. **Contrôle qualité** : écrire quelques prompts avec ✨ et au Plateau ; noter combien de fois « Prompt redemandé
    au LLM » apparaît, et si les prompts gardés tiennent la longueur conseillée.
24. **Repli** : couper llama-server pendant une réécriture au Plateau (par exemple en fermant son processus) ; l'image
    part avec l'idée telle quelle, et la file le dit.
25. **Proposer des directions** : une envie vague au Scénario, « 🧭 Proposer des directions » ; les trois directions
    sont-elles vraiment différentes ? « Proposer d'autres directions » s'en écarte-t-il ? « Prendre cette direction »
    la pose dans l'idée.
26. **Vérifier l'image** (avec un LLM qui voit) : « Vérifier l’image » sur une image ratée exprès (un détail demandé
    absent) ; le verdict le voit-il, et « Relancer avec ce prompt » corrige-t-il ? Noter le temps de la vérification.
    La case « Vérifier les images à la fin » du Plateau le fait pour chaque image.
27. **Choisir les meilleures** : sur une sélection de 4 images des Rushes, « ★ Choisir les meilleures » : les ★ tombent-elles
    sur les plus fidèles ?
28. **Expliquer une erreur** : provoquer une erreur connue (un modèle retiré du dossier, puis Créer) et une erreur
    inconnue ; « 🩺 Expliquer » donne-t-il la bonne cause et le bon geste ? La cause connue vient sans LLM.

## 6. Le Journal, la corbeille et le retrait

- **Si quelque chose ne va pas** : rapporter, pour chaque problème, l'étape du parcours, ce qui était attendu, ce qui
  s'est passé, l'heure, et joindre depuis le dossier de l'exe : `journal.json` ; `data\logs\comfyui.log` et
  `data\logs\llama.log` ; une capture de la fenêtre si l'interface est en cause. Ces fichiers ne contiennent ni clé ni
  mot de passe, mais ils peuvent contenir des prompts : les relire avant de les partager. La page Journal montre les
  mêmes opérations.
- **Retirer l'essai** : fermer la v3 (« Quitter »), puis supprimer le dossier d'essai. Si `emplacements` pointait vers
  la v2 : dans ses modèles, seuls des dossiers de famille vides ont pu être ajoutés ; son ComfyUI a été lancé mais pas
  mis à jour (ComfyUI peut écrire ses propres fichiers de travail dans son dossier, comme avec la v2).

## 7. L'installation par l'exe

À faire une fois l'essai portable passé, **sur un PC où la v2 n'est pas installée** d'abord, puis sur un PC qui l'a.
C'est le seul essai qui touche à `%USERPROFILE%\.Aria-Studio`. Sauvegarder ce dossier (au moins `reglages.json`,
`journal.json` et `data\`) avant la seconde passe.

1. Décompresser d'abord le zip en entier (section 2), **sans** créer de dossier `data` : en mode portable, l'exe ne
   propose rien.
2. Lancer `Aria-Studio.exe`. Une fenêtre « Installer Aria Studio » s'ouvre avant celle de l'appli : « Dossier
   d'installation » (`%USERPROFILE%\.Aria-Studio\programme`), la case « Créer un raccourci sur le Bureau » (décochée
   d'office ; si un raccourci existe, la case devient « Mettre à jour le raccourci du Bureau »), la case « Ne plus me proposer
   l'installation », puis « Lancer sans installer » ou « Installer ».
3. « Installer » : la fenêtre se ferme d'elle-même, et l'appli se relance depuis `programme\`. Relever : `programme\`
   contient `Aria-Studio.exe`, `fichiers.txt`, `BUILDINFO.txt` et `LICENCES-TIERS.txt` ; le Journal dit « Installation » ;
   le raccourci du Bureau, s'il a été demandé, ouvre bien `programme\Aria-Studio.exe`.
4. Une seconde passe avec une **v2 installée** (`programme\` sans `fichiers.txt`) : la v2 est gardée entière dans
   `programme.precedent\` (son Python, son interface : on peut la remettre), la v3 prend un `programme\` neuf, et la
   fenêtre demande de fermer la v2. Si le démarrage avec Windows était coché, l'entrée « Aria Studio » vise l'exe
   installé.
5. Paramètres → Installation : « Proposer l'installation quand l'exe est lancé hors de son dossier » se réactive ici.
   `--portable` sur la ligne de commande saute la proposition.

Le raccourci du Bureau et ce vrai clic sont les deux points que les tests ne couvrent pas (voir PARITE.md, sections 2
et 6) : les noter avec soin.

## 8. Le pont depuis la v2

Pour savoir si une v2.4.x **installée** se met à jour vers la v3 par son propre bouton, sans rien publier sur GitHub,
le script `tools\essai-pont-v2.ps1` sert en local, sur `127.0.0.1`, une fausse « dernière release » que la v2 propose et
installe. Le déroulé complet est dans [ESSAI-PONT.md](ESSAI-PONT.md) ; en résumé :

1. Sauvegarder `%USERPROFILE%\.Aria-Studio` et fermer la v2.
2. Prendre le zip `Aria-Studio-3.0.0-essai.N.zip` (section 1). Le script en refait un `Aria-Studio-3.0.0.zip` avec le
   témoin `app\__init__.py` à la version que la v2 attend (elle refuse un numéro d'essai) :

```powershell
.\tools\essai-pont-v2.ps1 -Zip "$env:USERPROFILE\Desktop\essai-v3\Aria-Studio-3.0.0-essai.1.zip" -AmorcerCache
```

3. Dans une autre fenêtre PowerShell, lancer la v2 avec `ARIA_TESTS_ISOLES=1` et
   `ARIA_ESSAI_GITHUB=http://127.0.0.1:8099` (le script les affiche), puis, dans la v2, « Installer » la mise à jour.
4. Attendu : la v2 se ferme, son icône disparaît (environ 12 s), la v3 s'ouvre depuis `programme\` ; `programme.precedent\`
   contient la v2 entière ; ni `python\` ni `app\` dans `programme\`. Le Journal dira « Mise à jour non installée » :
   l'exe garde sa version d'essai alors que la v2 attendait `3.0.0` (écart propre à l'essai).
5. Retour à la v2 : fermer la v3, lancer `programme.precedent\Aria-Studio.exe`.

Le script n'écrit qu'en local (`.work\essai-pont` du dépôt, et `cache\mise_a_jour.json` de la v2, dont l'ancien fichier est
gardé en `.avant-essai`).

## 9. Pas encore dans la v3

Ne pas le compter comme un défaut. La liste complète et à jour est dans [PARITE.md](PARITE.md) ; en bref :

- la restauration des sauvegardes de réglages (elles sont faites, mais ne se restaurent depuis aucune page) ;
- le dialogue de la garde VRAM (forcer ou annuler l'attente d'une carte occupée) ;
- le glisser-déposer d'une image dans la Retouche ;
- au Casting : le pic de RAM de ComfyUI, la pause des téléchargements pendant un casting, « Ouvrir dans Modèles » ;
- l'icône Windows propre à Aria Studio (`appli\icons\icon.ico` est celle de la famille tant que `tools\icone.ps1`
  n'a pas tourné) ;
- le banc de palier du mainteneur n'a encore jamais tourné : aucune mesure de palier de VRAM n'a été faite.

Le pont de mise à jour depuis la v2 et les licences tierces complètes (`LICENCES-TIERS.txt`, joint au zip) existent
maintenant : ils ne font plus partie de cette liste.
