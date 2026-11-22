<#
.SYNOPSIS
    Fabrique l'icône Windows de l'appli, docs\logo.png et le logo du menu à partir de docs\logo.svg.

.DESCRIPTION
    docs\logo.svg est la seule source du logo. Ce script le rend avec Microsoft Edge sans fenêtre (le halo du A
    demande un navigateur) à chaque taille de l'icône, puis écrit :
      - appli\icons\icon.ico : 16, 24, 32, 48, 64, 128 et 256 pixels, en PNG (exe, barre des tâches, raccourci,
        icône de notification) ;
      - docs\logo.png : 512 pixels (README) ;
      - interface\src\lib\logo.svg : la copie que lit le menu latéral.
    Les rendus passent par .work\icone\, vidé à chaque lancement. Rien d'autre n'est modifié.

.PARAMETER Navigateur
    Chemin d'un navigateur Chromium à utiliser à la place d'Edge.

.EXAMPLE
    .\tools\icone.ps1
#>
[CmdletBinding()]
param(
    [string]$Navigateur
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$racine = Split-Path -Parent $PSScriptRoot
$source = Join-Path $racine 'docs\logo.svg'
$icone = Join-Path $racine 'appli\icons\icon.ico'
$png = Join-Path $racine 'docs\logo.png'
$copieMenu = Join-Path $racine 'interface\src\lib\logo.svg'
$travail = Join-Path $racine '.work\icone'
$tailles = @(16, 24, 32, 48, 64, 128, 256)

if (-not (Test-Path -LiteralPath $source)) { throw "docs\logo.svg est introuvable : c'est la source du logo." }

if (-not $Navigateur) {
    $candidats = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft\Edge\Application\msedge.exe'),
        (Join-Path $env:ProgramFiles 'Microsoft\Edge\Application\msedge.exe')
    )
    $Navigateur = $candidats | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
    if (-not $Navigateur) { throw "Microsoft Edge est introuvable : indique un navigateur Chromium avec -Navigateur." }
}
if (-not (Test-Path -LiteralPath $Navigateur)) { throw "Navigateur introuvable : $Navigateur" }

if (Test-Path -LiteralPath $travail) { Remove-Item -LiteralPath $travail -Recurse -Force }
New-Item -ItemType Directory -Path $travail -Force | Out-Null
Copy-Item -LiteralPath $source -Destination (Join-Path $travail 'logo.svg')

# Largeur et hauteur d'un PNG, lues dans son en-tête (IHDR, en gros-boutiste).
function Get-TaillePng([byte[]]$octets) {
    if ($octets.Length -lt 24 -or $octets[1] -ne 0x50 -or $octets[2] -ne 0x4E -or $octets[3] -ne 0x47) {
        throw 'le rendu n''est pas une image PNG'
    }
    $fin = [Text.Encoding]::ASCII.GetString($octets, $octets.Length - 8, 4)
    if ($fin -ne 'IEND') { throw 'le rendu est une image PNG incomplète' }
    $lire = { param($i) ([int]$octets[$i] -shl 24) -bor ([int]$octets[$i + 1] -shl 16) -bor ([int]$octets[$i + 2] -shl 8) -bor [int]$octets[$i + 3] }
    return @((& $lire 16), (& $lire 20))
}

# Arrête le processus lancé et, sous Windows, les processus d'Edge restés sur ce profil.
function Stop-Edge($processus, [string]$profil) {
    if (-not $processus.HasExited) { Stop-Process -Id $processus.Id -Force -ErrorAction SilentlyContinue }
    if ($env:OS -eq 'Windows_NT') {
        $motif = '*' + $profil + '*'
        Get-CimInstance Win32_Process -Filter "Name = 'msedge.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -like $motif } |
            ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    }
}

