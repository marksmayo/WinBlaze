param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale")]
    [string]$Size = "tiny",
    [ValidateSet("add", "remove", "modify")]
    [string]$Mutation = "add",
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [int]$TimeoutSeconds = 60,
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
        [string]$Name
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
            return
        }
    }

    throw "Button not found: $Name"
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

    Invoke-Button -Window $window -Name "Overview"
    $editCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit)
    $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
    Assert-True ($null -ne $edit) "Root path edit not found"
    $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($datasetPath)

    Invoke-Button -Window $window -Name "Start scan"
    $initialPattern = "Correctness:*files=$($manifest.files)*directories=$($manifest.directories)*"
    $initialCorrectness = Find-TextLike -Window $window -Pattern $initialPattern -TimeoutSeconds $TimeoutSeconds

    $expectedFiles = [int]$manifest.files
    $expectedAdded = 0
    $expectedRemoved = 0
    $expectedModified = 0
    if ($Mutation -eq "add") {
        $addedPath = Join-Path $datasetPath "incremental-added.txt"
        Set-Content -Path $addedPath -Value ("incremental" * 256)
        $expectedFiles += 1
        $expectedAdded = 1
    } elseif ($Mutation -eq "remove") {
        $removeTarget = Get-ChildItem -LiteralPath $datasetPath -Recurse -File | Select-Object -First 1
        Assert-True ($null -ne $removeTarget) "No file available to remove"
        Remove-Item -LiteralPath $removeTarget.FullName -Force
        $expectedFiles -= 1
        $expectedRemoved = 1
    } else {
        $modifyTarget = Get-ChildItem -LiteralPath $datasetPath -Recurse -File | Select-Object -First 1
        Assert-True ($null -ne $modifyTarget) "No file available to modify"
        Add-Content -LiteralPath $modifyTarget.FullName -Value "modified"
        $expectedModified = 1
    }
    $incrementalStarted = Get-Date
    Invoke-Button -Window $window -Name "Incremental rescan"
    $incrementalPattern = "Correctness:*incremental added=$expectedAdded*removed=$expectedRemoved*modified=$expectedModified*files=$expectedFiles*directories=$($manifest.directories)*"
    $incrementalCorrectness = Find-TextLike -Window $window -Pattern $incrementalPattern -TimeoutSeconds $TimeoutSeconds
    $incrementalEnded = Get-Date

    Invoke-Button -Window $window -Name "Diagnostics"
    $frameStatus = Find-TextLike -Window $window -Pattern "*frames=*last frame=*peak frame=*" -TimeoutSeconds 8
    $flushCount = Get-StatusMetric -Text $frameStatus -Pattern "flushes=(\d+)"
    $queuedEvents = Get-StatusMetric -Text $frameStatus -Pattern "queued events=(\d+)"
    $lastLatencyMs = Get-StatusMetric -Text $frameStatus -Pattern "last latency=(\d+) ms"
    $lastInputMs = Get-StatusMetric -Text $frameStatus -Pattern "last input=(\d+) ms"
    $flushCostMs = Get-StatusMetric -Text $frameStatus -Pattern "flush cost=(\d+) ms"
    $peakFlushMs = Get-StatusMetric -Text $frameStatus -Pattern "peak flush=(\d+) ms"
    $frameCount = Get-StatusMetric -Text $frameStatus -Pattern "frames=(\d+)"
    $lastFrameMs = Get-StatusMetric -Text $frameStatus -Pattern "last frame=(\d+) ms"
    $peakFrameMs = Get-StatusMetric -Text $frameStatus -Pattern "peak frame=(\d+) ms"
    $treemapRenderFlushes = Get-StatusMetric -Text $frameStatus -Pattern "treemap renders=(\d+)/\d+ requests"
    $treemapRenderRequests = Get-StatusMetric -Text $frameStatus -Pattern "treemap renders=\d+/(\d+) requests"
    $treemapRenderCoalesced = Get-StatusMetric -Text $frameStatus -Pattern "coalesced=(\d+)"
    $scanDurationMs = Get-StatusMetric -Text $frameStatus -Pattern "Scan duration: (\d+) ms"

    $process = Get-Process -Id $app.Id -ErrorAction Stop
    $recentCrash = Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = $startTime } -ErrorAction SilentlyContinue |
        Where-Object { $_.ProviderName -eq "Application Error" -and $_.Message -like "*WinBlaze*" } |
        Select-Object -First 1
    Assert-True ($null -eq $recentCrash) "Recent WinBlaze Application Error found: $($recentCrash.Message)"

    [pscustomobject]@{
        dataset = $Size
        mutation = $Mutation
        root = $datasetPath
        initial_files = [int]$manifest.files
        incremental_expected_files = $expectedFiles
        expected_added = $expectedAdded
        expected_removed = $expectedRemoved
        expected_modified = $expectedModified
        incremental_elapsed_ms = [int](($incrementalEnded - $incrementalStarted).TotalMilliseconds)
        working_set_mb = [math]::Round($process.WorkingSet64 / 1MB, 1)
        flush_count = $flushCount
        queued_events = $queuedEvents
        last_latency_ms = $lastLatencyMs
        last_input_ms = $lastInputMs
        flush_cost_ms = $flushCostMs
        peak_flush_ms = $peakFlushMs
        frame_count = $frameCount
        last_frame_ms = $lastFrameMs
        peak_frame_ms = $peakFrameMs
        treemap_render_flushes = $treemapRenderFlushes
        treemap_render_requests = $treemapRenderRequests
        treemap_render_coalesced = $treemapRenderCoalesced
        scan_duration_ms = $scanDurationMs
        initial_correctness = $initialCorrectness
        incremental_correctness = $incrementalCorrectness
        frame_status = $frameStatus
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
