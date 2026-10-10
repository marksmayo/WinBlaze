param(
    [string]$AppPath = (Join-Path $PSScriptRoot '../../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'),
    [ValidateSet('empty','invalid','quoted','browse','expand','drive','window','version','guidance','cancel')][string]$Case = 'empty',
    [string]$ScreenshotPath = '',
    [switch]$RequireVisibleTree,
    [switch]$WindowOnlyScreenshot
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies UIAutomationClient,UIAutomationTypes -TypeDefinition @'
using System.Threading.Tasks;
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Windows.Automation;
public static class FolderPickerInvocation {
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] private static extern bool MoveWindow(IntPtr window, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] private static extern bool PrintWindow(IntPtr window, IntPtr context, uint flags);
    public static bool Capture(IntPtr window, int process, IntPtr context) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (owner != process) throw new Exception("Screenshot target owner mismatch");
        return PrintWindow(window, context, 2);
    }
    public static void Resize(IntPtr window) { MoveWindow(window, 50, 50, 900, 700, true); }
    public static void Cancel(IntPtr window) { PostMessage(window, 0x0111, new IntPtr(2), IntPtr.Zero); }
    public static IntPtr FindDialog(int process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process) return true;
            var title = new StringBuilder(512); GetWindowText(window, title, title.Capacity);
            if (title.ToString() != "Choose a folder to scan") return true;
            result = window; return false;
        }, IntPtr.Zero);
        return result;
    }
    public static Task Invoke(AutomationElement element) {
        return Task.Factory.StartNew(() => ((InvokePattern)element.GetCurrentPattern(InvokePattern.Pattern)).Invoke());
    }
}
'@
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$work = Join-Path $repo ('artifacts/ui-path-ux-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$fixture = Join-Path $work 'Folder with spaces'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'example.txt') -Value 'fixture'
if ($Case -eq 'expand') {
    New-Item -ItemType Directory -Path (Join-Path $fixture 'Documents') | Out-Null
    Set-Content -LiteralPath (Join-Path $fixture 'Documents/nested.txt') -Value 'nested fixture'
}
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
    $path = Find-Control 'Scan root path'
    $scan = Find-Control 'Start scan'
    $value = $path.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    if ($Case -eq 'cancel') {
        # A standard-user volume scan is long enough to exercise active controls.
        # Cancel immediately; no complete volume scan or persistent user cache is required.
        $value.SetValue($env:SystemDrive + '\')
        $scan.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        if (!(Find-Control 'Cancel').Current.IsEnabled) { throw 'Cancel must be available during a scan.' }
        foreach ($name in @('Scan root path','Scan drive','Browse for folder','Start scan','Incremental rescan')) {
            if ((Find-Control $name).Current.IsEnabled) { throw "Scan inputs must be locked while scanning: $name" }
        }
        (Find-Control 'Cancel').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while (!$scan.Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if (!$scan.Current.IsEnabled -or !(Find-Control 'Browse for folder').Current.IsEnabled -or !$path.Current.IsEnabled -or (Find-Control 'Cancel').Current.IsEnabled) { throw 'Cancelling must restore the directory controls and disable Cancel.' }
        $ackDeadline = [DateTime]::UtcNow.AddSeconds(8)
        do {
            $texts = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Text))
            $acknowledged = $texts | Where-Object { $_.Current.Name -like '*Scan cancelled.*' }
            if ($acknowledged) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $ackDeadline)
        if (!$acknowledged) { throw 'The scan must acknowledge cancellation.' }
    }
    if ($Case -eq 'guidance') {
        $textCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Text)
        $texts = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,$textCondition)
        foreach ($text in $texts) {
            if (!$text.Current.IsOffscreen -and $text.Current.Name -match 'virtualized ListView|SwapChainPanel|D3D feature') { throw 'RED: normal Explorer guidance must describe user actions, with engine details in Diagnostics.' }
        }
        $value.SetValue($fixture)
        $scan.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        (Find-Control 'Diagnostics').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            $texts = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,$textCondition)
            $metrics = $texts | Where-Object { $_.Current.Name -like '*layout=*labels=*' }
            if ($metrics) { break }; Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if (!$metrics) { throw 'Diagnostics must retain treemap metrics for benchmarking.' }
    }
    if ($Case -eq 'version') {
        $null = Find-Control 'Version 0.9.1'
    }
    if ($Case -eq 'window') {
        $presentation = $window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
        if ($presentation.Current.WindowVisualState -ne [System.Windows.Automation.WindowVisualState]::Maximized) { throw 'WinBlaze must launch maximized.' }
        $presentation.SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Normal)
        [FolderPickerInvocation]::Resize([IntPtr]$window.Current.NativeWindowHandle)
        Start-Sleep -Milliseconds 500
        $bounds = $window.Current.BoundingRectangle
        foreach ($name in @('Scan root path','Scan drive','Browse for folder','Start scan','Incremental rescan','Cancel','Search','Diagnostics')) {
            $control = Find-Control $name
            $rectangle = $control.Current.BoundingRectangle
            if ($control.Current.IsOffscreen -or $rectangle.Width -le 0 -or $rectangle.Left -lt $bounds.Left -or $rectangle.Right -gt $bounds.Right -or $rectangle.Top -lt $bounds.Top -or $rectangle.Bottom -gt $bounds.Bottom) { throw "Directory control is clipped in a restored window: $name ($rectangle)" }
        }
    }
    if ($Case -eq 'drive') {
        $value.SetValue($fixture)
        $drive = Find-Control 'Scan drive'
        $drive.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
        Start-Sleep -Milliseconds 200
        $items = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Drive C:'))
        $selected = $false
        foreach ($item in $items) {
            $selection = $null
            if ($item.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$selection)) { $selection.Select(); $selected = $true; break }
        }
        if (!$selected) { throw 'Drive C: cannot be selected.' }
        Start-Sleep -Milliseconds 200
        if ($value.Current.Value -ne 'C:\') { throw 'Choosing a drive must update the scan root.' }
        if ((Find-Control 'Cancel').Current.IsEnabled) { throw 'Choosing a drive must not start a scan.' }
    }
    if ($Case -eq 'expand') {
        $value.SetValue($fixture)
        $scan.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            $expand = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Expand Documents'))
            if ($expand) { break }; Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if (!$expand) { throw 'RED: a folder needs a named, clickable expansion control.' }
        if ($RequireVisibleTree) {
            if ($expand.Current.ProcessId -ne $app.Id -or $expand.Current.IsOffscreen -or
                $expand.Current.BoundingRectangle.Height -le 0) {
                throw 'Folder expand affordance is not visible in the owned tree'
            }
            $viewport = Find-Control 'Virtualized catalog tree rows'
            $itemBounds = $expand.Current.BoundingRectangle
            $viewportBounds = $viewport.Current.BoundingRectangle
            if ($itemBounds.Top -lt $viewportBounds.Top -or $itemBounds.Bottom -gt $viewportBounds.Bottom) {
                throw 'Folder expand affordance is clipped by the tree viewport'
            }
        }
        $expand.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 300
        $null = Find-Control 'Collapse Documents'
        $childDeadline = [DateTime]::UtcNow.AddSeconds(8)
        do {
            $children = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
            $nested = $children | Where-Object { $_.Current.Name -like 'nested.txt, File,*' }
            if ($nested) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $childDeadline)
        if (!$nested) { throw 'Expanding a folder must reveal its files.' }
        (Find-Control 'Collapse Documents').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 300
        $null = Find-Control 'Expand Documents'
        $childDeadline = [DateTime]::UtcNow.AddSeconds(8)
        do {
            $children = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
            $nested = $children | Where-Object { $_.Current.Name -like 'nested.txt, File,*' }
            if (!$nested) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $childDeadline)
        if ($nested) { throw 'Collapsing a folder must hide its files.' }
    }
    if ($Case -eq 'browse') {
        $value.SetValue($fixture)
        $invoke = [FolderPickerInvocation]::Invoke((Find-Control 'Browse for folder'))
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            $dialogHandle = [FolderPickerInvocation]::FindDialog($app.Id)
            if ($dialogHandle -ne [IntPtr]::Zero) { break }; Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if ($dialogHandle -eq [IntPtr]::Zero) { throw 'Folder picker did not open.' }
        # Native dialog providers can block UIA while the owner is inside Show().
        # Post IDCANCEL only to the titled dialog belonging to this test process.
        [FolderPickerInvocation]::Cancel($dialogHandle)
        if (!$invoke.Wait(3000)) { throw 'Folder picker did not close.' }
        if ($value.Current.Value -ne $fixture) { throw 'Cancelling the picker must preserve the selected directory.' }
        if (!$scan.Current.IsEnabled) { throw 'Cancelling the picker must leave the directory ready to scan.' }
    }
    if ($Case -eq 'empty') {
        $value.SetValue('')
        $deadline = [DateTime]::UtcNow.AddSeconds(2)
        while ($scan.Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if ($scan.Current.IsEnabled) { throw 'RED: a blank directory must disable Start scan.' }
    }
    if ($Case -eq 'invalid') {
        $value.SetValue((Join-Path $work 'does-not-exist'))
        $scan.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 500
        $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty,'DirectoryValidation')
        $feedback = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
        if (!$feedback -or $feedback.Current.Name -notlike '*existing folder*') { throw 'RED: an invalid directory must show actionable inline feedback.' }
        if ((Find-Control 'Cancel').Current.IsEnabled) { throw 'Invalid input must not start a scan.' }
    }
    if ($Case -eq 'quoted') {
        $value.SetValue('  "' + $fixture + '"  ')
        $scan.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 500
        if ($value.Current.Value -ne $fixture) { throw 'RED: pasted quoted folder paths must normalize and scan successfully.' }
        (Find-Control 'Diagnostics').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $deadline=[DateTime]::UtcNow.AddSeconds(10)
        do {
            $condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Text)
            $texts=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)
            $success=$false
            foreach ($text in $texts) { if ($text.Current.Name -like 'Correctness:*issues=0*files=1*directories=1*') { $success=$true; break } }
            if ($success) { break }; Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if (!$success) { throw 'Quoted folder scan did not produce the expected fixture totals.' }
    }
    if ($ScreenshotPath) {
        Add-Type -AssemblyName System.Drawing
        $bounds = $window.Current.BoundingRectangle
        $bitmap = [Drawing.Bitmap]::new([int]$bounds.Width,[int]$bounds.Height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $captured = $false
            if (!$WindowOnlyScreenshot) {
                try { $graphics.CopyFromScreen([int]$bounds.Left,[int]$bounds.Top,0,0,$bitmap.Size); $captured = $true }
                catch { $captured = $false }
            }
            if (!$captured) {
                $context = $graphics.GetHdc()
                try {
                    if (![FolderPickerInvocation]::Capture([IntPtr]$window.Current.NativeWindowHandle,$app.Id,$context)) { throw 'Windows could not capture the app window.' }
                } finally { $graphics.ReleaseHdc($context) }
            }
            $bitmap.Save([IO.Path]::GetFullPath($ScreenshotPath))
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
    Write-Output "PASS: $Case directory UX behavior"
} finally {
    if (!$app.HasExited) { $null=$app.CloseMainWindow(); if (!$app.WaitForExit(3000)) { $app.Kill() } }
}
