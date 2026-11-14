<p align="center">
  <img src="docs/logo.png" width="160" alt="Logo d'Aria Studio : un A néon glitché avec un clap de cinéma orange">
</p>

<h1 align="center">Aria Studio</h1>

Un studio de génération d'images et de vidéos **100 % local**, piloté depuis le navigateur. Tu composes ton idée
en français au Scénario, un LLM local t'aide à en faire un prompt fidèle, et ta carte graphique tourne exactement ce
prompt : rien ne part dans le cloud.

Sous le capot : [FastAPI](https://fastapi.tiangolo.com/) pour l'interface, [ComfyUI](https://github.com/comfyanonymous/ComfyUI)
pour les images et les vidéos, [llama.cpp](https://github.com/ggml-org/llama.cpp) pour le LLM.

> **Testée sur une seule machine.** Aria n'a été essayée que sous Windows 11, avec une RTX 4080 SUPER 16 Go et 64 Go de RAM.
> Les retours sur d'autres cartes sont les bienvenus (onglet *Issues*).

## Ce que ça fait

- **Scénario** : la seule place où le LLM écrit. Tu choisis Image, Clip ou Séquence et le modèle cible, puis tu
  remplis ce que tu veux :
  - **Départ** : *Depuis un texte* (aucune image), *Mélanger des images* (jusqu'à 10, pour une nouvelle scène) ou
    *Redessiner une image* (une seule, refaite plus ou moins fort) ; ou bien le rush à animer, l'image à retoucher ou
    le personnage d'une séquence. Le LLM voit les images quand il écrit.
  - **Sujet** (facultatif), **Décor et ambiance**, **Mise en scène** : des listes à cliquer, assemblées mot pour mot.
  - **Histoire** (séquence) : le synopsis, les contraintes et les plans ; le LLM écrit le prompt de chaque plan.
  - **Libre** : un chat où le LLM te pose les questions qui manquent (avec des réponses à cliquer), tient une fiche
    de ce qu'il a compris, puis écrit le prompt avec toi et l'ajuste.

  Le prompt final, toujours visible et modifiable, part vers le Plateau ou le Montage.
- **Plateau** : il tourne ce prompt tel quel, avec ses réglages (modèle, format, LoRA, références, redessiner une image).
- **Retouche** : retoucher un rush au pinceau (avec ou sans masque ; le Scénario peut écrire la consigne en voyant
  l'image) ou l'agrandir (avec des raccourcis de format), avec seed, nombre de variantes et « Ranger dans ». Le résultat se
  compare à l'original avec un curseur ou côte à côte, et on peut repartir de lui. Les Rushes, l'import et le
  glisser-déposer sont dans l'onglet.
- **Dossiers** : les Rushes (images) et les Bobines (clips) ont chacun leurs dossiers.
- **Montage** : des clips vidéo avec le son (MiniMax H3), en animant un rush ou depuis le texte, et les séquences :
  il tourne les plans écrits au Scénario, avec la voix de référence de ta bibliothèque, puis assemble le film.
- **Casting** : des bancs pour comparer les modèles d'image et les LLM sur les mêmes idées, et choisir tes favoris.
- **Réglages** : par LLM, par modèle et par checkpoint, avec un bouton *Optimiser* qui part des recommandations
  officielles et de ta carte. Le moteur de chaque LLM (llama.cpp officiel ou PrismML pour les GGUF ternaires, Bonsai 2
  et ses dérivés) se choisit tout seul d'après le fichier, et peut s'imposer à la main. La réflexion des LLM est
  activée par défaut : l'écriture est plus lente, mais plus soignée. Elle se coupe LLM par LLM ; les courtes
  traductions et les questions du chat Libre s'en passent toujours.
- **Vision des LLM** : avec son projecteur d'images (`mmproj`, livré avec Bonsai 2 et valable pour ses dérivés), le LLM
  voit l'image qu'il anime, qu'il retouche ou qu'il prend en référence avant d'écrire. Sans projecteur, il lit le
  prompt qui a créé l'image. Ce que tu fixes reste intouchable : il ne remplit que ce que tu as laissé ouvert.
- **Modèles**, en cinq sous-onglets :
  - **Rechercher** : modèles image, modèles vidéo, LoRA ou LLM, sur Civitai et Hugging Face. Seuls les modèles qui
    vont dans un workflow d'Aria sont montrés, leur dossier est choisi pour toi, et ce que tu as déjà porte
    « installé ✓ ».
  - **Ma collection** : ce que tu as, d'où ça vient, et quels workflows l'utilisent. « Retrouver les sources »
    retrouve la page Civitai d'un fichier déposé à la main, grâce à son empreinte SHA-256 et seulement sur un clic.
  - **Texte → image**, **Image → image**, **Vidéo** : les workflows de chaque usage, avec leur favori, leurs
    checkpoints, leurs LoRA, le LLM qui écrit pour chacun (le Scénario le prend tout seul), et un bouton pour
    télécharger ce qui leur manque. Quand l'auteur d'un modèle publie un LLM
    entraîné pour lui écrire ses prompts (le « prompt enhancer » de Qwen-Image 2.1, par exemple), Aria le propose
    🎯 : choisi au Scénario, il écrit avec sa propre consigne officielle. Les autres LLM reçoivent les règles du modèle cible,
    tirées de ses guides officiels.
- **✕ Annuler** : une création, un clip ou une séquence en cours s'arrête d'un clic, où qu'elle en soit ; ce qui
  est déjà produit reste.
- **Garde-VRAM** : une génération attend que les autres applis rendent la carte graphique au lieu de planter.

## Configuration requise

- **Windows 11, 64 bits.** Windows 10 n'a pas été vérifié.
- **Une carte NVIDIA** avec un pilote de la branche **580 ou plus récente** (les moteurs sont compilés pour CUDA 13).
  Testé seulement sur 16 Go de VRAM : les minimums par carte ne sont pas encore mesurés.
- **De la place disque**, selon ce que tu installes :

| Pack | Pour | Taille |
|---|---|---|
| Images (conseillé) | Créer depuis une idée ou des références | 16 Go |
| Réécriture des prompts (conseillé) | Le LLM qui transforme ton idée en prompt (Bonsai 2 27B, compressé en ternaire) | 6 Go |
| Retouche avancée | Retouche plus précise, avec masque | 29 Go |
| Vidéo | Clips et séquences avec voix | 39 Go |

Les packs partagent certains fichiers : Images, Retouche et Vidéo ensemble font environ 84 Go. Compte en plus
environ 8 Go pour les moteurs (archives gardées pour pouvoir revenir en arrière), et 10 Go de marge que l'assistant
garde libres.

## Installation

1. Télécharge `Aria-Studio-X.Y.Z.zip` depuis la [dernière release](https://github.com/ZedjeK/Aria-Studio/releases/latest)
   avec son fichier `.sha256`, et vérifie l'empreinte :

   ```powershell
   Get-FileHash .\Aria-Studio-X.Y.Z.zip -Algorithm SHA256
   ```

   L'empreinte prouve que le zip est intact. Pour savoir d'où il vient, les zips construits par le workflow de
   release actuel portent aussi une attestation de provenance de GitHub (`gh` connecté) :

   ```powershell
   gh attestation verify .\Aria-Studio-X.Y.Z.zip --repo ZedjeK/Aria-Studio
   ```

   Elle prouve que le zip sort du workflow de ce dépôt, sur le tag de la version. Les versions publiées avant ce
   workflow n'en ont pas.

2. Décompresse le zip où tu veux, puis double-clique sur `Aria-Studio.exe`. Une fenêtre te propose :
   - **Installer** : le programme est rangé dans `%USERPROFILE%\.Aria-Studio\programme`, à côté de tes données, puis
     Aria se lance depuis là. Une case crée un raccourci sur ton Bureau (décochée d'office). Le dossier du zip reste
     tel quel : tu peux le supprimer ensuite ;
   - **Lancer sans installer** : Aria tourne depuis le dossier du zip, rien n'est copié.

   « Ne plus me proposer » coupe la question ; elle se réactive dans Réglages → Système, carte Mises à jour. Avec
   `Aria-Studio.exe --portable`, ou un dossier `data` à côté de l'exe (mode portable), la fenêtre ne s'ouvre pas.

   L'interface s'ouvre ensuite dans ton navigateur. Pour fermer Aria, clique sur **⏻ Quitter**, au bout de la barre
   des onglets : son serveur, ComfyUI et Llama s'arrêtent.
3. Le lanceur n'est pas signé : Windows SmartScreen peut afficher « Windows a protégé votre ordinateur ». Clique sur
   *Informations complémentaires*, puis sur *Exécuter quand même*.
4. Au premier lancement, l'**assistant** te guide :
   1. **Ta machine** : carte graphique, pilote, mémoire, et le dossier où iront les modèles ;
   2. **Moteurs** : ComfyUI et llama.cpp, dans les versions testées avec Aria, téléchargés depuis leur release GitHub
      officielle et vérifiés par leur SHA-256 ;
   3. **Ce que tu veux faire** : les packs à cocher, avec la place nécessaire ;
   4. **Licences** : celle de chaque modèle, à lire et accepter avant tout téléchargement ;
   5. **Téléchargement** : depuis Hugging Face, chaque fichier vérifié par son empreinte à l'arrivée ;
   6. **C'est prêt** : les modèles réglés par défaut, et une image d'essai pour vérifier que tout tourne.

   Tu peux le relancer à tout moment depuis l'onglet Réglages pour ajouter un pack.

### Tu as déjà ComfyUI ?

Dans Réglages, carte Moteurs, *Utiliser mon ComfyUI…* accepte un **ComfyUI portable** (avec `python_embeded`) d'une
version au moins égale à celle d'Aria. Aria le lance une fois pour vérifier qu'il connaît tous les nœuds dont elle
a besoin, et n'y installe rien. Elle le lance avec ses propres dossiers de travail (dans `cache\comfyui\`) : ses
images et vidéos de départ et ses résultats ne passent pas par les dossiers `input` et `output` de ton ComfyUI.

## Où vont tes affaires

Tout ce qui t'appartient vit dans `%USERPROFILE%\.Aria-Studio`, qu'une mise à jour ne touche jamais :

| Dossier | Contenu |
|---|---|
| `data\` | Rushes, clips, séquences, voix, réglages, journaux |
| `modeles\` | Les modèles (ailleurs si tu l'as changé dans l'onglet Modèles) |
| `moteurs\` | ComfyUI portable et llama-server |
| `cache\` | Ce qui se recalcule (empreintes des modèles, listes annoncées par ComfyUI) et les dossiers de travail de ComfyUI (`cache\comfyui\`, vidés au fil des générations) : l'effacer ne perd rien |
| `reglages.json` | Tous tes réglages et choix : dossier des modèles, favoris, licences acceptées, fond, notifications, réglages des LLM et des workflows… |
| `sauvegardes\` | Une copie de `reglages.json` avant chaque changement (les 20 dernières, à restaurer dans Réglages → Système), et les anciens fichiers de réglages copiés quand une version les a réunis dans `reglages.json` |
| `journal.json` | Les 500 dernières opérations d'Aria sur tes affaires (téléchargements, moteurs, corbeilles, migrations, réglages), visibles dans Réglages → Système |
| `programme\` | Le programme, si tu l'as installé (la version d'avant reste dans `programme.precedent\`) |

**Mode portable** (clé USB, disque externe…) : crée un dossier `data` à côté de `Aria-Studio.exe`, et tout restera
dans le dossier du programme (les moteurs vont alors dans `services`).

**Mettre à jour** : quand une nouvelle version est publiée, le bandeau « ✨ Aria Studio X.Y.Z est disponible » propose
**Installer la mise à jour** (aussi dans Réglages → Système). Après ta confirmation, Aria la télécharge depuis la
release sur GitHub, vérifie son empreinte SHA-256 deux fois (celle que calcule GitHub et le `.sha256` publié), puis
redémarre : la nouvelle version remplace `programme\`, la version d'avant devient `programme.precedent\`, et
l'interface se rouvre dans un nouvel onglet. Une génération en cours empêche la mise à jour. Pour revenir à la version
d'avant, quitte Aria et lance `programme.precedent\Aria-Studio.exe` depuis l'Explorateur. Lancée sans installer,
Aria s'installe ainsi dans `programme\`. En mode portable, la nouvelle version est seulement décompressée à côté du
dossier actuel : recopie-y tes dossiers `data`, `modeles` et `services` et ton `reglages.json`. Les versions publiées
jusqu'à la 2.0.0 n'ont pas ce bouton : on les met à jour à la main (décompresser le zip, lancer, *Installer*).

**Désinstaller** : quitte Aria, puis supprime le dossier `%USERPROFILE%\.Aria-Studio\programme` (et
`programme.precedent`) et le raccourci du Bureau. Tes données restent dans `%USERPROFILE%\.Aria-Studio` ; supprime
ce dossier entier pour tout effacer, modèles compris.

## Licences des modèles

Aria ne fournit aucun modèle : chacun a sa propre licence, affichée dans l'assistant et dans l'onglet Modèles.
**C'est à toi de vérifier que ton usage la respecte.** Pour les modèles proposés par l'assistant :

| Modèle | Licence | À savoir |
|---|---|---|
| Qwen-Image 2.1 et 2.1 Edit | [Qwen Research License](https://huggingface.co/Qwen/Qwen-Image-2.1/blob/main/LICENSE) | Usage non commercial seulement |
| MiniMax H3 | [MiniMax H3 Community License](https://huggingface.co/MiniMaxAI/MiniMax-H3/blob/main/LICENSE) | Non autorisé dans l'Union européenne, au Royaume-Uni, en Corée du Sud et aux États-Unis ; lis les conditions commerciales ; le Turbo 8 steps n'a pas de licence déclarée |
| Bonsai 2 27B | Apache 2.0 | Usage commercial permis |

## Contenu et sécurité

Chaque prompt passe par le filtre **AS-18**, avant la file d'attente et juste avant d'être envoyé à ComfyUI. Un
personnage mineur est autorisé (scène de famille, manga…), mais un personnage mineur associé à un terme sensible
(nudité, sexualité, suggestif) est toujours bloqué. Le filtre ne se désactive pas : `securite/termes.yaml` peut
seulement ajouter des termes, jamais en retirer.

Aria n'écoute que sur ta machine (`127.0.0.1`), et ne répond qu'à l'interface ouverte par son lanceur (un jeton tiré à chaque lancement). Une fois par jour, elle demande à GitHub s'il existe une nouvelle
release (option à couper dans Réglages). Elle ne télécharge rien d'elle-même : chaque téléchargement part d'un clic.

Certains modèles Civitai et Hugging Face demandent ta clé d'accès. Colle-la dans Réglages, carte **🔑 Clés
d'accès**. Elle est rangée dans le Gestionnaire d'identification Windows, chiffrée et propre à ton compte, jamais
dans un fichier d'Aria, et l'interface ne la réaffiche jamais. Un bouton la teste auprès de sa source. Les
variables d'environnement `CIVITAI_TOKEN` et `HF_TOKEN` servent si aucune clé n'est enregistrée dans Aria.

## Limites à connaître

Ce qu'Aria ne fait pas, ou ne garantit pas, pour que tu ne le découvres pas par accident :

- **Tes clés d'accès.** Le coffre de Windows les protège contre quelqu'un qui copierait tes fichiers. Il ne les protège
  pas d'un programme lancé sous ton compte Windows : celui-là peut les lire comme Aria le fait.
- **Une mise à jour prouve son intégrité, pas sa provenance.** Les deux empreintes vérifiées avant l'installation
  prouvent que le zip est bien celui de la release, pas que la release est légitime : si le compte GitHub de l'auteur
  était compromis, une fausse release aurait de vraies empreintes. La provenance repose sur ce compte, sur les releases
  immuables et sur l'attestation jointe à chaque release (`gh attestation verify`, voir Installation).
- **Le jeton ne protège pas d'un programme de ton compte.** Aria tire un jeton à chaque lancement et l'exige à chaque
  requête : une autre page du navigateur ou un programme qui ne le connaît pas est refusé. Mais le jeton est écrit dans
  `cache\session.json` (pour que le lanceur le retrouve) : un programme lancé sous ton compte Windows peut le lire.
- **Un ancien onglet ou un favori ne rouvre pas Aria.** Le jeton change à chaque lancement : une page ouverte avant un
  redémarrage, ou un favori vers `http://127.0.0.1:8075`, affiche « Aria Studio s'ouvre par son lanceur ». Relance
  `Aria-Studio.exe`.
- **ComfyUI est joignable par tout programme de ta machine.** Il n'écoute que sur `127.0.0.1` et refuse les autres
  sites du navigateur, mais il n'a aucune option de clé d'accès : un programme local peut lui parler directement, sans
  passer par Aria. llama-server, lui, reçoit une clé neuve à chaque démarrage et refuse qui ne la présente pas.
- **Les anciens fichiers de travail de ComfyUI restent.** Jusqu'à la 2.0.0, Aria laissait dans `ComfyUI\input` et
  `ComfyUI\output` (dans `moteurs\comfyui\`, ou dans ton propre ComfyUI) une copie de chaque fichier envoyé et de
  chaque résultat. Elle ne les efface pas pour toi : certains peuvent être la dernière copie d'une création que tu as
  vidée de la corbeille. Supprime-les à la main si tu veux la place.
- **Fermer l'onglet ne ferme pas Aria.** Son serveur, et parfois un moteur et la mémoire de la carte graphique, restent
  occupés tant que tu n'as pas cliqué sur **⏻ Quitter**.
- **La corbeille garde la place.** Un modèle, une séquence ou un rush supprimé part à la corbeille d'Aria et se
  restaure ; la place sur le disque n'est libérée qu'avec « Vider la corbeille », qui, elle, est définitive. La
  corbeille des modèles vit dans leur dossier (`.corbeille`) : si tu changes de dossier des modèles, celle de l'ancien
  y reste, et elle ne se restaure qu'en reprenant ce dossier.
- **Les sauvegardes des réglages sont limitées.** Aria garde les 20 dernières versions de `reglages.json` : au-delà,
  les plus anciennes s'effacent. Revenir à une version d'avant d'Aria retrouve ses anciens fichiers de réglages, mais
  pas ce qui a changé depuis qu'ils ont été réunis dans `reglages.json`.
- **Testée sur une seule machine** (Windows 11, RTX 4080 SUPER 16 Go, 64 Go de RAM) et lanceur non signé : voir
  Installation.

## Pour les développeurs

Il faut Python 3.11. `run.ps1` prépare le `.venv`, installe `requirements.txt` et lance le serveur sur
<http://127.0.0.1:8075> :

```powershell
.\run.ps1
```

Les tests tournent dans un dossier utilisateur jetable : ils ne touchent jamais ton `%USERPROFILE%\.Aria-Studio`
(modèles installés, réglages, créations), et leur résultat ne dépend pas de ce qui est installé :

```powershell
.\tools\tester.ps1
```

(`.\tools\tester.ps1 tests.test_vision -v` passe ses arguments à `unittest` ; c'est une enveloppe de
`tools\tester.py`, que la CI appelle directement.)

Le banc de palier mesure la VRAM que chaque modèle demande vraiment, en simulant des cartes de 12 et 8 Go sur une
carte de 16 Go (une bonne heure de GPU, serveur d'Aria arrêté). `--ecrire` reporte ensuite les mesures dans les
fiches :

```powershell
.venv\Scripts\python.exe -m app.paliers
```

Construire la version portable (Python embarqué, dépendances du `.venv`, lanceur compilé avec le `csc.exe` de
Windows) et l'installer dans `%USERPROFILE%\.Aria-Studio`, à côté de tes affaires. Seuls les fichiers du programme
sont remplacés. Avec `--sans-installer`, elle reste dans `.build_cache\staging` pour faire le zip :

```powershell
.venv\Scripts\python.exe tools\build_portable.py
```

Pour seulement construire le zip et son `.sha256`, comme le fait la release, sans rien poser chez toi :
`.\build.ps1` les écrit dans `.work\sortie\`. `.\tools\version.ps1` affiche la version en cours.

Publier une version : passer `VERSION` de `app/__init__.py` au nouveau numéro, déposer les notes dans
`docs/releases/vX.Y.Z.md`, fusionner, puis pousser le tag. `.github/workflows/release.yml` construit alors le zip
et son `.sha256` sur Windows et crée la release en brouillon, à relire et à publier :

```powershell
git tag -a v2.0.0 -m "Version 2.0.0"; git push origin v2.0.0
```

| Dossier | Rôle |
|---|---|
| `app\` | Le serveur : Plateau, Montage, séquences, orchestrateur des moteurs, assistant… |
| `web\` | L'interface (HTML, CSS, JavaScript, sans framework) |
| `workflows\` | Un dossier par modèle : graphe ComfyUI (`api.json`) et sa fiche (fichiers, SHA-256, licence) |
| `catalogue\` | Moteurs épinglés, LLM, packs de l'assistant |
| `casting\`, `prompt\`, `securite\` | Idées des bancs, listes du prompt, termes du filtre AS-18 |
| `tools\` | Construction de la version portable et son lanceur |

## Crédits

- [ComfyUI](https://github.com/comfyanonymous/ComfyUI), [llama.cpp](https://github.com/ggml-org/llama.cpp) et le
  [llama.cpp de PrismML](https://github.com/PrismML-Eng/llama.cpp) pour les GGUF ternaires ;
- les modèles [Qwen-Image](https://github.com/QwenLM/Qwen-Image-2.1), [MiniMax H3](https://huggingface.co/MiniMaxAI/MiniMax-H3),
  [Bonsai 2](https://huggingface.co/prism-ml/Ternary-Bonsai-2-27B-gguf), avec les versions préparées pour ComfyUI
  par [Comfy-Org](https://huggingface.co/Comfy-Org) ;
- [FastAPI](https://fastapi.tiangolo.com/), [Uvicorn](https://www.uvicorn.org/), [Pillow](https://python-pillow.org/),
  [python-build-standalone](https://github.com/astral-sh/python-build-standalone).

## Licence

Le code d'Aria Studio est sous [licence MIT](LICENSE). Les moteurs et les modèles qu'il télécharge gardent leur
propre licence. Les polices de l'interface, [Space Grotesk](https://github.com/floriankarsten/space-grotesk) et
[IBM Plex Mono](https://github.com/IBM/plex), sont embarquées dans `web/polices/` sous licence SIL Open Font
License 1.1 (textes joints).
