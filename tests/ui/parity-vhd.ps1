param(
    [string]$AppPath = (Join-Path $PSScriptRoot '../../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'),
    [switch]$InvalidFixture
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies UIAutomationClient,UIAutomationTypes,System.Windows.Forms -TypeDefinition @'
using System.Threading.Tasks;
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Windows.Automation;
public static class ExportInvocation {
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static void CloseOwned(IntPtr window, int process) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (owner != process) throw new Exception("VHD test target owner mismatch");
        if (!PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero)) throw new Exception("Owned close failed");
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int size);
    public static IntPtr FindDialog(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process) return true;
            var title = new StringBuilder(512); GetWindowText(window, title, title.Capacity);
            if (title.ToString() != "Save As" && title.ToString() != "Move file to a new path (existing files are never overwritten)") return true;
            result = window; return false;
        }, IntPtr.Zero);
        return result;
    }
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    public static void Save(IntPtr dialog, string path) {
        SetForegroundWindow(dialog);
        System.Threading.Thread.Sleep(150);
        if (GetForegroundWindow() != dialog) throw new Exception("Owned save dialog could not receive keyboard focus");
        System.Windows.Forms.SendKeys.SendWait("%n");
        System.Windows.Forms.SendKeys.SendWait("^a");
        var escaped = new StringBuilder();
        foreach (char ch in path) {
            if ("+^%~()[]{}".Contains(ch.ToString())) escaped.Append("{").Append(ch).Append("}");
            else escaped.Append(ch);
        }
        System.Windows.Forms.SendKeys.SendWait(escaped.ToString());
        System.Windows.Forms.SendKeys.SendWait("{ENTER}");
    }
    public static Task Invoke(AutomationElement element) {
        return Task.Factory.StartNew(() => ((InvokePattern)element.GetCurrentPattern(InvokePattern.Pattern)).Invoke());
    }
}
'@
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$work = Join-Path $repo ('artifacts/ui-parity-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$fixture = Join-Path $work 'Folder with spaces'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'example.txt') -Value 'fixture'
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = (Resolve-Path -LiteralPath $AppPath).Path
$start.UseShellExecute = $false
$start.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $work 'appdata'
$app = [Diagnostics.Process]::Start($start)
function Find-Control {
    param([string]$Name)
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if ($element) { return $element }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Control not found: $Name"
}

try {
    $pidCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$app.Id)
    $nameCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'WinBlaze')
    $windowCondition = [System.Windows.Automation.AndCondition]::new($pidCondition,$nameCondition)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children,$windowCondition)
        if ($window) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (!$window) { throw 'Owned WinBlaze window did not open.' }
    Start-Sleep -Milliseconds 500

    $ownedBase=Join-Path $work 'virtual-disk.vhdx'
    if ($InvalidFixture) {
        $source=$ownedBase
        [IO.File]::WriteAllBytes($source,[Text.Encoding]::ASCII.GetBytes('owned invalid virtual disk'))
    } else {
        & (Join-Path $repo 'artifacts/virtual-disks/virtual-disks.exe') $ownedBase --create
        if($LASTEXITCODE -ne 0){throw 'Cannot create the owned empty VHDX fixture'}
        $source=$ownedBase+'.valid.vhdx'
    }
    $before=(Get-Item $source).Length
    $beforeHash=(Get-FileHash -LiteralPath $source).Hash
    $app.Refresh()
    $virtualDiskBefore=@($app.Modules | Where-Object ModuleName -eq 'virtdisk.dll').Count -gt 0
    (Find-Control 'File menu').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 100
    $null=[ExportInvocation]::Invoke((Find-Control 'NTFS maintenance...'))
    Start-Sleep -Milliseconds 300
    (Find-Control 'Maintenance file path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($source)
    (Find-Control 'Maintenance operation').GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Start-Sleep -Milliseconds 100
    (Find-Control 'Compact detached VHD / VHDX zero blocks').GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    (Find-Control 'Maintenance operation').GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
    $null=[ExportInvocation]::Invoke((Find-Control 'Review maintenance'))
    Start-Sleep -Milliseconds 300
    $null=[ExportInvocation]::Invoke((Find-Control 'Apply maintenance'))
    Start-Sleep -Milliseconds 500
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    while(!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
    if ($InvalidFixture) {
        $refused=$false
        $deadline=[DateTime]::UtcNow.AddSeconds(10)
        do {
            $texts=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Text))
            foreach ($text in $texts) {
                if ($text.Current.Name -match '^Maintenance could not complete\..*Status ([1-9][0-9]*)$') { $refused=$true; $status=$text.Current.Name; break }
            }
            if ($refused) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        @($texts | ForEach-Object { $_.Current.Name }) | ConvertTo-Json | Set-Content (Join-Path $work 'observed-text.json')
        if (!$refused -or (Get-FileHash -LiteralPath $source).Hash -ne $beforeHash) { throw 'Invalid VHDX must be refused without modifying its bytes' }
    } else {
        $null=Find-Control 'VHD zero-block compaction completed; disk contents and virtual size preserved.'
        if((Get-Item $source).Length -gt $before){throw 'Empty VHDX grew after compaction'}
        $status='VHD zero-block compaction completed'
    }
    $app.Refresh()
    $virtualDiskAfter=@($app.Modules | Where-Object ModuleName -eq 'virtdisk.dll').Count -gt 0
    if (!$virtualDiskAfter) { throw 'Real virtual disk API dependency was not loaded for the operation' }
    [ExportInvocation]::CloseOwned([IntPtr]$window.Current.NativeWindowHandle,$app.Id)
    if (!$app.WaitForExit(10000) -or $app.ExitCode -ne 0) { throw 'VHD operation failed normal shutdown' }
    [ordered]@{binary_sha256=(Get-FileHash -LiteralPath $AppPath).Hash;invalid_fixture=[bool]$InvalidFixture;
        status=$status;virtdisk_loaded_before=$virtualDiskBefore;virtdisk_loaded_after=$virtualDiskAfter;exit_code=$app.ExitCode} |
        ConvertTo-Json | Set-Content (Join-Path $work 'vhd-result.json')
    if ($InvalidFixture) { Write-Output "PASS: confirmed GUI refuses owned invalid VHDX, preserves bytes and exits normally; real VirtDisk API loaded; evidence=$work" }
    else { Write-Output "PASS: confirmed GUI compacts an owned empty VHDX without mounting; normal exit; evidence=$work" }
}finally{
    if(!$app.HasExited){$null=$app.CloseMainWindow();if(!$app.WaitForExit(3000)){$app.Kill()}}
}
