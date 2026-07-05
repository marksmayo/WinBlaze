param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [int]$TimeoutSeconds = 15,
    [string]$OutputPath = ""
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
        Start-Sleep -Milliseconds 100
    }

    throw "Text not found: $Pattern"
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
        Start-Sleep -Milliseconds 100
    }
    throw "WinBlaze window not found"
}

function Get-StatusMetric {
    param(
        [string]$Text,
        [string]$Pattern
    )

    $match = [regex]::Match($Text, $Pattern)
    if (-not $match.Success) {
        return $null
    }
    return [int]$match.Groups[1].Value
}

$resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$old = Get-Process WinBlaze.UI -ErrorAction SilentlyContinue
if ($old) {
    $old | Stop-Process -Force
}

$startTime = Get-Date
$app = Start-Process -FilePath $resolvedAppPath -PassThru
try {
    $window = Get-WinBlazeWindow -TimeoutSeconds $TimeoutSeconds
    $runtime = Invoke-Button -Window $window -Name "Diagnostics" -Required $false | Out-Null
    $runtime = Find-TextLike -Window $window -Pattern "*results=loaded*" -TimeoutSeconds $TimeoutSeconds
    $loadedAt = Get-Date
    $catalog = Find-TextLike -Window $window -Pattern "Catalog entries: *" -TimeoutSeconds 5
    $cacheLoad = Find-TextLike -Window $window -Pattern "*Cache load: *" -TimeoutSeconds 5
    $treemap = Find-TextLike -Window $window -Pattern "*GPU treemap catalog frame rendered*" -TimeoutSeconds 8

    $process = Get-Process -Id $app.Id -ErrorAction Stop
    $recentCrash = Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = $startTime } -ErrorAction SilentlyContinue |
        Where-Object { $_.ProviderName -eq "Application Error" -and $_.Message -like "*WinBlaze*" } |
        Select-Object -First 1
    Assert-True ($null -eq $recentCrash) "Recent WinBlaze Application Error found: $($recentCrash.Message)"

    $cacheReadKbText = $null
    $cacheReadMatch = [regex]::Match($cacheLoad, "read ([0-9.]+) KB")
    if ($cacheReadMatch.Success) {
        $cacheReadKbText = $cacheReadMatch.Groups[1].Value
    }

    $record = [pscustomobject]@{
        elapsed_ms = [int](($loadedAt - $startTime).TotalMilliseconds)
        working_set_mb = [math]::Round($process.WorkingSet64 / 1MB, 1)
        cache_read_kb = if ($null -ne $cacheReadKbText) { [double]$cacheReadKbText } else { $null }
        cache_read_ms = Get-StatusMetric -Text $cacheLoad -Pattern "read [0-9.]+ KB in (\d+) ms"
        cache_decode_ms = Get-StatusMetric -Text $cacheLoad -Pattern "decoded in (\d+) ms"
        cache_entries = Get-StatusMetric -Text $cacheLoad -Pattern "entries=(\d+)"
        cache_load_cap = Get-StatusMetric -Text $cacheLoad -Pattern "load cap=(\d+)"
        runtime_status = $runtime
        cache_load_status = $cacheLoad
        catalog_status = $catalog
        treemap_status = $treemap
    }

    if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
        $parent = Split-Path -Parent $OutputPath
        if ($parent) {
            New-Item -ItemType Directory -Force -Path $parent | Out-Null
        }
        $record | ConvertTo-Json -Depth 6 | Set-Content -Path $OutputPath -Encoding UTF8
    }

    $record | ConvertTo-Json
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
}
