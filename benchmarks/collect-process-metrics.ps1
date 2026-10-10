param(
    [string]$OutputPath = (Join-Path $PSScriptRoot 'current-process-samples.csv'),
    [int]$DurationSeconds = 1800,
    [int]$IntervalMs = 250
)

# Read-only process telemetry; no input injection or app configuration changes.
$ErrorActionPreference = 'Stop'
$writer = [IO.StreamWriter]::new($OutputPath, $false)
$writer.WriteLine('utc,name,pid,process_age_ms,cpu_ms,working_set_bytes,peak_working_set_bytes,private_bytes,window_present,responding')
$writer.AutoFlush = $true
$timer = [Diagnostics.Stopwatch]::StartNew()
try {
    while ($timer.Elapsed.TotalSeconds -lt $DurationSeconds) {
        $now = [DateTime]::UtcNow
        foreach ($process in @(Get-Process -Name 'WinBlaze.UI','WizTree64','WinDirStat' -ErrorAction SilentlyContinue)) {
            try {
                $process.Refresh()
                $values = @($now.ToString('o'), $process.ProcessName, $process.Id,
                    [math]::Round(($now - $process.StartTime.ToUniversalTime()).TotalMilliseconds),
                    [math]::Round($process.TotalProcessorTime.TotalMilliseconds),
                    $process.WorkingSet64, $process.PeakWorkingSet64, $process.PrivateMemorySize64,
                    [int]($process.MainWindowHandle -ne 0), [int]$process.Responding)
                $writer.WriteLine(($values -join ','))
            } catch { } # A process may exit between enumeration and sampling.
        }
        Start-Sleep -Milliseconds $IntervalMs
    }
} finally {
    $writer.Dispose()
}
