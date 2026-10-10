param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [string]$MissingPath = "",
    [string]$FileRootPath = "",
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
    $condition = [System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,"WinBlaze"),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$app.Id))
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

$work = Join-Path $PSScriptRoot ("../../artifacts/ui-negative-" + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$fixture = Join-Path $work 'fixture'
New-Item -ItemType Directory -Path $fixture | Out-Null
if (!$MissingPath) { $MissingPath = Join-Path $work 'missing' }
if (!$FileRootPath) { $FileRootPath = Join-Path $fixture 'file.txt' }
Assert-True (!(Test-Path -LiteralPath $MissingPath)) "Missing-path fixture must not exist."
Assert-True (!(Test-Path -LiteralPath $FileRootPath)) "File fixture must not overwrite an existing file."
Set-Content -LiteralPath $FileRootPath -Value "not a directory"
if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($FileRootPath)) -ne [IO.Path]::GetFullPath($fixture)) {
    Set-Content -LiteralPath (Join-Path $fixture 'baseline.txt') -Value 'baseline'
}

$resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path
Assert-True (Test-Path -LiteralPath $resolvedAppPath) "App executable not found: $AppPath"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$startTime = Get-Date
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $resolvedAppPath
$start.UseShellExecute = $false
$start.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $work 'appdata'
$app = [Diagnostics.Process]::Start($start)
try {
    Start-Sleep -Seconds 3
    $window = Get-WinBlazeWindow -TimeoutSeconds $WindowTimeoutSeconds
    Invoke-Button -Window $window -Name "Overview" -Required $false | Out-Null

    $editCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit)
    $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    Assert-True ($null -ne $edit) "Root path edit not found"
    $value = $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    $value.SetValue([IO.Path]::GetFullPath($fixture))
    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    Invoke-Button -Window $window -Name "Diagnostics" | Out-Null
    $before = Find-TextLike -Window $window -Pattern "Correctness:*issues=0*files=1*directories=1*" -TimeoutSeconds $ScanWaitSeconds
    $value.SetValue($MissingPath)
    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    $missingFeedback = Find-TextLike -Window $window -Pattern "Choose an existing folder*" -TimeoutSeconds $ScanWaitSeconds
    $afterMissing = Find-TextLike -Window $window -Pattern "Correctness:*issues=0*files=1*directories=1*" -TimeoutSeconds $ScanWaitSeconds
    Assert-True ($before -eq $afterMissing) "Invalid input must preserve existing scan results."

    $value.SetValue($FileRootPath)
    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    $fileFeedback = Find-TextLike -Window $window -Pattern "That path is a file*" -TimeoutSeconds $ScanWaitSeconds
    $afterFile = Find-TextLike -Window $window -Pattern "Correctness:*issues=0*files=1*directories=1*" -TimeoutSeconds $ScanWaitSeconds
    Assert-True ($before -eq $afterFile) "A file path must preserve existing scan results."
    $cancel = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Cancel'))
    Assert-True (!$cancel.Current.IsEnabled) "Invalid input must not start a scan."

    $recentCrash = Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = $startTime } -ErrorAction SilentlyContinue |
        Where-Object { $_.ProviderName -eq "Application Error" -and $_.Message -like "*WinBlaze*" } |
        Select-Object -First 1
    Assert-True ($null -eq $recentCrash) "Recent WinBlaze Application Error found: $($recentCrash.Message)"

    [pscustomobject]@{
        Result = "OK"
        Pid = $app.Id
        MissingPath = $MissingPath
        MissingFeedback = $missingFeedback
        FileRootPath = $FileRootPath
        FileFeedback = $fileFeedback
        PreservedResults = $afterFile
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
