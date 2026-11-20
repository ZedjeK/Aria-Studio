<#
.SYNOPSIS
Pose les protections GitHub d'une appli Aria : fusion en squash, rulesets (tests et analyse CodeQL exigés avant
fusion), signalement privé, alertes et correctifs Dependabot, secret scanning, releases immuables, actions épinglées
par SHA.

.DESCRIPTION
À lancer après le premier passage vert de la CI sur main : on n'exige jamais un check qui n'a pas encore tourné.

Le script lit l'état du dépôt, affiche ce qu'il changerait, et n'écrit qu'après ton accord. L'état d'avant est
enregistré à chaque lancement dans un fichier daté, .work\protection-avant-<date>.json, pour pouvoir revenir en
arrière : une relance de vérification n'écrase pas l'état d'avant un changement.

    .\tools\proteger-depot.ps1              affiche le plan et demande confirmation
    .\tools\proteger-depot.ps1 -Montrer     affiche le plan et ce qui serait envoyé pour les rulesets, sans rien écrire
    .\tools\proteger-depot.ps1 -Depot ZedjeK/Aria-Atlas

Il a besoin de gh, connecté avec un compte qui administre le dépôt.

Retour arrière des trois derniers réglages, si l'un d'eux gêne :

    gh api -X DELETE repos/ZedjeK/Aria-Atlas/immutable-releases
    gh api -X DELETE repos/ZedjeK/Aria-Atlas/automated-security-fixes
    gh api -X PUT repos/ZedjeK/Aria-Atlas/actions/permissions -F enabled=true -f allowed_actions=all -F sha_pinning_required=false

Les releases déjà publiées pendant que l'immuabilité était active le restent.
#>
[CmdletBinding()]
param(
    [string]$Depot,
    # Essai à blanc : le plan, et le corps des rulesets qui seraient envoyés. Rien n'est demandé ni écrit sur GitHub.
    [switch]$Montrer
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$racine = Split-Path -Parent $PSScriptRoot

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw "gh est introuvable." }
if (-not $Depot) {
    # Sans -Depot, c'est le dépôt de ce script qui est visé, quel que soit le dossier d'où il est lancé.
    Push-Location -LiteralPath $racine
    try { $Depot = "$(gh repo view --json nameWithOwner --jq .nameWithOwner)".Trim() }
    finally { Pop-Location }
}
if ($Depot -notmatch '\A[\w.-]+/[\w.-]+\z') { throw "Dépôt introuvable. Précise-le : -Depot proprietaire/nom" }

# Identifiant de l'application GitHub Actions : le check « tests » n'est accepté que s'il vient d'elle.
$idActions = 15368

# Appel en lecture : renvoie l'objet JSON, $true pour une réponse vide (204), $null pour un 404.
# Toute autre erreur (réseau, droits, quota) arrête le script : une panne ne doit pas passer pour « rien en place ».
function Lire([string]$chemin) {
    # Sous PowerShell 5.1, une sortie d'erreur de gh redirigée arrêterait le script : on la lit comme une réponse.
    $ErrorActionPreference = 'Continue'
    $tout = @(gh api $chemin 2>&1)
    $code = $LASTEXITCODE
    $erreurs = @($tout | Where-Object { $_ -is [Management.Automation.ErrorRecord] } | ForEach-Object { "$_" })
    $sortie = @($tout | Where-Object { $_ -isnot [Management.Automation.ErrorRecord] })
    if ($code -ne 0) {
        if (($erreurs -join ' ') -match 'HTTP 404') { return $null }
        throw "Lecture impossible : $chemin ($($erreurs -join ' '))"
    }
    if ($sortie.Count -eq 0) { return $true }
    return (($sortie -join "`n") | ConvertFrom-Json)
}

# Appel en écriture, avec un corps JSON passé par fichier.
function Ecrire([string]$methode, [string]$chemin, $corps) {
    if ($null -eq $corps) {
        gh api -X $methode $chemin | Out-Null
    }
    else {
        $fichier = Join-Path $racine '.work\protection-corps.json'
        [IO.File]::WriteAllText($fichier, ($corps | ConvertTo-Json -Depth 10), (New-Object Text.UTF8Encoding($false)))
        gh api -X $methode $chemin --input $fichier | Out-Null
        Remove-Item -LiteralPath $fichier -Force
    }
    if ($LASTEXITCODE -ne 0) { throw "GitHub a refusé : $methode $chemin" }
}

