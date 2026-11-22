<#
.SYNOPSIS
Lance les tests d'Aria Studio : interface (svelte-check, vitest), puis Rust (rustfmt, clippy, cargo test).

.DESCRIPTION
L'interface est compilée avant le Rust : l'exe l'embarque, et clippy compile l'appli telle qu'elle sera livrée.
Code de retour non nul au premier contrôle qui échoue.

    .\tools\tester.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$racine = Split-Path -Parent $PSScriptRoot
foreach ($outil in 'cargo', 'npm') {
    if (-not (Get-Command $outil -ErrorAction SilentlyContinue)) {
        throw "$outil est introuvable : installe Rust (rustup) et Node.js."
    }
}

# Lance un contrôle ; au premier échec, le dit et s'arrête avec un code non nul. Sous PowerShell 5.1, la sortie
# d'erreur redirigée d'un programme natif arrêterait le script : on lit son code de retour à la place.
function Controler([string]$quoi, [scriptblock]$commande) {
    Write-Host "== $quoi"
    $ErrorActionPreference = 'Continue'
    & $commande
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($code -ne 0) {
        Write-Host "Échec : $quoi (code $code)." -ForegroundColor Red
        exit 1
    }
}

Push-Location -LiteralPath (Join-Path $racine 'interface')
try {
    Controler 'Paquets de l''interface' { npm ci --no-audit --no-fund }
    Controler 'Types et composants (svelte-check)' { npm run check }
    Controler 'Tests de l''interface (vitest)' { npm test }
    Controler 'Compilation de l''interface' { npm run build }
}
finally { Pop-Location }

Push-Location -LiteralPath $racine
try {
    Controler 'Mise en forme du Rust (rustfmt)' { cargo fmt --all --check }
    Controler 'Contrôles du Rust (clippy)' { cargo clippy --workspace --all-targets --all-features --locked -- -D warnings }
    Controler 'Tests du Rust' { cargo test --workspace --all-features --locked }
}
finally { Pop-Location }

Write-Host "Tous les tests passent." -ForegroundColor Cyan
exit 0
