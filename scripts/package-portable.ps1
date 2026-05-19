param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",
    [string]$Platform = "x64",
    [string]$OutputRoot = (Join-Path $PSScriptRoot "..\artifacts\portable"),
    [switch]$IncludeSymbols,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$buildDir = Join-Path $repoRoot "src\WinBlaze.UI\bin\$Platform\$Configuration"
if (-not (Test-Path -LiteralPath $buildDir)) {
    throw "Build output not found: $buildDir"
}

$packageName = "WinBlaze-$Configuration-$Platform-portable"
$stageDir = Join-Path $OutputRoot $packageName
$zipPath = Join-Path $OutputRoot "$packageName.zip"

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
    "winblaze_native.dll",
    "WinBlaze.UI.pri",
    "WinBlaze.winmd"
)

foreach ($file in $requiredFiles) {
    $source = Join-Path $buildDir $file
    if (-not (Test-Path -LiteralPath $source)) {
        throw "Required build artifact missing: $source"
    }
    Copy-Item -LiteralPath $source -Destination $stageDir -Force
}

$optionalFiles = @(
    "resources.pri"
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
