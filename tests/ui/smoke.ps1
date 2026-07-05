param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [string]$FixturePath = "C:\tmp\WinBlazeSmoke",
    [string]$CancelFixturePath = "C:\tmp\WinBlazeSmokeCancel",
    [int]$WindowTimeoutSeconds = 10,
    [int]$ScanWaitSeconds = 8
)

$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

function Find-TextLike {
    param(
        [System.Windows.Automation.AutomationElement]$Window,
        [string]$Pattern,
        [int]$TimeoutSeconds = 8
    )

    $textCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Text)
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $texts = $Window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textCondition)
        foreach ($text in $texts) {
            if ($text.Current.Name -like $Pattern) {
                return $text.Current.Name
            }
        }
        Start-Sleep -Milliseconds 500
    }

    throw "Text not found: $Pattern"
}

function Invoke-Button {
    param(
        [System.Windows.Automation.AutomationElement]$Window,
        [string]$Name,
        [bool]$Required = $true
    )

    $buttonCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    $matches = $Window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $buttonCondition)
    foreach ($match in $matches) {
        $invokePattern = $null
        if ($match.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$invokePattern)) {
            $invokePattern.Invoke()
            Start-Sleep -Milliseconds 500
            return $true
        }
    }

    if ($Required) {
        throw "Button not found: $Name"
    }
    return $false
}

function Toggle-CheckBox {
    param(
        [System.Windows.Automation.AutomationElement]$Window,
        [string]$Name
    )

    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    $element = $Window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    Assert-True ($null -ne $element) "CheckBox not found: $Name"

    $togglePattern = $null
    Assert-True ($element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$togglePattern)) "Toggle pattern not available: $Name"
    $togglePattern.Toggle()
    Start-Sleep -Milliseconds 300
}

function Get-WinBlazeWindow {
    param([int]$TimeoutSeconds)

    $desktop = [System.Windows.Automation.AutomationElement]::RootElement
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        "WinBlaze")
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $window = $desktop.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
        if ($window) {
            return $window
        }
        Start-Sleep -Milliseconds 300
    }

    throw "WinBlaze window not found"
}

$resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path
Assert-True (Test-Path -LiteralPath $resolvedAppPath) "App executable not found: $AppPath"

