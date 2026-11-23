# Essayer la v3 sur un autre PC

La v3 (Rust et Tauri) n'est pas encore publiée. Ce guide décrit un essai sur un PC capable de faire tourner les
moteurs, sans toucher à une v2 installée : tout se passe dans un dossier d'essai, en mode portable, et se retire en
supprimant ce dossier.

## 1. Ce qu'il faut

- Windows 10 ou 11, 64 bits, avec WebView2 (présent d'office sur Windows 11).
- Une carte NVIDIA avec un pilote de la branche 580 ou plus récente (CUDA 13).
- La v2 fermée pendant l'essai : la v3 utilise les mêmes ports (ComfyUI sur 8195, llama-server sur 8095).
- Le zip de la v3. Le plus simple : la dernière release d'essai de la copie de test, `ZedjeK/Aria-Studio_refonte`,
  téléchargée depuis la page de ses [releases](https://github.com/ZedjeK/Aria-Studio_refonte/releases), sur le PC de
  l'essai. Ses versions sont numérotées `3.0.0-essai.1`, `3.0.0-essai.2`, et ainsi de suite (le zip s'appelle
  `Aria-Studio-3.0.0-essai.N.zip`). Ce sont des préversions : jamais proposées en mise à jour, et le vrai `v3.0.0`
  reste pour la sortie dans `ZedjeK/Aria-Studio`.

  Sans release d'essai à jour, l'essai à blanc du workflow Release produit le même zip en artefact, gardé 7 jours :

```powershell
gh workflow run Release --repo ZedjeK/Aria-Studio_refonte --ref main
```

  Une release d'essai se fait comme une vraie : la version passe à `X.Y.Z-essai.N` dans `Cargo.toml`, ses notes vont
  dans `docs/releases/vX.Y.Z-essai.N.md`, puis le tag `vX.Y.Z-essai.N` poussé dans la copie de test crée la release
  en brouillon, en préversion.

## 2. Préparer le dossier d'essai

Le dossier `data\` à côté de l'exe met la v3 en mode portable : réglages, journal, créations et moteurs restent dans
ce dossier, et la v3 ne propose pas de s'installer.

```powershell
$essai = "$env:USERPROFILE\Desktop\Essai-Aria-v3"
Expand-Archive -Path "$env:USERPROFILE\Downloads\Aria-Studio-3.0.0.zip" -DestinationPath $essai
Set-Location "$essai\Aria-Studio-3.0.0"
New-Item -ItemType Directory -Path data | Out-Null
```

L'exe n'est pas signé : Windows SmartScreen peut demander une confirmation au premier lancement (« Informations
complémentaires », puis « Exécuter quand même »).

## 3. Les modèles et les moteurs

Deux façons de faire, selon ce que le PC a déjà.

### Le PC a déjà la v2, avec ses modèles et ses moteurs

On fait pointer l'essai vers les dossiers de la v2 sans les déplacer. Dans le dossier de l'exe, créer
`reglages.json` (chemins complets ; ceux-ci sont ceux d'une v2 installée par défaut, à adapter si les modèles ou
ComfyUI sont ailleurs) :

```json
{
    "emplacements": {
        "modeles": "C:\\Users\\<nom>\\.Aria-Studio\\modeles",
        "comfyui": "C:\\Users\\<nom>\\.Aria-Studio\\moteurs\\comfyui"
    }
}
```

- `modeles` : le dossier des modèles de la v2 (il contient `image\`, `video\`, `llm\`). La v3 y crée seulement les
  dossiers de famille manquants.
- `comfyui` : le dossier de ComfyUI de la v2, celui qui contient `python_embeded` et `ComfyUI`. La v3 le traite
  comme « ton ComfyUI » : elle le lance sans le mettre à jour.
- llama-server : copier les dossiers `llama` et `llama-prism` de `.Aria-Studio\moteurs\` de la v2 dans un dossier
  `services\` à côté de l'exe, ou les installer depuis Réglages → Moteurs (section 4).

### Le PC n'a pas la v2

- Moteurs : les installer depuis Réglages → Moteurs (téléchargés depuis leurs releases GitHub officielles, empreinte
  SHA-256 vérifiée ; environ 2 Go pour ComfyUI, 0,6 Go pour llama.cpp).
- Modèles : l'assistant de premier lancement propose les packs de la v2 (licences à accepter, place vérifiée), et
  Modèles les télécharge aussi un par un. On peut encore les déposer à la main dans `modeles\` à côté de l'exe, rangés
  comme en v2, ou copier ceux d'une autre machine.

## 4. L'essai rapide

Le premier passage, pour voir si l'essentiel tient debout avant le parcours complet. Noter l'heure de ce qui ne va
pas.

1. **Lancement** : la fenêtre s'ouvre ; s'il manque de quoi générer, l'assistant de premier lancement s'ouvre aussi
   (« Plus tard » le ferme).
2. **Réglages → Moteurs** : la carte et sa VRAM sont lues, ComfyUI et llama.cpp sont prêts (ou s'installent).
3. **Une image sans réécriture** : au Plateau, un prompt anglais, Créer ; l'image arrive dans les Rushes.
4. **Une image avec réécriture** : une idée en français, « Réécrire mon idée » cochée ; la réécriture passe, puis
   l'image. La fiche porte l'idée, le prompt et le LLM.
5. **Le Scénario** : atelier Image, ✨ Écrire le prompt, puis l'envoyer au Plateau.
6. **Un clip** : valider d'abord les steps du checkpoint MiniMax H3 (Modèles → Ma collection), puis au Montage,
   animer un rush de 5 s en résolution rapide. Le clip arrive dans les Bobines, avec le son.
7. **Fermer** : ComfyUI et llama-server s'arrêtent.

Si ces sept points passent, enchaîner sur le parcours complet.

## 5. Le parcours complet

Cocher au fur et à mesure ; noter l'heure de ce qui ne va pas.

1. **Lancement** : la fenêtre s'ouvre, sans proposition d'installation ; Journal → « Aria lancée … (portable) ».
2. **Réglages → Moteurs** : la carte, son pilote et sa VRAM sont lus ; l'état de ComfyUI, de llama.cpp et de PrismML
   est juste (« ton ComfyUI » si `emplacements.comfyui` est donné).
3. **Plateau → options** : les workflows dont les fichiers sont là sont proposés ; les autres sont nommés comme
   incomplets.
4. **Steps à valider** : pour Qwen-Image 2.1, valider 25 steps ; il devient utilisable, et `reglages.json` contient
   `workflows.qwen_image_2_1.variantes.<checkpoint>.steps`.
5. **Créer sans réécriture** : décocher « Réécrire mon idée », écrire un prompt anglais, Créer. Suivre l'étape et la
   progression ; l'image arrive dans les Rushes ; `data\creations\` contient le `.png` et le `.json` (fiche indentée,
   clés de la v2).
6. **Créer avec réécriture** : cocher la réécriture, choisir le LLM, écrire une idée en français. L'étape « Réécriture
   du prompt » passe, puis la génération ; la fiche porte l'idée, le prompt et le LLM. Un seul moteur tient la carte
   à la fois (llama-server s'arrête quand ComfyUI démarre).
7. **Plusieurs images et seed** : 3 images avec une seed donnée ; les seeds se suivent dans les fiches.
8. **Annuler** : lancer 3 images, ✕ Annuler pendant la génération ; la commande passe « annulée », ce qui est déjà
   enregistré reste.
9. **Filtre AS-18** : une idée qui associe un âge mineur à un terme sensible est refusée avant d'entrer dans la file,
   avec le message « ERREUR AS-18 · Génération bloquée : … ».
10. **Rushes** : créer un dossier, y ranger des images, le renommer, archiver, désarchiver, mettre à la corbeille,
    restaurer, puis vider la corbeille (dialogue de confirmation ; image et fiche supprimées).
11. **Libérer la carte** (Réglages → Moteurs) : la VRAM redescend.
12. **Fermer** : ComfyUI et llama-server lancés par la v3 s'arrêtent ; Journal → « Aria fermée » au lancement suivant.
13. **Retouche** : une retouche avec un masque peint, puis un agrandissement ; « Ranger dans » range le résultat.
14. **Scénario** : les quatre ateliers (Image, Retouche, Clip, Séquence), « 💬 M'aider à préciser », ✕ Annuler pendant
    que le LLM écrit.
15. **Séquence** : découper une histoire de 3 plans au Scénario, l'envoyer au Montage, la tourner ; chaque plan
    arrive dans les Bobines, puis le film assemblé.
16. **Voix** : un essai de voix, le garder sous un nom, s'en servir comme voix d'une séquence.
17. **Casting** : un casting d'images de deux modèles, comparé à l'aveugle.
18. **Claude (MCP)** : brancher Claude avec la commande de Réglages → Claude (MCP), puis lui demander l'état d'Aria.

### Les nouveautés du LLM

Elles n'existaient pas en v2 : ce sont elles qui ont le plus besoin d'un vrai LLM pour être jugées.

19. **Recommander et Adapter à ma machine** (Réglages) : les propositions s'affichent avant d'être appliquées, et
    rien ne change sans confirmation.
20. **Contrôle qualité** : écrire quelques prompts avec ✨ et au Plateau ; noter combien de fois « Prompt redemandé
    au LLM » apparaît, et si les prompts gardés tiennent la longueur conseillée.
21. **Repli** : couper llama-server pendant une réécriture au Plateau (Réglages → Moteurs) ; l'image part avec
    l'idée telle quelle, et la file le dit.
22. **Proposer des directions** : une envie vague au Scénario, 🧭 Proposer des directions ; les trois directions
    sont-elles vraiment différentes ? « Proposer d'autres directions » s'en écarte-t-il ?
23. **Vérifier l'image** (avec un LLM qui voit) : vérifier une image ratée exprès (un détail demandé absent) ; le
    verdict le voit-il, et « Relancer avec ce prompt » corrige-t-il ? Noter le temps de la vérification.
24. **Choisir les meilleures** : sur un lot de 4 images, les ★ tombent-elles sur les plus fidèles ?
25. **Expliquer une erreur** : provoquer une erreur connue (un modèle retiré du dossier, puis Créer) et une erreur
    inconnue ; 🩺 Expliquer donne-t-il la bonne cause et le bon geste ?

## 6. Pas encore dans la v3

Ne pas le compter comme un défaut : pont de mise à jour depuis la v2, licences tierces complètes. La liste complète
et à jour est dans [PARITE.md](PARITE.md).

## 7. Si quelque chose ne va pas

Rapporter, pour chaque problème : l'étape du parcours, ce qui était attendu, ce qui s'est passé, l'heure, et joindre
depuis le dossier de l'exe :

- `journal.json` ;
- `data\logs\comfyui.log` et `data\logs\llama.log` ;
- une capture de la fenêtre si l'interface est en cause.

Ces fichiers ne contiennent ni clé ni mot de passe, mais ils peuvent contenir des prompts : les relire avant de les
partager.

## 8. Retirer l'essai

Fermer la v3, puis supprimer le dossier d'essai. Si `emplacements` pointait vers la v2 : dans ses modèles, seuls des
dossiers de famille vides ont pu être ajoutés ; son ComfyUI a été lancé mais pas mis à jour (ComfyUI peut écrire ses
propres fichiers de travail dans son dossier, comme avec la v2).
