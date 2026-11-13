# Journal des versions

Un bloc par version, le plus récent en haut. Les notes complètes d'une version
sont dans `docs/releases/` (pour la 2.0.0) et dans la page de sa release sur
GitHub (pour les précédentes).

## Non publié

Changements fusionnés dans `main` après le tag `v2.0.0`.

### Ajouté

- Habillage selon le contrat visuel des applis Aria : menu latéral, panneaux à coins coupés, sous-onglets en encoche avec leur encadré « À savoir », cartes de départ à onglet, boutons en biais, liseré accentué sur le panneau que touche le bouton principal (#63).
- Fond animé en gouttes, avec ses réglages (activé, vitesse, nombre de gouttes) dans Réglages → Système, gardés dans `data/fond.json` (#63).
- Le format se choisit pour « Redessiner une image » (#58) et au Scénario pour la séquence, et le LLM connaît le cadre (#60, #61).
- `docs/FICHE.md`, la fiche projet (#64), et les scripts `build.ps1`, `tools/tester.ps1` et `tools/version.ps1` (#65).

### Modifié

- Bonsai 2 devient le LLM par défaut (#54).
- Les modèles d'image et de vidéo sont rangés chacun de leur côté, avec une migration au démarrage (#57).
- La réponse du LLM est assez longue pour les mots visés, et le clip H3 du chat se remplit en trois champs (#53).
- La notification de fin de job a un tag unique par job et un bandeau si elle échoue (#55).
- La molette fait défiler la page, plus un cadre interne (#59).

### Retiré

- Les LLM Ministral, Qwen3.5 et Gemma (#54).

### Corrigé

- `catalogue/moteurs.yaml` est de nouveau lisible (#56).

## 2.0.0 (tag posé le 2026-11-09, release en brouillon, non publiée)

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
