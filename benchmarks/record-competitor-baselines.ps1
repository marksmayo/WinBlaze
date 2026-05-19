param(
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale")]
    [string]$Size = "tiny",
    [string]$OutputPath = (Join-Path $PSScriptRoot "competitor-baselines.json"),
    [string]$WizTreeElapsedMs,
    [string]$WinDirStatElapsedMs,
    [string]$EverythingElapsedMs,
    [string]$Notes = "Manual competitor timings are optional; blank values mean not recorded."
)

$ErrorActionPreference = "Stop"

function Find-Tool {
    param(
        [string]$Name,
        [string[]]$CandidatePaths
    )

    foreach ($path in $CandidatePaths) {
        if (Test-Path -LiteralPath $path) {
            $item = Get-Item -LiteralPath $path
            return [pscustomobject]@{
                name = $Name
                installed = $true
                path = $item.FullName
                version = $item.VersionInfo.ProductVersion
            }
        }
    }

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        $item = Get-Item -LiteralPath $command.Source
        return [pscustomobject]@{
            name = $Name
            installed = $true
            path = $item.FullName
            version = $item.VersionInfo.ProductVersion
        }
    }

    [pscustomobject]@{
        name = $Name
        installed = $false
        path = $null
        version = $null
    }
}

function Convert-OptionalInt {
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $null
    }
    return [int]$Value
}

$manifestPath = Join-Path $DatasetRoot "$Size.manifest.json"
$manifest = $null
if (Test-Path -LiteralPath $manifestPath) {
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
}

$tools = @(
    Find-Tool -Name "WizTree" -CandidatePaths @(
        "C:\Program Files\WizTree\WizTree64.exe",
        "C:\Program Files\WizTree\WizTree.exe",
        "C:\Program Files (x86)\WizTree\WizTree.exe"
    )
    Find-Tool -Name "WinDirStat" -CandidatePaths @(
        "C:\Program Files\WinDirStat\WinDirStat.exe",
        "C:\Program Files (x86)\WinDirStat\WinDirStat.exe"
    )
    Find-Tool -Name "Everything" -CandidatePaths @(
        "C:\Program Files\Everything\Everything.exe",
        "C:\Program Files (x86)\Everything\Everything.exe"
    )
)

$record = [pscustomobject]@{
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    dataset = [pscustomobject]@{
        size = $Size
        root = if ($manifest) { $manifest.root } else { Join-Path $DatasetRoot $Size }
        files = if ($manifest) { [int]$manifest.files } else { $null }
        directories = if ($manifest) { [int]$manifest.directories } else { $null }
        bytes = if ($manifest) { [int64]$manifest.bytes } else { $null }
    }
    tools = $tools
    manual_timings_ms = [pscustomobject]@{
        wiztree = Convert-OptionalInt $WizTreeElapsedMs
        windirstat = Convert-OptionalInt $WinDirStatElapsedMs
        everything = Convert-OptionalInt $EverythingElapsedMs
    }
    notes = $Notes
}

$parent = Split-Path -Parent $OutputPath
if ($parent) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$record | ConvertTo-Json -Depth 6 | Set-Content -Path $OutputPath -Encoding UTF8
$record | ConvertTo-Json -Depth 6
