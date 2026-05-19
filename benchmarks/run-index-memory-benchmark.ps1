param(
    [int]$Files = 10000,
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"

if ($Files -lt 0) {
    throw "Files must be non-negative."
}

$json = cargo run -q -p winblaze-index --example index_memory -- $Files
if ($LASTEXITCODE -ne 0) {
    throw "index memory example failed with exit code $LASTEXITCODE"
}

$record = $json | ConvertFrom-Json
if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
    $parent = Split-Path -Parent $OutputPath
    if ($parent) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $record | ConvertTo-Json -Depth 6 | Set-Content -Path $OutputPath -Encoding UTF8
}

$record | ConvertTo-Json -Depth 6
