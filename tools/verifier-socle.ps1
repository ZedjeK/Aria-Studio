<#
.SYNOPSIS
Compare le socle commun d'une appli Aria à celui du dépôt modèle, et dit quels fichiers sont en retard.

.DESCRIPTION
Le socle, ce sont les fichiers que toute appli garde identiques à ceux du dépôt modèle : leur liste est dans
tools\socle.txt. Quand le modèle avance (une protection de plus, un contrôle de release de plus), rien ne le dit aux
applis déjà créées : ce script le dit.

Il lit la liste et les fichiers du modèle sur GitHub, à la pointe de sa branche par défaut : un clone local du modèle
peut lui-même être en retard. Il ne modifie rien, ni dans le dossier vérifié ni sur GitHub.

    .\tools\verifier-socle.ps1                          compare ce dépôt au modèle, lu sur GitHub
    .\tools\verifier-socle.ps1 -Modele <dossier>        compare à un dossier (hors ligne, ou une branche du modèle)
    .\tools\verifier-socle.ps1 -Dossier ..\Aria-Atlas   vérifie un autre dossier que celui de ce script
    .\tools\verifier-socle.ps1 -Ref v1.4.0             compare à une branche ou à un tag du modèle sur GitHub

Les fins de ligne ne comptent pas : un fichier est « identique » quand son texte l'est.
Code de retour : 0 si tout est identique, 1 s'il y a un écart.

