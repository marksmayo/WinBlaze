param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [string]$MissingPath = "C:\tmp\WinBlazeMissingRoot",
    [string]$FileRootPath = "C:\tmp\WinBlazeFileRoot.txt",
    [int]$WindowTimeoutSeconds = 10,
    [int]$ScanWaitSeconds = 10
)

$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
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
        Start-Sleep -Milliseconds 250
    }

    throw "Text not found: $Pattern"
}

function Get-AllTextNames {
    param([System.Windows.Automation.AutomationElement]$Window)

    $textCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Text)
    $texts = $Window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textCondition)
    $names = @()
    foreach ($text in $texts) {
        if (-not [string]::IsNullOrWhiteSpace($text.Current.Name)) {
            $names += $text.Current.Name
        }
    }
    return $names
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
            Start-Sleep -Milliseconds 300
            return $true
        }
    }

    if ($Required) {
        throw "Button not found: $Name"
    }
    return $false
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
        Start-Sleep -Milliseconds 250
    }
    throw "WinBlaze window not found"
}

if (Test-Path -LiteralPath $MissingPath) {
    Remove-Item -LiteralPath $MissingPath -Recurse -Force
}
Set-Content -LiteralPath $FileRootPath -Value "not a directory"

$resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path
Assert-True (Test-Path -LiteralPath $resolvedAppPath) "App executable not found: $AppPath"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$startTime = Get-Date
$old = Get-Process WinBlaze.UI -ErrorAction SilentlyContinue
if ($old) {
    $old | Stop-Process -Force
}

$app = Start-Process -FilePath $resolvedAppPath -PassThru
try {
    Start-Sleep -Seconds 3
    $window = Get-WinBlazeWindow -TimeoutSeconds $WindowTimeoutSeconds
    Invoke-Button -Window $window -Name "Overview" -Required $false | Out-Null

    $editCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit)
    $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    Assert-True ($null -ne $edit) "Root path edit not found"
    $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($MissingPath)

    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    Invoke-Button -Window $window -Name "Diagnostics" -Required $false | Out-Null
    try {
        $correctness = Find-TextLike -Window $window -Pattern "Correctness:*issues=1*last issue=*$MissingPath*" -TimeoutSeconds $ScanWaitSeconds
        $recentIssues = Find-TextLike -Window $window -Pattern "Recent issues:*$MissingPath*" -TimeoutSeconds $ScanWaitSeconds
        $issueDrilldown = Find-TextLike -Window $window -Pattern "Issue drill-down:*errors=1*skipped=1*missing=1*last=*$MissingPath*" -TimeoutSeconds $ScanWaitSeconds
    } catch {
        $dump = Get-AllTextNames -Window $window
        throw "$($_.Exception.Message)`nVisible text:`n$($dump -join "`n")"
    }
    $summary = Find-TextLike -Window $window -Pattern "*State: idle*Results: loaded*" -TimeoutSeconds 8

    $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($FileRootPath)
    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    Invoke-Button -Window $window -Name "Diagnostics" -Required $false | Out-Null
    try {
        $fileRootCorrectness = Find-TextLike -Window $window -Pattern "Correctness:*issues=1*last issue=*$FileRootPath*" -TimeoutSeconds $ScanWaitSeconds
        $fileRootRecentIssues = Find-TextLike -Window $window -Pattern "Recent issues:*$FileRootPath*" -TimeoutSeconds $ScanWaitSeconds
        $fileRootIssueDrilldown = Find-TextLike -Window $window -Pattern "Issue drill-down:*errors=1*last=*$FileRootPath*" -TimeoutSeconds $ScanWaitSeconds
    } catch {
        $dump = Get-AllTextNames -Window $window
        throw "$($_.Exception.Message)`nVisible text:`n$($dump -join "`n")"
    }
    $fileRootSummary = Find-TextLike -Window $window -Pattern "*State: idle*Results: loaded*" -TimeoutSeconds 8

    $recentCrash = Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = $startTime } -ErrorAction SilentlyContinue |
        Where-Object { $_.ProviderName -eq "Application Error" -and $_.Message -like "*WinBlaze*" } |
        Select-Object -First 1
    Assert-True ($null -eq $recentCrash) "Recent WinBlaze Application Error found: $($recentCrash.Message)"

    [pscustomobject]@{
        Result = "OK"
        Pid = $app.Id
        MissingPath = $MissingPath
        Correctness = $correctness
        RecentIssues = $recentIssues
        IssueDrilldown = $issueDrilldown
        Summary = $summary
        FileRootPath = $FileRootPath
        FileRootCorrectness = $fileRootCorrectness
        FileRootRecentIssues = $fileRootRecentIssues
        FileRootIssueDrilldown = $fileRootIssueDrilldown
        FileRootSummary = $fileRootSummary
    } | Format-List
}
finally {
    try {
        $window = Get-WinBlazeWindow -TimeoutSeconds 2
        if ($window) {
            $window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
            Start-Sleep -Seconds 2
        }
    } catch {
    }

    $remaining = Get-Process -Id $app.Id -ErrorAction SilentlyContinue
    if ($remaining) {
        $remaining | Stop-Process -Force
    }
    if (Test-Path -LiteralPath $FileRootPath) {
        Remove-Item -LiteralPath $FileRootPath -Force
    }
}
