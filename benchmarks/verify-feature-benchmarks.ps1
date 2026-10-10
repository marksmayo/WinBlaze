$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'feature-benchmark-lib.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$rows = Get-Content (Join-Path $PSScriptRoot 'current-feature-performance-results.json') -Raw | ConvertFrom-Json
$measured = @($rows | Where-Object { $_.status -eq 'measured' -and $_.operation -ne 'raw-mft-dump' })
$requested = $rows[0].requested_runs
$manifest=Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-feature-performance-batch.json')
$expected=@($manifest.jobs | Where-Object kind -ne mft)
if ($requested -ne 10 -or $measured.Count -ne $expected.Count*$requested) { throw 'Expected ten validated observations per requested standard feature workload.' }
foreach($job in $expected){if(@($measured | Where-Object job -eq $job.id).Count -ne 10){throw ('Missing complete workload: '+$job.id)}}
foreach ($group in ($measured | Group-Object job)) {
    if ($group.Count -ne $requested) { throw "Incomplete job $($group.Name)." }
    foreach ($row in $group.Group) {
        if (!$row.validation.verified -or !$row.binary.sha256 -or $row.elapsed_ms -le 0) { throw 'Missing validation, identity or timing.' }
        if (!$row.sample_count -and ($null -ne $row.cpu_ms_sampled_lower_bound -or $null -ne $row.peak_working_set_bytes_sampled)) { throw 'Missing samples must not be zero usage.' }
    }
}
$folder = Get-ChildItem (Join-Path $repo 'artifacts/feature-benchmarks') -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName 'wd-duplicates-1.csv') } |
    Sort-Object Name | Select-Object -Last 1
$fixture = Get-Content (Join-Path $PSScriptRoot "runs/features-$($folder.Name)/fixture.json") -Raw | ConvertFrom-Json
$temporary = Join-Path $repo ('artifacts/feature-validation-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $temporary | Out-Null
function Assert-Rejected {
    param([string]$Kind,[string]$Path)
    $rejected = $false
    try { $null = Test-FeatureOutput $Kind $Path '' $fixture } catch { $rejected = $true }
    if (!$rejected) { throw "Invalid $Kind output was accepted." }
}
$duplicatePath = Join-Path $folder.FullName 'wd-duplicates-1.csv'
$null = Test-FeatureOutput duplicates $duplicatePath '' $fixture
$duplicates = Read-FeatureCsv $duplicatePath
$duplicates[0].Name = 'decoy-000.dat'
$badDuplicates = Join-Path $temporary 'false-positive.csv'
$duplicates | Export-Csv -LiteralPath $badDuplicates -NoTypeInformation
Assert-Rejected duplicates $badDuplicates
$jsonPath = Join-Path $folder.FullName 'wd-json-1.json'
$null = Test-FeatureOutput json $jsonPath '' $fixture
$decoded = Get-Content -LiteralPath $jsonPath -Raw | ConvertFrom-Json
$decoded[0].Files = 1
$badJson = Join-Path $temporary 'wrong-count.json'
Write-FeatureJson $badJson $decoded
Assert-Rejected json $badJson
$filteredPath = Join-Path $folder.FullName 'wiz-filter-1.csv'
$null = Test-FeatureOutput filtered $filteredPath '' $fixture
$filtered = Read-FeatureCsv $filteredPath -WizTree
$filtered[0].'File Name' = 'unexpected.txt'
$badFilter = Join-Path $temporary 'wrong-filter.csv'
@('WizTree test export') + @($filtered | ConvertTo-Csv -NoTypeInformation) | Set-Content -LiteralPath $badFilter
Assert-Rejected filtered $badFilter
$pngPath = Join-Path $folder.FullName 'wiz-png-1.png'
$null = Test-FeatureOutput png $pngPath '' $fixture
$png = [IO.File]::ReadAllBytes($pngPath); $png[19] = 1
$badPng = Join-Path $temporary 'wrong-dimensions.png'
[IO.File]::WriteAllBytes($badPng,$png)
Assert-Rejected png $badPng
$badMft = Join-Path $temporary 'not-mft.bin'
[IO.File]::WriteAllBytes($badMft,(New-Object byte[] 1024))
Assert-Rejected mft $badMft
$nativeFolder = Get-ChildItem (Join-Path $repo 'artifacts/feature-benchmarks') -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName 'wb-json-1.json') } | Sort-Object Name | Select-Object -Last 1
$nativeFixture = Get-Content (Join-Path $PSScriptRoot "runs/features-$($nativeFolder.Name)/fixture.json") -Raw | ConvertFrom-Json
$nativeJson = Join-Path $nativeFolder.FullName 'wb-json-1.json'
$nativeStdout = Join-Path $nativeFolder.FullName 'wb-json-1.stdout'
$null = Test-FeatureOutput json-native $nativeJson $nativeStdout $nativeFixture
$decoded = Get-Content $nativeJson -Raw | ConvertFrom-Json
$decoded.schema_version = 99
$badNativeJson = Join-Path $temporary 'bad-native-schema.json'
Write-FeatureJson $badNativeJson $decoded
$rejected=$false
try { $null = Test-FeatureOutput json-native $badNativeJson $nativeStdout $nativeFixture } catch { $rejected=$true }
if (!$rejected) { throw 'Native JSON schema corruption was accepted.' }
$nativeCsv = Join-Path $nativeFolder.FullName 'wb-filter-1.csv'
$nativeStdout = Join-Path $nativeFolder.FullName 'wb-filter-1.stdout'
$null = Test-FeatureOutput filtered-native $nativeCsv $nativeStdout $nativeFixture
$decoded = @(Import-Csv $nativeCsv); $decoded[0].Path='unexpected.txt'
$badNativeCsv = Join-Path $temporary 'bad-native-filter.csv'
$decoded | Export-Csv -LiteralPath $badNativeCsv -NoTypeInformation
$rejected=$false
try { $null = Test-FeatureOutput filtered-native $badNativeCsv $nativeStdout $nativeFixture } catch { $rejected=$true }
if (!$rejected) { throw 'Native filtered export included an unexpected extension.' }
$duplicateFolder=Get-ChildItem (Join-Path $repo 'artifacts/feature-benchmarks') -Directory |
    Where-Object {Test-Path (Join-Path $_.FullName 'wb-duplicates-1.csv')} | Sort-Object Name | Select-Object -Last 1
