# Lance Aria - Studio : prépare le .venv si besoin, puis démarre le serveur.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$python = Join-Path $PSScriptRoot '.venv\Scripts\python.exe'
if (-not (Test-Path $python)) {
    Write-Host 'Création du .venv...'
    python -m venv .venv
    & $python -m pip install --quiet --upgrade pip
}

& $python -m pip install --quiet -r requirements.txt
& $python -m app.main
