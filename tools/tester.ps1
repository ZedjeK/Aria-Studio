# Lance les tests d'Aria Studio dans un dossier utilisateur jetable (tools/tester.py) : ils ne touchent jamais au
# vrai %USERPROFILE%\.Aria-Studio.
#
#   .\tools\tester.ps1                          tous les tests
#   .\tools\tester.ps1 tests.test_vision -v     arguments de unittest, passés tels quels
#   .\tools\tester.ps1 -Python C:\chemin\python.exe   un autre interpréteur que .venv
#
# Le code de sortie est celui des tests : 0 s'ils passent tous.
# Pas de bloc param : un attribut [Parameter] ferait de -v l'option -Verbose de PowerShell, et unittest ne la verrait pas.
$Python = $null
$Arguments = @($args)
if ($Arguments.Count -ge 1 -and $Arguments[0] -eq '-Python') {
    if ($Arguments.Count -lt 2) { Write-Error '-Python demande un chemin.'; exit 1 }
    $Python = $Arguments[1]
    $Arguments = @($Arguments | Select-Object -Skip 2)
}
$ErrorActionPreference = 'Stop'

$racine = Split-Path $PSScriptRoot -Parent
if (-not $Python) { $Python = Join-Path $racine '.venv\Scripts\python.exe' }
if (-not (Test-Path $Python)) {
    Write-Error "Python est introuvable : $Python. Lance .\run.ps1 une fois pour créer le .venv, ou donne -Python."
    exit 1
}

# Sous Windows PowerShell 5.1, la sortie d'erreur d'un programme natif arrête le script en mode 'Stop' :
# on laisse passer le temps de l'appel, puis on lit le code de sortie.
$ErrorActionPreference = 'Continue'
& $Python (Join-Path $PSScriptRoot 'tester.py') @Arguments
exit $LASTEXITCODE
