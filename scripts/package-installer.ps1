param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",
    [string]$Platform = "x64",
    [string]$Version = "",
    [string]$SourceDir = "",
    [string]$OutputDir = (Join-Path $PSScriptRoot "..\artifacts\installer"),
    [switch]$ValidateOnly
)

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

function Assert-FileExists {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Installer source file missing: $Path"
    }
}

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = powershell.exe -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "get-release-version.ps1")
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($Version)) {
        throw "Failed to resolve release version."
    }
    $Version = $Version.Trim()
}
if ([string]::IsNullOrWhiteSpace($SourceDir)) {
    $SourceDir = Join-Path $root "artifacts\portable\WinBlaze-$Configuration-$Platform-portable"
}

if (-not (Test-Path -LiteralPath $SourceDir -PathType Container)) {
    & (Join-Path $PSScriptRoot "package-portable.ps1") -Configuration $Configuration -Platform $Platform -Clean
}

$resolvedSourceDir = (Resolve-Path -LiteralPath $SourceDir).Path
$requiredFiles = @(
    "WinBlaze.UI.exe",
    "winblaze_native.dll",
    "WinBlaze.UI.pri",
    "WinBlaze.winmd",
    "WebView2Loader.dll",
    "Microsoft.Web.WebView2.Core.dll",
    "Microsoft.Web.WebView2.Core.Projection.dll",
    "Microsoft.Web.WebView2.Core.winmd",
    "Microsoft.WindowsAppRuntime.Bootstrap.dll",
    "Microsoft.Windows.ApplicationModel.Background.UniversalBGTask.dll",
    "docs\PACKAGING.md",
    "docs\README.md",
    "docs\RELEASE_CHECKLIST.md",
    "docs\TROUBLESHOOTING.md"
)

foreach ($relative in $requiredFiles) {
    Assert-FileExists (Join-Path $resolvedSourceDir $relative)
}

if ($ValidateOnly) {
    [pscustomobject]@{
        configuration = $Configuration
        platform = $Platform
        version = $Version
        source_dir = $resolvedSourceDir
        required_files_checked = $requiredFiles.Count
        installer_source = (Join-Path $root "installer\WinBlaze.wxs")
        validate_only = $true
        ready_for_wix = $true
    } | ConvertTo-Json
    return
}

$wix = Find-WixExe
if ([string]::IsNullOrWhiteSpace($wix)) {
    throw "WiX v4+ wix.exe was not found. Install WiX v4 or run scripts\check-installer-prereqs.ps1 for details."
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$resolvedOutputDir = (Resolve-Path -LiteralPath $OutputDir).Path
$outputPath = Join-Path $resolvedOutputDir "WinBlaze-$Configuration-$Platform-$Version.msi"
$wxsPath = Join-Path $root "installer\WinBlaze.wxs"

& $wix build `
    $wxsPath `
    -d "SourceDir=$resolvedSourceDir" `
    -d "Version=$Version" `
    -arch x64 `
    -o $outputPath

if ($LASTEXITCODE -ne 0) {
    throw "WiX installer build failed with exit code $LASTEXITCODE."
}

[pscustomobject]@{
    configuration = $Configuration
    platform = $Platform
    version = $Version
    source_dir = $resolvedSourceDir
    installer_path = $outputPath
} | ConvertTo-Json