if (Test-Path -LiteralPath $FixturePath) {
    Remove-Item -LiteralPath $FixturePath -Recurse -Force
}
if (Test-Path -LiteralPath $CancelFixturePath) {
    Remove-Item -LiteralPath $CancelFixturePath -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $FixturePath | Out-Null
Set-Content -Path (Join-Path $FixturePath "alpha.txt") -Value ("alpha" * 1000)
New-Item -ItemType Directory -Force -Path (Join-Path $FixturePath "beta") | Out-Null
Set-Content -Path (Join-Path $FixturePath "beta\gamma.bin") -Value ("gamma" * 5000)

New-Item -ItemType Directory -Force -Path $CancelFixturePath | Out-Null
for ($index = 0; $index -lt 300; $index++) {
    Set-Content -Path (Join-Path $CancelFixturePath ("cancel-{0:D4}.txt" -f $index)) -Value ("cancel" * 200)
}

$startTime = Get-Date
$old = Get-Process WinBlaze.UI -ErrorAction SilentlyContinue
if ($old) {
    $old | Stop-Process -Force
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$app = Start-Process -FilePath $resolvedAppPath -PassThru
try {
    Start-Sleep -Seconds 3
    $window = Get-WinBlazeWindow -TimeoutSeconds $WindowTimeoutSeconds

    Find-TextLike -Window $window -Pattern "*virtualized ListView containers*" -TimeoutSeconds 8 | Out-Null
    Invoke-Button -Window $window -Name "Treemap" -Required $false | Out-Null
    $treemapStatus = Find-TextLike -Window $window -Pattern "*layout=*labels=*" -TimeoutSeconds 10

    Invoke-Button -Window $window -Name "Overview" -Required $false | Out-Null
    $editCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit)
    $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    Assert-True ($null -ne $edit) "Root path edit not found"
    $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($FixturePath)

    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    Start-Sleep -Seconds $ScanWaitSeconds

    # Diagnostics are hidden by default in the High Velocity layout;
    # reveal them before asserting on their text.
    Invoke-Button -Window $window -Name "Diagnostics" -Required $false | Out-Null
    $correctness = Find-TextLike -Window $window -Pattern "Correctness:*totals*" -TimeoutSeconds 8
    Assert-True ($correctness -like "*issues=0*") "Correctness diagnostics did not report zero issues: $correctness"

    Set-Content -Path (Join-Path $FixturePath "beta\delta.txt") -Value ("delta" * 200)
    Invoke-Button -Window $window -Name "Incremental rescan" | Out-Null
    $incrementalCorrectness = Find-TextLike -Window $window -Pattern "Correctness:*incremental added=1*files=3*directories=2*" -TimeoutSeconds 8
    Assert-True ($incrementalCorrectness -like "*issues=0*") "Incremental correctness diagnostics did not report zero issues: $incrementalCorrectness"

    Start-Sleep -Milliseconds 1200
    Add-Content -LiteralPath (Join-Path $FixturePath "alpha.txt") -Value "modified"
    Invoke-Button -Window $window -Name "Incremental rescan" | Out-Null
    $modifyCorrectness = Find-TextLike -Window $window -Pattern "Correctness:*incremental added=0*removed=0*modified=1*files=3*directories=2*" -TimeoutSeconds 8
    Assert-True ($modifyCorrectness -like "*issues=0*") "Incremental modify diagnostics did not report zero issues: $modifyCorrectness"

    Remove-Item -LiteralPath (Join-Path $FixturePath "beta\delta.txt") -Force
    Invoke-Button -Window $window -Name "Incremental rescan" | Out-Null
    $removeCorrectness = Find-TextLike -Window $window -Pattern "Correctness:*incremental added=0*removed=1*modified=0*files=2*directories=2*" -TimeoutSeconds 8
    Assert-True ($removeCorrectness -like "*issues=0*") "Incremental remove diagnostics did not report zero issues: $removeCorrectness"

    Invoke-Button -Window $window -Name "Search" -Required $false | Out-Null
    $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    Assert-True ($null -ne $edit) "Search/root edit not found after scan"
    $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue("gamma")
    Find-TextLike -Window $window -Pattern "*gamma*" -TimeoutSeconds 8 | Out-Null

    Invoke-Button -Window $window -Name "Diagnostics" -Required $false | Out-Null
    $frameStatus = Find-TextLike -Window $window -Pattern "*frames=*last frame=*peak frame=*" -TimeoutSeconds 8
    Find-TextLike -Window $window -Pattern "Issue drill-down:*errors=0*skipped=0*last=none*" -TimeoutSeconds 8 | Out-Null
    Toggle-CheckBox -Window $window -Name "Developer diagnostics"
    Find-TextLike -Window $window -Pattern "*Developer diagnostics hidden.*" -TimeoutSeconds 8 | Out-Null
    Toggle-CheckBox -Window $window -Name "Developer diagnostics"
    Find-TextLike -Window $window -Pattern "*frames=*last frame=*peak frame=*" -TimeoutSeconds 8 | Out-Null

    Invoke-Button -Window $window -Name "Overview" -Required $false | Out-Null
    $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    Assert-True ($null -ne $edit) "Root path edit not found before cancel smoke"
    $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($CancelFixturePath)
    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    Start-Sleep -Milliseconds 100
    Invoke-Button -Window $window -Name "Cancel" | Out-Null
    Start-Sleep -Seconds 1

    $process = Get-Process -Id $app.Id -ErrorAction Stop
    $recentCrash = Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = $startTime } -ErrorAction SilentlyContinue |
        Where-Object { $_.ProviderName -eq "Application Error" -and $_.Message -like "*WinBlaze*" } |
        Select-Object -First 1
    Assert-True ($null -eq $recentCrash) "Recent WinBlaze Application Error found: $($recentCrash.Message)"

    [pscustomobject]@{
        Result = "OK"
        Pid = $process.Id
        WorkingSetMB = [math]::Round($process.WorkingSet64 / 1MB, 1)
        TreemapStatus = $treemapStatus
        Correctness = $removeCorrectness
        FrameStatus = $frameStatus
    } | Format-List
}
finally {
    $window = $null
    try {
        $window = Get-WinBlazeWindow -TimeoutSeconds 2
    } catch {
        $window = $null
    }

    if ($window) {
        $window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        Start-Sleep -Seconds 2
    }

    $remaining = Get-Process -Id $app.Id -ErrorAction SilentlyContinue
    if ($remaining) {
        $remaining | Stop-Process -Force
    }
}