# ----- État des lieux, en lecture seule -----

Write-Host "Dépôt : $Depot"
$depotInfo = Lire "repos/$Depot"
if ($null -eq $depotInfo) { throw "Impossible de lire le dépôt $Depot." }
$branche = [string]$depotInfo.default_branch
$rulesets = @(Lire "repos/$Depot/rulesets")
$reglesBranche = @(Lire "repos/$Depot/rules/branches/$([uri]::EscapeDataString($branche))")
$immuables = Lire "repos/$Depot/immutable-releases"
$actions = Lire "repos/$Depot/actions/permissions"
$correctifs = Lire "repos/$Depot/automated-security-fixes"

New-Item -ItemType Directory -Path (Join-Path $racine '.work') -Force | Out-Null
$maintenant = Get-Date
$avant = [ordered]@{
    date = $maintenant.ToString('s'); depot = $depotInfo; rulesets = $rulesets; regles = $reglesBranche
    releasesImmuables = $immuables; actions = $actions; correctifsDependabot = $correctifs
}
# Un fichier par lancement : relancer le script pour vérifier ne doit pas écraser l'état d'avant un changement.
$relatifAvant = ".work\protection-avant-$($maintenant.ToString('yyyy-MM-ddTHH-mm-ss')).json"
[IO.File]::WriteAllText((Join-Path $racine $relatifAvant), ($avant | ConvertTo-Json -Depth 12), (New-Object Text.UTF8Encoding($false)))

# Checks exigés : « tests » partout ; « squelettes » en plus sur le dépôt modèle, le seul qui a ce workflow (lu dans
# le dépôt visé, pas dans ce dossier : le script peut viser un autre dépôt avec -Depot).
$exiges = @('tests')
if (Lire "repos/$Depot/contents/.github/workflows/squelettes.yml") { $exiges += 'squelettes' }
$listeExiges = ($exiges | ForEach-Object { "« $_ »" }) -join ' et '

# Ces checks ont-ils réussi sur la branche par défaut ? On n'exige jamais un check qui n'a pas encore tourné.
$checks = Lire "repos/$Depot/commits/$([uri]::EscapeDataString($branche))/check-runs"
$testsVerts = $false
if ($checks -and $checks.PSObject.Properties['check_runs']) {
    $verts = @($checks.check_runs | Where-Object { $_.conclusion -eq 'success' } | ForEach-Object { $_.name })
    $testsVerts = @($exiges | Where-Object { $verts -notcontains $_ }).Count -eq 0
}

# Analyse de code (CodeQL) : sur un dépôt qui a ce workflow, la fusion attend ses résultats, et une alerte de sécurité
# élevée ou critique, ou une alerte au niveau « erreur », la retient. Sans cette règle, seul le check de tests retient
# une PR : CodeQL peut signaler une faille sans empêcher la fusion. Comme pour les tests, on ne l'exige qu'une fois
# qu'il a rendu une analyse sur la branche par défaut.
$outilAnalyse = 'CodeQL'
$aCodeQL = [bool](Lire "repos/$Depot/contents/.github/workflows/codeql.yml")
$codeqlATourne = $false
if ($aCodeQL) {
    $refBranche = [uri]::EscapeDataString("refs/heads/$branche")
    $analyses = Lire "repos/$Depot/code-scanning/analyses?tool_name=$outilAnalyse&ref=$refBranche&per_page=1"
    $codeqlATourne = $null -ne $analyses -and $analyses -isnot [bool] -and @($analyses).Count -gt 0
}

$plan = New-Object Collections.Generic.List[object]
# $corps : ce qui serait envoyé, pour l'essai à blanc (-Montrer). Facultatif.
function Prevoir([string]$titre, [scriptblock]$action, $corps = $null) {
    $script:plan.Add([pscustomobject]@{ Titre = $titre; Action = $action; Corps = $corps })
}