La lecture sur GitHub a besoin de gh. Le dépôt modèle est public : aucun droit particulier n'est demandé.
#>
[CmdletBinding()]
param(
    # Le dossier à vérifier. Par défaut : le dépôt de ce script.
    [string]$Dossier,
    # Le modèle : un dossier, ou un dépôt GitHub « proprietaire/nom ». Par défaut : le dépôt modèle des applis Aria.
    [string]$Modele,
    # Sur GitHub seulement : la branche ou le tag du modèle à lire. Par défaut : sa branche par défaut.
    [string]$Ref
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Le nom du dépôt modèle est écrit en deux morceaux : init.ps1 le remplace par celui de l'appli dans tous les fichiers,
# et ce script, qui fait partie du socle, doit rester identique dans l'appli (il se comparerait sinon à elle-même).
if (-not $Modele) { $Modele = 'ZedjeK/Aria-' + 'Modele' }

$racine = Split-Path -Parent $PSScriptRoot
if ($Dossier) {
    if (-not (Test-Path -LiteralPath $Dossier -PathType Container)) { throw "Dossier introuvable : $Dossier" }
    $racine = (Resolve-Path -LiteralPath $Dossier).Path
}

$modeleLocal = Test-Path -LiteralPath $Modele -PathType Container
if ($modeleLocal) { $Modele = (Resolve-Path -LiteralPath $Modele).Path }
elseif ($Modele -notmatch '\A[\w.-]+/[\w.-]+\z') { throw "Modèle introuvable : $Modele. Donne un dossier, ou un dépôt GitHub proprietaire/nom." }
elseif (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "gh est introuvable : il lit le modèle sur GitHub. Sans lui, compare à un dossier du modèle : -Modele <dossier>"
}

# Un chemin du socle : relatif, sans remontée, avec des barres obliques. La liste vient du modèle, mais un chemin
# mal formé ne doit jamais faire lire un fichier hors du dossier vérifié.
function Chemin-Sur([string]$chemin) {
    return $chemin -match '\A[\w.-]+(/[\w.-]+)*\z' -and $chemin -notmatch '(\A|/)\.\.?(/|\z)'
}

# Les octets d'un fichier du modèle, ou $null s'il n'y est pas.
function Lire-Modele([string]$chemin) {
    if ($modeleLocal) {
        $fichier = Join-Path $Modele ($chemin -replace '/', '\')
        if (Test-Path -LiteralPath $fichier -PathType Leaf) { return , [IO.File]::ReadAllBytes($fichier) }
        return $null
    }
    # Le contenu arrive en base64 : aucun accent ne dépend de l'encodage de la console, sous PowerShell 5.1 comme 7.
    # Sous 5.1, une sortie d'erreur de gh redirigée arrêterait le script : on la lit comme une réponse.
    $ErrorActionPreference = 'Continue'
    $adresse = "repos/$Modele/contents/$chemin"
    if ($Ref) { $adresse += "?ref=$([uri]::EscapeDataString($Ref))" }
    $tout = @(gh api $adresse --jq .content 2>&1)
    $code = $LASTEXITCODE
    $erreurs = @($tout | Where-Object { $_ -is [Management.Automation.ErrorRecord] } | ForEach-Object { "$_" })
    $sortie = @($tout | Where-Object { $_ -isnot [Management.Automation.ErrorRecord] })
    if ($code -ne 0) {
        if (($erreurs -join ' ') -match 'HTTP 404') { return $null }
        throw "Lecture impossible : $chemin dans $Modele ($($erreurs -join ' '))"
    }
    return , [Convert]::FromBase64String(($sortie -join ''))
}

# Le texte d'un fichier, fins de ligne ramenées à LF. Le BOM, s'il y en a un, fait partie du texte : un script
# PowerShell qui l'a perdu n'est plus le même fichier.
function Texte([byte[]]$octets) {
    return (New-Object Text.UTF8Encoding($false)).GetString($octets).Replace("`r`n", "`n")
}

$origine = $Modele
if (-not $modeleLocal) {
    $origine = "$Modele (GitHub)"
    if ($Ref) { $origine = "$Modele (GitHub, $Ref)" }
}
elseif ($Ref) { throw "-Ref ne vaut que pour un modèle lu sur GitHub : un dossier est lu tel qu'il est." }
Write-Host "Socle de $racine, comparé à $origine"

$octetsListe = Lire-Modele 'tools/socle.txt'
if ($null -eq $octetsListe) { throw "Le modèle n'a pas de tools/socle.txt : $origine" }
$liste = @((Texte $octetsListe).TrimStart([char]0xFEFF) -split "`n" | ForEach-Object { $_.Trim() } |
    Where-Object { $_ -and -not $_.StartsWith('#') })
if ($liste.Count -eq 0) { throw "tools/socle.txt du modèle ne liste aucun fichier." }

$ecarts = 0
foreach ($chemin in $liste) {
    if (-not (Chemin-Sur $chemin)) { throw "Chemin refusé dans tools/socle.txt : $chemin" }
    $attendu = Lire-Modele $chemin
    if ($null -eq $attendu) { throw "Le modèle liste $chemin dans son socle, mais ne l'a pas." }
    $ici = Join-Path $racine ($chemin -replace '/', '\')
    if (-not (Test-Path -LiteralPath $ici -PathType Leaf)) {
        Write-Host "  ABSENT     $chemin" -ForegroundColor Yellow
        $ecarts++
        continue
    }
    $voulu = Texte $attendu
    $present = Texte ([IO.File]::ReadAllBytes($ici))
    if ($present -ceq $voulu) {
        Write-Host "  identique  $chemin"
        continue
    }
    $lignesVoulues = New-Object 'Collections.Generic.HashSet[string]' (, [string[]]($voulu -split "`n"))
    $lignesPresentes = New-Object 'Collections.Generic.HashSet[string]' (, [string[]]($present -split "`n"))
    $manquent = @($lignesVoulues | Where-Object { -not $lignesPresentes.Contains($_) }).Count
    $enTrop = @($lignesPresentes | Where-Object { -not $lignesVoulues.Contains($_) }).Count
    Write-Host "  DIFFÉRENT  $chemin ($manquent ligne(s) du modèle manquent, $enTrop en trop)" -ForegroundColor Yellow
    $ecarts++
}

Write-Host ""
if ($ecarts -eq 0) {
    Write-Host "Le socle est à jour : $($liste.Count) fichier(s) identiques à ceux du modèle." -ForegroundColor Cyan
    exit 0
}
Write-Host "$ecarts écart(s) sur $($liste.Count) fichier(s). Pour rattraper : reprends ces fichiers du modèle tels quels." -ForegroundColor Yellow
exit 1
