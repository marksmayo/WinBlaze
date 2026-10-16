# Dot-source after gui-win32.ps1, gui-menu.ps1 and scan-adapters.ps1: GUI session
# control for the added products (products.mjs), used by the GUI rescan and
# session runners. Signals were established by probing each product:
#   TreeSize Free     status bar "N Files" when ready (the part disappears while
#                     scanning); F5 posted to the focused control rescans.
#   SpaceSniffer      "Master status: Scanning..." / "Ready"; no rescan command
#                     (its views update live from file-system notifications).
#   Directory Report  scans on its UI thread: the Rescan command (32805) is done
#                     when a WM_NULL round trip returns; status "N object(s)".
#   Everything        private instance indexing the folder with change monitoring:
#                     status "Scanning ..." then "N objects"; `es -reindex` rescans
#                     and returns once indexing completes.
if (-not ('WinBlazeBench.GuiFocus' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace WinBlazeBench {
public static class GuiFocus {
    [StructLayout(LayoutKind.Sequential)] struct GuiThreadInfo { public int cbSize, flags; public IntPtr active, focus, capture, menuOwner, moveSize, caret; public int left, top, right, bottom; }
    [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint thread, ref GuiThreadInfo info);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    // The control with keyboard focus in the window's thread (no focus change).
    public static IntPtr Focused(IntPtr window) {
        uint pid; uint thread = GetWindowThreadProcessId(window, out pid);
        var info = new GuiThreadInfo(); info.cbSize = Marshal.SizeOf(info);
        return GetGUIThreadInfo(thread, ref info) && info.focus != IntPtr.Zero ? info.focus : window;
    }
    // Key press to the focused control: VCL routes it through the form's shortcuts.
    public static void Key(IntPtr window, int virtualKey) {
        IntPtr target = Focused(window);
        PostMessage(target, 0x0100, (IntPtr)virtualKey, IntPtr.Zero);
        PostMessage(target, 0x0101, (IntPtr)virtualKey, IntPtr.Zero);
    }
    // Sends a menu command and returns once the window's thread has handled it
    // (a product that scans on its UI thread returns only after the scan).
    public static bool Command(IntPtr window, int id, uint timeoutMs) { IntPtr r; return SendMessageTimeout(window, 0x0111, (IntPtr)id, IntPtr.Zero, 0, timeoutMs, out r) != IntPtr.Zero; }
    // Returns once the window's thread is free to answer a sent message.
    public static bool Drain(IntPtr window, uint timeoutMs) { IntPtr r; return SendMessageTimeout(window, 0, IntPtr.Zero, IntPtr.Zero, 0, timeoutMs, out r) != IntPtr.Zero; }
}
}
'@
}
$script:GuiProductTools = @('TreeSize Free', 'SpaceSniffer', 'Directory Report', 'Everything')
function Test-GuiProduct([string]$Tool) { return $Tool -in $script:GuiProductTools }

# Starts the product scanning $Target; returns a session object for the helpers below.
function Start-GuiProduct([string]$Tool, [string]$Target, [string]$Work) {
    New-Item -ItemType Directory -Path $Work -Force | Out-Null
    $exe = Get-BenchToolExecutable $Tool
    $session = [pscustomobject]@{ Tool = $Tool; Target = $Target; Work = $Work; Process = $null; Companion = $null; Instance = $null; Es = $null; SawScan = $false }
    switch ($Tool) {
        'TreeSize Free' { $session.Process = Start-Process $exe -ArgumentList (ConvertTo-ScanArgument $Target) -PassThru }
        'SpaceSniffer' { $session.Process = Start-Process $exe -ArgumentList ('scan ' + (ConvertTo-ScanArgument $Target)) -PassThru }
        'Directory Report' {
            $settings = Join-Path $Work 'dr-gui-settings.txt'
            # A drive root keeps its backslash ("C:\"); no target (startup) stays empty.
            $start = if ($Target) { $Target.TrimEnd('\') + $(if ($Target.TrimEnd('\') -match '^[A-Za-z]:$') { '\' } else { '' }) } else { '' }
            (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'directory-report-settings.txt') -Encoding UTF8).Replace('{{TARGET}}', $start).Replace('{{REPORT}}', (Join-Path $Work 'unused.csv')).Replace('autoprint=1', 'autoprint=0').Replace('Exit program after printing=1', 'Exit program after printing=0') |
                Set-Content -LiteralPath $settings -Encoding Unicode
            $session.Process = Start-Process $exe -ArgumentList "`"$settings`"" -PassThru
            $session.Companion = Start-Process (Get-DialogDismisser) -ArgumentList '3600 "Directory Report Licensing|OK"' -PassThru
        }
        'Everything' {
            $session.Instance = 'WBBGUI' + [Guid]::NewGuid().ToString('N').Substring(0, 6)
            $session.Es = Join-Path (Split-Path $exe) 'es.exe'
            $folder = $Target.TrimEnd('\') + $(if ($Target.TrimEnd('\') -match '^[A-Za-z]:$') { '\' } else { '' })
            @('[Everything]', 'app_data=0', 'run_as_admin=0', 'allow_multiple_instances=1', 'auto_include_fixed_volumes=0', 'auto_include_removable_volumes=0',
              'auto_include_fixed_refs_volumes=0', 'auto_include_removable_refs_volumes=0', "folders=$folder", 'folder_monitor_changes=1', 'folder_update_rescan_asap=1',
              'index_size=1', 'index_attributes=1', 'index_folder_size=1', 'show_tray_icon=0', 'check_for_updates_on_startup=0') |
                Set-Content -LiteralPath (Join-Path $Work 'cfg.ini') -Encoding ASCII
            Remove-Item -LiteralPath (Join-Path $Work 'index.db') -ErrorAction SilentlyContinue
            $session.Process = Start-Process $exe -ArgumentList "-instance $($session.Instance) -config `"$Work\cfg.ini`" -db `"$Work\index.db`"" -PassThru
        }
    }
    return $session
}
function Get-GuiProductWindow($Session) { return [WinBlazeBench.Win32Window]::AppWindow($Session.Process.Id) }
function Get-GuiProductStatus($Session) {
    $window = Get-GuiProductWindow $Session
    if ($window -eq [IntPtr]::Zero) { return '' }
    return ((([WinBlazeBench.Win32Window]::Describe($window)) | Where-Object { $_ -match 'statusbar' }) -join ' | ') -replace '\s+', ' '
}
function Test-GuiProductScanning($Session) {
    $status = Get-GuiProductStatus $Session
    $scanning = switch ($Session.Tool) {
        'TreeSize Free' { [bool]$status -and $status -notmatch '[\d,.]+ Files' }
        'SpaceSniffer' { $status -match 'Master status: Scanning' }
        'Everything' { $status -match 'Scanning|Indexing' }
        'Directory Report' { $false }
    }
    if ($scanning) { $Session.SawScan = $true }
    return $scanning
}
function Test-GuiProductReady($Session) {
    if (Test-GuiProductScanning $Session) { return $false }
    $status = Get-GuiProductStatus $Session
    switch ($Session.Tool) {
        'TreeSize Free' { return $status -match '[\d,.]+ Files' }
        'SpaceSniffer' { return $Session.SawScan -and $status -match 'Master status: Ready' }
        'Everything' { return $status -match '[\d,]+ objects' }
        'Directory Report' {
            $window = Get-GuiProductWindow $Session
            return $status -match 'object\(s\)' -and [WinBlazeBench.GuiFocus]::Drain($window, 250)
        }
    }
}
# Number shown by the status bar (TreeSize files, Everything objects), or $null.
function Get-GuiProductCount($Session) {
    $status = Get-GuiProductStatus $Session
    $pattern = switch ($Session.Tool) { 'TreeSize Free' { '([\d,.]+) Files' } 'Everything' { '([\d,]+) objects' } default { $null } }
    if ($pattern -and $status -match $pattern) { return [long]($Matches[1] -replace '[,.]', '') }
    return $null
}
# Waits for the initial scan; returns elapsed ms from $Started (a Stopwatch).
function Wait-GuiProductReady($Session, $Started, [int]$TimeoutSeconds = 600) {
    while ($Started.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        if ($Session.Process.HasExited) { throw "$($Session.Tool) exited while scanning" }
        if (Test-GuiProductReady $Session) { return [math]::Round($Started.Elapsed.TotalMilliseconds, 2) }
        Start-Sleep -Milliseconds 5
    }
    throw "$($Session.Tool) was not ready within $TimeoutSeconds s"
}
# One full rescan; returns ms, or $null when the product has no rescan command.
function Invoke-GuiProductRescan($Session, [int]$TimeoutSeconds = 600) {
    $window = Get-GuiProductWindow $Session
    switch ($Session.Tool) {
        'SpaceSniffer' { return $null }
        'Directory Report' {
            $watch = [Diagnostics.Stopwatch]::StartNew()
            if (![WinBlazeBench.GuiFocus]::Command($window, 32805, [uint32]($TimeoutSeconds * 1000))) { throw 'Directory Report rescan did not finish' }
            return [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
        }
        'Everything' {
            $watch = [Diagnostics.Stopwatch]::StartNew()
            $client = Start-Process $Session.Es -ArgumentList "-instance $($Session.Instance) -reindex" -WindowStyle Hidden -PassThru
            if (!$client.WaitForExit($TimeoutSeconds * 1000)) { Stop-Process -Id $client.Id -Force; throw 'Everything reindex did not finish' }
            while (!(Test-GuiProductReady $Session)) { if ($watch.Elapsed.TotalSeconds -gt $TimeoutSeconds) { throw 'Everything not ready after reindex' }; Start-Sleep -Milliseconds 2 }
            return [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
        }
        'TreeSize Free' {
            $watch = [Diagnostics.Stopwatch]::StartNew()
            $Session.SawScan = $false
            [WinBlazeBench.GuiFocus]::Key($window, 0x74)
            while ($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
                $ready = !(Test-GuiProductScanning $Session) -and (Test-GuiProductReady $Session)
                # A tiny folder can rescan between two polls; after 2 s without a
                # visible scanning state, the ready status means it finished.
                if ($ready -and ($Session.SawScan -or $watch.Elapsed.TotalSeconds -gt 2)) { return [math]::Round($watch.Elapsed.TotalMilliseconds, 2) }
                Start-Sleep -Milliseconds 2
            }
            throw 'TreeSize Free rescan did not finish'
        }
    }
}
# One added file, then the product's fastest refresh until done; returns ms
# from the file's creation (auto-refreshing products) or the refresh command.
function Measure-GuiProductSmallChange($Session, [string]$Marker, [int]$TimeoutSeconds = 120) {
    $before = Get-GuiProductCount $Session
    $Session.SawScan = $false
    switch ($Session.Tool) {
        'SpaceSniffer' {
            $watch = [Diagnostics.Stopwatch]::StartNew()
            Set-Content -LiteralPath $Marker -Value 'changed'
            while ($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
                [void](Test-GuiProductScanning $Session)
                if ($Session.SawScan -and (Test-GuiProductReady $Session)) { return [math]::Round($watch.Elapsed.TotalMilliseconds, 2) }
                Start-Sleep -Milliseconds 2
            }
            throw 'SpaceSniffer did not refresh after the change'
        }
        'Everything' {
            $watch = [Diagnostics.Stopwatch]::StartNew()
            Set-Content -LiteralPath $Marker -Value 'changed'
            while ($watch.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
                $count = Get-GuiProductCount $Session
                if ($null -ne $count -and $count -gt $before) { return [math]::Round($watch.Elapsed.TotalMilliseconds, 2) }
                Start-Sleep -Milliseconds 2
            }
            throw 'Everything did not show the added file'
        }
        default {
            Set-Content -LiteralPath $Marker -Value 'changed'
            return Invoke-GuiProductRescan $Session $TimeoutSeconds
        }
    }
}
function Stop-GuiProduct($Session) {
    if ($Session.Process -and !$Session.Process.HasExited) { Stop-Process -Id $Session.Process.Id -Force -ErrorAction SilentlyContinue }
    if ($Session.Companion -and !$Session.Companion.HasExited) { Stop-Process -Id $Session.Companion.Id -Force -ErrorAction SilentlyContinue }
}
