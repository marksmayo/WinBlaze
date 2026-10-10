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
    $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
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

    New-Item -ItemType Directory -Path (Join-Path $fixture 'skip') | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'skip/hidden.txt'),'excluded')
    (Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($fixture)
    (Find-Control 'Excluded scan paths').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('skip')
    (Find-Control 'Included file patterns').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('*.txt')
    (Find-Control 'Start scan').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 500
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    while(!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
    (Find-Control 'Cleanup').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 300
    $null=Find-Control ('Rename file: '+(Join-Path $fixture 'example.txt'))
    $condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,('Rename file: '+(Join-Path $fixture 'skip/hidden.txt')))
    if($window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)){throw 'Excluded file is present in results'}
    Write-Output 'PASS: scan rules exclude a subtree before catalog publication'
}finally{
    if(!$app.HasExited){$null=$app.CloseMainWindow();if(!$app.WaitForExit(3000)){$app.Kill()}}
}
