# Essayer la v3 sur un autre PC

La v3 (Rust et Tauri) n'est pas encore publiée. Ce guide décrit un essai sur un PC capable de faire tourner les
moteurs, sans toucher à une v2 installée : tout se passe dans un dossier d'essai, en mode portable, et se retire en
supprimant ce dossier.

## 1. Ce qu'il faut

- Windows 10 ou 11, 64 bits, avec WebView2 (présent d'office sur Windows 11).
- Une carte NVIDIA avec un pilote de la branche 580 ou plus récente (CUDA 13).
- La v2 fermée pendant l'essai : la v3 utilise les mêmes ports (ComfyUI sur 8195, llama-server sur 8095).
- Le zip de la v3, produit par l'essai à blanc du workflow Release sur la branche `refonte/v3` :

```powershell
gh workflow run Release --repo ZedjeK/Aria-Studio --ref refonte/v3
```

  Une fois le run terminé, l'artefact `release` de sa page contient `Aria-Studio-3.0.0.zip` et son empreinte. Il est
  gardé 7 jours.

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
- Modèles : la v3 ne les télécharge pas encore. Les déposer à la main dans `modeles\` à côté de l'exe, rangés comme
  en v2 (les noms et les dossiers attendus sont dans la fiche de chaque workflow, `workflows\<id>\fiche.yaml`,
  section `fichiers`), ou copier ceux d'une autre machine.

## 4. Le parcours à vérifier

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

## 5. Pas encore dans la v3

Ne pas le compter comme un défaut : téléchargement des modèles, références et variation, LoRA, retouche,
agrandissement, Montage et vidéo, Scénario, Casting, assistant de premier lancement, Réglages → Sécurité et LLM,
installation « J'ai déjà ComfyUI » par un sélecteur de dossier, pont de mise à jour depuis la v2. La liste complète
et à jour est dans [PARITE.md](PARITE.md).

## 6. Si quelque chose ne va pas

Rapporter, pour chaque problème : l'étape du parcours, ce qui était attendu, ce qui s'est passé, l'heure, et joindre
depuis le dossier de l'exe :

- `journal.json` ;
- `data\logs\comfyui.log` et `data\logs\llama.log` ;
- une capture de la fenêtre si l'interface est en cause.

Ces fichiers ne contiennent ni clé ni mot de passe, mais ils peuvent contenir des prompts : les relire avant de les
partager.

## 7. Retirer l'essai

Fermer la v3, puis supprimer le dossier d'essai. Si `emplacements` pointait vers la v2 : dans ses modèles, seuls des
dossiers de famille vides ont pu être ajoutés ; son ComfyUI a été lancé mais pas mis à jour (ComfyUI peut écrire ses
propres fichiers de travail dans son dossier, comme avec la v2).
