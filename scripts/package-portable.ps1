param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",
    [string]$Platform = "x64",
    [ValidateSet("WinUI", "OsXaml")]
    [string]$UiHost = "WinUI",
    [string]$BuildDirectory = "",
    [string]$OutputRoot = (Join-Path $PSScriptRoot "..\artifacts\portable"),
    [switch]$IncludeSymbols,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
function Resolve-PackagePath {
    param([string]$Value)
    $path = if ([IO.Path]::IsPathRooted($Value)) { $Value } else { Join-Path $repoRoot $Value }
    return [IO.Path]::GetFullPath($path)
}
$buildDir = if ($BuildDirectory) {
    Resolve-PackagePath $BuildDirectory
} elseif ($UiHost -eq 'OsXaml') {
    Join-Path $repoRoot 'artifacts/os-xaml-ui/full-os-candidate'
} else {
    Join-Path $repoRoot "src\WinBlaze.UI\bin\$Platform\$Configuration"
}
if (-not (Test-Path -LiteralPath $buildDir)) {
    throw "Build output not found: $buildDir"
}

$packageName = "WinBlaze-$Configuration-$Platform-portable"
if ($UiHost -eq 'OsXaml') {
    $packageName = "WinBlaze-$Configuration-$Platform-os-xaml-portable"
    $inventoryPath = Join-Path (Split-Path $buildDir -Parent) 'build-inventory.json'
    $inventory = Get-Content -LiteralPath $inventoryPath -Raw | ConvertFrom-Json
    if ($inventory.host -ne 'OS XAML') { throw 'Build inventory does not describe an OS XAML app' }
    $binaryNames = @{ui='WinBlaze.UI.exe';native='winblaze_native.dll';cli='winblaze-cli.exe';updater='winblaze-updater.exe'}
    foreach ($component in $binaryNames.Keys) {
        $binary = Join-Path $buildDir $binaryNames[$component]
        if ((Get-FileHash -LiteralPath $binary).Hash -ne $inventory.($component + '_sha256')) {
            throw "Build inventory does not match $binary"
        }
    }
}
$OutputRoot = Resolve-PackagePath $OutputRoot
$stageDir = [IO.Path]::GetFullPath((Join-Path $OutputRoot $packageName))
$zipPath = [IO.Path]::GetFullPath((Join-Path $OutputRoot "$packageName.zip"))
$outputPrefix = $OutputRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if (!$stageDir.StartsWith($outputPrefix, [StringComparison]::OrdinalIgnoreCase) -or
    !$zipPath.StartsWith($outputPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Package paths must stay inside the requested output directory'
}

if ($Clean -and (Test-Path -LiteralPath $stageDir)) {
    Remove-Item -LiteralPath $stageDir -Recurse -Force
}
if ($Clean -and (Test-Path -LiteralPath $zipPath)) {
    Remove-Item -LiteralPath $zipPath -Force
}

New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $stageDir "docs") | Out-Null

$requiredFiles = @(
    "WinBlaze.UI.exe",
    "winblaze_native.dll"
)
if ($UiHost -eq 'WinUI') { $requiredFiles += 'WinBlaze.UI.pri' }

foreach ($file in $requiredFiles) {
    $source = Join-Path $buildDir $file
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Required build artifact missing: $source"
    }
    Copy-Item -LiteralPath $source -Destination $stageDir -Force
}

# The update helper that applies downloaded updates (a portable app can't
# overwrite its own running exe). Always the release build — it is a tiny,
# config-independent std-only binary.
$updaterExe = Join-Path $repoRoot "target\release\winblaze-updater.exe"
if ($UiHost -eq 'OsXaml') { $updaterExe = Join-Path $buildDir 'winblaze-updater.exe' }
if (-not (Test-Path -LiteralPath $updaterExe)) {
    throw "Updater missing: $updaterExe (build with: cargo build --release -p winblaze-updater)"
}
Copy-Item -LiteralPath $updaterExe -Destination $stageDir -Force

$cliExe = Join-Path $repoRoot "target\release\winblaze-cli.exe"
if ($UiHost -eq 'OsXaml') { $cliExe = Join-Path $buildDir 'winblaze-cli.exe' }
if (-not (Test-Path -LiteralPath $cliExe)) { throw "CLI missing: build with cargo build --release -p winblaze-native --bin winblaze-cli" }
Copy-Item -LiteralPath $cliExe -Destination $stageDir -Force
if ($UiHost -eq 'OsXaml') { Copy-Item -LiteralPath $inventoryPath -Destination $stageDir -Force }


$optionalFiles = @(
    "resources.pri",
    "WinBlaze.winmd"
)
foreach ($file in $optionalFiles) {
    $source = Join-Path $buildDir $file
    if (Test-Path -LiteralPath $source) {
        Copy-Item -LiteralPath $source -Destination $stageDir -Force
    }
}

$runtimePatterns = @(
    "Microsoft.*.dll",
    "Microsoft.*.winmd",
    "WebView2Loader.dll"
)
foreach ($pattern in $runtimePatterns) {
    Get-ChildItem -LiteralPath $buildDir -Filter $pattern -File |
        Copy-Item -Destination $stageDir -Force
}

if ($IncludeSymbols) {
    Get-ChildItem -LiteralPath $buildDir -Filter "*.pdb" -File |
        Copy-Item -Destination $stageDir -Force
}

$docFiles = @(
    "README.md",
    "docs\TROUBLESHOOTING.md",
    "docs\PACKAGING.md",
    "docs\RELEASE_CHECKLIST.md"
)
foreach ($doc in $docFiles) {
    $source = Join-Path $repoRoot $doc
    if (Test-Path -LiteralPath $source) {
        Copy-Item -LiteralPath $source -Destination (Join-Path $stageDir "docs") -Force
    }
}

if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}
$archiveInputs = Get-ChildItem -LiteralPath $stageDir | ForEach-Object { $_.FullName }
Compress-Archive -LiteralPath $archiveInputs -DestinationPath $zipPath -Force

$files = Get-ChildItem -LiteralPath $stageDir -Recurse -File
[pscustomobject]@{
    package = $packageName
    stage_dir = (Resolve-Path -LiteralPath $stageDir).Path
    zip = (Resolve-Path -LiteralPath $zipPath).Path
    file_count = $files.Count
    zip_bytes = (Get-Item -LiteralPath $zipPath).Length
} | ConvertTo-Json