# Compléter un ruleset déjà en place : ses règles sont renvoyées telles qu'elles ont été lues (GitHub y ajoute des
# paramètres que ce script ne connaît pas), et seules celles qui manquent sont ajoutées. Le check exigé est réécrit
# quand il n'est pas lié à GitHub Actions.
function ReglesCompletees($actuelles, $voulues, [bool]$checkLie) {
    $resultat = @()
    foreach ($r in @($actuelles)) {
        if ($r.type -eq 'required_status_checks' -and -not $checkLie) { continue }
        $regle = @{ type = [string]$r.type }
        if ($r.PSObject.Properties['parameters'] -and $null -ne $r.parameters) { $regle.parameters = $r.parameters }
        $resultat += $regle
    }
    $presentes = @($resultat | ForEach-Object { $_.type })
    foreach ($v in $voulues) { if ($presentes -notcontains $v.type) { $resultat += $v } }
    return , $resultat
}

# 1. Méthode de fusion
$fusionOk = $depotInfo.allow_squash_merge -and -not $depotInfo.allow_merge_commit -and -not $depotInfo.allow_rebase_merge -and
    $depotInfo.delete_branch_on_merge -and $depotInfo.allow_auto_merge
if ($fusionOk) { Write-Host "  déjà en place : fusion en squash seulement, branche supprimée après fusion" }
else {
    Prevoir "Fusion : squash seulement, branche supprimée après fusion, fusion automatique autorisée" {
        Ecrire 'PATCH' "repos/$Depot" @{
            allow_squash_merge = $true; allow_merge_commit = $false; allow_rebase_merge = $false
            delete_branch_on_merge = $true; allow_auto_merge = $true
        }
    }
}

# 2. Ruleset de la branche par défaut
$nomRuleset = "Protection de $branche"
$corpsBranche = @{
    name = $nomRuleset; target = 'branch'; enforcement = 'active'
    conditions = @{ ref_name = @{ include = @('~DEFAULT_BRANCH'); exclude = @() } }
    rules = @(
        @{ type = 'deletion' },
        @{ type = 'non_fast_forward' },
        # Tout passe par une PR, fusionnée en squash. Aucune relecture exigée : le dépôt n'a qu'un auteur.
        @{ type = 'pull_request'; parameters = @{
                required_approving_review_count = 0
                dismiss_stale_reviews_on_push = $false
                require_code_owner_review = $false
                require_last_push_approval = $false
                required_review_thread_resolution = $false
                allowed_merge_methods = @('squash')
            }
        },
        @{ type = 'required_status_checks'; parameters = @{
                strict_required_status_checks_policy = $false
                required_status_checks = @($exiges | ForEach-Object { @{ context = $_; integration_id = $idActions } })
            }
        }
    )
}

$regles = @($reglesBranche | Where-Object { $_ -and $_.PSObject.Properties['type'] })
$types = @($regles | ForEach-Object { $_.type })
$lies = @($regles | Where-Object { $_.type -eq 'required_status_checks' } |
    ForEach-Object { $_.parameters.required_status_checks } |
    Where-Object { $_.PSObject.Properties['integration_id'] -and $_.integration_id -eq $idActions } | ForEach-Object { $_.context })
$checkLie = @($exiges | Where-Object { $lies -notcontains $_ }).Count -eq 0
$analyseExigee = @($regles | Where-Object { $_.type -eq 'code_scanning' } | ForEach-Object { $_.parameters.code_scanning_tools } |
    Where-Object { $_.tool -eq $outilAnalyse }).Count -gt 0
$analyseAPoser = $aCodeQL -and $codeqlATourne -and -not $analyseExigee
if ($aCodeQL -and ($codeqlATourne -or $analyseExigee)) {
    $corpsBranche.rules += @{ type = 'code_scanning'; parameters = @{
            code_scanning_tools = @(@{ tool = $outilAnalyse; security_alerts_threshold = 'high_or_higher'; alerts_threshold = 'errors' })
        }
    }
}
$avecAnalyse = ''
if ($analyseExigee -or $analyseAPoser) { $avecAnalyse = ", analyse $outilAnalyse exigée avant fusion" }
$complet = $types -contains 'deletion' -and $types -contains 'non_fast_forward' -and $types -contains 'pull_request' -and $checkLie -and
    -not $analyseAPoser
# Le ruleset posé par ce script se reconnaît à son nom ; les règles venues d'ailleurs ne sont jamais modifiées.
$notre = @($rulesets | Where-Object { $_ -and $_.PSObject.Properties['name'] -and $_.name -eq $nomRuleset -and $_.target -eq 'branch' })
$etrangeres = @($regles | Where-Object { $notre.Count -eq 0 -or $_.ruleset_id -ne $notre[0].id } | ForEach-Object { $_.type })

