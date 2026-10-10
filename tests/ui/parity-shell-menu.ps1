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
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static void PostOwned(IntPtr window, int process, uint message) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (owner != process) throw new Exception("Shell test target owner mismatch");
        if (!PostMessage(window, message, IntPtr.Zero, IntPtr.Zero)) throw new Exception("Owned shell test message failed");
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int size);
    public static IntPtr FindDialog(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process || !IsWindowVisible(window)) return true;
            var title = new StringBuilder(512); GetWindowText(window, title, title.Capacity);
            if (title.ToString() != "Save As" && title.ToString() != "Move file to a new path (existing files are never overwritten)") return true;
            result = window; return false;
        }, IntPtr.Zero);
        return result;
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window, StringBuilder text, int size);
    public static IntPtr FindMenu(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process || !IsWindowVisible(window)) return true;
            var name = new StringBuilder(256); GetClassName(window, name, name.Capacity);
            if (name.ToString() != "#32768") return true;
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
$folder=Join-Path $fixture 'old folder'
New-Item -ItemType Directory -Path $folder | Out-Null
Set-Content -LiteralPath (Join-Path $folder 'child.txt') -Value 'child'
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

    (Find-Control 'Scan root path').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($fixture)
    (Find-Control 'Start scan').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 500
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    while(!(Find-Control 'Start scan').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
    $source=Join-Path $fixture 'example.txt'
    $before=(Get-FileHash $source).Hash
    $null=[ExportInvocation]::Invoke((Find-Control ('Windows menu: '+$source)))
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {
        Start-Sleep -Milliseconds 100
        $menu=[ExportInvocation]::FindMenu($app.Id)
    } while($menu -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if($menu -eq [IntPtr]::Zero){throw 'Native Windows shell popup did not open'}
    [ExportInvocation]::PostOwned([IntPtr]$window.Current.NativeWindowHandle,$app.Id,0x001F)
    $deadline=[DateTime]::UtcNow.AddSeconds(5)
    do {
        if ([ExportInvocation]::FindMenu($app.Id) -eq [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ([ExportInvocation]::FindMenu($app.Id) -ne [IntPtr]::Zero) { throw 'Owned shell popup did not cancel' }
    if((Get-FileHash $source).Hash -ne $before){throw 'Canceling the shell menu changed content'}
    [ExportInvocation]::PostOwned([IntPtr]$window.Current.NativeWindowHandle,$app.Id,0x0010)
    if (!$app.WaitForExit(10000) -or $app.ExitCode -ne 0) { throw 'Shell menu test failed normal shutdown' }
    [ordered]@{binary_sha256=(Get-FileHash -LiteralPath $AppPath).Hash;popup_observed=$true;owned_cancel=$true;fixture_preserved=$true;exit_code=$app.ExitCode} |
        ConvertTo-Json | Set-Content (Join-Path $work 'shell-menu-result.json')
    Write-Output "PASS: native Windows shell popup opens; owned cancel preserves contents; normal shutdown; evidence=$work"
}finally{
    if(!$app.HasExited){$null=$app.CloseMainWindow();if(!$app.WaitForExit(3000)){$app.Kill()}}
}
