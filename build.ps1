# Construit la version portable d'Aria Studio et son zip, dans .work\sortie\ :
#   Aria-Studio-X.Y.Z.zip et Aria-Studio-X.Y.Z.zip.sha256 (« empreinte  nom »).
#
# C'est une enveloppe : le travail est fait par tools/build_portable.py (--sans-installer : rien n'est posé dans
# %USERPROFILE%\.Aria-Studio) puis tools/zip_release.py, les mêmes étapes que .github/workflows/release.yml.
# Le build télécharge CPython la première fois (vérifié par SHA-256, gardé dans .build_cache) et compile le lanceur
# avec le csc.exe de Windows. Les dépendances embarquées sont celles du .venv.
#
#   .\build.ps1
#   .\build.ps1 -Python C:\chemin\python.exe   un autre interpréteur que .venv
param(
    [string]$Python
)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

if (-not $Python) { $Python = Join-Path $PSScriptRoot '.venv\Scripts\python.exe' }
if (-not (Test-Path $Python)) {
    Write-Error "Python est introuvable : $Python. Lance .\run.ps1 une fois pour créer le .venv, ou donne -Python."
    exit 1
}
$sortie = Join-Path $PSScriptRoot '.work\sortie'

# Sous Windows PowerShell 5.1, la sortie d'erreur d'un programme natif arrête le script en mode 'Stop' :
# on laisse passer le temps de chaque appel, puis on lit le code de sortie.
$ErrorActionPreference = 'Continue'
& $Python tools\build_portable.py --sans-installer
if ($LASTEXITCODE -ne 0) { Write-Error "La construction a échoué (code $LASTEXITCODE)."; exit $LASTEXITCODE }
& $Python tools\zip_release.py --sortie $sortie
if ($LASTEXITCODE -ne 0) { Write-Error "Le zip a échoué (code $LASTEXITCODE)."; exit $LASTEXITCODE }
Write-Host "Terminé : $sortie"
