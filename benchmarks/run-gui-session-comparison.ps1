[CmdletBinding()]
param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [string]$TargetName = 'lognormal',
    [ValidateSet('WinBlaze','WinDirStat','WizTree','TreeSize Free','SpaceSniffer','Directory Report','Everything')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree'),
    [int]$TimeoutSeconds = 300,
    [string]$OutputPath = '',
    [string]$HistoryPath = ''
)
# GUI session events (EXTENDED_MEDALS_PLAN.md #4, #7, #53) on an extended dataset:
#   first_result_ms  launch with the dataset until the first result row is on screen (#4)
#   reload_ms   launch with the product's own saved scan until its loaded-results signal
#   resize_ms   with those results shown, resize 1200x800 -> 1600x1000 until the window's
#               pixels last change before 400 ms without change (gui-settle.ps1, #53);
#               only the visible frame is captured and the pointer is parked off
#               the window, so neither what is behind it nor hover visuals count
# Saved scans are made untimed once per binary: WinDirStat /SaveTo, WizTree /export,
# winblaze-cli CSV export. Loaded-results signals: WinDirStat's title becomes
# "WinDirStat <version> - <target>"; WizTree's becomes "[<csv>] - WizTree" (two spaces
# before the dash while loading) with its Cancel button hidden; WinBlaze's becomes
# "WinBlaze - <target>". WinBlaze runs with an empty LOCALAPPDATA per run so no
# catalog from an earlier session can show the same title.
$ErrorActionPreference = 'Stop'
if (!$OutputPath) { $OutputPath = Join-Path $PSScriptRoot 'current-gui-session-results.json' }
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
. (Join-Path $PSScriptRoot 'gui-win32.ps1')
. (Join-Path $PSScriptRoot 'gui-settle.ps1')
. (Join-Path $PSScriptRoot 'gui-menu.ps1')
. (Join-Path $PSScriptRoot 'scan-adapters.ps1')
. (Join-Path $PSScriptRoot 'gui-products.ps1')
if (-not ('WinBlazeBench.GuiProbe' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace WinBlazeBench {
public static class GuiProbe {
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flags);
    public static double Ping(IntPtr hwnd) {
        var watch = System.Diagnostics.Stopwatch.StartNew(); IntPtr r;
        return SendMessageTimeout(hwnd, 0, IntPtr.Zero, IntPtr.Zero, 0x0002, 5000, out r) == IntPtr.Zero ? -1 : watch.Elapsed.TotalMilliseconds;
    }
}
}
'@
}
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$target = [IO.Path]::GetFullPath((Join-Path $repo "artifacts/extended-datasets/$TargetName"))
if (!(Test-Path -LiteralPath $target)) { throw "Dataset $TargetName is missing" }
$executables = @{
    WinBlaze = [IO.Path]::GetFullPath((Join-Path $repo 'src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'))
    WinDirStat = if ($env:WINBLAZE_BENCH_WINDIRSTAT) { $env:WINBLAZE_BENCH_WINDIRSTAT } else { 'C:\Program Files\WinDirStat\WinDirStat.exe' }
    WizTree = if ($env:WINBLAZE_BENCH_WIZTREE) { $env:WINBLAZE_BENCH_WIZTREE } else { 'C:\Program Files\WizTree\WizTree64.exe' }
}
foreach ($tool in $Tools) { if (Test-GuiProduct $tool) { $executables[$tool] = Get-BenchToolExecutable $tool } }
$cli = Join-Path $repo 'target/release/winblaze-cli.exe'
$work = Join-Path $repo 'artifacts/gui-session'
New-Item -ItemType Directory -Force -Path $work | Out-Null