if ($complet) {
    Write-Host "  déjà en place : $branche protégée, PR obligatoire, check(s) $listeExiges exigé(s) et lié(s) à GitHub Actions$avecAnalyse"
}
elseif ($etrangeres.Count -gt 0) {
    Write-Host "  À FAIRE À LA MAIN : $branche a des règles venues d'un autre ruleset ($($etrangeres -join ', ')). Complète-le plutôt que d'en créer un second." -ForegroundColor Yellow
}
elseif (-not $testsVerts) {
    Write-Host "  PAS ENCORE : le(s) check(s) $listeExiges n'ont pas tous réussi sur $branche. Relance ce script après le premier passage vert." -ForegroundColor Yellow
}
elseif ($notre.Count -gt 0) {
    $idRuleset = $notre[0].id
    $actuel = Lire "repos/$Depot/rulesets/$idRuleset"
    $corpsComplete = @{
        name = $nomRuleset; target = 'branch'; enforcement = 'active'
        conditions = $corpsBranche.conditions
        rules = (ReglesCompletees $actuel.rules $corpsBranche.rules $checkLie)
    }
    Prevoir "Ruleset « $nomRuleset » complété : PR obligatoire, check(s) $listeExiges liés à GitHub Actions$avecAnalyse" {
        Ecrire 'PUT' "repos/$Depot/rulesets/$idRuleset" $corpsComplete
    } $corpsComplete
}
else {
    Prevoir "Ruleset « $nomRuleset » : suppression et réécriture interdites, PR obligatoire, check(s) $listeExiges exigé(s)$avecAnalyse" {
        Ecrire 'POST' "repos/$Depot/rulesets" $corpsBranche
    } $corpsBranche
}
if ($aCodeQL -and -not $codeqlATourne -and -not $analyseExigee) {
    Write-Host "  PAS ENCORE : $outilAnalyse n'a pas encore rendu d'analyse sur $branche. Relance ce script après son premier passage : son analyse sera alors exigée avant fusion." -ForegroundColor Yellow
}

# 3. Ruleset des tags de version
$rulesetsTags = @($rulesets | Where-Object { $_ -and $_.PSObject.Properties['target'] -and $_.target -eq 'tag' })
if (@($rulesetsTags | Where-Object { $_.name -eq 'Tags de version' -and $_.enforcement -eq 'active' }).Count -gt 0) {
    Write-Host "  déjà en place : ruleset « Tags de version », actif"
}
elseif ($rulesetsTags.Count -gt 0) {
    $liste = ($rulesetsTags | ForEach-Object { "« $($_.name) » ($($_.enforcement))" }) -join ', '
    Write-Host "  À FAIRE À LA MAIN : un autre ruleset de tags existe : $liste. Vérifie qu'il est actif et qu'il couvre v*." -ForegroundColor Yellow
}
else {
    Prevoir "Ruleset « Tags de version » : un tag v* ne peut être ni supprimé ni déplacé" {
        Ecrire 'POST' "repos/$Depot/rulesets" @{
            name = 'Tags de version'; target = 'tag'; enforcement = 'active'
            conditions = @{ ref_name = @{ include = @('refs/tags/v*'); exclude = @() } }
            rules = @(@{ type = 'deletion' }, @{ type = 'update' })
        }
    }
}

# 4. Signalement privé des failles
$signalement = Lire "repos/$Depot/private-vulnerability-reporting"
if ($signalement -and $signalement.PSObject.Properties['enabled'] -and $signalement.enabled) {
    Write-Host "  déjà en place : signalement privé des failles"
}
else {
    Prevoir "Signalement privé des failles : activé" { Ecrire 'PUT' "repos/$Depot/private-vulnerability-reporting" $null }
}

# 5. Alertes Dependabot
if (Lire "repos/$Depot/vulnerability-alerts") { Write-Host "  déjà en place : alertes Dependabot" }
else { Prevoir "Alertes Dependabot : activées" { Ecrire 'PUT' "repos/$Depot/vulnerability-alerts" $null } }

