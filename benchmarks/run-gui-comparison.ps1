[CmdletBinding()]
param(
    [ValidateRange(1,100)][int]$Runs = 10,
    # Scan target for the GUI scan events; 'C:\' for the full drive.
    [string]$Target = 'C:\',
    [string]$TargetName = 'full',
    [ValidateSet('WinBlaze','WinDirStat','WizTree','TreeSize Free','SpaceSniffer','Directory Report','Everything')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree'),
    [int]$TimeoutSeconds = 900,
    [string]$OutputPath = (Join-Path $PSScriptRoot 'current-gui-medal-results.json'),
    [string]$HistoryPath = ''
)
# GUI events for all three products (EXTENDED_MEDALS_PLAN.md #1, #2, #3, #5, #60):
#   startup_ms        launch (no target) until the main window is visible and answers WM_NULL
#   scan_ms           launch with the target until the product's own "results ready" signal
#   working_set_bytes working set when results are ready
#   responsiveness_p95_ms  95th percentile WM_NULL round trip polled every 100 ms during the scan
#   exit_ms           WM_CLOSE with results loaded until the process exits
#   gdi_objects / user_objects  GetGuiResources when results are ready (#49)
#   cancel_ms         product's own cancel command after 2 s of scanning until it stops (#6)
# Results-ready signals: WinDirStat's title drops "Scanning" (folders) or "NN%" (drives); WizTree's scan button
# returns from "Cancel" to "Scan"; WinBlaze's title becomes "WinBlaze - <target>".
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
. (Join-Path $PSScriptRoot 'gui-win32.ps1')
. (Join-Path $PSScriptRoot 'gui-menu.ps1')
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
if (-not ('WinBlazeBench.GuiProbe' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace WinBlazeBench {
public static class GuiProbe {
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    // GR_GDIOBJECTS = 0, GR_USEROBJECTS = 1.
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flags);
    // Milliseconds for the UI thread to process WM_NULL; -1 if it did not answer within 5 s.
    public static double Ping(IntPtr hwnd) {
        var watch = System.Diagnostics.Stopwatch.StartNew(); IntPtr r;
        return SendMessageTimeout(hwnd, 0, IntPtr.Zero, IntPtr.Zero, 0x0002, 5000, out r) == IntPtr.Zero ? -1 : watch.Elapsed.TotalMilliseconds;
    }
}
}
'@
}
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$executables = @{
    # WINBLAZE_BENCH_WINBLAZE points at another build for interleaved A/B runs.
    WinBlaze = if ($env:WINBLAZE_BENCH_WINBLAZE) { [IO.Path]::GetFullPath($env:WINBLAZE_BENCH_WINBLAZE) } else { [IO.Path]::GetFullPath((Join-Path $repo 'src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe')) }
    WinDirStat = if ($env:WINBLAZE_BENCH_WINDIRSTAT) { $env:WINBLAZE_BENCH_WINDIRSTAT } else { 'C:\Program Files\WinDirStat\WinDirStat.exe' }
    WizTree = if ($env:WINBLAZE_BENCH_WIZTREE) { $env:WINBLAZE_BENCH_WIZTREE } else { 'C:\Program Files\WizTree\WizTree64.exe' }
}
$bare = $Target.TrimEnd('\')
$scanArguments = @{
    WinBlaze = $(if ($bare -match '^[A-Za-z]:$') { "$bare\" } else { "`"$bare`"" })
    WinDirStat = "`"$bare`""
    WizTree = "`"$bare`" /admin=0"
}
$scanArguments['TreeSize Free'] = $scanArguments.WinBlaze
$scanArguments['SpaceSniffer'] = 'scan ' + $scanArguments.WinBlaze
# Added products (scan-adapters.ps1). Both are VCL apps whose process main
# window can be the hidden TApplication window, so their forms are found by class:
#   TreeSize Free: the status bar gains an "N Files" part once results are ready.
#   SpaceSniffer: the status bar reads "Master status: Scanning..." until "Ready".
# Neither exposes a scriptable cancel command, so cancel latency is not measured.
. (Join-Path $PSScriptRoot 'scan-adapters.ps1')
foreach ($tool in 'TreeSize Free', 'SpaceSniffer', 'Directory Report', 'Everything') { $executables[$tool] = Get-BenchToolExecutable $tool }
. (Join-Path $PSScriptRoot 'gui-products.ps1')
$formClass = @{ 'TreeSize Free' = 'TMainForm'; SpaceSniffer = 'TfrmMain' }
function Get-StatusText([string]$tool, $process) {
    $form = [WinBlazeBench.Win32Window]::TopLevelOfClass($process.Id, $formClass[$tool])
    if ($form -eq [IntPtr]::Zero) { return $null }
    return ([WinBlazeBench.Win32Window]::Describe($form, $false) | Where-Object { $_ -match 'StatusBar' }) -join ' | '
}
function Wait-MainWindow($process, $timer) {
    while ($timer.Elapsed.TotalSeconds -lt 60) {
        $process.Refresh()
        $hwnd = (Get-AppWindow $process)
        if ($hwnd -ne [IntPtr]::Zero -and [WinBlazeBench.GuiProbe]::IsWindowVisible($hwnd) -and [WinBlazeBench.GuiProbe]::Ping($hwnd) -ge 0) { return $hwnd }
        Start-Sleep -Milliseconds 5
    }
    throw "$($process.ProcessName) showed no responsive main window within 60 s"
}
function Test-ResultsReady([string]$tool, $process, [IntPtr]$hwnd, [ref]$sawScanning) {
    $process.Refresh()
    $title = (Get-AppTitle $process)
    switch ($tool) {
        'WinDirStat' {
            if ($title -match 'Scanning|\d+% ') { $sawScanning.Value = $true; return $false }
            return $sawScanning.Value -or ($title -like "*$bare*")
        }
        'WinBlaze' {
            if ($title -like 'WinBlaze - Scanning*') { $sawScanning.Value = $true; return $false }
            # A startup catalog load can show the finished title before the scan begins.
            return $sawScanning.Value -and ($title -like "WinBlaze - $bare*")
        }
        'TreeSize Free' {
            $status = Get-StatusText $tool $process
            if ($status -match '[\d,.]+ Files') { return $true }
            if ($status) { $sawScanning.Value = $true }
            return $false
        }
        'SpaceSniffer' {
            $status = Get-StatusText $tool $process
            if ($status -match 'Master status: Scanning') { $sawScanning.Value = $true; return $false }
            return $sawScanning.Value -and $status -match 'Master status: Ready'
        }
        'WizTree' {
            # Both buttons always exist; WizTree shows Cancel only while scanning.
            # Its main window handle can change during startup, so re-read it.
            $current = (Get-AppWindow $process)
            if ($current -eq [IntPtr]::Zero) { return $false }
            $cancelVisible = @([WinBlazeBench.Win32Window]::ChildrenOfClass($current, 'TButton') |
                Where-Object { [WinBlazeBench.Win32Window]::Text($_) -eq 'Cancel' -and [WinBlazeBench.GuiProbe]::IsWindowVisible($_) }).Count -gt 0
            if ($cancelVisible) { $sawScanning.Value = $true; return $false }
            return $sawScanning.Value
        }
    }
}
function Test-Scanning([string]$tool, $process) {
    $process.Refresh()
    switch ($tool) {
        'WinDirStat' { return (Get-AppTitle $process) -match 'Scanning|\d+% ' }
        'WinBlaze' { return (Get-AppTitle $process) -like 'WinBlaze - Scanning*' }
        'TreeSize Free' { $status = Get-StatusText $tool $process; return [bool]$status -and $status -notmatch '[\d,.]+ Files' }
        'SpaceSniffer' { return (Get-StatusText $tool $process) -match 'Master status: Scanning' }
        'WizTree' {
            $current = (Get-AppWindow $process)
            if ($current -eq [IntPtr]::Zero) { return $false }
            return @([WinBlazeBench.Win32Window]::ChildrenOfClass($current, 'TButton') |
                Where-Object { [WinBlazeBench.Win32Window]::Text($_) -eq 'Cancel' -and [WinBlazeBench.GuiProbe]::IsWindowVisible($_) }).Count -gt 0
        }
    }
}
# Cancel latency (#6): scan for 2 s, issue the product's own cancel command,
# time until it stops showing a scan in progress.
function Measure-Cancel([string]$tool) {
    if ($tool -in 'TreeSize Free', 'SpaceSniffer') { return $null }
    $process = Start-Process -FilePath $executables[$tool] -ArgumentList $scanArguments[$tool] -PassThru
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $hwnd = Wait-MainWindow $process $timer
    $seen = $null
    while ($timer.Elapsed.TotalSeconds -lt 60) {
        if (Test-Scanning $tool $process) { if ($seen -eq $null) { $seen = $timer.Elapsed.TotalSeconds } elseif ($timer.Elapsed.TotalSeconds - $seen -ge 2) { break } }
        Start-Sleep -Milliseconds 20
    }
    if ($seen -eq $null) { Stop-Process -Id $process.Id -Force; throw "$tool never showed a scan in progress" }
    $process.Refresh(); $hwnd = (Get-AppWindow $process)
    $cancel = $null
    switch ($tool) {
        'WinDirStat' { $cancel = { [void][WinBlazeBench.GuiMenu]::Invoke($hwnd, 32820) } }  # Toolbar "stop scan" command.
        'WizTree' {
            $button = [WinBlazeBench.Win32Window]::ChildrenOfClass($hwnd, 'TButton') | Where-Object { [WinBlazeBench.Win32Window]::Text($_) -eq 'Cancel' -and [WinBlazeBench.GuiProbe]::IsWindowVisible($_) } | Select-Object -First 1
            $cancel = { [void][WinBlazeBench.GuiProbe]::PostMessage($button, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero) }  # BM_CLICK
        }
        'WinBlaze' {
            $window = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
            $condition = New-Object System.Windows.Automation.AndCondition(
                (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)),
                (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Cancel')))
            $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            $pattern = $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
            $cancel = { $pattern.Invoke() }
        }
    }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    & $cancel
    while ((Test-Scanning $tool $process) -and $watch.Elapsed.TotalSeconds -lt 60) { Start-Sleep -Milliseconds 5 }
    $elapsed = if ($watch.Elapsed.TotalSeconds -lt 60) { [math]::Round($watch.Elapsed.TotalMilliseconds, 2) } else { $null }
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue; $process.WaitForExit()
    return $elapsed
}
function Close-AndTime($process, [IntPtr]$hwnd) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    [void][WinBlazeBench.GuiProbe]::PostMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)  # WM_CLOSE
    if (!$process.WaitForExit(30000)) { Stop-Process -Id $process.Id -Force; return $null }
    return [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
}
$rows = [Collections.Generic.List[object]]::new()
if (Test-Path -LiteralPath $OutputPath) {
    foreach ($row in @((Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json) | ForEach-Object { $_ })) {
        if (!($row.tool -in $Tools -and $row.target -eq $TargetName)) { $rows.Add($row) }
    }
}
# Directory Report and Everything (gui-products.ps1). Directory Report scans on
# its UI thread, so its responsiveness samples record that thread's blocking.
# Everything as a standard user can index C: only as a folder, which never
# completes (it follows junction loops; see the extended Full C: results): it is
# measured for startup only and its full-drive events are unsupported-events.json.
function Measure-GuiProductRun([string]$tool, $identity, [int]$run) {
    $work = Join-Path $repo "artifacts/gui-full/$($tool -replace ' ','')"
    # Startup: no target, until a visible window answers messages.
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $session = Start-GuiProduct $tool '' $work
    try { $hwnd = Wait-MainWindow $session.Process $timer; $startup = $timer.Elapsed.TotalMilliseconds } finally { Stop-GuiProduct $session }
    $row = [ordered]@{ tool = $tool; target = $TargetName; run = $run; binary = $identity; started_utc = [DateTime]::UtcNow.ToString('o')
        scope = 'GUI: launch until results-ready signal; startup is a separate no-target launch'; startup_ms = [math]::Round($startup, 2) }
    if ($tool -eq 'Directory Report') {
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $session = Start-GuiProduct $tool $bare $work
        try {
            $hwnd = Wait-MainWindow $session.Process $timer
            $pings = [Collections.Generic.List[double]]::new(); $nextPing = 0.0
            while (!(Test-GuiProductReady $session)) {
                if ($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds) { throw "$tool GUI scan timed out" }
                if ($timer.Elapsed.TotalMilliseconds -ge $nextPing) {
                    $current = Get-GuiProductWindow $session; if ($current -ne [IntPtr]::Zero) { $hwnd = $current }
                    $ping = [WinBlazeBench.GuiProbe]::Ping($hwnd)
                    $pings.Add($(if ($ping -lt 0) { 5000.0 } else { $ping }))
                    $nextPing = $timer.Elapsed.TotalMilliseconds + 100
                }
                Start-Sleep -Milliseconds 20
            }
            $row.scan_ms = [math]::Round($timer.Elapsed.TotalMilliseconds, 2)
            $session.Process.Refresh(); $row.working_set_bytes = $session.Process.WorkingSet64
            $row.gdi_objects = [WinBlazeBench.GuiProbe]::GetGuiResources($session.Process.Handle, 0)
            $row.user_objects = [WinBlazeBench.GuiProbe]::GetGuiResources($session.Process.Handle, 1)
            $sorted = @($pings | Sort-Object)
            $row.responsiveness_p95_ms = if ($sorted.Count) { [math]::Round($sorted[[math]::Min($sorted.Count - 1, [math]::Ceiling(0.95 * $sorted.Count) - 1)], 2) } else { $null }
            $row.responsiveness_samples = $pings.Count
            Start-Sleep -Milliseconds 500
            $row.exit_ms = Close-AndTime $session.Process (Get-GuiProductWindow $session)
        } finally { Stop-GuiProduct $session }
    }
    $rows.Add([pscustomobject]$row)
    Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
    Write-Output ("{0} GUI run {1}: startup {2:N0} ms; scan {3}; ws {4}; p95 {5}; exit {6}" -f $tool, $run, $startup, $row.scan_ms, $(if ($row.working_set_bytes) { '{0:N0} MiB' -f ($row.working_set_bytes / 1MB) }), $row.responsiveness_p95_ms, $row.exit_ms)
}
foreach ($tool in $Tools) {
    if ($tool -in 'Directory Report', 'Everything') {
        $identityOptions = @{ Tool = $tool; Executable = $executables[$tool] }
        if ($HistoryPath) { $identityOptions.HistoryPath = $HistoryPath }
        $identity = Get-BenchmarkIdentity @identityOptions
        for ($run = 1; $run -le $Runs; $run++) { Measure-GuiProductRun $tool $identity $run }
        continue
    }
    $identityOptions = @{ Tool = $tool; Executable = $executables[$tool] }
    if ($HistoryPath) { $identityOptions.HistoryPath = $HistoryPath }
    $identity = Get-BenchmarkIdentity @identityOptions
    for ($run = 1; $run -le $Runs; $run++) {
        # Only leftovers from this exact benchmark executable, never the user's own copies.
        Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($executables[$tool])) -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $executables[$tool] } | Stop-Process -Force
        # Startup: no target, until a visible window answers messages.
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $executables[$tool] -PassThru
        $hwnd = Wait-MainWindow $process $timer
        $startup = $timer.Elapsed.TotalMilliseconds
        Stop-Process -Id $process.Id -Force; $process.WaitForExit()
        # Scan to results, sampling responsiveness and memory.
        $started = [DateTime]::UtcNow
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $executables[$tool] -ArgumentList $scanArguments[$tool] -PassThru
        $hwnd = Wait-MainWindow $process $timer
        $sawScanning = $false; $pings = [Collections.Generic.List[double]]::new(); $nextPing = 0.0
        while ($true) {
            if ($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds) { Stop-Process -Id $process.Id -Force; throw "$tool GUI scan timed out" }
            if (Test-ResultsReady $tool $process $hwnd ([ref]$sawScanning)) { break }
            # Follow the current main window; some products replace it during startup.
            if ((Get-AppWindow $process) -ne [IntPtr]::Zero) { $hwnd = (Get-AppWindow $process) }
            if ($timer.Elapsed.TotalMilliseconds -ge $nextPing) {
                $ping = [WinBlazeBench.GuiProbe]::Ping($hwnd)
                $pings.Add($(if ($ping -lt 0) { 5000.0 } else { $ping }))
                $nextPing = $timer.Elapsed.TotalMilliseconds + 100
            }
            Start-Sleep -Milliseconds 20
        }
        $scan = $timer.Elapsed.TotalMilliseconds
        $process.Refresh(); $workingSet = $process.WorkingSet64
        $gdiObjects = [WinBlazeBench.GuiProbe]::GetGuiResources($process.Handle, 0)
        $userObjects = [WinBlazeBench.GuiProbe]::GetGuiResources($process.Handle, 1)
        $sorted = @($pings | Sort-Object)
        $p95 = if ($sorted.Count) { $sorted[[math]::Min($sorted.Count - 1, [math]::Ceiling(0.95 * $sorted.Count) - 1)] } else { $null }
        Start-Sleep -Milliseconds 500  # Let the results view settle before closing.
        $process.Refresh()
        if ((Get-AppWindow $process) -ne [IntPtr]::Zero) { $hwnd = (Get-AppWindow $process) }
        if ($formClass.ContainsKey($tool)) { $hwnd = [WinBlazeBench.Win32Window]::TopLevelOfClass($process.Id, $formClass[$tool]) }
        $exit = Close-AndTime $process $hwnd
        $cancelMs = Measure-Cancel $tool
        $rows.Add([pscustomobject][ordered]@{
            tool = $tool; target = $TargetName; run = $run; binary = $identity; started_utc = $started.ToString('o')
            scope = 'GUI: launch until results-ready signal; startup is a separate no-target launch'
            startup_ms = [math]::Round($startup, 2); scan_ms = [math]::Round($scan, 2)
            working_set_bytes = $workingSet; responsiveness_p95_ms = $(if ($p95 -ne $null) { [math]::Round($p95, 2) } else { $null })
            responsiveness_samples = $pings.Count; exit_ms = $exit
            gdi_objects = $gdiObjects; user_objects = $userObjects; cancel_ms = $cancelMs
        })
        Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
        Write-Output ("{0} GUI run {1}: startup {2:N0} ms; scan {3:N0} ms; ws {4:N0} MiB; p95 {5} ms; exit {6} ms" -f $tool, $run, $startup, $scan, ($workingSet / 1MB), $p95, $exit)
    }
}
