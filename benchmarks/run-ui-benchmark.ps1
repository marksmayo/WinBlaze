param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale", "full")]
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
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')

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
        if ($script:sampleApp) {
            $script:sampleApp.Refresh()
            $script:peakWs = [Math]::Max($script:peakWs, $script:sampleApp.WorkingSet64)
            $script:cpuMs = Merge-BenchmarkCpuLowerBound $script:cpuMs $script:sampleApp.TotalProcessorTime.TotalMilliseconds
        }
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
    $traceStart = $Name -eq 'Start scan' -and !!$env:WINBLAZE_UI_BENCH_PHASES
    if ($traceStart) { $phaseTimer = [Diagnostics.Stopwatch]::StartNew() }
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

    if ($traceStart) { $script:buttonFindMs = $phaseTimer.Elapsed.TotalMilliseconds; $phaseTimer.Restart() }
    $invokePattern.Invoke()
    if ($traceStart) { $script:providerInvokeMs = $phaseTimer.Elapsed.TotalMilliseconds }
    Start-Sleep -Milliseconds 300
    return $true
}

function Get-WinBlazeWindow {
    param([int]$TimeoutSeconds)

    $desktop = [System.Windows.Automation.AutomationElement]::RootElement
    $condition = [System.Windows.Automation.AndCondition]::new(
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, "WinBlaze"),
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $app.Id))
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

$datasetPath = if ($Size -eq 'full') { 'C:\' } else { Join-Path $DatasetRoot $Size }
$manifestPath = Join-Path $DatasetRoot "$Size.manifest.json"
if ($Size -ne 'full') { Assert-True (Test-Path -LiteralPath $manifestPath) "Dataset manifest not found: $manifestPath"
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json } else { $manifest = [pscustomobject]@{files=0;directories=0;bytes=0} }
$resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes


$startTime = Get-Date
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $resolvedAppPath
$start.UseShellExecute = $false
$start.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $DatasetRoot ('appdata-' + [Guid]::NewGuid().ToString('N'))
$app = [Diagnostics.Process]::Start($start)
if ($Size -eq 'full') {
    # Let a subsequent cache-load benchmark in this process reuse this owned
    # snapshot rather than the user's unrelated global cache.
    $env:WINBLAZE_BENCH_CACHE_ROOT = $start.EnvironmentVariables['LOCALAPPDATA']
}
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

    # Diagnostics are hidden by default in the High Velocity layout; reveal
    # them before the scan so the correctness text is in the UIA tree.
    Invoke-Button -Window $window -Name "Diagnostics" -Required $false | Out-Null

    $script:sampleApp = $app
    $script:peakWs = 0L; $script:cpuMs = 0.0
    $scanStarted = Get-Date
    Invoke-Button -Window $window -Name "Start scan" | Out-Null
    if ($Size -eq 'full') { Find-TextLike -Window $window -Pattern '*State: idle*Results: loaded*Scan duration: * ms*' -TimeoutSeconds $TimeoutSeconds | Out-Null }
    $expectedPattern = if ($Size -eq 'full') { '*Correctness:*' } else { "*Correctness:*files=$($manifest.files)*directories=$($manifest.directories)*" }
    $correctness = Find-TextLike -Window $window -Pattern $expectedPattern -TimeoutSeconds $TimeoutSeconds
    $scanEnded = Get-Date

    $frameStatus = Find-TextLike -Window $window -Pattern "*frames=*last frame=*peak frame=*" -TimeoutSeconds 8
    Invoke-Button -Window $window -Name "Treemap" -Required $false | Out-Null
    $treemapStatus = Find-TextLike -Window $window -Pattern "*layout=*labels=*" -TimeoutSeconds 8

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

    if ($env:WINBLAZE_UI_BENCH_PHASES) {
        [ordered]@{
            scope='Unchanged UI budget boundaries; optional observer phase diagnosis'
            invocation_mode='Original synchronous invocation'
            elapsed_ms=$elapsedMs
            button_find_ms=$script:buttonFindMs
            provider_invoke_ms=$script:providerInvokeMs
            fixed_post_invoke_sleep_ms=300
            native_scan_duration_ms=$scanDurationMs
            correctness=$correctness
            frame_status=$frameStatus
            binary=(Get-BenchmarkIdentity -Tool 'WinBlaze UI budget diagnostic' -Executable $resolvedAppPath)
        } | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $env:WINBLAZE_UI_BENCH_PHASES
    }

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
        sampled_peak_working_set_bytes_through_observation = $script:peakWs
        sampled_cpu_ms_through_observation = $script:cpuMs
        cache_directory = $start.EnvironmentVariables['LOCALAPPDATA']
        dataset = $Size
        root = $datasetPath
        expected_files = if ($Size -eq "full") { Get-StatusMetric -Text $correctness -Pattern "files=(\d+)" } else { [int]$manifest.files }
        expected_directories = if ($Size -eq "full") { Get-StatusMetric -Text $correctness -Pattern "directories=(\d+)" } else { [int]$manifest.directories }
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
