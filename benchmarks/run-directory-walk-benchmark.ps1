param(
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale")]
    [string]$Size = "tiny",
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [switch]$GenerateDataset,
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

if ($GenerateDataset) {
    & (Join-Path $PSScriptRoot "make-datasets.ps1") -Root $DatasetRoot -Size $Size -Clean | Out-Null
}

$datasetPath = Join-Path $DatasetRoot $Size
$manifestPath = Join-Path $DatasetRoot "$Size.manifest.json"
Assert-True (Test-Path -LiteralPath $datasetPath -PathType Container) "Dataset not found: $datasetPath"
Assert-True (Test-Path -LiteralPath $manifestPath -PathType Leaf) "Dataset manifest not found: $manifestPath"

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$json = cargo run -q -p winblaze-scanner --example directory_walk_benchmark -- $datasetPath
if ($LASTEXITCODE -ne 0) {
    throw "directory walk benchmark failed with exit code $LASTEXITCODE"
}

$result = $json | ConvertFrom-Json
Assert-True ($result.completed -eq $true) "Directory-walk scan did not complete"
Assert-True ($result.failed -eq $false) "Directory-walk scan failed: $($result.failure_message)"
Assert-True ([int64]$result.summary_files -eq [int64]$manifest.files) "Expected $($manifest.files) files, got $($result.summary_files)"
Assert-True ([int64]$result.summary_directories -eq [int64]$manifest.directories) "Expected $($manifest.directories) directories, got $($result.summary_directories)"
Assert-True ([int64]$result.summary_bytes -eq [int64]$manifest.bytes) "Expected $($manifest.bytes) bytes, got $($result.summary_bytes)"

$output = [pscustomobject]@{
    profile = $Size
    root = $datasetPath
    elapsed_ms = $result.elapsed_ms
    files = $result.summary_files
    directories = $result.summary_directories
    bytes = $result.summary_bytes
    issues = $result.issues
    issues_by_kind = $result.issues_by_kind
    recent_issues = $result.recent_issues
    files_per_second = $result.files_per_second
}

$jsonOutput = $output | ConvertTo-Json -Depth 8
if ($OutputPath) {
    $resolvedOutputPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath)
    $outputDirectory = Split-Path -Parent $resolvedOutputPath
    if ($outputDirectory -and -not (Test-Path -LiteralPath $outputDirectory -PathType Container)) {
        New-Item -ItemType Directory -Path $outputDirectory | Out-Null
    }
    Set-Content -LiteralPath $resolvedOutputPath -Value $jsonOutput -Encoding UTF8
}

$jsonOutput
