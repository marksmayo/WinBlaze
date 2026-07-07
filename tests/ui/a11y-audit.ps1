param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\..\src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe"),
    [int]$WindowTimeoutSeconds = 12
)

# Automated slice of the accessibility gate: launches the app, visits each
# sidebar view, and asserts that every interactive chrome control exposes a
# UI Automation name and is keyboard-focusable. The visual parts of the a11y
# pass (Narrator announcement quality, high-contrast theme, 125-200% display
# scaling, narrow window sizes, long-path display) still need manual review.

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient

$resolved = (Resolve-Path -LiteralPath $AppPath).Path
Get-Process WinBlaze.UI -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$app = Start-Process -FilePath $resolved -PassThru
try {
    Start-Sleep -Seconds 4
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, "WinBlaze")
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
    $total = 0; $named = 0; $focusable = 0
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
        if ($e.Current.IsKeyboardFocusable) { $focusable++ } elseif ($name) { $notFocusable.Add("$ct '$name'") }
    }

    Write-Host "=== Accessibility (UIA) audit ==="
    Write-Host ("interactive chrome controls: {0}" -f $total)
    Write-Host ("  with automation name:      {0}" -f $named)
    Write-Host ("  keyboard-focusable:        {0}" -f $focusable)

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
        Write-Host "PASS: all interactive chrome controls are named and keyboard-focusable."
    } else {
        throw "Accessibility audit found gaps (see above)."
    }
}
finally {
    if ($app -and -not $app.HasExited) { $app | Stop-Process -Force -ErrorAction SilentlyContinue }
}
