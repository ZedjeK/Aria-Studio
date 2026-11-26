# Essayer le pont de mise à jour v2 → v3

Le pont (section 1 de [PARITE.md](PARITE.md)) n'a été vérifié que par des tests du cœur et par une release à blanc. Cet
essai le fait tourner pour de vrai : une v2.4.x **installée** (`%USERPROFILE%\.Aria-Studio\programme\`) se met à jour
vers la v3 par son propre bouton « Installer ».

Rien n'est publié sur GitHub : la v2 est dirigée vers un faux « releases/latest » servi sur `127.0.0.1` par
`tools\essai-pont-v2.ps1`. La v2 n'accepte ce faux GitHub que si elle est lancée avec `ARIA_TESTS_ISOLES=1` et
`ARIA_ESSAI_GITHUB=http://127.0.0.1:<port>` (variables lues seulement par elle, jamais en usage normal).

## 1. Avant de commencer

- Un PC avec une v2.4.x installée par l'exe (dossier `programme\`), et ses données.
- **Sauvegarde** de `%USERPROFILE%\.Aria-Studio` (au moins `reglages.json`, `journal.json`, `data\`) :

```powershell
$maison = "$env:USERPROFILE\.Aria-Studio"
Copy-Item -LiteralPath "$maison\reglages.json", "$maison\journal.json" -Destination "$env:USERPROFILE\Desktop" -ErrorAction SilentlyContinue
```

- La v2 **fermée** (⏻ Quitter dans son icône).
- Un clone du dépôt `ZedjeK/Aria-Studio` sur la branche `refonte/v3` à jour, et `gh` connecté.

## 2. Le zip de la v3

L'essai à blanc du workflow Release fabrique le zip (avec le témoin que lit la v2) et le garde 7 jours :

```powershell
gh workflow run Release --repo ZedjeK/Aria-Studio --ref refonte/v3
$id = gh run list --repo ZedjeK/Aria-Studio --workflow Release --limit 1 --json databaseId --jq '.[0].databaseId'
gh run watch $id --repo ZedjeK/Aria-Studio --exit-status
gh run download $id --repo ZedjeK/Aria-Studio --name release --dir "$env:USERPROFILE\Desktop\essai-pont"
```

Le zip s'appelle `Aria-Studio-3.0.0-essai.N.zip`.

## 3. Le faux GitHub

Depuis le clone, dans une première fenêtre PowerShell :

```powershell
.\tools\essai-pont-v2.ps1 -Zip "$env:USERPROFILE\Desktop\essai-pont\Aria-Studio-3.0.0-essai.1.zip" -AmorcerCache
```

Le script refait un zip `Aria-Studio-3.0.0.zip` (la v2 refuse un numéro d'essai), écrit le cache de la v2 pour que
son interface propose la mise à jour (l'ancien fichier est gardé en `.avant-essai`) et sert la release sur
`http://127.0.0.1:8099`. Laisse cette fenêtre ouverte : on y voit chaque requête de la v2.

## 4. Lancer la v2 et installer

Dans une **seconde** fenêtre PowerShell (les variables ne valent que pour elle et ses enfants) :

```powershell
$env:ARIA_TESTS_ISOLES = '1'
$env:ARIA_ESSAI_GITHUB = 'http://127.0.0.1:8099'
& "$env:USERPROFILE\.Aria-Studio\programme\Aria-Studio.exe"
```

Dans la v2 : la mise à jour 3.0.0 est proposée, puis « Installer ». Attendu, dans l'ordre : la v2 se ferme, son icône
disparaît (environ 12 s), puis la v3 s'ouvre depuis `programme\`.

## 5. Ce qu'il faut relever

| À regarder | Attendu |
| --- | --- |
| `programme\` | `Aria-Studio.exe` de la v3, `fichiers.txt`, `BUILDINFO.txt`, `LICENCES-TIERS.txt` ; **ni `python\` ni `app\`** |
| `programme.precedent\` | la v2 entière (`python\`, `app\`, son lanceur, ses fichiers) |
| `programme.nouveau\`, `programme.precedent.avant\` | absents |
| `cache\mise-a-jour\relais.json` | absent après le premier lancement de la v3 |
| Journal de la v3 | « Aria lancée » en 3.0.0-essai.N ; **le bilan dira « Mise à jour non installée »** : l'exe garde sa version d'essai alors que la v2 attendait 3.0.0 (écart propre à l'essai, pas à la release) |
| Réglages, créations, modèles, clés | intacts : la v3 relit les données de la v2 |
| Raccourci du Bureau, démarrage avec Windows | à noter : pointent-ils encore `programme\Aria-Studio.exe` ? (section 2 de PARITE.md) |

## 6. Retour à la v2

Fermer la v3 (⏻ Quitter), puis lancer `programme.precedent\Aria-Studio.exe` : la v2 repart avec ses données. Pour
revenir à l'état d'avant l'essai, remettre `programme.precedent\` à la place de `programme\` (renommer l'un, puis
l'autre) et restaurer `cache\mise_a_jour.json.avant-essai`.

## 7. Cas d'échec à essayer ensuite

- **`programme\` occupé** : relancer l'essai en gardant ouverte une v2 lancée depuis `programme\` sans passer par la
  mise à jour (par exemple un fichier de `programme\` ouvert dans un éditeur) : la v3 attend 15 s, dit « tourne
  encore » dans une boîte de dialogue, la v2 repart, et son journal note « Mise à jour non installée ».
- **Zip incomplet** : retirer `LICENCES-TIERS.txt` du zip d'origine (l'ouvrir, supprimer le fichier) avant de
  relancer le script : la v3 dit ce qui manque, rien ne change dans `programme\`, la v2 repart.
