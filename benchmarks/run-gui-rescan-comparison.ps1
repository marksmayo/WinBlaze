[CmdletBinding()]
param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [int]$Rescans = 10,
    [string]$TargetName = 'lognormal',
    [ValidateSet('WinBlaze','WinDirStat','WizTree','TreeSize Free','SpaceSniffer','Directory Report','Everything')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree'),
    [string]$OutputPath = '',
    [string]$HistoryPath = ''
)
# GUI rescan events (EXTENDED_MEDALS_PLAN.md #69, #8, #56) on an extended dataset:
#   rescan_ms             median time of $Rescans full rescans in one session
#   memory_growth_bytes   working set after the last rescan minus after the first scan
#   small_change_ms       one added file, then the product's fastest refresh until done
# working_set_series_bytes / private_bytes_series hold one sample after the
# first scan and one after each rescan, to tell a plateau from steady growth.
# Refresh commands: WinDirStat File > Refresh All (32813); WizTree Scan button;
# WinBlaze "Start scan" / "Incremental rescan" buttons (UI Automation).
$ErrorActionPreference = 'Stop'
if (!$OutputPath) { $OutputPath = Join-Path $PSScriptRoot 'current-gui-rescan-results.json' }
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
. (Join-Path $PSScriptRoot 'gui-win32.ps1')
. (Join-Path $PSScriptRoot 'gui-menu.ps1')
. (Join-Path $PSScriptRoot 'scan-adapters.ps1')
. (Join-Path $PSScriptRoot 'gui-products.ps1')
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
$arguments = @{ WinBlaze = "`"$target`""; WinDirStat = "`"$target`""; WizTree = "`"$target`" /admin=0" }

