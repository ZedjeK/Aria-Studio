# Affiche la version sur sa dernière ligne, sans le « v ». Elle vit dans [workspace.package] de Cargo.toml, et nulle
# part ailleurs.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$cargo = Join-Path (Split-Path -Parent $PSScriptRoot) 'Cargo.toml'
$dansLaSection = $false
$version = $null
foreach ($ligne in Get-Content -LiteralPath $cargo -Encoding UTF8) {
    if ($ligne -match '^\s*\[([^\]]+)\]\s*$') {
        $dansLaSection = ($Matches[1].Trim() -eq 'workspace.package')
        continue
    }
    if ($dansLaSection -and $ligne -match '^\s*version\s*=\s*"([^"]*)"') {
        $version = $Matches[1]
        break
    }
}
if (-not $version -or $version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version introuvable ou mal formée dans [workspace.package] de Cargo.toml (attendu : X.Y.Z)."
}
$version
