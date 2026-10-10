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
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    public static IntPtr FindDialog(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process || !IsWindowVisible(window)) return true;
            var type = new StringBuilder(128); GetClassName(window, type, type.Capacity);
            if (type.ToString() != "#32770") return true;
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
    param([string]$Name)
    $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
    $deadline=[DateTime]::UtcNow.AddSeconds(5)
    do {
        $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if ($element) {break}
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
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
    $source = Join-Path $work 'offline.csv'
    $count=9001
    $csv=[Text.StringBuilder]::new()
    $null=$csv.Append("Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds`r`nZ:\offline,Directory,$(7*$count),$(4096*$count),$count,0,`r`n")
    for($i=0;$i -lt $count;$i++) {$null=$csv.Append("Z:\offline\file-$i.ext$($i%43),File,7,4096,0,0,134116992000000000`r`n")}
    [IO.File]::WriteAllText($source,$csv.ToString())
    Open-Export 'Open scan (CSV, TSV, JSON, MFT)...'
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do { $handle=[ExportInvocation]::FindDialog($app.Id); if ($handle -ne [IntPtr]::Zero) { break }; Start-Sleep -Milliseconds 100 } while ([DateTime]::UtcNow -lt $deadline)
    if ($handle -eq [IntPtr]::Zero) {throw 'Import picker missing'}
    [ExportInvocation]::Save($handle,$source)
    $null=Find-Control 'WizTree timestamp convention'
    $null=[ExportInvocation]::Invoke((Find-Control 'Open'))
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {Start-Sleep -Milliseconds 100; $value=(Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value} while ($value -ne 'Z:\offline' -and [DateTime]::UtcNow -lt $deadline)
    if ((Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -ne 'Z:\offline') { $texts=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Text)); foreach($t in $texts){Write-Output $t.Current.Name}; throw 'Imported root did not appear'}
    Open-Export 'Export JSON...'
    $jsonPath=Join-Path $work 'reopened.json'
    Save-Export $jsonPath
    $json=Get-Content $jsonPath -Raw | ConvertFrom-Json
    if (@($json.entries | Where-Object Kind -eq File).Count -ne $count -or $json.entries[0].AllocatedBytes -ne 4096*$count) {throw 'Imported catalog could not be exported'}
    if (($json.entries | Where-Object Kind -eq File | Select-Object -First 1).ModifiedUnixSeconds -ne 1767225600) {throw 'Older CSV date was not normalized'}
    Open-Export 'Export treemap PNG...'
    $null=Find-Control 'PNG width'
    $null=[ExportInvocation]::Invoke((Find-Control 'Continue'))
    $pngPath=Join-Path $work 'treemap.png'
    Save-Export $pngPath
    Add-Type -AssemblyName System.Drawing
    $image=[System.Drawing.Bitmap]::new($pngPath)
    try {
        if ($image.Width -ne 1920 -or $image.Height -ne 1080) {throw 'Treemap export dimensions incorrect'}
        $filled=0; $samples=0
        for($x=48;$x -lt 1920;$x+=96) {for($y=27;$y -lt 1080;$y+=54) {$samples++; if($image.GetPixel($x,$y).ToArgb() -ne $image.GetPixel(0,0).ToArgb()) {$filled++}}}
        if ($filled/$samples -lt 0.9) {throw 'Dense treemap lost more than 10% of the interior area'}
        if ($image.GetPixel(960,540).ToArgb() -eq $image.GetPixel(0,0).ToArgb()) {throw 'Exported treemap is blank'}
    } finally {$image.Dispose()}
    (Find-Control 'Cleanup').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 300
    (Find-Control 'Next 50 files').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 300
    $null=Find-Control 'Showing 51-100 of 9001 files (allocated bytes)'
    (Find-Control 'Previous 50 files').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $null=Find-Control 'Showing 1-50 of 9001 files (allocated bytes)'
    Write-Output 'PASS: Offline external CSV reopened from File menu and exported without accessing source volume'
} finally {
    if (!$app.HasExited) { $null=$app.CloseMainWindow(); if (!$app.WaitForExit(3000)) { $app.Kill() } }
}
