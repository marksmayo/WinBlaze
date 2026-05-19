param(
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale")]
    [string]$Size = "tiny",
    [int]$Runs = 3,
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [int]$TimeoutSeconds = 60,
    [switch]$GenerateDataset
)

$ErrorActionPreference = "Stop"

if ($Runs -lt 1) {
    throw "Runs must be at least 1."
}

if ($GenerateDataset) {
    & (Join-Path $PSScriptRoot "make-datasets.ps1") -Root $DatasetRoot -Size $Size -Clean | Out-Null
}

$results = @()
for ($run = 1; $run -le $Runs; $run++) {
    $json = & (Join-Path $PSScriptRoot "run-ui-benchmark.ps1") `
        -AppPath $AppPath `
        -Size $Size `
        -DatasetRoot $DatasetRoot `
        -TimeoutSeconds $TimeoutSeconds
    $result = $json | ConvertFrom-Json
    $result | Add-Member -NotePropertyName run -NotePropertyValue $run
    $result | Add-Member -NotePropertyName cache_state -NotePropertyValue $(if ($run -eq 1) { "first" } else { "warmed" })
    $results += $result
}

$elapsed = @($results | ForEach-Object { [int]$_.elapsed_ms } | Sort-Object)
$memory = @($results | ForEach-Object { [double]$_.working_set_mb } | Sort-Object)
$middle = [int][math]::Floor(($elapsed.Count - 1) / 2)

function Get-MedianMetric {
    param(
        [object[]]$Records,
        [string]$Name,
        [string]$Type = "int"
    )

    $values = @($Records | ForEach-Object { $_.$Name } | Where-Object { $null -ne $_ } | Sort-Object)
    if ($values.Count -eq 0) {
        return $null
    }

    $index = [int][math]::Floor(($values.Count - 1) / 2)
    if ($Type -eq "double") {
        return [double]$values[$index]
    }
    return [int]$values[$index]
}

[pscustomobject]@{
    dataset = $Size
    runs = $Runs
    first_elapsed_ms = [int]$results[0].elapsed_ms
    warmed_median_elapsed_ms = if ($results.Count -gt 1) {
        $warmed = @($results | Where-Object { $_.run -gt 1 } | ForEach-Object { [int]$_.elapsed_ms } | Sort-Object)
        $warmed[[int][math]::Floor(($warmed.Count - 1) / 2)]
    } else {
        $null
    }
    median_elapsed_ms = $elapsed[$middle]
    median_working_set_mb = $memory[$middle]
    median_last_latency_ms = Get-MedianMetric -Records $results -Name "last_latency_ms"
    median_last_input_ms = Get-MedianMetric -Records $results -Name "last_input_ms"
    median_peak_frame_ms = Get-MedianMetric -Records $results -Name "peak_frame_ms"
    median_peak_flush_ms = Get-MedianMetric -Records $results -Name "peak_flush_ms"
    median_scan_duration_ms = Get-MedianMetric -Records $results -Name "scan_duration_ms"
    median_treemap_render_flushes = Get-MedianMetric -Records $results -Name "treemap_render_flushes"
    median_treemap_render_requests = Get-MedianMetric -Records $results -Name "treemap_render_requests"
    median_treemap_render_coalesced = Get-MedianMetric -Records $results -Name "treemap_render_coalesced"
    results = $results
} | ConvertTo-Json -Depth 6
