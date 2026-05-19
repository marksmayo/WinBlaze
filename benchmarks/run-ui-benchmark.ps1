param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale")]
    [string]$Size = "tiny",
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [int]$TimeoutSeconds = 60,
    [int]$MaxElapsedMs = 0,
    [int]$MaxWorkingSetMb = 0,
    [int]$MaxPeakFrameMs = 0,
    [int]$MaxPeakFlushMs = 0,
    [switch]$GenerateDataset
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
    $button = $null
    $invokePattern = $null
    foreach ($match in $matches) {
        $candidatePattern = $null
        if ($match.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$candidatePattern)) {
            $button = $match
            $invokePattern = $candidatePattern
            break
        }
    }

    if (-not $button) {
        if ($Required) {
            throw "Button not found: $Name"
        }
        return $false
    }

    $invokePattern.Invoke()
    Start-Sleep -Milliseconds 300
    return $true
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

if ($GenerateDataset) {
    & (Join-Path $PSScriptRoot "make-datasets.ps1") -Root $DatasetRoot -Size $Size -Clean | Out-Null
}

$datasetPath = Join-Path $DatasetRoot $Size
$manifestPath = Join-Path $DatasetRoot "$Size.manifest.json"
Assert-True (Test-Path -LiteralPath $manifestPath) "Dataset manifest not found: $manifestPath"
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
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
    Start-Sleep -Seconds 3
    $window = Get-WinBlazeWindow -TimeoutSeconds 10

    Invoke-Button -Window $window -Name "Overview" -Required $false | Out-Null
    $editCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit)
    $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    Assert-True ($null -ne $edit) "Root path edit not found"
    $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($datasetPath)

    $scanStarted = Get-Date
    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    $expectedPattern = "Correctness:*files=$($manifest.files)*directories=$($manifest.directories)*"
    $correctness = Find-TextLike -Window $window -Pattern $expectedPattern -TimeoutSeconds $TimeoutSeconds
    $scanEnded = Get-Date

    Invoke-Button -Window $window -Name "Diagnostics" -Required $false | Out-Null
    $frameStatus = Find-TextLike -Window $window -Pattern "*frames=*last frame=*peak frame=*" -TimeoutSeconds 8
    Invoke-Button -Window $window -Name "Treemap" -Required $false | Out-Null
    $treemapStatus = Find-TextLike -Window $window -Pattern "*layout=balanced*labels=*" -TimeoutSeconds 8

    $process = Get-Process -Id $app.Id -ErrorAction Stop
    $recentCrash = Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = $startTime } -ErrorAction SilentlyContinue |
        Where-Object { $_.ProviderName -eq "Application Error" -and $_.Message -like "*WinBlaze*" } |
        Select-Object -First 1
    Assert-True ($null -eq $recentCrash) "Recent WinBlaze Application Error found: $($recentCrash.Message)"

    $elapsedMs = [int](($scanEnded - $scanStarted).TotalMilliseconds)
    $workingSetMb = [math]::Round($process.WorkingSet64 / 1MB, 1)
    $flushCount = Get-StatusMetric -Text $frameStatus -Pattern "flushes=(\d+)"
    $queuedEvents = Get-StatusMetric -Text $frameStatus -Pattern "queued events=(\d+)"
    $lastLatencyMs = Get-StatusMetric -Text $frameStatus -Pattern "last latency=(\d+) ms"
    $lastInputMs = Get-StatusMetric -Text $frameStatus -Pattern "last input=(\d+) ms"
    $flushCostMs = Get-StatusMetric -Text $frameStatus -Pattern "flush cost=(\d+) ms"
    $frameCount = Get-StatusMetric -Text $frameStatus -Pattern "frames=(\d+)"
    $lastFrameMs = Get-StatusMetric -Text $frameStatus -Pattern "last frame=(\d+) ms"
    $peakFrameMs = Get-StatusMetric -Text $frameStatus -Pattern "peak frame=(\d+) ms"
    $peakFlushMs = Get-StatusMetric -Text $frameStatus -Pattern "peak flush=(\d+) ms"
    $treemapRenderFlushes = Get-StatusMetric -Text $frameStatus -Pattern "treemap renders=(\d+)/\d+ requests"
    $treemapRenderRequests = Get-StatusMetric -Text $frameStatus -Pattern "treemap renders=\d+/(\d+) requests"
    $treemapRenderCoalesced = Get-StatusMetric -Text $frameStatus -Pattern "coalesced=(\d+)"
    $scanDurationMs = Get-StatusMetric -Text $frameStatus -Pattern "Scan duration: (\d+) ms"

    if ($MaxElapsedMs -gt 0) {
        Assert-True ($elapsedMs -le $MaxElapsedMs) "Elapsed time $elapsedMs ms exceeded threshold $MaxElapsedMs ms"
    }
    if ($MaxWorkingSetMb -gt 0) {
        Assert-True ($workingSetMb -le $MaxWorkingSetMb) "Working set $workingSetMb MB exceeded threshold $MaxWorkingSetMb MB"
    }
    if ($MaxPeakFrameMs -gt 0 -and $null -ne $peakFrameMs) {
        Assert-True ($peakFrameMs -le $MaxPeakFrameMs) "Peak frame $peakFrameMs ms exceeded threshold $MaxPeakFrameMs ms"
    }
    if ($MaxPeakFlushMs -gt 0 -and $null -ne $peakFlushMs) {
        Assert-True ($peakFlushMs -le $MaxPeakFlushMs) "Peak flush $peakFlushMs ms exceeded threshold $MaxPeakFlushMs ms"
    }

    [pscustomobject]@{
        dataset = $Size
        root = $datasetPath
        expected_files = [int]$manifest.files
        expected_directories = [int]$manifest.directories
        expected_bytes = [int64]$manifest.bytes
        elapsed_ms = $elapsedMs
        working_set_mb = $workingSetMb
        working_set_bytes_per_file = if ($manifest.files -gt 0) { [int64]($process.WorkingSet64 / $manifest.files) } else { 0 }
        flush_count = $flushCount
        queued_events = $queuedEvents
        last_latency_ms = $lastLatencyMs
        last_input_ms = $lastInputMs
        flush_cost_ms = $flushCostMs
        frame_count = $frameCount
        last_frame_ms = $lastFrameMs
        peak_frame_ms = $peakFrameMs
        peak_flush_ms = $peakFlushMs
        treemap_render_flushes = $treemapRenderFlushes
        treemap_render_requests = $treemapRenderRequests
        treemap_render_coalesced = $treemapRenderCoalesced
        scan_duration_ms = $scanDurationMs
        correctness = $correctness
        frame_status = $frameStatus
        treemap_status = $treemapStatus
    } | ConvertTo-Json
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
