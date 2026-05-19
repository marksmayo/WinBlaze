param()

$ErrorActionPreference = "Stop"

function Find-CommandPath {
    param([string]$Name)
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }
    return $null
}

function Find-WixExe {
    $command = Find-CommandPath "wix.exe"
    if ($command) {
        return $command
    }

    $roots = @(
        "${env:ProgramFiles}\WiX Toolset",
        "${env:ProgramFiles(x86)}\WiX Toolset"
    )
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) {
            continue
        }
        $match = Get-ChildItem -LiteralPath $root -Recurse -Filter wix.exe -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($match) {
            return $match.FullName
        }
    }

    return $null
}

$wix = Find-WixExe
$candle = Find-CommandPath "candle.exe"
$light = Find-CommandPath "light.exe"
$dotnet = Find-CommandPath "dotnet.exe"
$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$installerSource = Join-Path $repoRoot "installer\WinBlaze.wxs"
$installerScript = Join-Path $repoRoot "scripts\package-installer.ps1"

[pscustomobject]@{
    wix_v4_or_newer_found = -not [string]::IsNullOrWhiteSpace($wix)
    wix_path = $wix
    wix_v3_candle_found = -not [string]::IsNullOrWhiteSpace($candle)
    candle_path = $candle
    wix_v3_light_found = -not [string]::IsNullOrWhiteSpace($light)
    light_path = $light
    dotnet_found = -not [string]::IsNullOrWhiteSpace($dotnet)
    dotnet_path = $dotnet
    installer_source_found = Test-Path -LiteralPath $installerSource -PathType Leaf
    installer_source_path = $installerSource
    installer_package_script_found = Test-Path -LiteralPath $installerScript -PathType Leaf
    installer_package_script_path = $installerScript
    installed_build_ready = (-not [string]::IsNullOrWhiteSpace($wix)) -or
        ((-not [string]::IsNullOrWhiteSpace($candle)) -and (-not [string]::IsNullOrWhiteSpace($light)))
} | ConvertTo-Json
