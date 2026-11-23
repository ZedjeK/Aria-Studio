<#
.SYNOPSIS
Tient à jour et vérifie la partie générée de LICENCES-TIERS.txt : ce qui est compilé dans l'exe, et leurs licences.

.DESCRIPTION
La liste vient de l'outillage, jamais de la mémoire :
- crates Rust : cargo tree, dépendances normales de l'appli pour Windows, sans les dépendances de dev, de build ni les
  macros procédurales, qui ne finissent pas dans l'exe ; licence et dépôt lus dans leur Cargo.toml ;
- paquets npm : ceux dont le code ou les polices finissent dans l'interface livrée, lus dans les sourcemaps d'une
  construction faite dans un dossier temporaire (interface\dist n'est pas touché ; tools\licences-npm.mjs) ;
- textes de licence : les fichiers LICENSE, COPYING, NOTICE… de chaque paquet, sans doublon ; pour une crate qui n'en
  livre pas, celui de tools\licences-supplementaires\<crate>.txt, copié de son dépôt.

Tout ce qui précède la ligne de début de la partie générée s'écrit à la main et n'est pas touché.

Sans -Ecrire, échoue si la partie générée du fichier n'est plus celle que donnent les dépendances (une crate ajoutée,
retirée ou montée de version) : relancer avec -Ecrire, relire le diff, commiter. Échoue toujours si une licence n'est
pas admise dans l'exe (copyleft fort : GPL, LGPL, AGPL…) ou si un texte de licence manque.

    .\tools\licences-tierces.ps1           # vérifier (lancé par tools\tester.ps1)
    .\tools\licences-tierces.ps1 -Ecrire   # réécrire la partie générée de LICENCES-TIERS.txt
#>
[CmdletBinding()]
param([switch]$Ecrire)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$racine = Split-Path -Parent $PSScriptRoot
$fichier = Join-Path $racine 'LICENCES-TIERS.txt'
$supplementaires = Join-Path $PSScriptRoot 'licences-supplementaires'
$debut = '=== Partie générée par tools\licences-tierces.ps1 : ne pas la modifier à la main ==='
$cible = 'x86_64-pc-windows-msvc'

# Licences admises dans l'exe : permissives, ou copyleft faible limité au fichier (MPL-2.0, ses sources restent celles
# du paquet publié). Toute autre licence fait échouer le contrôle, qui se relit alors à la main.
$admises = @('MIT', 'MIT-0', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'Zlib', '0BSD', 'Unlicense',
    'CC0-1.0', 'Unicode-3.0', 'BSL-1.0', 'MPL-2.0', 'CDLA-Permissive-2.0', 'OFL-1.1')

