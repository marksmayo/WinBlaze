param(
    [int]$SampleFiles = 100000,
    [int64[]]$ProjectedFiles = @(1000000, 10000000, 50000000),
    [string]$OutputPath = (Join-Path $PSScriptRoot "index-scale-estimate.json")
)

$ErrorActionPreference = "Stop"

if ($SampleFiles -le 0) {
    throw "SampleFiles must be positive."
}

$json = cargo run -q -p winblaze-index --example index_memory -- $SampleFiles
if ($LASTEXITCODE -ne 0) {
    throw "index memory example failed with exit code $LASTEXITCODE"
}

$sample = $json | ConvertFrom-Json
$workingSetPerFile = [double]$sample.working_set_bytes_per_file
$snapshotPerFile = [double]$sample.snapshot_bytes_per_file
$elapsedPerFileMs = if ($sample.files -gt 0) { [double]$sample.elapsed_ms / [double]$sample.files } else { 0.0 }

$projections = @()
foreach ($count in $ProjectedFiles) {
    if ($count -le 0) {
        continue
    }
    $projections += [pscustomobject]@{
        files = $count
        projected_working_set_bytes = [int64]($workingSetPerFile * [double]$count)
        projected_snapshot_bytes = [int64]($snapshotPerFile * [double]$count)
        projected_elapsed_ms = [int64]($elapsedPerFileMs * [double]$count)
    }
}

$record = [pscustomobject]@{
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    sample = $sample
    assumptions = "Linear projection from synthetic FileRecord index-memory sample; validates budget planning only, not UI or filesystem traversal."
    projections = $projections
}

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$record | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
$record | ConvertTo-Json -Depth 6