# Rend le logo en carré de $taille pixels, fond transparent, et renvoie les octets du PNG.
function Get-Rendu([int]$taille) {
    $page = Join-Path $travail "logo-$taille.html"
    $image = Join-Path $travail "logo-$taille.png"
    $html = "<!DOCTYPE html><html><head><meta charset=`"utf-8`"><style>html,body{margin:0;background:transparent;overflow:hidden}" +
        "img{display:block;width:${taille}px;height:${taille}px}</style></head><body><img src=`"logo.svg`" alt=`"`"></body></html>"
    [IO.File]::WriteAllText($page, $html, (New-Object Text.UTF8Encoding $false))
    $adresse = [Uri]::new($page, [UriKind]::Absolute).AbsoluteUri
    # On attend l'image, pas la fin d'Edge : sous Windows, le processus lancé ne rend pas toujours la main une fois
    # la capture écrite, et il lui arrive de caler sans rien écrire. Dès que le PNG est complet (taille stable), Edge
    # et ses processus annexes sont arrêtés ; sinon, après 20 s, il est arrêté et relancé, 3 essais au plus. Un
    # profil par essai : un Edge resté ouvert sur le même profil recevrait la commande à la place du nouveau. Un
    # argument qui contient une espace est mis entre guillemets ; les messages d'Edge vont dans un journal par essai.
    $essais = 3
    $journaux = @()
    $prete = $false
    for ($essai = 1; $essai -le $essais -and -not $prete; $essai++) {
        $profil = Join-Path $travail "profil-$taille-$essai"
        $journal = Join-Path $travail "navigateur-$taille-$essai.txt"
        $journaux += $journal
        if (Test-Path -LiteralPath $image) { Remove-Item -LiteralPath $image -Force }
        $arguments = @(
            '--headless=new', '--disable-gpu', '--hide-scrollbars', '--no-first-run', '--no-default-browser-check',
            '--force-device-scale-factor=1', '--default-background-color=00000000',
            "--user-data-dir=$profil", '--disable-crash-reporter',
            "--window-size=$taille,$taille", "--screenshot=$image", $adresse
        )
        $ligne = ($arguments | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
        $processus = Start-Process -FilePath $Navigateur -ArgumentList $ligne -PassThru -NoNewWindow `
            -RedirectStandardError $journal -RedirectStandardOutput "$journal.sortie"
        $limite = (Get-Date).AddSeconds(20)
        $taillePrecedente = -1
        while ((Get-Date) -lt $limite) {
            Start-Sleep -Milliseconds 250
            if (Test-Path -LiteralPath $image) {
                $actuelle = (Get-Item -LiteralPath $image).Length
                if ($actuelle -gt 0 -and $actuelle -eq $taillePrecedente) { $prete = $true; break }
                $taillePrecedente = $actuelle
            }
            elseif ($processus.HasExited) { break }
        }
        Stop-Edge $processus $profil
        if (-not $prete -and $essai -lt $essais) {
            Write-Host "  Edge n'a rien rendu en $taille px (essai $essai sur $essais) : nouvel essai."
        }
    }
    if (-not $prete) {
        foreach ($j in $journaux) {
            Write-Host "--- $j"
            if (Test-Path -LiteralPath $j) { Get-Content -LiteralPath $j -Tail 30 | ForEach-Object { Write-Host "    $_" } }
        }
        throw "Le rendu en $taille px n'a rien donné après $essais essais de 20 s ; les journaux d'Edge sont ci-dessus."
    }
    $octets = [IO.File]::ReadAllBytes($image)
    $lu = Get-TaillePng $octets
    if ($lu[0] -ne $taille -or $lu[1] -ne $taille) {
        throw "Le rendu en $taille px fait $($lu[0]) x $($lu[1]) : le navigateur n'a pas respecté la taille demandée."
    }
    return , $octets
}

$images = @()
foreach ($taille in $tailles) {
    Write-Host "Rendu $taille px"
    $images += , (Get-Rendu $taille)
}
$grand = Get-Rendu 512

# Le fichier .ico : un en-tête, une entrée par taille (256 s'écrit 0), puis les PNG à la suite.
$flux = New-Object IO.MemoryStream
$ecrivain = New-Object IO.BinaryWriter $flux
$ecrivain.Write([uint16]0)
$ecrivain.Write([uint16]1)
$ecrivain.Write([uint16]$tailles.Count)
$position = 6 + 16 * $tailles.Count
for ($i = 0; $i -lt $tailles.Count; $i++) {
    $cote = if ($tailles[$i] -ge 256) { 0 } else { $tailles[$i] }
    $ecrivain.Write([byte]$cote)
    $ecrivain.Write([byte]$cote)
    $ecrivain.Write([byte]0)
    $ecrivain.Write([byte]0)
    $ecrivain.Write([uint16]1)
    $ecrivain.Write([uint16]32)
    $ecrivain.Write([uint32]$images[$i].Length)
    $ecrivain.Write([uint32]$position)
    $position += $images[$i].Length
}
foreach ($octets in $images) { $ecrivain.Write($octets) }
$ecrivain.Flush()

New-Item -ItemType Directory -Path (Split-Path -Parent $icone) -Force | Out-Null
[IO.File]::WriteAllBytes($icone, $flux.ToArray())
[IO.File]::WriteAllBytes($png, $grand)
Copy-Item -LiteralPath $source -Destination $copieMenu -Force

# Le dossier de travail (gardé si un processus le tient encore : .work\ n'est pas suivi par git).
Remove-Item -LiteralPath $travail -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "Écrits : appli\icons\icon.ico ($($tailles -join ', ') px), docs\logo.png (512 px), interface\src\lib\logo.svg."
