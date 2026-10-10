param(
    [string]$AppPath = (Join-Path $PSScriptRoot '../../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe')
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
public static class ParityInvocation {
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    public static IntPtr FindDialog(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process || !IsWindowVisible(window)) return true;
            var title = new StringBuilder(512); GetWindowText(window, title, title.Capacity);
            if (title.ToString() != "Save As" && title.ToString() != "Open saved scan") return true;
            result = window; return false;
        }, IntPtr.Zero);
        return result;
    }
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr parent, EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern int GetDlgCtrlID(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr GetParent(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wparam, string lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", SetLastError=true)] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static void Save(IntPtr dialog, string path) {
        IntPtr filename = IntPtr.Zero, save = IntPtr.Zero;
        var controls = new StringBuilder();
        EnumChildWindows(dialog, (child, parameter) => {
            var type = new StringBuilder(128); GetClassName(child, type, type.Capacity);
            controls.AppendFormat("{0}: id={1}, parent={2}; ", type, GetDlgCtrlID(child), GetDlgCtrlID(GetParent(child)));
            if ((GetDlgCtrlID(child) == 1001 || GetDlgCtrlID(child) == 1148) && type.ToString() == "Edit") filename = child;
            if (GetDlgCtrlID(child) == 1 && type.ToString() == "Button") save = child;
            return true;
        }, IntPtr.Zero);
        if (filename == IntPtr.Zero || save == IntPtr.Zero) throw new Exception("Owned save dialog controls were not found: " + controls);
        IntPtr result;
        if (SendMessageTimeout(filename, 0x000C, IntPtr.Zero, path, 2, 2000, out result) == IntPtr.Zero || result == IntPtr.Zero)
            throw new Exception("Owned filename control rejected the export path");
        var combo = GetParent(filename);
        if (!PostMessage(GetParent(combo), 0x0111, new IntPtr((5 << 16) | (GetDlgCtrlID(combo) & 0xffff)), combo))
            throw new Exception("Owned filename change could not be published");
        if (!PostMessage(save, 0x00F5, IntPtr.Zero, IntPtr.Zero)) throw new Exception("Owned Save button could not be invoked");
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
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if($element){break}
        Start-Sleep -Milliseconds 100
    } while([DateTime]::UtcNow -lt $deadline)
    if (!$element) { throw "Control not found: $Name" }
    return $element
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

    $csv = Join-Path $work 'offline.csv'
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add('Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds')
    for($i=0;$i -lt 9000;$i++) { $lines.Add(('Z:\offline\file{0:D5}.bin,File,7,4096,0,0,1767225600' -f $i)) }
    [IO.File]::WriteAllLines($csv,$lines,[Text.UTF8Encoding]::new($false))
    (Find-Control 'File menu').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 100
    $null = [ParityInvocation]::Invoke((Find-Control 'Open scan (CSV, TSV, JSON, MFT)...'))
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do { $dialog=[ParityInvocation]::FindDialog($app.Id);Start-Sleep -Milliseconds 100 } while($dialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if($dialog -eq [IntPtr]::Zero){throw 'Open scan picker did not appear'}
    [ParityInvocation]::Save($dialog,$csv)
    Start-Sleep -Milliseconds 400
    $null=[ParityInvocation]::Invoke((Find-Control 'Open'))
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 100
        $rootText=(Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
    } while($rootText -ne 'Z:\offline' -and [DateTime]::UtcNow -lt $deadline)
    if($rootText -ne 'Z:\offline'){throw 'Offline CSV did not reopen'}
    Start-Sleep -Milliseconds 500
    (Find-Control 'Search').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 250
    $query=Find-Control 'Search query'
    $query.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('file08999')
    Start-Sleep -Milliseconds 300
    Write-Output 'Checking matching row'
    $null=Find-Control 'file08999.bin'
    $mode=Find-Control 'Search match mode'
    $mode.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Start-Sleep -Milliseconds 150
    (Find-Control 'Regex').GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    $mode.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
    Start-Sleep -Milliseconds 200
    $query.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('file08999\.bin$')
    (Find-Control 'Minimum allocated size').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('4096')
    (Find-Control 'Maximum allocated size').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('4096')
    Start-Sleep -Milliseconds 300
    Write-Output 'Checking matching row'
    $null=Find-Control 'file08999.bin'
    $query.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('[')
    Start-Sleep -Milliseconds 300
    $null=Find-Control 'Invalid search pattern or bounds.'
    Write-Output 'PASS: offline reopen, complete-index GUI search beyond preview cap, regex, allocation bounds, invalid-pattern feedback'
} finally {
    if (!$app.HasExited) {
        $null=$app.CloseMainWindow()
        if (!$app.WaitForExit(3000)) {
            try { $app.Kill() }
            catch {
                # The owned process can exit between the wait and Kill.
                # Preserve genuine cleanup failures while accepting that race.
                if (!$app.WaitForExit(1000)) { throw }
            }
        }
    }
}