# 5 bis. Correctifs de sécurité Dependabot : une PR ouverte d'office pour chaque alerte qui a un correctif.
# Ils demandent les alertes : l'étape précédente passe avant dans le plan.
if ($correctifs -and $correctifs -isnot [bool] -and $correctifs.PSObject.Properties['enabled'] -and $correctifs.enabled) {
    Write-Host "  déjà en place : correctifs de sécurité Dependabot"
}
else {
    Prevoir "Correctifs de sécurité Dependabot : activés" { Ecrire 'PUT' "repos/$Depot/automated-security-fixes" $null }
}

# 6. Secret scanning et push protection
$analyse = $null
if ($depotInfo.PSObject.Properties['security_and_analysis']) { $analyse = $depotInfo.security_and_analysis }
$secretsOk = $analyse -and $analyse.PSObject.Properties['secret_scanning'] -and $analyse.secret_scanning.status -eq 'enabled' -and
    $analyse.PSObject.Properties['secret_scanning_push_protection'] -and $analyse.secret_scanning_push_protection.status -eq 'enabled'
if ($secretsOk) { Write-Host "  déjà en place : secret scanning et push protection" }
else {
    Prevoir "Secret scanning et push protection : activés" {
        Ecrire 'PATCH' "repos/$Depot" @{ security_and_analysis = @{
                secret_scanning = @{ status = 'enabled' }
                secret_scanning_push_protection = @{ status = 'enabled' }
            }
        }
    }
}

# 7. Releases immuables : une release publiée ne change plus de fichiers et son tag est verrouillé.
# Le README de l'appli s'appuie dessus pour la provenance des mises à jour.
if ($immuables -and $immuables -isnot [bool] -and $immuables.PSObject.Properties['enabled'] -and $immuables.enabled) {
    Write-Host "  déjà en place : releases immuables"
}
else {
    Prevoir "Releases immuables : activées (les releases déjà publiées ne changent pas)" {
        Ecrire 'PUT' "repos/$Depot/immutable-releases" $null
    }
}

# 8. Actions épinglées par SHA : un workflow qui appelle une action par une étiquette (v4, main) est refusé.
if (-not ($actions -and $actions -isnot [bool] -and $actions.PSObject.Properties['enabled'])) {
    Write-Host "  À FAIRE À LA MAIN : réglages Actions illisibles, épinglage par SHA non vérifié." -ForegroundColor Yellow
}
elseif ($actions.PSObject.Properties['sha_pinning_required'] -and $actions.sha_pinning_required) {
    Write-Host "  déjà en place : actions épinglées par SHA exigées"
}
else {
    # Les autres réglages Actions sont renvoyés tels qu'ils ont été lus.
    $corpsActions = @{ enabled = [bool]$actions.enabled; sha_pinning_required = $true }
    if ($actions.PSObject.Properties['allowed_actions'] -and $actions.allowed_actions) {
        $corpsActions.allowed_actions = [string]$actions.allowed_actions
    }
    Prevoir "Actions épinglées par SHA : exigées" { Ecrire 'PUT' "repos/$Depot/actions/permissions" $corpsActions }
}

# ----- Plan, accord, application -----

Write-Host ""
Write-Host "État d'avant enregistré dans $relatifAvant."
Write-Host ""

if ($plan.Count -eq 0) {
    Write-Host "Rien à changer." -ForegroundColor Cyan
    exit 0
}

Write-Host "Ce script va faire $($plan.Count) changement(s) sur $Depot :"
$n = 0
foreach ($etape in $plan) { $n++; Write-Host "  $n. $($etape.Titre)" }
Write-Host ""
if ($Montrer) {
    foreach ($etape in $plan) {
        if ($null -eq $etape.Corps) { continue }
        Write-Host "Ce qui serait envoyé pour : $($etape.Titre)"
        Write-Host ($etape.Corps | ConvertTo-Json -Depth 10)
        Write-Host ""
    }
    Write-Host "Essai à blanc : rien n'a été modifié." -ForegroundColor Cyan
    exit 0
}
$reponse = (Read-Host "Appliquer ? (o/N)").Trim()
if ($reponse -notmatch '^[oO]') {
    Write-Host "Rien n'a été modifié."
    exit 0
}

foreach ($etape in $plan) {
    Write-Host "  $($etape.Titre)..."
    & $etape.Action
}
Write-Host "Terminé. Relance le script pour vérifier : il doit répondre « Rien à changer »." -ForegroundColor Cyan
