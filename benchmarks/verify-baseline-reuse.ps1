$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'competitor-refresh-policy.ps1')
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$fixture=Join-Path $PSScriptRoot ('../artifacts/baseline-policy-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$source=Join-Path $fixture 'source.json'
'{}' | Set-Content -LiteralPath $source
$definition=[pscustomobject]@{name='Startup working set at first idle';unit='MiB';scope='first idle';comparable=$true;observational=$false}
$identities=@{WinDirStat=[pscustomobject]@{product_version='2.9';file_version='2.9';sha256='AA'};WizTree=[pscustomobject]@{product_version='4.33';file_version='4.33';sha256='BB'}}
$entries=@(foreach($tool in @('WinDirStat','WizTree')) {
 $identity=$identities[$tool]
 [pscustomobject]@{key=(ConvertTo-Json -Compress -InputObject @($tool,$definition.name,$definition.unit,$definition.scope,$true,$false,$identity.product_version,$identity.file_version,$identity.sha256));tool=$tool;name=$definition.name;value=12;samples=10;values=@(1..10);source=$source}
})
function Assert-Tools($Actual,$Expected) {
 if(($Actual -join ',') -ne ($Expected -join ',')){throw "Expected $($Expected -join ','); got $($Actual -join ',')"}
}
Assert-Tools @(Get-CompetitorRefreshTools -Definitions @($definition) -Entries $entries -Identities $identities) @('WinBlaze')
$identities.WinDirStat.product_version='3.0'
Assert-Tools @(Get-CompetitorRefreshTools -Definitions @($definition) -Entries $entries -Identities $identities) @('WinBlaze','WinDirStat')
$identities.WinDirStat.product_version='2.9'
$identities.WizTree.sha256='CC'
Assert-Tools @(Get-CompetitorRefreshTools -Definitions @($definition) -Entries $entries -Identities $identities) @('WinBlaze','WizTree')
$identities.WizTree.sha256='BB'
$entries[0].samples=9
Assert-Tools @(Get-CompetitorRefreshTools -Definitions @($definition) -Entries $entries -Identities $identities) @('WinBlaze','WinDirStat')
$entries[0].samples=10
$entries[1].source=Join-Path $fixture 'missing.json'
Assert-Tools @(Get-CompetitorRefreshTools -Definitions @($definition) -Entries $entries -Identities $identities) @('WinBlaze','WizTree')
Assert-Tools @(Get-CompetitorRefreshTools -Definitions @($definition) -Entries $entries -Identities $identities -RefreshCompetitors) @('WinBlaze','WinDirStat','WizTree')
Write-Output 'PASS: unchanged complete competitors are reused; changed versions/hashes, missing or invalid baselines are refreshed; WinBlaze always runs.'
$unicodeName='Full C: volume '+[char]0x00B7+' timing variability'
$jsonPath=Join-Path $fixture 'utf8-without-bom.json'
[IO.File]::WriteAllText($jsonPath,(ConvertTo-Json @{name=$unicodeName}),[Text.UTF8Encoding]::new($false))
if((Read-BenchmarkJson $jsonPath).name -ne $unicodeName){throw 'UTF-8 medal names must survive reading in Windows PowerShell 5.1'}
Write-Output 'PASS: UTF-8 names are preserved for matching unchanged baseline definitions.'
$policyPath=Join-Path $fixture 'batch.json'
@{state='awaiting-administrator';competitor_refresh_policy='reuse-complete-version-baselines-v1';selected_tools=@('WinBlaze')} | ConvertTo-Json | Set-Content -LiteralPath $policyPath
$elevatedPath=Join-Path $PSScriptRoot 'current-elevated-competitor-results.json'
$before=(Get-FileHash -LiteralPath $elevatedPath).Hash
Assert-Tools @(Get-InstalledBenchmarkRefreshTools) @('WinBlaze')
$output=@(& (Join-Path $PSScriptRoot 'run-competitor-comparison.ps1') -Tools WinDirStat,WizTree -Profiles full -TimeoutSeconds 1 -OutputPath $elevatedPath -BatchPolicyPath $policyPath)
if(($output -join '') -notmatch 'Reusing complete version-matched administrator competitor baselines'){throw 'Retained-worker entry point must skip unchanged competitor launches'}
if((Get-FileHash -LiteralPath $elevatedPath).Hash -ne $before){throw 'Reused elevated competitor observations must remain unchanged'}
Write-Output 'PASS: retained-worker competitor entry point skips both unchanged apps and preserves elevated evidence.'
$processDirectory=[Environment]::CurrentDirectory
$savedEvidence=[IO.File]::ReadAllBytes($elevatedPath)
try {
    # Set-Location does not update the CLR working directory in an elevated worker.
    # An empty profile list makes a failed skip safe: no competitor can launch.
    [Environment]::CurrentDirectory=$fixture
    $relativePath=Join-Path 'benchmarks' 'current-elevated-competitor-results.json'
    $output=@(& (Join-Path $PSScriptRoot 'run-competitor-comparison.ps1') -Tools WinDirStat,WizTree -Profiles @() -OutputPath $relativePath -BatchPolicyPath $policyPath)
    if(($output -join '') -notmatch 'Reusing complete version-matched administrator competitor baselines') {
        throw 'Relative retained-worker output paths must use the PowerShell location, even when the CLR working directory differs'
    }
    if((Get-FileHash -LiteralPath $elevatedPath).Hash -ne $before){throw 'Relative-path reuse must preserve elevated evidence'}
} finally {
    [Environment]::CurrentDirectory=$processDirectory
    [IO.File]::WriteAllBytes($elevatedPath,$savedEvidence)
}
Write-Output 'PASS: relative retained-worker output paths skip unchanged competitors with an independent CLR working directory.'