# Une expression SPDX est admise si l'une de ses alternatives (OR) n'a que des licences admises. Avec des
# parenthèses, chaque licence citée doit l'être : on ne démêle pas le parenthésage, on reste prudent.
function Test-LicenceAdmise([string]$expression) {
    $e = ($expression -replace '/', ' OR ').Trim()
    if (-not $e) { return $false }
    if ($e -match '[()]') {
        $ids = @(($e -replace '[()]', ' ') -split '\s+(?:OR|AND|WITH)\s+|\s+' | Where-Object { $_ -and $_ -notin 'OR', 'AND' })
        return -not @($ids | Where-Object { $admises -notcontains $_ }).Count
    }
    foreach ($alternative in ($e -split '\s+OR\s+')) {
        $parts = @($alternative -split '\s+AND\s+' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
        if ($parts.Count -gt 0 -and -not @($parts | Where-Object { $admises -notcontains $_ }).Count) { return $true }
    }
    return $false
}

# Lance un programme natif et rend sa sortie ; s'arrête s'il échoue. Sous PowerShell 5.1, sa sortie d'erreur
# redirigée arrêterait le script : on l'écarte et on lit son code de retour.
function Invoke-Natif([string]$quoi, [scriptblock]$commande) {
    $ErrorActionPreference = 'Continue'
    $sortie = & $commande 2>$null
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($code -ne 0) { throw "$quoi a échoué (code $code)." }
    return $sortie
}

# Un texte de licence, propre : sans BOM, fins de ligne LF, sans espaces en fin de ligne ni lignes vides autour.
function Format-Texte([string]$texte) {
    $lignes = ($texte -replace "`r`n", "`n" -replace "`r", "`n").TrimStart([char]0xFEFF) -split "`n" |
        ForEach-Object { $_.TrimEnd() }
    return ($lignes -join "`n").Trim("`n")
}

$motifTexte = '^(licen[cs]e|copying|notice|unlicense|copyright)'
$problemes = New-Object System.Collections.Generic.List[string]
# Texte de licence -> les paquets qui le portent (« nom version : fichier »), dans l'ordre où ils sont trouvés, en
# comparant les textes au caractère près.
$textes = New-Object System.Collections.Specialized.OrderedDictionary ([StringComparer]::Ordinal)
function Add-Texte([string]$texte, [string]$porteur) {
    $propre = Format-Texte $texte
    if (-not $textes.Contains($propre)) { $textes[$propre] = New-Object System.Collections.Generic.List[string] }
    $textes[$propre].Add($porteur)
}

# --- Crates Rust compilées dans l'exe ---

Push-Location -LiteralPath $racine
try {
    $arbre = Invoke-Natif 'cargo tree' {
        cargo tree -p aria-appli --target $cible -e 'normal,no-proc-macro' --locked --prefix none --format '{p}|{l}|{r}'
    }
}
finally { Pop-Location }

$crates = [ordered]@{}
foreach ($ligne in $arbre) {
    $ligne = ($ligne -replace '\s*\(\*\)\s*$', '').Trim()
    $champs = $ligne -split '\|'
    if ($champs.Count -lt 3 -or $champs[0] -match '\(') { continue } # un paquet du dépôt (chemin local) : c'est Aria
    if ($champs[0] -notmatch '^(\S+) v(\S+)$') { throw "Ligne de cargo tree inattendue : « $ligne »." }
    $cle = "$($Matches[1]) $($Matches[2])"
    if (-not $crates.Contains($cle)) {
        $crates[$cle] = [pscustomobject]@{ Nom = $Matches[1]; Version = $Matches[2]; Licence = $champs[1].Trim(); Depot = $champs[2].Trim() }
    }
}
if ($crates.Count -eq 0) { throw "cargo tree n'a rendu aucune crate." }

# Les sources des crates, telles que cargo les a téléchargées.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
$registres = @(Get-ChildItem -LiteralPath (Join-Path $cargoHome 'registry\src') -Directory -ErrorAction SilentlyContinue)
# Tri ordinal : le même sur tous les postes, quelle que soit la langue de Windows.
$cles = [string[]]@($crates.Keys)
[Array]::Sort($cles, [StringComparer]::Ordinal)
$triees = @($cles | ForEach-Object { $crates[$_] })
foreach ($c in $triees) {
    if (-not (Test-LicenceAdmise $c.Licence)) { $problemes.Add("Licence non admise dans l'exe : $($c.Nom) $($c.Version) ($($c.Licence)).") }
    $dossier = $null
    foreach ($r in $registres) {
        $d = Join-Path $r.FullName "$($c.Nom)-$($c.Version)"
        if (Test-Path -LiteralPath $d) { $dossier = $d; break }
    }
    if (-not $dossier) { $problemes.Add("Sources introuvables pour $($c.Nom) $($c.Version) : lance d'abord cargo fetch."); continue }
    $noms = [string[]]@(Get-ChildItem -LiteralPath $dossier -File | Where-Object { $_.Name -match $motifTexte } | ForEach-Object { $_.Name })
    [Array]::Sort($noms, [StringComparer]::Ordinal)
    $trouves = @($noms | Where-Object { $_ } | ForEach-Object { Get-Item -LiteralPath (Join-Path $dossier $_) })
    if ($trouves.Count -eq 0) {
        $repli = Join-Path $supplementaires "$($c.Nom).txt"
        if (-not (Test-Path -LiteralPath $repli)) {
            $problemes.Add("Aucun texte de licence pour $($c.Nom) $($c.Version) : ajoute-le dans tools\licences-supplementaires\$($c.Nom).txt, copié de son dépôt.")
            continue
        }
        $brut = Get-Content -LiteralPath $repli -Raw -Encoding UTF8
        $texte = (($brut -replace "`r`n", "`n") -split "`n" | Where-Object { $_ -notmatch '^#' }) -join "`n"
        Add-Texte $texte "$($c.Nom) $($c.Version) : tools\licences-supplementaires\$($c.Nom).txt"
        continue
    }
    foreach ($f in $trouves) { Add-Texte (Get-Content -LiteralPath $f.FullName -Raw -Encoding UTF8) "$($c.Nom) $($c.Version) : $($f.Name)" }
}

# --- Paquets npm compilés dans l'interface ---

$interface = Join-Path $racine 'interface'
$vite = Join-Path $interface 'node_modules\vite\bin\vite.js'
if (-not (Test-Path -LiteralPath $vite)) { throw "Vite est absent : lance d'abord npm ci dans interface\." }
$construction = Join-Path ([IO.Path]::GetTempPath()) ("aria-licences-" + [guid]::NewGuid().ToString('N'))
# Le relevé npm s'écrit dans un fichier lu en UTF-8 : lu sur la console, Windows PowerShell 5.1 abîmerait ses accents.
$releve = "$construction.txt"
Push-Location -LiteralPath $interface
try {
    $null = Invoke-Natif 'La construction de l''interface (sourcemaps)' {
        node $vite build --sourcemap --outDir $construction --emptyOutDir --logLevel error
    }
    $null = Invoke-Natif 'tools\licences-npm.mjs' {
        node (Join-Path $PSScriptRoot 'licences-npm.mjs') $construction $interface $releve
    }
    $npm = ((Get-Content -LiteralPath $releve -Raw -Encoding UTF8) -replace "`r`n", "`n").TrimEnd("`n") -split "`n"
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $construction -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $releve -Force -ErrorAction SilentlyContinue
}

$paquets = New-Object System.Collections.Generic.List[object]
$courant = $null
$dansTexte = $false
$fichierTexte = ''
$lignesTexte = New-Object System.Collections.Generic.List[string]
foreach ($ligne in $npm) {
    if ($dansTexte) {
        if ($ligne -eq '@@fin') {
            Add-Texte ($lignesTexte -join "`n") "$($courant.Nom) $($courant.Version) : $fichierTexte"
            $dansTexte = $false
        }
        else { $lignesTexte.Add($ligne) }
        continue
    }
    if ($ligne -match '^@@texte (.+)$') {
        $fichierTexte = $Matches[1]
        $lignesTexte.Clear()
        $dansTexte = $true
        continue
    }
    $champs = $ligne -split '\|'
    if ($champs.Count -lt 4) { continue }
    $courant = [pscustomobject]@{ Nom = $champs[0]; Version = $champs[1]; Licence = $champs[2]; Depot = $champs[3] }
    $paquets.Add($courant)
    if (-not (Test-LicenceAdmise $courant.Licence)) { $problemes.Add("Licence non admise dans l'exe : $($courant.Nom) $($courant.Version) ($($courant.Licence)).") }
}
if ($paquets.Count -eq 0) { throw "Aucun paquet npm relevé dans l'interface : la construction a-t-elle échoué ?" }

# --- La partie générée ---

$toolchain = Get-Content -LiteralPath (Join-Path $racine 'rust-toolchain.toml') -Raw -Encoding UTF8
if ($toolchain -notmatch 'channel\s*=\s*"([^"]+)"') { throw "Version de Rust introuvable dans rust-toolchain.toml." }
$rust = $Matches[1]

$sortie = New-Object System.Collections.Generic.List[string]
$sortie.Add($debut)
$sortie.Add('')
$sortie.Add("Bibliothèque standard de Rust $rust (rust-toolchain.toml), compilée dans l'exe")
$sortie.Add('  Source  : https://github.com/rust-lang/rust')
$sortie.Add('  Licence : MIT ou Apache-2.0 ; ses notices complètes sont dans COPYRIGHT-library.html, livré avec Rust')
$sortie.Add("            (share\doc\rust) et sur https://github.com/rust-lang/rust/blob/$rust/COPYRIGHT-library.html")
$sortie.Add('')
$sortie.Add("Crates Rust compilées dans l'exe ($($triees.Count)), d'après Cargo.lock. Vérification : cargo contrôle")
$sortie.Add("l'empreinte SHA-256 de chacune, inscrite dans Cargo.lock, avant de la compiler (--locked).")
$sortie.Add('')
foreach ($c in $triees) { $sortie.Add("  $($c.Nom) $($c.Version) : $($c.Licence) : $($c.Depot)") }
$sortie.Add('')
$sortie.Add("Paquets npm compilés dans l'interface ($($paquets.Count)), d'après interface\package-lock.json. Vérification :")
$sortie.Add("npm ci contrôle l'empreinte (integrity) de chacun, inscrite dans package-lock.json.")
$sortie.Add('')
foreach ($p in $paquets) { $sortie.Add("  $($p.Nom) $($p.Version) : $($p.Licence) : $($p.Depot)") }
$sortie.Add('')
$sortie.Add("Textes des licences ($($textes.Count), sans doublon), tels que les paquets les livrent")
$sortie.Add('')
foreach ($t in $textes.Keys) {
    $sortie.Add('-' * 78)
    $porteurs = [string[]]@($textes[$t])
    [Array]::Sort($porteurs, [StringComparer]::Ordinal)
    foreach ($porteur in $porteurs) { $sortie.Add("Pour $porteur") }
    $sortie.Add('')
    $sortie.Add($t)
    $sortie.Add('')
}
$genere = ($sortie -join "`n").TrimEnd() + "`n"

$actuel = (Get-Content -LiteralPath $fichier -Raw -Encoding UTF8) -replace "`r`n", "`n"
$position = $actuel.IndexOf($debut)
if ($position -lt 0) { throw "La ligne de début de la partie générée manque dans LICENCES-TIERS.txt : « $debut »." }
$manuel = $actuel.Substring(0, $position)

if ($problemes.Count -gt 0) {
    foreach ($p in $problemes) { Write-Host "  - $p" -ForegroundColor Red }
    Write-Host "Licences tierces : $($problemes.Count) problème(s)." -ForegroundColor Red
    exit 1
}

if ($Ecrire) {
    [IO.File]::WriteAllText($fichier, $manuel + $genere, (New-Object Text.UTF8Encoding($false)))
    Write-Host "LICENCES-TIERS.txt réécrit : $($triees.Count) crates, $($paquets.Count) paquets npm, $($textes.Count) textes de licence."
    exit 0
}
if ($actuel.Substring($position) -ne $genere) {
    Write-Host "LICENCES-TIERS.txt n'est plus à jour avec les dépendances de l'exe : lance .\tools\licences-tierces.ps1 -Ecrire, relis le diff et commite-le." -ForegroundColor Red
    exit 1
}
Write-Host "Licences tierces à jour : $($triees.Count) crates, $($paquets.Count) paquets npm, toutes admises."
exit 0
