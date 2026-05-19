param(
    [switch]$PackageVersion
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$manifestPath = Join-Path $repoRoot "src\WinBlaze.UI\Package.appxmanifest"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "App manifest not found: $manifestPath"
}

[xml]$manifest = Get-Content -LiteralPath $manifestPath -Raw
$identity = $manifest.Package.Identity
if (-not $identity -or [string]::IsNullOrWhiteSpace($identity.Version)) {
    throw "Package identity version was not found in $manifestPath"
}

$version = [string]$identity.Version
if (-not $PackageVersion) {
    $parts = $version.Split(".")
    if ($parts.Count -ge 3) {
        $version = "$($parts[0]).$($parts[1]).$($parts[2])"
    }
}

$version
