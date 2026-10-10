param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\..\src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe"),
    [int]$WindowTimeoutSeconds = 12
)

# Automated slice of the accessibility gate: launches the app, visits each
# sidebar view, and asserts that every interactive chrome control exposes a
# UI Automation name and client controls are keyboard-focusable. Windows title-bar
# descendants are checked for invocation separately; they are outside client Tab order.
# https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-support-for-the-titlebar-control-type
# The visual parts of the a11y
# pass (Narrator announcement quality, high-contrast theme, 125-200% display
# scaling, narrow window sizes, long-path display) still need manual review.

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient

$resolved = (Resolve-Path -LiteralPath $AppPath).Path
$work = Join-Path $PSScriptRoot ("../../artifacts/ui-a11y-" + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $resolved
$start.UseShellExecute = $false
$start.EnvironmentVariables['LOCALAPPDATA'] = $work
$app = [Diagnostics.Process]::Start($start)
try {
    Start-Sleep -Seconds 4
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = [System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,"WinBlaze"),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$app.Id))
    $deadline = (Get-Date).AddSeconds($WindowTimeoutSeconds); $win = $null
    while ((Get-Date) -lt $deadline -and -not $win) {
        $win = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
        Start-Sleep -Milliseconds 300
    }
    if (-not $win) { throw "WinBlaze window not found" }

    foreach ($view in @("Explorer","Dashboard","Insights","Cleanup","SETTINGS","SUPPORT","Diagnostics")) {
        $c = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty, $view)
        $e = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
        if ($e) {
            $p = $null
            if ($e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke() }
            elseif ($e.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$p)) { $p.Select() }
            Start-Sleep -Milliseconds 400
        }
    }

    $interactive = "Button|Edit|CheckBox|ComboBox|TabItem|Hyperlink|MenuItem|Slider|RadioButton"
    $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
    $total = 0; $named = 0; $focusable = 0; $enabled = 0; $caption = 0
    $unnamed = New-Object System.Collections.Generic.List[string]
    $notFocusable = New-Object System.Collections.Generic.List[string]
    $seen = @{}
    foreach ($e in $all) {
        $ct = $e.Current.ControlType.ProgrammaticName -replace 'ControlType\.',''
        # ListItem excluded: catalog rows are content, not chrome.
        if ($ct -notmatch $interactive) { continue }
        $name = $e.Current.Name
        $key = "$ct|$name|$($e.Current.BoundingRectangle)"
        if ($seen.ContainsKey($key)) { continue }
        $seen[$key] = $true
        $total++
        if ($name -and $name.Trim().Length -gt 0) { $named++ } else { $unnamed.Add("$ct @ $($e.Current.BoundingRectangle)") }
        if ($e.Current.IsEnabled) {
            $ancestor = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($e)
            $systemCaption = $false
            while ($ancestor -and $ancestor.Current.ControlType -ne [System.Windows.Automation.ControlType]::Window) {
                if ($ancestor.Current.ControlType -eq [System.Windows.Automation.ControlType]::TitleBar) { $systemCaption = $true; break }
                $ancestor = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($ancestor)
            }
            if ($systemCaption -and $ct -eq 'Button') {
                $caption++
                $invoke = $null
                if (!$e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) { $notFocusable.Add("Caption button '$name' lacks invocation support") }
                continue
            }
            $enabled++
            if ($e.Current.IsKeyboardFocusable) { $focusable++ } elseif ($name) { $notFocusable.Add("$ct '$name'") }
        }
    }

    Write-Host "=== Accessibility (UIA) audit ==="
    Write-Host ("interactive chrome controls: {0}" -f $total)
    Write-Host ("  with automation name:      {0}" -f $named)
    Write-Host ("  enabled and focusable:     {0}/{1}" -f $focusable,$enabled)
    Write-Host ("  Windows caption buttons:  {0} (invocation checked separately)" -f $caption)

    $ok = $true
    if ($unnamed.Count -gt 0) {
        $ok = $false
        Write-Host "FAIL - controls without an automation name:"
        $unnamed | ForEach-Object { Write-Host "  - $_" }
    }
    if ($notFocusable.Count -gt 0) {
        $ok = $false
        Write-Host "FAIL - named controls not keyboard-focusable:"
        $notFocusable | ForEach-Object { Write-Host "  - $_" }
    }
    if ($ok) {
        Write-Host "PASS: observed chrome controls are named; enabled client controls are keyboard-focusable."
        if ($caption -gt 0) {
            Write-Host "PASS: observed caption buttons support invocation."
        } else {
            Write-Host "UNVERIFIED: no Windows caption buttons were exposed to this observer; caption accessibility requires separate evidence."
        }
    } else {
        throw "Accessibility audit found gaps (see above)."
    }
}
finally {
    if ($app -and -not $app.HasExited) { $app | Stop-Process -Force -ErrorAction SilentlyContinue }
}
