param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [string[]]$Profiles = @('tiny','scale','fanout-large','full'),
    [switch]$SkipBuild,
    [switch]$SkipFeatures,
    [switch]$IncludeElevated,
    [switch]$RefreshCompetitors
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
. (Join-Path $PSScriptRoot 'competitor-refresh-policy.ps1')
$measurementLock=Enter-BenchmarkMeasurementLock
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location -LiteralPath $repo
$selectedTools=@(Get-InstalledBenchmarkRefreshTools -RefreshCompetitors:$RefreshCompetitors)
$batchId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$batch = Join-Path $PSScriptRoot "runs/$batchId"
New-Item -ItemType Directory -Force -Path $batch | Out-Null
$workflowResultsPath = Join-Path $batch 'workflow-results.json'
$preservedWorkflow=@()
if(Test-Path -LiteralPath (Join-Path $PSScriptRoot 'current-workflow-results.json')) {
    $preservedWorkflow=@(Read-BenchmarkJson (Join-Path $PSScriptRoot 'current-workflow-results.json') | Where-Object {$_.tool -notin $selectedTools -and $_.profile -in $Profiles})
}
ConvertTo-Json -InputObject $preservedWorkflow -Depth 30 | Set-Content -LiteralPath $workflowResultsPath -Encoding UTF8
$manifest = [ordered]@{batch_id=$batchId;started_utc=[DateTime]::UtcNow.ToString('o');requested_runs=$Runs;profiles=$Profiles;state='running';revision=(git rev-parse HEAD);elevation=if (([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {'administrator token'} else {'standard user token'};scope='sequential process startup + native scan/index (WinBlaze) + CSV export; no GUI rendering';cache='uncontrolled; first observation is not verified cold';error=$null}
$manifest['selected_tools']=$selectedTools
$manifest['competitor_refresh_policy']='reuse-complete-version-baselines-v1'
function Publish-Report {
    ConvertTo-Json -InputObject $manifest -Depth 8 | Set-Content (Join-Path $batch 'manifest.json') -Encoding UTF8
    Copy-Item -LiteralPath (Join-Path $batch 'manifest.json') -Destination (Join-Path $PSScriptRoot 'current-benchmark-batch.json')
    if (Test-Path -LiteralPath $workflowResultsPath) { Copy-Item -LiteralPath $workflowResultsPath -Destination (Join-Path $PSScriptRoot 'current-workflow-results.json') }
    & node (Join-Path $PSScriptRoot 'render-report.mjs')
    if ($LASTEXITCODE -ne 0) { throw 'HTML generation failed.' }
}
try {
    if (!$SkipBuild) {
        & cargo +1.96.0 build --release --offline -p winblaze-native -p winblaze-scanner --examples
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
        & cargo +1.96.0 build --release --offline -p winblaze-native
        if ($LASTEXITCODE -ne 0) { throw 'Native DLL build failed.' }
        $msbuild = 'C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/MSBuild/Current/Bin/amd64/MSBuild.exe'
        if (!(Test-Path -LiteralPath $msbuild)) { throw 'Visual Studio 2022 Build Tools are required, or use -SkipBuild with rebuilt binaries.' }
        @'
import os, subprocess
r = subprocess.run([r'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe', 'src/WinBlaze.UI/WinBlaze.UI.vcxproj', '/p:Configuration=Release', '/p:Platform=x64', '/m', '/nologo', '/v:minimal'], env=dict(os.environ))
if r.returncode: raise SystemExit(r.returncode)
for project in ['tests/native/file-actions.vcxproj', 'tests/native/directory-watcher.vcxproj', 'tests/native/virtual-disks.vcxproj', 'tests/native/shell-context.vcxproj', 'tests/native/portable-devices.vcxproj', 'tests/native/treemap.vcxproj', 'tests/native/search-input.vcxproj', 'tests/native/color-resources.vcxproj', 'tests/native/permission-view.vcxproj', 'tests/native/parity-layouts.vcxproj']:
    r = subprocess.run([r'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe', project, '/p:Configuration=Release', '/p:Platform=x64', '/m', '/nologo', '/v:minimal'], env=dict(os.environ))
    if r.returncode: raise SystemExit(r.returncode)
'@ | python -
        if ($LASTEXITCODE -ne 0) { throw 'GUI release build failed.' }
    }
    if (!(Test-Path -LiteralPath 'target/release/winblaze-cli.exe')) { throw 'Build winblaze-cli.exe before benchmarking.' }
    Publish-Report
    foreach ($profile in $Profiles) {
        if ($profile -ne 'full') {
            if (!(Test-Path -LiteralPath "artifacts/benchmark-datasets/$profile.manifest.json")) {
                & (Join-Path $PSScriptRoot 'make-datasets.ps1') -Root artifacts/benchmark-datasets -Size $profile
            }
        }
        & (Join-Path $PSScriptRoot 'run-competitor-comparison.ps1') -Profiles $profile -Runs $Runs -Tools $selectedTools -OutputPath $workflowResultsPath -AppendResults -OnToolComplete { Publish-Report }
    }
    & (Join-Path $PSScriptRoot 'run-startup-comparison.ps1') -Runs $Runs -Tools $selectedTools
    & (Join-Path $PSScriptRoot 'run-ui-ready-comparison.ps1') -Runs $Runs
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'current-startup-results.json') -Destination (Join-Path $batch 'startup-results.json')
    Publish-Report
    & (Join-Path $PSScriptRoot 'run-current-engine-comparison.ps1') -Runs $Runs -Profiles $Profiles -OutputPath (Join-Path $batch 'engine-results.json')
    Copy-Item -LiteralPath (Join-Path $batch 'engine-results.json') -Destination (Join-Path $PSScriptRoot 'current-batch-engine-results.json')
    Publish-Report
    if (!$SkipFeatures) {
        & (Join-Path $PSScriptRoot 'run-feature-comparison.ps1') -Runs $Runs -Tools $selectedTools -IncludeMft:$IncludeElevated
        $featureRows = Get-Content (Join-Path $PSScriptRoot 'current-feature-performance-results.json') -Raw | ConvertFrom-Json
        $manifest['feature_failures'] = @($featureRows | Where-Object status -eq failed).Count
        foreach ($name in @('current-feature-performance-results.json','current-feature-mft-results.json')) {
            if (Test-Path -LiteralPath (Join-Path $PSScriptRoot $name)) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $batch $name) }
        }
        Publish-Report
    }
    if ($IncludeElevated) {
        if ($manifest.elevation -ne 'administrator token') { throw '-IncludeElevated requires an administrator terminal; rerun with that token.' }
        & (Join-Path $PSScriptRoot 'run-elevated-comparison.ps1') -Runs $Runs
        & (Join-Path $PSScriptRoot 'run-elevated-native-comparison.ps1') -Runs $Runs
        foreach ($name in @('current-elevated-mft-results.json','current-elevated-competitor-results.json','current-elevated-native-results.json')) {
            Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $batch $name)
        }
    }
    $manifest.state=if ($manifest['feature_failures']) {'complete-with-feature-failures'} else {'complete'}
    $manifest['completed_utc']=[DateTime]::UtcNow.ToString('o')
    Publish-Report
    Write-Output "Benchmark batch $batchId $($manifest.state). Report: benchmarks/CURRENT_COMPARISON.html"
} catch {
    $manifest.state='failed'; $manifest.error=$_.Exception.Message
    Publish-Report
    throw
}
finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
