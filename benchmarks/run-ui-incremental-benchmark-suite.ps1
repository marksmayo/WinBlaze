param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale")]
    [string]$Size = "tiny",
    [string[]]$Mutations = @("add", "remove", "modify"),
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [int]$TimeoutSeconds = 60,
    [string]$OutputPath = "",
    [switch]$GenerateDataset
)

$ErrorActionPreference = "Stop"

$Mutations = @($Mutations | ForEach-Object { $_ -split "," } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
$validMutations = @("add", "remove", "modify")
foreach ($mutation in $Mutations) {
    if ($validMutations -notcontains $mutation) {
        throw "Unknown mutation: $mutation"
    }
}

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

$resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path
$results = @()
foreach ($mutation in $Mutations) {
    $benchmarkArgs = @{
        AppPath = $resolvedAppPath
        Size = $Size
        Mutation = $mutation
        DatasetRoot = $DatasetRoot
        TimeoutSeconds = $TimeoutSeconds
    }
    if ($GenerateDataset) {
        $benchmarkArgs.GenerateDataset = $true
    }

    $json = & (Join-Path $PSScriptRoot "run-ui-incremental-benchmark.ps1") @benchmarkArgs
    $results += ($json | ConvertFrom-Json)
}

$record = [pscustomobject]@{
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    app_path = $resolvedAppPath
    dataset = $Size
    dataset_root = $DatasetRoot
    mutations = $Mutations
    median_incremental_elapsed_ms = Get-MedianMetric -Records $results -Name "incremental_elapsed_ms"
    median_working_set_mb = Get-MedianMetric -Records $results -Name "working_set_mb" -Type "double"
    median_last_latency_ms = Get-MedianMetric -Records $results -Name "last_latency_ms"
    median_last_input_ms = Get-MedianMetric -Records $results -Name "last_input_ms"
    median_peak_frame_ms = Get-MedianMetric -Records $results -Name "peak_frame_ms"
    median_peak_flush_ms = Get-MedianMetric -Records $results -Name "peak_flush_ms"
    median_scan_duration_ms = Get-MedianMetric -Records $results -Name "scan_duration_ms"
    results = $results
}

if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
    $parent = Split-Path -Parent $OutputPath
    if ($parent) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $record | ConvertTo-Json -Depth 8 | Set-Content -Path $OutputPath -Encoding UTF8
}

$record | ConvertTo-Json -Depth 8