function Test-Scanning([string]$tool, $process) {
    $process.Refresh()
    switch ($tool) {
        'WinDirStat' { return (Get-AppTitle $process) -match 'Scanning|\d+% ' }
        'WinBlaze' { return (Get-AppTitle $process) -like 'WinBlaze - Scanning*' }
        'WizTree' {
            $current = [WinBlazeBench.Win32Window]::TopLevelOfClass($process.Id, 'TfrmWizTreeMain')
            if ($current -eq [IntPtr]::Zero) { return $false }
            return @([WinBlazeBench.Win32Window]::ChildrenOfClass($current, 'TButton') |
                Where-Object { [WinBlazeBench.Win32Window]::Text($_) -eq 'Cancel' -and [WinBlazeBench.GuiProbe]::IsWindowVisible($_) }).Count -gt 0
        }
    }
}
function Test-Ready([string]$tool, $process) {
    if (Test-Scanning $tool $process) { return $false }
    switch ($tool) {
        'WinBlaze' { return (Get-AppTitle $process) -like 'WinBlaze - *' }
        'WinDirStat' { return (Get-AppTitle $process) -like '*WinDirStat*-*' }
        'WizTree' {
            $main = [WinBlazeBench.Win32Window]::TopLevelOfClass($process.Id, 'TfrmWizTreeMain')
            return $main -ne [IntPtr]::Zero -and [WinBlazeBench.Win32Window]::Text($main) -like '*WizTree*'
        }
    }
}
# Runs $action, then waits until a scan was seen and has finished. Returns ms.
function Measure-Scan([string]$tool, $process, [scriptblock]$action, [int]$TimeoutSeconds = 600) {
    # WinBlaze also counts each time it shows results (Get-AppResultsShown):
    # a refresh patched from recorded changes can end before any poll sees
    # its "Scanning" title, and a change in the count shows it finished.
    $shown = if ($tool -eq 'WinBlaze') { Get-AppResultsShown $process } else { 0 }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    & $action
    # Timing starts once the rescan is requested (after any confirmation dialog).
    $watch.Restart()
    $seen = $false
    while ($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        if (Test-Scanning $tool $process) { $seen = $true }
        elseif (($seen -or ($tool -eq 'WinBlaze' -and (Get-AppResultsShown $process) -gt $shown)) -and (Test-Ready $tool $process)) {
            return [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
        }
        Start-Sleep -Milliseconds 2
    }
    throw "$tool scan did not finish within $TimeoutSeconds s (scan seen: $seen)"
}
function Get-WinBlazeButton($process, [string]$name) {
    $condition = New-Object System.Windows.Automation.AndCondition(
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)),
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)))
    # A descendant search can miss while the results tree is still appending
    # rows after the scan finished; retry briefly before giving up.
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        try {
            $window = [System.Windows.Automation.AutomationElement]::FromHandle((Get-AppWindow $process))
            $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            if ($element) { return $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern) }
        } catch { }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "WinBlaze button '$name' not found"
}
function Get-RescanAction([string]$tool, $process, [bool]$smallChange) {
    $process.Refresh(); $hwnd = (Get-AppWindow $process)
    switch ($tool) {
        'WinDirStat' { return { [void][WinBlazeBench.GuiMenu]::Invoke($hwnd, 32813) }.GetNewClosure() }
        'WizTree' {
            # A folder scan's Scan button opens a Select Folder dialog with the
            # scanned folder preselected; Select starts the rescan.
            $main = [WinBlazeBench.Win32Window]::TopLevelOfClass($process.Id, 'TfrmWizTreeMain')
            $button = [WinBlazeBench.Win32Window]::ChildrenOfClass($main, 'TButton') | Where-Object { [WinBlazeBench.Win32Window]::Text($_) -eq 'Scan' } | Select-Object -First 1
            $processId = $process.Id
            return {
                [void][WinBlazeBench.GuiProbe]::PostMessage($button, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
                $deadline = [DateTime]::UtcNow.AddSeconds(15)
                do {
                    Start-Sleep -Milliseconds 20
                    $dialog = [WinBlazeBench.Win32Window]::TopLevelOfClass($processId, '#32770')
                } while ($dialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
                if ($dialog -eq [IntPtr]::Zero) { throw 'WizTree Select Folder dialog did not appear' }
                $select = [WinBlazeBench.Win32Window]::ChildrenOfClass($dialog, 'Button') | Where-Object { [WinBlazeBench.Win32Window]::Text($_) -eq 'Select' } | Select-Object -First 1
                [void][WinBlazeBench.GuiProbe]::PostMessage($select, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
            }.GetNewClosure()
        }
        'WinBlaze' {
            $pattern = Get-WinBlazeButton $process $(if ($smallChange) { 'Incremental rescan' } else { 'Start scan' })
            return { $pattern.Invoke() }.GetNewClosure()
        }
    }
}

$rows = [Collections.Generic.List[object]]::new()
if (Test-Path -LiteralPath $OutputPath) {
    foreach ($row in @((Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json) | ForEach-Object { $_ })) {
        if (!($row.tool -in $Tools -and $row.target -eq $TargetName)) { $rows.Add($row) }
    }
}
$marker = Join-Path $target '_small_change.txt'
foreach ($tool in $Tools) {
    $identityOptions = @{ Tool = $tool; Executable = $executables[$tool] }
    if ($HistoryPath) { $identityOptions.HistoryPath = $HistoryPath }
    $identity = Get-BenchmarkIdentity @identityOptions
    for ($run = 1; $run -le $Runs; $run++) {
        Remove-Item -LiteralPath $marker -ErrorAction SilentlyContinue
        if (Test-GuiProduct $tool) {
            # Added products (gui-products.ps1): same measurements; a product
            # without a rescan command records only the small-change refresh.
            $session = Start-GuiProduct $tool $target (Join-Path $repo "artifacts/gui-rescan/$($tool -replace ' ','')")
            try {
                [void](Wait-GuiProductReady $session ([Diagnostics.Stopwatch]::StartNew()))
                Start-Sleep -Milliseconds 500
                $session.Process.Refresh(); $first = $session.Process.WorkingSet64
                $times = @(for ($i = 1; $i -le $Rescans; $i++) { $t = Invoke-GuiProductRescan $session; if ($null -ne $t) { $t; Start-Sleep -Milliseconds 300 } })
                $session.Process.Refresh(); $last = $session.Process.WorkingSet64
                $small = Measure-GuiProductSmallChange $session $marker
                $sorted = @($times | Sort-Object)
                $median = if (!$sorted.Count) { $null } elseif ($sorted.Count % 2) { $sorted[[int][math]::Floor($sorted.Count / 2)] } else { ($sorted[$sorted.Count / 2 - 1] + $sorted[$sorted.Count / 2]) / 2 }
                $rows.Add([pscustomobject][ordered]@{
                    tool = $tool; target = $TargetName; run = $run; binary = $identity; started_utc = [DateTime]::UtcNow.ToString('o')
                    scope = "GUI session on the $TargetName dataset: $Rescans full rescans, then one added file and the product's fastest refresh"
                    rescan_ms = if ($null -ne $median) { [math]::Round($median, 2) } else { $null }; rescan_times_ms = $times
                    memory_growth_bytes = if ($times.Count) { $last - $first } else { $null }; working_set_first_bytes = $first; working_set_last_bytes = $last
                    small_change_ms = $small
                })
                Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
                Write-Output ("{0} rescan run {1}: rescan {2} ms; growth {3} MiB; small change {4:N0} ms" -f $tool, $run, $(if ($null -ne $median) { '{0:N0}' -f $median } else { 'n/a' }), $(if ($times.Count) { '{0:N1}' -f (($last - $first) / 1MB) } else { 'n/a' }), $small)
            } finally {
                Stop-GuiProduct $session
                Remove-Item -LiteralPath $marker -ErrorAction SilentlyContinue
            }
            continue
        }
        $process = Start-Process -FilePath $executables[$tool] -ArgumentList $arguments[$tool] -PassThru
        try {
            $deadline = [DateTime]::UtcNow.AddSeconds(600)
            # The initial scan is not timed. WinBlaze's title names the target
            # only once its own scan of it has finished, and a command-line
            # scan can now finish before a "Scanning" title is ever observable.
            $seen = $tool -eq 'WinBlaze'
            while (!($seen -and (Test-Ready $tool $process))) {
                if ([DateTime]::UtcNow -gt $deadline) { throw "$tool initial scan did not finish" }
                if (Test-Scanning $tool $process) { $seen = $true }
                Start-Sleep -Milliseconds 5
            }
            Start-Sleep -Milliseconds 500
            $process.Refresh(); $first = $process.WorkingSet64
            $workingSetSeries = @($first); $privateSeries = @($process.PrivateMemorySize64)
            $times = @()
            for ($i = 1; $i -le $Rescans; $i++) {
                $times += Measure-Scan $tool $process (Get-RescanAction $tool $process $false)
                Start-Sleep -Milliseconds 300
                $process.Refresh(); $workingSetSeries += $process.WorkingSet64; $privateSeries += $process.PrivateMemorySize64
            }
            $process.Refresh(); $last = $process.WorkingSet64
            Set-Content -LiteralPath $marker -Value 'changed'
            $small = Measure-Scan $tool $process (Get-RescanAction $tool $process $true)
            $sorted = @($times | Sort-Object)
            $median = if ($sorted.Count % 2) { $sorted[[int][math]::Floor($sorted.Count / 2)] } else { ($sorted[$sorted.Count / 2 - 1] + $sorted[$sorted.Count / 2]) / 2 }
            $rows.Add([pscustomobject][ordered]@{
                tool = $tool; target = $TargetName; run = $run; binary = $identity; started_utc = [DateTime]::UtcNow.ToString('o')
                scope = "GUI session on the $TargetName dataset: $Rescans full rescans, then one added file and the product's fastest refresh"
                rescan_ms = [math]::Round($median, 2); rescan_times_ms = $times
                memory_growth_bytes = $last - $first; working_set_first_bytes = $first; working_set_last_bytes = $last
                working_set_series_bytes = $workingSetSeries; private_bytes_series = $privateSeries
                small_change_ms = $small
            })
            Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
            Write-Output ("{0} rescan run {1}: rescan {2:N0} ms; growth {3:N1} MiB; small change {4:N0} ms" -f $tool, $run, $median, (($last - $first) / 1MB), $small)
        } finally {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $marker -ErrorAction SilentlyContinue
        }
    }
}
