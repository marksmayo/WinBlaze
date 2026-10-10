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
public static class ExportInvocation {
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int size);
    public static IntPtr FindDialog(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process) return true;
            var title = new StringBuilder(512); GetWindowText(window, title, title.Capacity);
            if (title.ToString() != "Save As" || !IsWindowVisible(window)) return true;
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
            if (GetDlgCtrlID(child) == 1001 && type.ToString() == "Edit") filename = child;
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
$work = Join-Path $repo ('artifacts/ui-export-features-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
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
    param([string]$Name, [int]$TimeoutMilliseconds = 0)
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
    $controlDeadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if ($element -or [DateTime]::UtcNow -ge $controlDeadline) { break }
        Start-Sleep -Milliseconds 50
    } while ($true)
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
    [IO.File]::WriteAllText((Join-Path $fixture 'sample.BIN'), '1234567')
    (Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($fixture)
    (Find-Control 'Start scan').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 100
        $events = Join-Path $work 'appdata/WinBlaze/logs/events.jsonl'
        $done = (Test-Path $events) -and (Get-Content $events -Raw) -match 'completed'
    } while (!$done -and [DateTime]::UtcNow -lt $deadline)
    # The scan controls become enabled only after completion.
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    function Open-Export([string]$name) {
        (Find-Control 'File menu').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $null = [ExportInvocation]::Invoke((Find-Control $name -TimeoutMilliseconds 8000))
        Start-Sleep -Milliseconds 250
    }
    function Save-Export([string]$destination) {
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            $handle = [ExportInvocation]::FindDialog($app.Id)
            if ($handle -ne [IntPtr]::Zero) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if ($handle -eq [IntPtr]::Zero) { throw 'Save dialog did not appear.' }
        Start-Sleep -Milliseconds 500
        [ExportInvocation]::Save($handle, $destination)
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do { Start-Sleep -Milliseconds 100 } while (!(Test-Path $destination) -and [DateTime]::UtcNow -lt $deadline)
        if (!(Test-Path $destination)) {
            if ([ExportInvocation]::FindDialog($app.Id) -eq [IntPtr]::Zero) {
                $texts=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Text))
                foreach ($t in $texts) { if ($t.Current.Name -like '*export*') { Write-Output $t.Current.Name } }
            }
            throw 'Export file did not appear.'
        }
        Start-Sleep -Milliseconds 300
    }
    Open-Export 'Export JSON...'
    $jsonPath = Join-Path $work 'scan.json'
    Save-Export $jsonPath
    $json = Get-Content $jsonPath -Raw | ConvertFrom-Json
    if ($json.schema_version -ne 1 -or @($json.entries | Where-Object Kind -eq File).Count -ne 2) { throw 'UI JSON export lost catalog files.' }
    Open-Export 'Export filtered CSV...'
    (Find-Control 'Export filename patterns' -TimeoutMilliseconds 8000).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('*.bIn;no-match?.txt')
    (Find-Control 'Export minimum bytes').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('7')
    (Find-Control 'Export maximum bytes').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('7')
    $null = [ExportInvocation]::Invoke((Find-Control 'Continue'))
    $csvPath = Join-Path $work 'filtered.csv'
    Save-Export $csvPath
    $csv = @(Import-Csv $csvPath)
    if ($csv.Count -ne 1 -or $csv[0].Path -notlike '*sample.BIN' -or $csv[0].Kind -ne 'File') { throw 'UI filtered export selected the wrong files.' }
    Open-Export 'Export filtered CSV...'
    (Find-Control 'Export filename patterns' -TimeoutMilliseconds 8000).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('bad/path')
    $null = [ExportInvocation]::Invoke((Find-Control 'Continue'))
    $guidanceDeadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $texts = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Text))
        $guidance = @($texts | Where-Object { $_.Current.Name -like 'Check export filters*' })
        if ($guidance.Count -or [DateTime]::UtcNow -ge $guidanceDeadline) { break }
        Start-Sleep -Milliseconds 50
    } while ($true)
    if (!$guidance.Count) { throw 'Invalid export filter guidance missing.' }
    if ([ExportInvocation]::FindDialog($app.Id) -ne [IntPtr]::Zero) { throw 'Invalid export filter opened a save dialog.' }
    Write-Output 'PASS: UI JSON saved complete catalog; filtered CSV selected one file; invalid filter rejected'
} finally {
    if (!$app.HasExited) { $null=$app.CloseMainWindow(); if (!$app.WaitForExit(3000)) { $app.Kill() } }
}
