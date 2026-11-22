**Aria Studio** {VERSION} : Un studio d'images et de vidéos 100 % local : un LLM local écrit le prompt, ComfyUI le tourne.

<!-- Modèle des notes de version. À chaque version : copie ce fichier vers docs/releases/v{VERSION}.md dans la PR
     de version, remplace {VERSION} et {REPO_URL}, remplis les sections, supprime ce commentaire. Le workflow de
     release publie ce fichier tel quel. -->

## Nouveautés

- Ce qui change pour toi, une ligne par nouveauté.

## À savoir

- Ruptures, migrations, changements de licence. Supprime la section si elle est vide.

## Installation

1. Télécharge `Aria-Studio-{VERSION}.zip` ci-dessous.
2. Décompresse d'abord le zip en entier (clic droit, « Extraire tout »).
   Lancé depuis l'aperçu du zip dans l'Explorateur, l'exe démarre sans les fichiers livrés à côté de lui.
3. Lance `Aria-Studio.exe`.
   Choisis « Installer » pour le ranger dans `%USERPROFILE%\.Aria-Studio\programme`, ou « Lancer sans installer ».
4. L'exe n'est pas signé : si Windows affiche « Windows a protégé votre ordinateur », clique sur « Informations
   complémentaires », puis sur « Exécuter quand même ».

## Mise à jour depuis une version précédente

L'appli te propose elle-même la nouvelle version : elle remplace l'exe là où il est et garde la précédente.

Pour le faire à la main : ferme l'appli, décompresse le nouveau zip en entier, puis
lance son exe et choisis « Installer » : la version installée est remplacée, la précédente est gardée.

Tes réglages sont dans `%USERPROFILE%\.Aria-Studio` et sont repris tels quels.

## Contenu

| Composant | Version | Source |
| --- | --- | --- |
| Aria Studio | {VERSION} | [ce dépôt]({REPO_URL}) |

Le détail des composants embarqués est dans `LICENCES-TIERS.txt`, joint au zip.

## Comment vérifier ce que tu as téléchargé

L'empreinte du zip est dans `Aria-Studio-{VERSION}.zip.sha256`. Dans PowerShell :

```powershell
(Get-FileHash .\Aria-Studio-{VERSION}.zip -Algorithm SHA256).Hash
```

L'empreinte seule ne prouve pas d'où vient le fichier. Pour vérifier qu'il a bien été construit par ce dépôt :

```powershell
gh attestation verify .\Aria-Studio-{VERSION}.zip --repo ZedjeK/Aria-Studio
```
