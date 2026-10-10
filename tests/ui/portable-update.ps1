param(
    [Parameter(Mandatory)][string]$InstalledDirectory,
    [Parameter(Mandatory)][string]$PackagePath
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OwnedPortableUpdate {
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static void Close(IntPtr window, int process) {
        uint actual;
        GetWindowThreadProcessId(window, out actual);
        if (actual != process) throw new Exception("Update test window owner mismatch");
        if (!PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero)) throw new Exception("Owned close failed");
    }
}
'@
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$work = Join-Path $repo ('artifacts/ui-portable-update-' + [guid]::NewGuid().ToString('N'))
$install = Join-Path $work 'install'
$stage = Join-Path $work 'staging'
$appdata = Join-Path $work 'appdata'
New-Item -ItemType Directory -Path $install, $stage -Force | Out-Null
$baseline = (Resolve-Path -LiteralPath $InstalledDirectory).Path
$archive = (Resolve-Path -LiteralPath $PackagePath).Path
Get-ChildItem -LiteralPath $baseline | Copy-Item -Destination $install -Recurse
Expand-Archive -LiteralPath $archive -DestinationPath $stage
$appPath = Join-Path $install 'WinBlaze.UI.exe'
$stagedApp = Join-Path $stage 'WinBlaze.UI.exe'
$before = (Get-FileHash -LiteralPath $appPath).Hash
$expected = (Get-FileHash -LiteralPath $stagedApp).Hash
if ($before -eq $expected) { throw 'Update contract requires distinct installed and candidate UI binaries' }
$helperPath = Join-Path $work 'winblaze-updater.exe'
Copy-Item -LiteralPath (Join-Path $install 'winblaze-updater.exe') -Destination $helperPath
$helperHash = (Get-FileHash -LiteralPath $helperPath).Hash
$expectedHashes = @{}
foreach ($file in @('WinBlaze.UI.exe','winblaze_native.dll','winblaze-cli.exe','winblaze-updater.exe')) {
    $expectedHashes[$file] = (Get-FileHash -LiteralPath (Join-Path $stage $file)).Hash
}
function Start-IsolatedProcess {
    param([string]$Executable, [string]$Arguments = '')
    $start = [Diagnostics.ProcessStartInfo]::new($Executable)
    $start.Arguments = $Arguments
    $start.UseShellExecute = $false
    $start.WorkingDirectory = $work
    $start.EnvironmentVariables['LOCALAPPDATA'] = $appdata
    return [Diagnostics.Process]::Start($start)
}
function Wait-OwnedWindow {
    param([Diagnostics.Process]$Process)
    $condition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, $Process.Id),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'WinBlaze'))
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($Process.HasExited) { throw "Owned app exited before a ready window: $($Process.ExitCode)" }
        $window = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children, $condition)
        if ($window) {
            $start = $window.FindFirst([Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'Start scan'))
            if ($start -and $start.Current.IsEnabled) { return $window }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Owned updated app did not expose its ready scan controls'
}
$oldApp = $null; $helper = $null; $newApp = $null
$result = [ordered]@{installed_ui_sha256=$before;candidate_ui_sha256=$expected;helper_sha256=$helperHash
    scope='Local real updater wait/copy/relaunch; isolated installation; no release download or installed-app mutation';passed=$false}
try {
    $oldApp = Start-IsolatedProcess $appPath
    $oldWindow = Wait-OwnedWindow $oldApp
    $arguments = '--pid ' + $oldApp.Id + ' --source "' + $stage + '" --target "' + $install + '" --relaunch "' + $appPath + '" --cleanup "' + $stage + '"'
    # Updater cleanup is limited to this generated fixture beneath repository artifacts.
    $resolvedStage = [IO.Path]::GetFullPath($stage)
    if (!$resolvedStage.StartsWith([IO.Path]::GetFullPath($work) + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Updater cleanup escaped the owned fixture'
    }
    $helper = Start-IsolatedProcess $helperPath $arguments
    Start-Sleep -Milliseconds 700
    if ($helper.HasExited -or (Get-FileHash -LiteralPath $appPath).Hash -ne $before) {
        throw 'Updater must wait while the original app is alive'
    }
    [OwnedPortableUpdate]::Close([IntPtr]$oldWindow.Current.NativeWindowHandle, $oldApp.Id)
    if (!$oldApp.WaitForExit(15000) -or $oldApp.ExitCode -ne 0) { throw 'Original app failed normal update shutdown' }
    if (!$helper.WaitForExit(20000) -or $helper.ExitCode -ne 0) { throw 'Updater did not finish successfully' }
    foreach ($file in $expectedHashes.Keys) {
        if ((Get-FileHash -LiteralPath (Join-Path $install $file)).Hash -ne $expectedHashes[$file]) {
            throw "Updater did not preserve candidate binary: $file"
        }
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $matches = @(Get-Process -Name 'WinBlaze.UI' -ErrorAction SilentlyContinue | Where-Object {
            try { $_.Path -eq $appPath -and $_.Id -ne $oldApp.Id } catch { $false }
        })
        if ($matches.Count -eq 1) {
            $newApp = $matches[0]
            # Retain a handle before closing a process discovered after its parent launched it.
            $null = $newApp.Handle
            break
        }
        if ($matches.Count -gt 1) { throw 'Updater relaunched more than one candidate app' }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (!$newApp) { throw 'Updater did not relaunch the candidate from the isolated install directory' }
    $newWindow = Wait-OwnedWindow $newApp
    [OwnedPortableUpdate]::Close([IntPtr]$newWindow.Current.NativeWindowHandle, $newApp.Id)
    $newExited = $newApp.WaitForExit(15000)
    $result.new_process_id = $newApp.Id
    $result.new_exit_code = $newApp.ExitCode
    if (!$newExited -or $newApp.ExitCode -ne 0) { throw 'Updated app failed normal shutdown' }
    if (Test-Path -LiteralPath $stage) { throw 'Updater did not clean its owned staging directory' }
    $result.old_process_id = $oldApp.Id
    $result.new_process_id = $newApp.Id
    $result.old_exit_code = $oldApp.ExitCode
    $result.new_exit_code = $newApp.ExitCode
    $result.helper_exit_code = $helper.ExitCode
    $result.installed_hashes = $expectedHashes
    $result.passed = $true
    Write-Output "PASS: updater waits, installs exact candidate binaries, relaunches ready UI, cleans staging and exits normally; evidence=$work"
} catch {
    $result.failure = $_.Exception.Message
    throw
} finally {
    foreach ($ownedProcess in @($oldApp, $newApp, $helper)) {
        if ($ownedProcess -and !$ownedProcess.HasExited) { Stop-Process -Id $ownedProcess.Id -Force -ErrorAction SilentlyContinue }
    }
    $result | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $work 'result.json')
}