$nativeDuplicate=Join-Path $duplicateFolder.FullName 'wb-duplicates-1.csv'
$null=Test-FeatureOutput duplicates-native $nativeDuplicate '' $fixture
$decoded=@(Import-Csv $nativeDuplicate);$decoded[0].Path='decoy-000.dat'
$badNativeDuplicate=Join-Path $temporary 'native-false-positive.csv'
$decoded | Export-Csv -LiteralPath $badNativeDuplicate -NoTypeInformation
Assert-Rejected duplicates-native $badNativeDuplicate
$badAcl=Join-Path $temporary 'invalid-acl.csv'
1..227 | ForEach-Object {[pscustomobject]@{Path="entry-$_";SDDL='invalid';Status='ok'}} | Export-Csv -LiteralPath $badAcl -NoTypeInformation
Assert-Rejected permissions-native $badAcl
$badStatus=Join-Path $temporary 'failed-native.stdout'
Set-Content -LiteralPath $badStatus -Value 'FAIL'
foreach ($kind in @('actions-native','watch-native')) {
    $rejected=$false
    try {$null=Test-FeatureOutput $kind '' $badStatus $fixture} catch {$rejected=$true}
    if (!$rejected) {throw 'Failed component adapter was accepted'}
}
foreach($kind in @('flame-layout-native','sunburst-layout-native','permission-controls-native')) {
    $invalid=Join-Path $temporary ($kind+'.json')
    Write-FeatureJson $invalid @{operation='wrong';verified=$true;records_checked=409700;repeats=100}
    Assert-Rejected $kind $invalid
}
$badDetailed=Join-Path $temporary 'missing-rich-metadata.csv'
Read-FeatureCsv (Join-Path $folder.FullName 'wb-detailed-csv-1.csv') | Select-Object Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds | Export-Csv $badDetailed -NoTypeInformation
Assert-Rejected detailed-csv-native $badDetailed
foreach ($name in @('run-feature-comparison.ps1','feature-benchmark-lib.ps1','run-comparison.ps1')) {
    $tokens=$null; $errors=$null
    [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name),[ref]$tokens,[ref]$errors) | Out-Null
    if ($errors.Count) { throw ($errors | Out-String) }
}
Write-Output "Verified $($measured.Count) feature observations and rejection of duplicate false positives, wrong JSON counts, wrong filters, wrong PNG dimensions and non-MFT output."
