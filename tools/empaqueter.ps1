<#
.SYNOPSIS
Fait le zip de la version et son empreinte, à partir de ce que build.ps1 a posé dans .work\sortie\.

.DESCRIPTION
Produit, dans .work\release\ :
  Aria-Studio-<version>.zip          dossier racine Aria-Studio-<version>\ : ce que build.ps1 a produit,
                                      LICENCES-TIERS.txt, BUILDINFO.txt et fichiers.txt
  Aria-Studio-<version>.zip.sha256   « empreinte  nom du fichier »

fichiers.txt liste, un nom par ligne, tout ce que contient le dossier du zip (sauf lui-même). L'installation par l'exe
et les mises à jour copient exactement cette liste ; un exe lancé sans ce fichier à côté de lui ne propose pas de
s'installer.

Commun à toutes les stacks. La version vient de tools\version.ps1.
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$racine = Split-Path -Parent $PSScriptRoot
$nom = 'Aria-Studio'
$version = "$(& (Join-Path $PSScriptRoot 'version.ps1') | Select-Object -Last 1)".Trim()
if ($version -notmatch '^\d+\.\d+\.\d+(-essai\.[1-9]\d*)?$') { throw "Version inattendue : « $version ». Attendu : X.Y.Z ou X.Y.Z-essai.N." }

$sortie = Join-Path $racine '.work\sortie'
if (-not (Test-Path -LiteralPath $sortie) -or -not (Get-ChildItem -LiteralPath $sortie -Force)) {
    throw "Rien à empaqueter : .work\sortie\ est vide. Lance d'abord .\build.ps1."
}

$release = Join-Path $racine '.work\release'
# Une jonction à la place de .work ou de .work\release mènerait la suppression qui suit hors du dépôt.
foreach ($d in (Join-Path $racine '.work'), $release) {
    if ((Test-Path -LiteralPath $d) -and ((Get-Item -LiteralPath $d -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "$d est un lien ou une jonction : rien n'est supprimé. Retire-le, puis relance."
    }
}
if (Test-Path -LiteralPath $release) { Remove-Item -LiteralPath $release -Recurse -Force }
$dossier = Join-Path $release "$nom-$version"
New-Item -ItemType Directory -Path $dossier | Out-Null

Get-ChildItem -LiteralPath $sortie -Force | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $dossier -Recurse -Force
}
Copy-Item -LiteralPath (Join-Path $racine 'LICENCES-TIERS.txt') -Destination $dossier -Force

# BUILDINFO.txt : de quoi retrouver d'où vient ce zip. La stack peut ajouter les versions de ses outils dans .work\outils.txt.
# Sous PowerShell 5.1, une sortie d'erreur de git redirigée arrêterait le script : on la tolère le temps de l'appel.
$ErrorActionPreference = 'Continue'
$commit = "$(git -C $racine rev-parse HEAD 2>$null)".Trim()
$ErrorActionPreference = 'Stop'
$lignes = @(
    "$nom $version",
    "Commit : $commit",
    "Date   : $((Get-Date).ToUniversalTime().ToString('yyyy-MM-dd HH:mm')) UTC"
)
$outils = Join-Path $racine '.work\outils.txt'
if (Test-Path -LiteralPath $outils) { $lignes += '', 'Outils :'; $lignes += Get-Content -LiteralPath $outils -Encoding UTF8 }
[IO.File]::WriteAllLines((Join-Path $dossier 'BUILDINFO.txt'), [string[]]$lignes, (New-Object Text.UTF8Encoding($false)))

# fichiers.txt : la liste de ce que livre cette version, pour l'installation et les mises à jour.
# Seulement des fichiers à la racine du dossier : un sous-dossier ne se copierait pas.
if (@(Get-ChildItem -LiteralPath $dossier -Directory).Count -gt 0) {
    throw "Le dossier du zip contient un sous-dossier : l'installation par l'exe ne copie que des fichiers. Range tout à la racine de .work\sortie\."
}
$liste = @(Get-ChildItem -LiteralPath $dossier -File | ForEach-Object Name | Sort-Object)
[IO.File]::WriteAllLines((Join-Path $dossier 'fichiers.txt'), [string[]]$liste, (New-Object Text.UTF8Encoding($false)))

$zip = Join-Path $release "$nom-$version.zip"
Compress-Archive -LiteralPath $dossier -DestinationPath $zip
$empreinte = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$zip.sha256", "$empreinte  $nom-$version.zip`n", (New-Object Text.UTF8Encoding($false)))

Write-Host "Zip       : $zip"
Write-Host "Empreinte : $empreinte"
