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

    (Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($fixture)
    (Find-Control 'Start scan').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 500
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    while(!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
    (Find-Control 'File menu').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 100
    $null=[ExportInvocation]::Invoke((Find-Control 'Export detailed CSV...'))
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do{$dialog=[ExportInvocation]::FindDialog($app.Id);Start-Sleep -Milliseconds 100}while($dialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if($dialog -eq [IntPtr]::Zero){throw 'Detailed CSV save picker absent'}
    $output=Join-Path $work 'detailed.csv'
    [ExportInvocation]::Save($dialog,$output)
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do{Start-Sleep -Milliseconds 100}while(!(Test-Path $output)-and [DateTime]::UtcNow -lt $deadline)
    if(!(Test-Path $output)){throw 'Detailed CSV not written'}
    $records=@(Import-Csv -LiteralPath $output) | Where-Object Kind -eq File
    if(@($records).Count -ne 1 -or !$records.Owner -or !$records.FileIdentity -or !$records.VolumeIdentity -or !$records.CreatedUnixSeconds -or !$records.AccessedUnixSeconds -or !$records.Attributes -or !$records.LinkCount){
        throw 'Detailed CSV lacks actual owner or physical identity'
    }
    Write-Output 'PASS: GUI detailed CSV exports current owner and physical identities'
}finally{
    if(!$app.HasExited){$null=$app.CloseMainWindow();if(!$app.WaitForExit(3000)){try{$app.Kill()}catch{if(!$app.WaitForExit(2000)){throw}}}}
}
