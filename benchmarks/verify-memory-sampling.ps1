$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
foreach ($peak in @(0L, -1L)) {
    if ($null -ne (Get-BenchmarkMemoryPeakValue -ObservedPeak $peak -Samples 1)) {
        throw 'Unavailable/nonpositive process memory must be missing telemetry, never a zero-memory medal score.'
    }
}
if ($null -ne (Get-BenchmarkMemoryPeakValue -ObservedPeak 42MB -Samples 0)) {
    throw 'No samples must not fabricate memory telemetry.'
}
if ((Get-BenchmarkMemoryPeakValue -ObservedPeak 42MB -Samples 3) -ne 42MB) {
    throw 'Keep an already observed positive peak despite later exit-race zero readings.'
}
Write-Output 'PASS: memory telemetry keeps positive peaks and never converts missing exit-race counters into zero-memory scores.'
