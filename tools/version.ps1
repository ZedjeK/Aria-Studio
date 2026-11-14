# Affiche la version d'Aria Studio : celle de app/__init__.py, l'unique endroit où elle vit.
# Le tag d'une release (vX.Y.Z) doit lui correspondre.
$ErrorActionPreference = 'Stop'

$fichier = Join-Path (Split-Path $PSScriptRoot -Parent) 'app\__init__.py'
$ligne = Select-String -Path $fichier -Pattern '^VERSION\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $ligne) {
    Write-Error "VERSION est introuvable dans $fichier"
    exit 1
}
$ligne.Matches[0].Groups[1].Value
