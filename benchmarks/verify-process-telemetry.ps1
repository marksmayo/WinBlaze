$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'process-telemetry.ps1')
$start=[Diagnostics.ProcessStartInfo]::new($env:ComSpec,'/c exit 0')
$start.UseShellExecute=$false
$start.CreateNoWindow=$true
$process=[Diagnostics.Process]::new()
$process.StartInfo=$start
try {
    if(!$process.Start()){throw 'Owned telemetry fixture did not start'}
    # Deliberately miss every live poll. Completion must still expose genuine
    # kernel counters through the retained handle, never invented zero usage.
    if(!$process.WaitForExit(5000)){throw 'Owned telemetry fixture timed out'}
    $sample=Get-BenchmarkProcessTelemetry $process
    if($null -eq $sample.cpu_ms -or $sample.cpu_ms -lt 0){throw 'Exited process CPU counter was lost'}
    if($null -eq $sample.peak_working_set_bytes -or $sample.peak_working_set_bytes -le 0){throw 'Exited process peak working set was lost'}
} finally { $process.Dispose() }
$missing=[Diagnostics.Process]::new()
try {
    $sample=Get-BenchmarkProcessTelemetry $missing
    if($null -ne $sample.cpu_ms -or $null -ne $sample.peak_working_set_bytes -or $null -ne $sample.private_bytes){throw 'Unavailable counters became zero usage'}
} finally { $missing.Dispose() }
$state=@{sample_count=0;cpu_ms=$null;peak_working_set_bytes=$null;private_bytes=$null}
Add-BenchmarkProcessTelemetry $state ([pscustomobject]@{cpu_ms=31.25;peak_working_set_bytes=4096;private_bytes=2048})
Add-BenchmarkProcessTelemetry $state ([pscustomobject]@{cpu_ms=0;peak_working_set_bytes=$null;private_bytes=$null})
Add-BenchmarkProcessTelemetry $state ([pscustomobject]@{cpu_ms=$null;peak_working_set_bytes=$null;private_bytes=$null})
if($state.sample_count -ne 2 -or $state.cpu_ms -ne 31.25 -or $state.peak_working_set_bytes -ne 4096 -or $state.private_bytes -ne 2048){throw 'Completion races erased observed counters or counted missing telemetry'}
Write-Output 'PASS: owned exited processes retain real CPU/peak counters; unavailable telemetry remains null.'
