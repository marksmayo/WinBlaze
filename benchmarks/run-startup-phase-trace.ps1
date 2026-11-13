[CmdletBinding()]
param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [string]$AppPath = '',
    [string]$OutputPath = ''
)
# Startup phases of the shipping (OS XAML) host. Each launch runs with
# WINBLAZE_STARTUP_TRACE=1, which makes the app append its phase markers and
# their QueryPerformanceCounter values to %TEMP%\WinBlaze-startup.log. The
# launch and the first "visible and answers WM_NULL" observation (the usable-
# window signal of run-gui-comparison.ps1) are stamped with the same counter.
# Tracing writes one log line per marker, so traced launches are slower than
# the medal measurement; use the phases for proportions, not absolute time.
$ErrorActionPreference = 'Stop'
if (!$AppPath) { $AppPath = Join-Path $PSScriptRoot '..\src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe' }
if (!$OutputPath) { $OutputPath = Join-Path $PSScriptRoot 'current-startup-phase-inventory.json' }
. (Join-Path $PSScriptRoot 'gui-win32.ps1')
Add-Type -Namespace WinBlazeBench -Name PhaseProbe -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
[DllImport("user32.dll", SetLastError=true)] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wp, IntPtr lp, uint flags, uint timeout, out IntPtr result);
public static bool Answers(IntPtr hwnd) { IntPtr r; return SendMessageTimeout(hwnd, 0, IntPtr.Zero, IntPtr.Zero, 2, 5000, out r) != IntPtr.Zero; }
'@
$app = (Resolve-Path -LiteralPath $AppPath).Path
$log = Join-Path ([IO.Path]::GetTempPath()) 'WinBlaze-startup.log'
$frequency = [Diagnostics.Stopwatch]::Frequency
$rows = [Collections.Generic.List[object]]::new()
$savedTrace = $env:WINBLAZE_STARTUP_TRACE
try {
    $env:WINBLAZE_STARTUP_TRACE = '1'
    for ($run = 1; $run -le $Runs; $run++) {
        $launch = [Diagnostics.Stopwatch]::GetTimestamp()
        $process = Start-Process -FilePath $app -PassThru
        $usable = 0
        while (([Diagnostics.Stopwatch]::GetTimestamp() - $launch) / $frequency -lt 60) {
            $hwnd = Get-AppWindow $process
            if ($hwnd -ne [IntPtr]::Zero -and [WinBlazeBench.PhaseProbe]::IsWindowVisible($hwnd) -and [WinBlazeBench.PhaseProbe]::Answers($hwnd)) {
                $usable = [Diagnostics.Stopwatch]::GetTimestamp(); break
            }
            Start-Sleep -Milliseconds 5
        }
        if (!$usable) { Stop-Process -Id $process.Id -Force; throw 'WinBlaze showed no responsive window within 60 s' }
        # Let the first frames and deferred start-up work reach the log.
        Start-Sleep -Milliseconds 1500
        Stop-Process -Id $process.Id -Force; $process.WaitForExit()
        $phases = @(Get-Content -LiteralPath $log -Encoding UTF8 | ForEach-Object { try { $_ | ConvertFrom-Json } catch {} } |
            Where-Object { $_.pid -eq $process.Id } | ForEach-Object {
                [pscustomobject]@{ message = $_.message; since_launch_ms = [math]::Round(($_.qpc - $launch) * 1000.0 / $frequency, 3)
                    before_usable = $_.qpc -le $usable }
            })
        $rows.Add([pscustomobject]@{ run = $run; executable_path = $app; sha256 = (Get-FileHash -LiteralPath $app).Hash
            observed_utc = [DateTime]::UtcNow.ToString('o'); usable_ms = [math]::Round(($usable - $launch) * 1000.0 / $frequency, 3); phases = $phases })
        Write-Host ("run {0}: usable at {1:N1} ms, {2} markers" -f $run, $rows[-1].usable_ms, $phases.Count)
    }
} finally { $env:WINBLAZE_STARTUP_TRACE = $savedTrace }
ConvertTo-Json -InputObject $rows.ToArray() -Depth 6 | Set-Content -LiteralPath $OutputPath -Encoding UTF8

# Median first occurrence of each marker, in launch order.
function Get-Median([double[]]$values) { $s = @($values | Sort-Object); if ($s.Count % 2) { $s[($s.Count - 1) / 2] } else { ($s[$s.Count / 2 - 1] + $s[$s.Count / 2]) / 2 } }
$first = @{}
foreach ($row in $rows) {
    $seen = @{}
    foreach ($phase in $row.phases) {
        if ($seen.ContainsKey($phase.message)) { continue }
        $seen[$phase.message] = $true
        if (!$first.ContainsKey($phase.message)) { $first[$phase.message] = [Collections.Generic.List[double]]::new() }
        $first[$phase.message].Add($phase.since_launch_ms)
    }
}
$usableMedian = Get-Median ([double[]]$rows.usable_ms)
$summary = @($first.GetEnumerator() | Where-Object { $_.Value.Count -ge [math]::Ceiling($Runs / 2) } |
    ForEach-Object { [pscustomobject]@{ median_ms = [math]::Round((Get-Median ([double[]]$_.Value)), 1); runs = $_.Value.Count; marker = $_.Key } } |
    Sort-Object median_ms)
$summary + [pscustomobject]@{ median_ms = [math]::Round($usableMedian, 1); runs = $rows.Count; marker = '== harness: window visible and answering ==' } |
    Sort-Object median_ms | Format-Table -AutoSize | Out-String -Width 200 | Write-Output
