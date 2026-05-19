param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [string[]]$Profiles = @("tiny", "fanout", "scale"),
    [string]$OutputPath = (Join-Path $PSScriptRoot "winblaze-baselines.json"),
    [string]$BudgetPath = (Join-Path $PSScriptRoot "performance-budgets.json"),
    [int]$TimeoutSeconds = 120,
    [switch]$GenerateDatasets,
    [switch]$EnforceBudgets
)

$ErrorActionPreference = "Stop"

$Profiles = @($Profiles | ForEach-Object { $_ -split "," } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
$validProfiles = @("tiny", "small", "medium", "fanout", "fanout-large", "scale")
foreach ($profile in $Profiles) {
    if ($validProfiles -notcontains $profile) {
        throw "Unknown profile: $profile"
    }
}

$budgets = $null
if ($EnforceBudgets) {
    if (-not (Test-Path -LiteralPath $BudgetPath)) {
        throw "Budget file not found: $BudgetPath"
    }
    $budgets = Get-Content -LiteralPath $BudgetPath -Raw | ConvertFrom-Json
}

$results = @()
foreach ($profile in $Profiles) {
    $benchmarkArgs = @{
        AppPath = $AppPath
        Size = $profile
        DatasetRoot = $DatasetRoot
        TimeoutSeconds = $TimeoutSeconds
    }
    if ($GenerateDatasets) {
        $benchmarkArgs.GenerateDataset = $true
    }
    if ($EnforceBudgets) {
        $budget = $budgets.profiles.$profile
        if ($null -eq $budget) {
            throw "No budget configured for profile: $profile"
        }
        if ($budget.max_elapsed_ms -gt 0) {
            $benchmarkArgs.MaxElapsedMs = [int]$budget.max_elapsed_ms
        }
        if ($budget.max_working_set_mb -gt 0) {
            $benchmarkArgs.MaxWorkingSetMb = [int]$budget.max_working_set_mb
        }
        if ($budget.max_peak_frame_ms -gt 0) {
            $benchmarkArgs.MaxPeakFrameMs = [int]$budget.max_peak_frame_ms
        }
        if ($budget.max_peak_flush_ms -gt 0) {
            $benchmarkArgs.MaxPeakFlushMs = [int]$budget.max_peak_flush_ms
        }
    }
    $json = & (Join-Path $PSScriptRoot "run-ui-benchmark.ps1") @benchmarkArgs
    $results += ($json | ConvertFrom-Json)
}

$record = [pscustomobject]@{
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    app_path = (Resolve-Path -LiteralPath $AppPath).Path
    dataset_root = $DatasetRoot
    profiles = $Profiles
    budgets_enforced = [bool]$EnforceBudgets
    budget_path = if ($EnforceBudgets) { (Resolve-Path -LiteralPath $BudgetPath).Path } else { $null }
    results = $results
}

$parent = Split-Path -Parent $OutputPath
if ($parent) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$record | ConvertTo-Json -Depth 8 | Set-Content -Path $OutputPath -Encoding UTF8
$record | ConvertTo-Json -Depth 8
