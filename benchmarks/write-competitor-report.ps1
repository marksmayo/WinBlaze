param(
    [string]$WinBlazeBaselinePath = (Join-Path $PSScriptRoot "winblaze-baselines.json"),
    [string]$ReleaseBaselinePath = (Join-Path $PSScriptRoot "winblaze-release-medians.json"),
    [string]$CompetitorBaselinePath = (Join-Path $PSScriptRoot "competitor-baselines.json"),
    [string]$OutputPath = (Join-Path $PSScriptRoot "competitor-report.md")
)

$ErrorActionPreference = "Stop"

function Format-Optional {
    param($Value, [string]$Suffix = "")
    if ($null -eq $Value -or [string]::IsNullOrWhiteSpace([string]$Value)) {
        return "not recorded"
    }
    return "$Value$Suffix"
}

if (-not (Test-Path -LiteralPath $WinBlazeBaselinePath)) {
    throw "WinBlaze baseline not found: $WinBlazeBaselinePath"
}
if (-not (Test-Path -LiteralPath $CompetitorBaselinePath)) {
    throw "Competitor baseline not found: $CompetitorBaselinePath"
}

$winblaze = Get-Content -LiteralPath $WinBlazeBaselinePath -Raw | ConvertFrom-Json
$release = if (Test-Path -LiteralPath $ReleaseBaselinePath) {
    Get-Content -LiteralPath $ReleaseBaselinePath -Raw | ConvertFrom-Json
} else {
    $null
}
$competitors = Get-Content -LiteralPath $CompetitorBaselinePath -Raw | ConvertFrom-Json

$lines = @()
$lines += "# Competitor Baseline Report"
$lines += ""
$lines += "Generated from local benchmark artifacts."
$lines += ""
$lines += "## WinBlaze Local Baselines"
$lines += ""
$lines += "| Profile | Files | Directories | Elapsed ms | Working set MB | Peak frame ms | Peak flush ms |"
$lines += "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"
foreach ($result in $winblaze.results) {
    $lines += "| $($result.dataset) | $($result.expected_files) | $($result.expected_directories) | $($result.elapsed_ms) | $($result.working_set_mb) | $($result.peak_frame_ms) | $($result.peak_flush_ms) |"
}

if ($release) {
    $lines += ""
    $lines += "## WinBlaze Release Medians"
    $lines += ""
    $lines += "| Profile | Runs | First elapsed ms | Warmed median ms | Overall median ms | Median working set MB |"
    $lines += "| --- | ---: | ---: | ---: | ---: | ---: |"
    foreach ($result in $release.results) {
        $lines += "| $($result.dataset) | $($result.runs) | $($result.first_elapsed_ms) | $($result.warmed_median_elapsed_ms) | $($result.median_elapsed_ms) | $($result.median_working_set_mb) |"
    }
}

$lines += ""
$lines += "## Competitor Tool Inventory"
$lines += ""
$lines += "| Tool | Installed | Version | Manual timing ms | Path |"
$lines += "| --- | --- | --- | ---: | --- |"
foreach ($tool in $competitors.tools) {
    $timing = switch ($tool.name) {
        "WizTree" { $competitors.manual_timings_ms.wiztree }
        "WinDirStat" { $competitors.manual_timings_ms.windirstat }
        "Everything" { $competitors.manual_timings_ms.everything }
        default { $null }
    }
    $installed = if ($tool.installed) { "yes" } else { "no" }
    $version = Format-Optional $tool.version
    $manual = Format-Optional $timing
    $path = Format-Optional $tool.path
    $lines += "| $($tool.name) | $installed | $version | $manual | $path |"
}

$lines += ""
$lines += "## Dataset Used For Competitor Timing"
$lines += ""
$lines += "- Profile: $($competitors.dataset.size)"
$lines += "- Root: $($competitors.dataset.root)"
$lines += "- Files: $(Format-Optional $competitors.dataset.files)"
$lines += "- Directories: $(Format-Optional $competitors.dataset.directories)"
$lines += "- Bytes: $(Format-Optional $competitors.dataset.bytes)"
$lines += ""
$lines += "## Notes"
$lines += ""
$lines += '- WinBlaze local baselines are single-run UI-driven measurements; Release medians are separate repeated-run measurements when `winblaze-release-medians.json` is present.'
$lines += '- Competitor timings are manual fields in `competitor-baselines.json`; blank values intentionally render as `not recorded`.'
$lines += "- Tool inventory is still useful because it records which comparison targets are locally available before timed runs."
$lines += "- To add manual timings, rerun benchmarks\record-competitor-baselines.ps1 with -WizTreeElapsedMs, -WinDirStatElapsedMs, or -EverythingElapsedMs, then regenerate this report."
$lines += "- Source WinBlaze baseline: $((Resolve-Path -LiteralPath $WinBlazeBaselinePath).Path)."
if ($release) {
    $lines += "- Source Release median baseline: $((Resolve-Path -LiteralPath $ReleaseBaselinePath).Path)."
}
$lines += "- Source competitor baseline: $((Resolve-Path -LiteralPath $CompetitorBaselinePath).Path)."

$parent = Split-Path -Parent $OutputPath
if ($parent) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$lines | Set-Content -Path $OutputPath -Encoding UTF8
Get-Content -LiteralPath $OutputPath
