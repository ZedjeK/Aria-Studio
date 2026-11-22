<#
.SYNOPSIS
Compile Aria Studio en un exe portable, dans .work\sortie\.

.DESCRIPTION
Compile l'interface (Svelte, dans interface\dist), puis l'exe Rust qui l'embarque : Aria-Studio.exe, un seul
fichier, rien à extraire au lancement, rien d'autre à installer. L'exe s'appuie sur WebView2, déjà présent sous
Windows 11. Ne demande pas les droits administrateur et ne lance rien. Les versions des outils vont dans
.work\outils.txt, que tools\empaqueter.ps1 recopie dans BUILDINFO.txt.

    .\build.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$racine = $PSScriptRoot
foreach ($outil in 'cargo', 'rustc', 'npm', 'node') {
    if (-not (Get-Command $outil -ErrorAction SilentlyContinue)) {
        throw "$outil est introuvable : installe Rust (rustup) et Node.js."
    }
}

$work = Join-Path $racine '.work'
$sortie = Join-Path $work 'sortie'
# Une jonction à la place de .work ou de .work\sortie mènerait la suppression qui suit hors du dépôt.
foreach ($d in $work, $sortie) {
    if ((Test-Path -LiteralPath $d) -and ((Get-Item -LiteralPath $d -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "$d est un lien ou une jonction : rien n'est supprimé. Retire-le, puis relance."
    }
}
if (Test-Path -LiteralPath $sortie) { Remove-Item -LiteralPath $sortie -Recurse -Force }
New-Item -ItemType Directory -Path $sortie -Force | Out-Null

# Lance un programme natif et s'arrête s'il échoue. Sous PowerShell 5.1, sa sortie d'erreur redirigée (cargo y écrit
# sa progression) arrêterait le script : on lit son code de retour à la place.
function Lancer([string]$quoi, [scriptblock]$commande) {
    $ErrorActionPreference = 'Continue'
    & $commande
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($code -ne 0) { throw "$quoi a échoué (code $code)." }
}

# Lit la version d'un outil sans jamais faire échouer le build.
function Version-De([scriptblock]$commande) {
    $ErrorActionPreference = 'Continue'
    $texte = "$(& $commande 2>$null)".Trim()
    $ErrorActionPreference = 'Stop'
    if ($texte) { return $texte }
    return 'inconnue'
}

Push-Location -LiteralPath (Join-Path $racine 'interface')
try {
    Lancer "L'installation des paquets de l'interface" { npm ci --no-audit --no-fund }
    Lancer "La compilation de l'interface" { npm run build }
}
finally { Pop-Location }

Push-Location -LiteralPath $racine
try {
    # custom-protocol : l'interface est embarquée dans l'exe au lieu d'être chargée depuis le serveur de développement.
    Lancer "La compilation d'Aria Studio" { cargo build --release --locked -p aria-appli --features custom-protocol }
}
finally { Pop-Location }

$cible = Join-Path $racine 'target'
if ($env:CARGO_TARGET_DIR) { $cible = $env:CARGO_TARGET_DIR }
$compile = Join-Path $cible 'release\aria-appli.exe'
if (-not (Test-Path -LiteralPath $compile)) { throw "aria-appli.exe est absent de $cible\release après la compilation." }
$exe = Join-Path $sortie 'Aria-Studio.exe'
Copy-Item -LiteralPath $compile -Destination $exe

$outils = @(
    "Rust : $(Version-De { rustc --version })",
    "Cargo : $(Version-De { cargo --version })",
    "Node.js : $(Version-De { node --version })",
    "npm : $(Version-De { npm --version })"
)
[IO.File]::WriteAllLines((Join-Path $work 'outils.txt'), [string[]]$outils, (New-Object Text.UTF8Encoding($false)))
Write-Host "Prêt : $exe ($([math]::Round((Get-Item -LiteralPath $exe).Length / 1MB, 1)) Mo)"
