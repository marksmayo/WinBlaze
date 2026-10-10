param([ValidateRange(10,100)][int]$Runs=10)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$queuedLock=Enter-BenchmarkMeasurementLock
try {
 $batch=Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-benchmark-batch.json')
 if($batch.state -ne 'complete'){throw 'Complete the cross-product standard batch before supplemental refresh'}
 & (Join-Path $PSScriptRoot 'verify-parity-interop.ps1') -Runs $Runs
 & (Join-Path $PSScriptRoot 'finish-parity-refresh.ps1') -Runs $Runs -SkipFeatures -SkipStartup
} finally {$queuedLock.ReleaseMutex();$queuedLock.Dispose()}
