param(
    [ValidateSet('Horizontal strips','Flame graph','Sunburst')][string]$Layout = 'Horizontal strips',
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
            if (!IsWindowVisible(window)) return true;
            var title = new StringBuilder(512); GetWindowText(window, title, title.Capacity);
            if (title.ToString() != "Save As" && title.ToString() != "Move file to a new path (existing files are never overwritten)") return true;
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
        var deadline = DateTime.UtcNow.AddSeconds(5);
        do {
            EnumChildWindows(dialog, (child, parameter) => {
                var type = new StringBuilder(128); GetClassName(child, type, type.Capacity);
                if (GetDlgCtrlID(child) == 1001 && type.ToString() == "Edit") filename = child;
                if (GetDlgCtrlID(child) == 1 && type.ToString() == "Button") save = child;
                return true;
            }, IntPtr.Zero);
            if (filename != IntPtr.Zero && save != IntPtr.Zero) break;
            System.Threading.Thread.Sleep(50);
        } while (DateTime.UtcNow < deadline);
        if (filename == IntPtr.Zero || save == IntPtr.Zero) throw new Exception("Owned PNG save controls missing");
        IntPtr result;
        if (SendMessageTimeout(filename, 0x000C, IntPtr.Zero, path, 2, 2000, out result) == IntPtr.Zero || result == IntPtr.Zero)
            throw new Exception("Owned PNG filename rejected the path");
        var combo = GetParent(filename);
        if (!PostMessage(GetParent(combo), 0x0111, new IntPtr((5 << 16) | (GetDlgCtrlID(combo) & 0xffff)), combo))
            throw new Exception("Owned PNG filename change could not be published");
        if (!PostMessage(save, 0x00F5, IntPtr.Zero, IntPtr.Zero)) throw new Exception("Owned PNG Save button could not be invoked");
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

    $null=Find-Control 'Treemap size basis'
    (Find-Control 'Treemap layout').GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Start-Sleep -Milliseconds 100
    (Find-Control $Layout).GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    (Find-Control 'Treemap layout').GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
    (Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($fixture)
    (Find-Control 'Start scan').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 500
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    while(!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
    (Find-Control 'File menu').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 100
    $null=[ExportInvocation]::Invoke((Find-Control 'Export treemap PNG...'))
    Start-Sleep -Milliseconds 300
    (Find-Control 'PNG width').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('640')
    (Find-Control 'PNG height').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('360')
    $null=Find-Control 'PNG size basis'
    $null=Find-Control 'PNG free and unaccounted space'
    (Find-Control 'PNG grayscale').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    $null=[ExportInvocation]::Invoke((Find-Control 'Continue'))
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do{$dialog=[ExportInvocation]::FindDialog($app.Id);Start-Sleep -Milliseconds 100}while($dialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if($dialog -eq [IntPtr]::Zero){throw 'PNG save picker absent'}
    $png=Join-Path $work 'configured.png'
    [ExportInvocation]::Save($dialog,$png)
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do{Start-Sleep -Milliseconds 100}while(!(Test-Path $png)-and [DateTime]::UtcNow -lt $deadline)
    if(!(Test-Path $png)){throw 'PNG not written'}
    Add-Type -AssemblyName System.Drawing
    $image=[Drawing.Image]::FromFile($png)
    try {
        if($image.Width -ne 640 -or $image.Height -ne 360){throw 'Configured PNG dimensions ignored'}
        $colors=[Collections.Generic.HashSet[int]]::new()
        for($y=0;$y -lt $image.Height;$y+=17) {
            for($x=0;$x -lt $image.Width;$x+=17) {
                $pixel=$image.GetPixel($x,$y)
                $null=$colors.Add($pixel.ToArgb())
                if($pixel.R -ne $pixel.G -or $pixel.R -ne $pixel.B){throw 'Grayscale PNG option ignored'}
            }
        }
        if($colors.Count -lt 2){throw "Selected layout produced an empty PNG"}
    } finally {$image.Dispose()}
    Write-Output 'PASS: treemap size-mode control and configured PNG dimensions'
}finally{
    if(!$app.HasExited){$null=$app.CloseMainWindow();if(!$app.WaitForExit(3000)){try{$app.Kill()}catch{if(!$app.WaitForExit(2000)){throw}}}}
}