function Test-WizTreeCancelVisible($process) {
    $process.Refresh(); $hwnd = (Get-AppWindow $process)
    if ($hwnd -eq [IntPtr]::Zero) { return $false }
    return @([WinBlazeBench.Win32Window]::ChildrenOfClass($hwnd, 'TButton') |
        Where-Object { [WinBlazeBench.Win32Window]::Text($_) -eq 'Cancel' -and [WinBlazeBench.GuiProbe]::IsWindowVisible($_) }).Count -gt 0
}
# Saved scan of the target in each product's own format, made untimed.
function Get-SavedScan([string]$tool, $identity) {
    $stamp = ([string]$identity.identity_sha256).Substring(0, 12)
    $path = Join-Path $work "$tool-$TargetName-$stamp.csv"
    if (Test-Path -LiteralPath $path) { return $path }
    $partial = "$path.partial.csv"
    Remove-Item -LiteralPath $partial -ErrorAction SilentlyContinue
    switch ($tool) {
        'WinDirStat' { Start-Process $executables.WinDirStat -ArgumentList "/SaveTo `"$partial`" `"$target`"" -Wait }
        'WizTree' { Start-Process $executables.WizTree -ArgumentList "`"$target`" /admin=0 /export=`"$partial`"" -Wait }
        'WinBlaze' { & $cli $target $partial | Out-Null; if ($LASTEXITCODE) { throw "winblaze-cli export failed ($LASTEXITCODE)" } }
    }
    if (!(Test-Path -LiteralPath $partial) -or (Get-Item -LiteralPath $partial).Length -eq 0) { throw "$tool saved scan was not written" }
    Move-Item -LiteralPath $partial -Destination $path
    return $path
}
function Test-Reloaded([string]$tool, $process, [string]$saved) {
    $process.Refresh(); $title = (Get-AppTitle $process)
    switch ($tool) {
        'WinBlaze' { return $title -eq "WinBlaze - $target" }
        'WinDirStat' { return $title -like 'WinDirStat*' -and $title.EndsWith(" - $target") -and $title -notmatch 'Scanning' }
        'WizTree' { return $title -eq "[$saved] - WizTree" -and !(Test-WizTreeCancelVisible $process) }
    }
}

$rows = [Collections.Generic.List[object]]::new()
if (Test-Path -LiteralPath $OutputPath) {
    foreach ($row in @((Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json) | ForEach-Object { $_ })) {
        if (!($row.tool -in $Tools -and $row.target -eq $TargetName)) { $rows.Add($row) }
    }
}
# Starts the product; WinBlaze gets an empty LOCALAPPDATA so no earlier catalog loads.
function Start-Tool([string]$tool, [string]$arguments, [string]$appData) {
    $startInfo = [Diagnostics.ProcessStartInfo]::new($executables[$tool])
    $startInfo.UseShellExecute = $false
    $startInfo.Arguments = $arguments
    if ($tool -eq 'WinBlaze') {
        Remove-Item -LiteralPath $appData -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $appData | Out-Null
        $startInfo.EnvironmentVariables['LOCALAPPDATA'] = $appData
    }
    return [Diagnostics.Process]::Start($startInfo)
}
function Stop-Tool($process, [string]$appData) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    $process.WaitForExit(10000) | Out-Null
    Remove-Item -LiteralPath $appData -Recurse -Force -ErrorAction SilentlyContinue
}
# First result row on screen while scanning: WinDirStat's directory list (LVM_GETITEMCOUNT)
# holds a row below the root; WinBlaze's title reports rows shown (or its loaded-results
# title); WizTree draws
# no rows until its scan completes (Cancel button hidden again after being shown).
function Test-FirstResult([string]$tool, $process, [ref]$sawScan) {
    $process.Refresh(); $hwnd = (Get-AppWindow $process)
    if ($hwnd -eq [IntPtr]::Zero) { return $false }
    switch ($tool) {
        'WinDirStat' {
            return @([WinBlazeBench.Win32Window]::ChildrenOfClass($hwnd, 'SysListView32') |
                Where-Object { [WinBlazeBench.Win32Window]::ListItems($_) -ge 2 }).Count -gt 0
        }
        'WizTree' {
            if (Test-WizTreeCancelVisible $process) { $sawScan.Value = $true; return $false }
            return $sawScan.Value -and (Get-AppTitle $process) -like '*WizTree*'
        }
        'WinBlaze' {
            # The title reports rows in the results list while scanning, and becomes
            # "WinBlaze - <target>" only once the tree holds the final results. Read
            # with GetWindowText like the other products' Win32 probes: a UI Automation
            # descendant search ran on WinBlaze's UI thread (83 ms cold) and slowed
            # the startup it was timing.
            $title = Get-AppTitle $process
            return $title -like 'WinBlaze - Scanning * rows shown' -or $title -eq "WinBlaze - $target"
        }
    }
}

# Added products (gui-products.ps1). First result: Directory Report scans on its
# UI thread and shows rows only when done, Everything lists results once it
# reports "N objects"; TreeSize Free and SpaceSniffer draw rows progressively
# with no observable signal (unsupported-events.json). Reload: only Everything
# keeps a saved index (its database), reloaded by restarting on it. Resize runs
# with the reloaded results, or with the scanned results when there is no reload.
function Measure-GuiProductSession([string]$tool, $identity, [int]$run) {
    $productWork = Join-Path $work ($tool -replace ' ', '')
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $session = Start-GuiProduct $tool $target $productWork
    $first = $null; $reload = $null
    try {
        $ready = Wait-GuiProductReady $session $watch $TimeoutSeconds
        if ($tool -in 'Directory Report', 'Everything') { $first = $ready }
        if ($tool -eq 'Everything') {
            # Exit cleanly (closing the window only hides it) so the index database
            # is saved, then restart on it.
            $client = Start-Process $session.Es -ArgumentList "-instance $($session.Instance) -exit" -WindowStyle Hidden -PassThru
            [void]$client.WaitForExit(30000)
            if (!$session.Process.WaitForExit(30000)) { throw 'Everything did not close to save its index' }
            $watch = [Diagnostics.Stopwatch]::StartNew()
            $session.Process = Start-Process (Get-BenchToolExecutable $tool) -ArgumentList "-instance $($session.Instance) -config `"$productWork\cfg.ini`" -db `"$productWork\index.db`"" -PassThru
            $reload = Wait-GuiProductReady $session $watch $TimeoutSeconds
        }
        $hwnd = Get-GuiProductWindow $session
        [WinBlazeBench.GuiSettle]::Place($hwnd, 40, 40, 1200, 800)
        [void][WinBlazeBench.GuiSettle]::SettleWindow($hwnd, 40, 40, 1200, 800, 1000, 15000)
        $resize = [WinBlazeBench.GuiSettle]::MeasureResize($hwnd, 40, 40, 1200, 800, 1600, 1000, 400, 15000)
        [WinBlazeBench.GuiSettle]::Release($hwnd)
        if ($resize -lt 0) { throw "$tool did not settle after resizing" }
        # 0 ms = no pixel ever changed: the screen was not capturable (display off,
        # locked session), not an instant redraw.
        if ($resize -eq 0) { throw "$tool resize produced no observable redraw (screen not capturable)" }
        $rows.Add([pscustomobject][ordered]@{
            tool = $tool; target = $TargetName; run = $run; binary = $identity; started_utc = [DateTime]::UtcNow.ToString('o')
            scope = "GUI session on the $TargetName dataset: first result while scanning, reload of the product's own saved state where it has one, and a resize"
            first_result_ms = $first; reload_ms = $reload; resize_ms = [math]::Round($resize, 2)
        })
        Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
        Write-Output ("{0} session run {1}: first result {2}; reload {3}; resize {4:N0} ms" -f $tool, $run, $(if ($null -ne $first) { '{0:N0} ms' -f $first } else { 'n/a' }), $(if ($null -ne $reload) { '{0:N0} ms' -f $reload } else { 'n/a' }), $resize)
    } finally { Stop-GuiProduct $session }
}
foreach ($tool in $Tools) {
    if (Test-GuiProduct $tool) {
        $identityOptions = @{ Tool = $tool; Executable = $executables[$tool] }
        if ($HistoryPath) { $identityOptions.HistoryPath = $HistoryPath }
        $identity = Get-BenchmarkIdentity @identityOptions
        for ($run = 1; $run -le $Runs; $run++) { Measure-GuiProductSession $tool $identity $run }
        continue
    }
    $identityOptions = @{ Tool = $tool; Executable = $executables[$tool] }
    if ($HistoryPath) { $identityOptions.HistoryPath = $HistoryPath }
    $identity = Get-BenchmarkIdentity @identityOptions
    $saved = Get-SavedScan $tool $identity
    $appData = Join-Path $work 'appdata'
    for ($run = 1; $run -le $Runs; $run++) {
        $scanArguments = $(if ($tool -eq 'WizTree') { "`"$target`" /admin=0" } else { "`"$target`"" })
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Tool $tool $scanArguments $appData
        try {
            $first = $null; $sawScan = $false
            while ($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
                if (Test-FirstResult $tool $process ([ref]$sawScan)) { $first = [math]::Round($watch.Elapsed.TotalMilliseconds, 2); break }
                if ($process.HasExited) { throw "$tool exited while scanning" }
                Start-Sleep -Milliseconds 5
            }
            if ($null -eq $first) { throw "$tool showed no result within $TimeoutSeconds s" }
        } finally { Stop-Tool $process $appData }

        $loadArguments = $(if ($tool -eq 'WinDirStat') { "/LoadFrom `"$saved`"" } else { "`"$saved`"" })
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Tool $tool $loadArguments $appData
        try {
            $reload = $null
            while ($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
                if (Test-Reloaded $tool $process $saved) { $reload = [math]::Round($watch.Elapsed.TotalMilliseconds, 2); break }
                if ($process.HasExited) { throw "$tool exited while reloading" }
                Start-Sleep -Milliseconds 5
            }
            if ($null -eq $reload) { throw "$tool did not reload within $TimeoutSeconds s" }
            $process.Refresh(); $hwnd = (Get-AppWindow $process)
            [WinBlazeBench.GuiSettle]::Place($hwnd, 40, 40, 1200, 800)
            [void][WinBlazeBench.GuiSettle]::SettleWindow($hwnd, 40, 40, 1200, 800, 1000, 15000)
            $resize = [WinBlazeBench.GuiSettle]::MeasureResize($hwnd, 40, 40, 1200, 800, 1600, 1000, 400, 15000)
            [WinBlazeBench.GuiSettle]::Release($hwnd)
            if ($resize -lt 0) { throw "$tool did not settle after resizing" }
        # 0 ms = no pixel ever changed: the screen was not capturable (display off,
        # locked session), not an instant redraw.
        if ($resize -eq 0) { throw "$tool resize produced no observable redraw (screen not capturable)" }
            $rows.Add([pscustomobject][ordered]@{
                tool = $tool; target = $TargetName; run = $run; binary = $identity; started_utc = [DateTime]::UtcNow.ToString('o')
                scope = "GUI session on the $TargetName dataset: first result while scanning, then reload of the product's own saved scan and a resize"
                saved_scan = [IO.Path]::GetFileName($saved); saved_scan_bytes = (Get-Item -LiteralPath $saved).Length
                first_result_ms = $first; reload_ms = $reload; resize_ms = [math]::Round($resize, 2)
            })
            Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
            Write-Output ("{0} session run {1}: first result {2:N0} ms; reload {3:N0} ms; resize {4:N0} ms" -f $tool, $run, $first, $reload, $resize)
        } finally { Stop-Tool $process $appData }
    }
}
