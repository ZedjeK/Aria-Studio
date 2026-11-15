<#
.SYNOPSIS
Vérifie qu'une version est prête à sortir : notes de version complètes, fiche projet et licences sans trou.

.DESCRIPTION
Lancé par release.yml avant le build. Commun à toutes les stacks.

    .\tools\verifier-release.ps1 -Tag v1.2.3      liste tous les trous, puis échoue s'il y en a
    .\tools\verifier-release.ps1 -EssaiABlanc     liste les trous sans échouer ; le tag vient de tools\version.ps1

Contrôles :
  - docs\releases\<tag>.md existe ;
  - il ne garde ni {VERSION}, ni {REPO_URL}, ni marqueur du modèle, ni commentaire HTML du modèle ;
  - il dit de décompresser le zip avant de lancer l'exe ;
  - LICENCES-TIERS.txt ne garde ni « à renseigner » ni « à compléter » ;
  - aucune ligne du tableau de docs\FICHE.md ne garde « à renseigner ».
#>
[CmdletBinding()]
param(
    [string]$Tag,
    [switch]$EssaiABlanc
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$racine = Split-Path -Parent $PSScriptRoot

if (-not $Tag) {
    $version = "$(& (Join-Path $PSScriptRoot 'version.ps1') | Select-Object -Last 1)".Trim()
    $Tag = "v$version"
}
if ($Tag -notmatch '\Av\d+\.\d+\.\d+\z') { throw "Tag inattendu : « $Tag ». Attendu : vX.Y.Z." }

$problemes = New-Object Collections.Generic.List[string]

function Lire([string]$relatif) {
    $chemin = Join-Path $racine $relatif
    if (-not (Test-Path -LiteralPath $chemin -PathType Leaf)) { return $null }
    return [IO.File]::ReadAllText($chemin, [Text.Encoding]::UTF8)
}

# Notes de version
$relatifNotes = "docs\releases\$Tag.md"
$notes = Lire $relatifNotes
if ($null -eq $notes) {
    $problemes.Add("$relatifNotes n'existe pas : pars de .github\release-notes.md.")
}
else {
    if ($notes -match '\{VERSION\}|\{REPO_URL\}') { $problemes.Add("$relatifNotes garde {VERSION} ou {REPO_URL}.") }
    # Motif écrit « <<STACK\: » : écrit en clair, il serait pris pour un marqueur oublié dans ce fichier même.
    if ($notes -match '\{\{|<<STACK\:') { $problemes.Add("$relatifNotes garde un marqueur du modèle.") }
    if ($notes.Contains('<!--')) { $problemes.Add("$relatifNotes garde un commentaire HTML : celui du modèle est à supprimer.") }
    if ($notes -notmatch "(?i)d[ée]compresse d'abord") {
        $problemes.Add("$relatifNotes ne dit pas « décompresse d'abord le zip » : c'est obligatoire dans chaque version.")
    }
}

# Licences des composants embarqués
$licences = Lire 'LICENCES-TIERS.txt'
if ($null -eq $licences) { $problemes.Add("LICENCES-TIERS.txt n'existe pas.") }
elseif ($licences -match '(?i)à renseigner|à compléter') {
    $problemes.Add("LICENCES-TIERS.txt garde « à renseigner » ou « à compléter ».")
}

# Fiche projet : seules les lignes du tableau comptent, la phrase sous le tableau cite « à renseigner » exprès.
$fiche = Lire 'docs\FICHE.md'
if ($null -eq $fiche) { $problemes.Add("docs\FICHE.md n'existe pas.") }
else {
    $trous = @($fiche -split "`r?`n" | Where-Object { $_ -match '^\|' -and $_ -match '(?i)à renseigner' } |
        ForEach-Object { ($_ -split '\|')[1].Trim() })
    if ($trous.Count -gt 0) { $problemes.Add("docs\FICHE.md garde « à renseigner » : $($trous -join ', ').") }
}

if ($problemes.Count -eq 0) {
    Write-Host "Version $Tag prête : notes, fiche projet et licences complètes."
    exit 0
}
foreach ($p in $problemes) { Write-Host "  - $p" }
if ($EssaiABlanc) {
    Write-Host "Essai à blanc : $($problemes.Count) point(s) à régler avant de pousser le tag $Tag."
    exit 0
}
Write-Host "Version $Tag pas prête : $($problemes.Count) point(s) à régler. Aucune release." -ForegroundColor Red
exit 1
