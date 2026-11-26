<#
.SYNOPSIS
Prépare l'essai du pont de mise à jour v2 -> v3 : sert en local une fausse « dernière release » que la v2 installée
propose et installe, sans rien publier sur GitHub.

.DESCRIPTION
La mise à jour intégrée de la v2 ne lit que la dernière release publiée (hors préversion) et refuse un numéro
« X.Y.Z-essai.N ». Ce script :

  1. prend le zip de la v3 (celui de la release à blanc : Aria-Studio-3.0.0-essai.N.zip) et en refait un zip
     « Aria-Studio-<Version>.zip » : dossier racine renommé, témoin app\__init__.py à la version demandée, avec son
     .sha256 ; l'exe n'est pas touché (il garde sa version d'essai) ;
  2. sert, sur 127.0.0.1 seulement, « /repos/<dépôt>/releases/latest » et les deux fichiers à télécharger, comme
     le ferait api.github.com ;
  3. avec -AmorcerCache, écrit cache\mise_a_jour.json de la v2 pour que son interface propose la mise à jour sans
     interroger GitHub (l'ancien fichier est gardé à côté, en .avant-essai).

La v2 n'accepte ce faux GitHub que si elle est lancée avec ARIA_TESTS_ISOLES=1 et ARIA_ESSAI_GITHUB=http://127.0.0.1:<port>,
dans la même fenêtre PowerShell : le script les affiche. Ctrl+C arrête le serveur.

    .\tools\essai-pont-v2.ps1 -Zip .\Aria-Studio-3.0.0-essai.1.zip -AmorcerCache

Attention : comme l'exe garde sa version d'essai, le journal dira « Mise à jour non installée » au démarrage suivant
même si l'installation a réussi (le bilan compare 3.0.0 à 3.0.0-essai.N). Regarde plutôt le contenu de programme\.
#>
[CmdletBinding()]
param(
    # Le zip de la v3 : Aria-Studio-X.Y.Z-essai.N.zip, fait par la release à blanc avec le témoin app\__init__.py.
    [Parameter(Mandatory)][string]$Zip,
    # La version que la v2 croira trouver (elle exige X.Y.Z, sans suffixe).
    [string]$Version = '3.0.0',
    [int]$Port = 8099,
    # Le dépôt que la v2 interroge par défaut.
    [string]$Depot = 'ZedjeK/Aria-Studio',
    # Écrit cache\mise_a_jour.json dans le dossier de données de la v2.
    [switch]$AmorcerCache
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version inattendue : « $Version ». La v2 n'accepte que X.Y.Z." }
if ($Port -lt 1024 -or $Port -gt 65535) { throw "Port inattendu : $Port." }
if ($Depot -notmatch '^[\w.-]+/[\w.-]+$') { throw "Dépôt inattendu : « $Depot ». Attendu : proprietaire/nom." }
if (-not (Test-Path -LiteralPath $Zip -PathType Leaf)) { throw "Zip introuvable : $Zip" }
$zipSource = (Resolve-Path -LiteralPath $Zip).Path

$racine = Split-Path -Parent $PSScriptRoot
$travail = Join-Path $racine '.work\essai-pont'
# Seul ce dossier est vidé ; une jonction à sa place mènerait la suppression hors du dépôt.
foreach ($d in (Join-Path $racine '.work'), $travail) {
    if ((Test-Path -LiteralPath $d) -and ((Get-Item -LiteralPath $d -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "$d est un lien ou une jonction : rien n'est supprimé. Retire-le, puis relance."
    }
}
if (Test-Path -LiteralPath $travail) { Remove-Item -LiteralPath $travail -Recurse -Force }
$extrait = Join-Path $travail 'extrait'
New-Item -ItemType Directory -Path $extrait -Force | Out-Null

# 1. Le zip refait : dossier racine Aria-Studio-<Version>, témoin à la bonne version.
Expand-Archive -LiteralPath $zipSource -DestinationPath $extrait
$dossiers = @(Get-ChildItem -LiteralPath $extrait -Directory)
if ($dossiers.Count -ne 1 -or @(Get-ChildItem -LiteralPath $extrait -File).Count -gt 0) {
    throw 'Le zip doit contenir un seul dossier racine Aria-Studio-X.Y.Z.'
}
$nom = "Aria-Studio-$Version"
if ($dossiers[0].Name -ne $nom) { Rename-Item -LiteralPath $dossiers[0].FullName -NewName $nom }
$dossier = Join-Path $extrait $nom
if (-not (Test-Path -LiteralPath (Join-Path $dossier 'Aria-Studio.exe'))) { throw "Aria-Studio.exe manque à la racine du zip." }
$temoin = Join-Path $dossier 'app\__init__.py'
if (-not (Test-Path -LiteralPath $temoin)) {
    throw "Le zip n'a pas app\__init__.py : prends un zip fait par une release à blanc qui contient le témoin."
}
[IO.File]::WriteAllLines($temoin, [string[]]@('# Témoin de l''essai du pont v2 -> v3.', "VERSION = `"$Version`""), (New-Object Text.UTF8Encoding($false)))

$zipSortie = Join-Path $travail "$nom.zip"
Compress-Archive -LiteralPath $dossier -DestinationPath $zipSortie
$empreinte = (Get-FileHash -LiteralPath $zipSortie -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$zipSortie.sha256", "$empreinte  $nom.zip`n", (New-Object Text.UTF8Encoding($false)))
$taille = (Get-Item -LiteralPath $zipSortie).Length
Write-Host "Zip refait : $zipSortie ($taille octets)"
Write-Host "Empreinte  : $empreinte"

# 2. Le cache de la v2, pour que son interface propose la mise à jour.
if ($AmorcerCache) {
    $cache = Join-Path $env:USERPROFILE '.Aria-Studio\cache'
    New-Item -ItemType Directory -Path $cache -Force | Out-Null
    $fichier = Join-Path $cache 'mise_a_jour.json'
    if (Test-Path -LiteralPath $fichier) { Copy-Item -LiteralPath $fichier -Destination "$fichier.avant-essai" -Force }
    $etat = [ordered]@{
        verifie_le = (Get-Date).ToString('yyyy-MM-ddTHH:mm:ss')
        erreur     = $null
        derniere   = [ordered]@{
            version = $Version
            nom     = "v$Version"
            url     = "https://github.com/$Depot/releases"
            notes   = 'Essai du pont de mise à jour v2 -> v3 (faux GitHub local).'
            date    = (Get-Date).ToString('yyyy-MM-dd')
            taille  = $taille
        }
    }
    [IO.File]::WriteAllText($fichier, ($etat | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding($false)))
    Write-Host "Cache de la v2 amorcé : $fichier (l'ancien est gardé en .avant-essai)"
}

# 3. Le faux GitHub, sur la boucle locale seulement.
$base = "http://127.0.0.1:$Port"
$release = [ordered]@{
    tag_name   = "v$Version"
    name       = "v$Version"
    draft      = $false
    prerelease = $false
    html_url   = "https://github.com/$Depot/releases"
    body       = 'Essai du pont de mise à jour v2 -> v3.'
    assets     = @(
        [ordered]@{ name = "$nom.zip"; browser_download_url = "$base/download/$nom.zip"; size = $taille; digest = "sha256:$empreinte" },
        [ordered]@{ name = "$nom.zip.sha256"; browser_download_url = "$base/download/$nom.zip.sha256"; size = (Get-Item -LiteralPath "$zipSortie.sha256").Length }
    )
}
$json = [Text.Encoding]::UTF8.GetBytes(($release | ConvertTo-Json -Depth 5 -Compress))
$fichiers = @{ "$nom.zip" = $zipSortie; "$nom.zip.sha256" = "$zipSortie.sha256" }

function Repondre($contexte, [int]$code, [byte[]]$octets, [string]$type) {
    $reponse = $contexte.Response
    $reponse.StatusCode = $code
    $reponse.ContentType = $type
    $reponse.ContentLength64 = $octets.Length
    $reponse.OutputStream.Write($octets, 0, $octets.Length)
    $reponse.OutputStream.Close()
}

$ecouteur = New-Object System.Net.HttpListener
$ecouteur.Prefixes.Add("$base/")
$ecouteur.Start()
Write-Host ''
Write-Host "Faux GitHub prêt sur $base. Dans UNE AUTRE fenêtre PowerShell, v2 fermée d'abord :"
Write-Host '    $env:ARIA_TESTS_ISOLES = ''1'''
Write-Host "    `$env:ARIA_ESSAI_GITHUB = '$base'"
Write-Host '    & "$env:USERPROFILE\.Aria-Studio\programme\Aria-Studio.exe"'
Write-Host 'Puis, dans la v2 : Paramètres > Mises à jour > Installer. Ctrl+C arrête ce serveur.'
try {
    $attente = $ecouteur.GetContextAsync()
    while ($true) {
        if (-not $attente.Wait(500)) { continue }
        $contexte = $attente.Result
        $attente = $ecouteur.GetContextAsync()
        $chemin = $contexte.Request.Url.AbsolutePath
        Write-Host "$((Get-Date).ToString('HH:mm:ss')) $($contexte.Request.HttpMethod) $chemin"
        if ($contexte.Request.HttpMethod -ne 'GET') { Repondre $contexte 405 ([byte[]]@()) 'text/plain'; continue }
        if ($chemin -eq "/repos/$Depot/releases/latest") { Repondre $contexte 200 $json 'application/json'; continue }
        $demande = if ($chemin.StartsWith('/download/')) { $chemin.Substring('/download/'.Length) } else { '' }
        if ($fichiers.ContainsKey($demande)) {
            Repondre $contexte 200 ([IO.File]::ReadAllBytes($fichiers[$demande])) 'application/octet-stream'
            continue
        }
        Repondre $contexte 404 ([byte[]]@()) 'text/plain'
    }
}
finally { $ecouteur.Stop(); $ecouteur.Close() }
