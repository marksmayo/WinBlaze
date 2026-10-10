$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
# A process can exit between WaitForExit(false), Refresh and the CPU read.
# Windows can then return zero; that cannot erase CPU already observed.
$bound = 0.0
foreach ($observation in @(0.0, 15.625, 31.25, 0.0)) {
    $bound = Merge-BenchmarkCpuLowerBound -PreviousMilliseconds $bound -ObservedMilliseconds $observation
}
if ($bound -ne 31.25) { throw "Exit race erased previously observed CPU: $bound ms" }
Write-Output 'PASS: cumulative CPU observations survive a zero-valued exit race.'
