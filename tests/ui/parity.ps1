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
            if ((title.ToString() != "Save As" && title.ToString() != "Move file to a new path (existing files are never overwritten)") || !IsWindowVisible(window)) return true;
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
    $controlDeadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if ($element) { break }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $controlDeadline)
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
    [IO.File]::WriteAllText((Join-Path $fixture 'duplicate.BIN'), '1234567')
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
        Start-Sleep -Milliseconds 150
        $null = [ExportInvocation]::Invoke((Find-Control $name))
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
    Open-Export 'Export duplicate report...'
    $dupes=Join-Path $work 'duplicates.csv';Save-Export $dupes
    $rows=@(Import-Csv $dupes);if($rows.Count -ne 2 -or @($rows|Where-Object Status -ne verified).Count){throw 'Content duplicate groups were not verified'}
    Open-Export 'Export permissions report...'
    $acl=Join-Path $work 'permissions.csv';Save-Export $acl
    $rows=@(Import-Csv $acl);if($rows.Count -ne 4 -or @($rows|Where-Object Status -ne ok).Count){throw 'ACL report is incomplete'}
    (Find-Control 'File menu').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke();Start-Sleep -Milliseconds 150
    (Find-Control 'Monitor file-system changes').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    Start-Sleep -Milliseconds 250
    $before=@(Get-Content $events|Where-Object {$_ -match 'scanner.completed'}).Count
    [IO.File]::WriteAllText((Join-Path $fixture 'watch-added.txt'),'watch update')
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {Start-Sleep -Milliseconds 100;$after=@(Get-Content $events|Where-Object {$_ -match 'scanner.completed'}).Count}while($after -le $before -and [DateTime]::UtcNow -lt $deadline)
    if($after -le $before){throw 'Automatic change monitoring did not rescan'}
    while(!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
    Open-Export 'Export JSON...';$json=Join-Path $work 'watched.json';Save-Export $json
    if(@((Get-Content $json -Raw|ConvertFrom-Json).entries|Where-Object Kind -eq File).Count -ne 4){throw 'Watcher refresh did not publish the added file'}
    (Find-Control 'File menu').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke();Start-Sleep -Milliseconds 150
    (Find-Control 'Monitor file-system changes').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    (Find-Control 'Cleanup').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke();Start-Sleep -Milliseconds 300
    $source=Join-Path $fixture 'sample.BIN';$destination=Join-Path $work 'moved.BIN'
    $null=[ExportInvocation]::Invoke((Find-Control ('Move file: '+$source)));Start-Sleep -Milliseconds 300
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {
        $dialog=[ExportInvocation]::FindDialog($app.Id)
        if($dialog -ne [IntPtr]::Zero){break}
        Start-Sleep -Milliseconds 50
    } while([DateTime]::UtcNow -lt $deadline)
    if($dialog -eq [IntPtr]::Zero){throw 'Move destination picker absent'}
    [ExportInvocation]::Save($dialog,$destination);Start-Sleep -Milliseconds 400
    $null=[ExportInvocation]::Invoke((Find-Control 'Cancel'));Start-Sleep -Milliseconds 200
    if(!(Test-Path $source)-or (Test-Path $destination)){throw 'Cancel changed a source file'}
    $null=[ExportInvocation]::Invoke((Find-Control ('Move file: '+$source)));Start-Sleep -Milliseconds 300
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {
        $dialog=[ExportInvocation]::FindDialog($app.Id)
        if($dialog -ne [IntPtr]::Zero){break}
        Start-Sleep -Milliseconds 50
    } while([DateTime]::UtcNow -lt $deadline)
    if($dialog -eq [IntPtr]::Zero){throw 'Second move destination picker absent'}
    [ExportInvocation]::Save($dialog,$destination);Start-Sleep -Milliseconds 400
    $null=[ExportInvocation]::Invoke((Find-Control 'Move file'))
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do{Start-Sleep -Milliseconds 100}while(!(Test-Path $destination)-and [DateTime]::UtcNow -lt $deadline)
    if(!(Test-Path $destination)-or (Test-Path $source)){throw 'Confirmed move failed'}
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    while(!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
    Start-Sleep -Milliseconds 300
    (Find-Control 'Cleanup').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke();Start-Sleep -Milliseconds 200
    $recycle=Join-Path $fixture 'duplicate.BIN'
    $null=[ExportInvocation]::Invoke((Find-Control ('Recycle file: '+$recycle)));Start-Sleep -Milliseconds 300
    $null=[ExportInvocation]::Invoke((Find-Control 'Recycle file'))
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do{Start-Sleep -Milliseconds 100}while((Test-Path $recycle)-and [DateTime]::UtcNow -lt $deadline)
    if(Test-Path $recycle){throw 'Confirmed recycle failed'}
    Write-Output 'PASS: duplicate/ACL exports, automatic watcher publication, move cancellation/success and confirmed Recycle Bin action'

} finally {
    if (!$app.HasExited) { $null=$app.CloseMainWindow(); if (!$app.WaitForExit(3000)) { $app.Kill() } }
}
