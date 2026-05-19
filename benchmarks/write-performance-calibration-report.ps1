param(
    [string]$ReleaseMediansPath = (Join-Path $PSScriptRoot "winblaze-release-medians.json"),
    [string]$EnvironmentPath = (Join-Path $PSScriptRoot "environment.json"),
    [string]$BudgetPath = (Join-Path $PSScriptRoot "performance-budgets.release.json"),
    [string]$OutputPath = (Join-Path $PSScriptRoot "performance-calibration-report.md")
)

$ErrorActionPreference = "Stop"

function Format-Value {
    param([object]$Value, [string]$Suffix = "")
    if ($null -eq $Value) {
        return "n/a"
    }
    return "$Value$Suffix"
}

if (-not (Test-Path -LiteralPath $ReleaseMediansPath)) {
    throw "Release medians file not found: $ReleaseMediansPath"
}
if (-not (Test-Path -LiteralPath $BudgetPath)) {
    throw "Release budget file not found: $BudgetPath"
}

$release = Get-Content -LiteralPath $ReleaseMediansPath -Raw | ConvertFrom-Json
$budgets = Get-Content -LiteralPath $BudgetPath -Raw | ConvertFrom-Json
$environment = if (Test-Path -LiteralPath $EnvironmentPath) {
    Get-Content -LiteralPath $EnvironmentPath -Raw | ConvertFrom-Json
} else {
    $null
}

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# WinBlaze Performance Calibration Report")
$lines.Add("")
$lines.Add("Generated: $((Get-Date).ToUniversalTime().ToString("o"))")
$lines.Add("")
$lines.Add("## Environment")
$lines.Add("")
if ($environment) {
    $lines.Add("- Machine: $(Format-Value $environment.machine.name)")
    $lines.Add("- OS: $(Format-Value $environment.os.caption) $(Format-Value $environment.os.version) build $(Format-Value $environment.os.build_number)")
    $lines.Add("- CPU: $(Format-Value $environment.cpu.name), logical processors $(Format-Value $environment.cpu.logical_processors)")
    $lines.Add("- Dataset storage: $(Format-Value $environment.dataset_storage.requested_root) on $(Format-Value $environment.dataset_storage.drive_root), filesystem $(Format-Value $environment.dataset_storage.filesystem)")
    $lines.Add("- Power: $(Format-Value $environment.power.active_scheme)")
} else {
    $lines.Add("- Environment capture not found. Run ``benchmarks\record-environment.ps1`` before release calibration.")
}
$lines.Add("")
$lines.Add("## Release Medians")
$lines.Add("")
$lines.Add("| Profile | First ms | Warmed median ms | Median ms | Working set MB | Last latency ms | Input ms | Peak frame ms | Peak flush ms | Scan duration ms | Treemap renders | Budget result |")
$lines.Add("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |")

foreach ($result in $release.results) {
    $profile = [string]$result.dataset
    $budget = $budgets.profiles.$profile
    $budgetResult = "not budgeted"
    if ($null -ne $budget) {
        $failures = @()
        if ($result.median_elapsed_ms -gt $budget.max_elapsed_ms) {
            $failures += "elapsed"
        }
        if ($result.median_working_set_mb -gt $budget.max_working_set_mb) {
            $failures += "working set"
        }
        if ($null -ne $result.median_peak_frame_ms -and $result.median_peak_frame_ms -gt $budget.max_peak_frame_ms) {
            $failures += "peak frame"
        }
        if ($null -ne $result.median_peak_flush_ms -and $result.median_peak_flush_ms -gt $budget.max_peak_flush_ms) {
            $failures += "peak flush"
        }
        $budgetResult = if ($failures.Count -eq 0) { "pass" } else { "fail: $($failures -join ", ")" }
    }

    $lines.Add("| $profile | $(Format-Value $result.first_elapsed_ms) | $(Format-Value $result.warmed_median_elapsed_ms) | $(Format-Value $result.median_elapsed_ms) | $(Format-Value $result.median_working_set_mb) | $(Format-Value $result.median_last_latency_ms) | $(Format-Value $result.median_last_input_ms) | $(Format-Value $result.median_peak_frame_ms) | $(Format-Value $result.median_peak_flush_ms) | $(Format-Value $result.median_scan_duration_ms) | $(Format-Value $result.median_treemap_render_flushes)/$(Format-Value $result.median_treemap_render_requests) | $budgetResult |")
}

$lines.Add("")
$lines.Add("## Calibration Notes")
$lines.Add("")
$lines.Add("- The report is generated from checked-in Release repeated-run medians and local Release budgets.")
$lines.Add("- Treat these values as machine-specific stability gates until multiple Windows machines have recorded comparable environment captures and Release medians.")
$lines.Add("- Re-run ``benchmarks\record-environment.ps1`` and ``benchmarks\run-release-baseline-set.ps1`` before updating release budgets.")

$parent = Split-Path -Parent $OutputPath
if ($parent) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$lines | Set-Content -Path $OutputPath -Encoding UTF8
$lines -join [Environment]::NewLine
