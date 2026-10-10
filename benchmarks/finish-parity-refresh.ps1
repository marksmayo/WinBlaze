param([ValidateRange(10,100)][int]$Runs=10,[switch]$SkipFeatures,[switch]$SkipStartup)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$measurementLock=Enter-BenchmarkMeasurementLock
try {
if(!$SkipStartup){& (Join-Path $PSScriptRoot 'run-startup-comparison.ps1') -Runs $Runs}
if(!$SkipFeatures){& (Join-Path $PSScriptRoot 'run-feature-comparison.ps1') -Runs $Runs -Tools WinBlaze}
$featureRows=Get-Content (Join-Path $PSScriptRoot 'current-feature-performance-results.json') -Raw | ConvertFrom-Json
if(@($featureRows | Where-Object status -eq failed).Count){throw 'Feature output validation failed'}
& (Join-Path $PSScriptRoot 'run-current-engine-comparison.ps1') -Runs $Runs -Profiles tiny,small,medium,fanout,fanout-large,scale,full
& (Join-Path $PSScriptRoot 'run-winblaze-refresh.ps1') -Runs $Runs -SkipBuild -SkipStandard
& node (Join-Path $PSScriptRoot 'summarize-repository-performance.mjs')
if($LASTEXITCODE -ne 0){throw 'Complete current-binary observation validation failed'}
& node (Join-Path $PSScriptRoot 'render-report.mjs')
if($LASTEXITCODE -ne 0){throw 'Final focused report rendering failed'}
} finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
