param(
    [string]$Version = "",
    [string]$Channel = "preview",
    [string]$PortablePath = (Join-Path $PSScriptRoot "..\artifacts\portable\WinBlaze-Release-x64-portable.zip"),
    [string]$InstallerPath = "",
    [string]$ReleaseNotesUrl = "",
    [string]$OutputPath = (Join-Path $PSScriptRoot "..\artifacts\release\winblaze-update-manifest.json")
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = powershell.exe -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "get-release-version.ps1")
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($Version)) {
        throw "Failed to resolve release version."
    }
    $Version = $Version.Trim()
}

function New-ArtifactRecord {
    param(
        [string]$Kind,
        [string]$Path
    )

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Release artifact not found: $Path"
    }

    $item = Get-Item -LiteralPath $Path
    $hash = Get-FileHash -LiteralPath $Path -Algorithm SHA256
    [pscustomobject]@{
        kind = $Kind
        file_name = $item.Name
        bytes = $item.Length
        sha256 = $hash.Hash.ToLowerInvariant()
    }
}

$artifacts = @()
$portable = New-ArtifactRecord -Kind "portable_zip" -Path $PortablePath
if ($portable) {
    $artifacts += $portable
}
$installer = New-ArtifactRecord -Kind "msi" -Path $InstallerPath
if ($installer) {
    $artifacts += $installer
}
if ($artifacts.Count -eq 0) {
    throw "No release artifacts were provided."
}

$manifest = [pscustomobject]@{
    schema_version = 1
    product = "WinBlaze"
    version = $Version
    channel = $Channel
    published_utc = (Get-Date).ToUniversalTime().ToString("o")
    release_notes_url = $ReleaseNotesUrl
    artifacts = $artifacts
}

$parent = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}

$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
$manifest | ConvertTo-Json -Depth 5
