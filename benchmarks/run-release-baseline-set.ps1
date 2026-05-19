param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\src\WinBlaze.UI\bin\x64\Release\WinBlaze.UI.exe"),
    [string]$DatasetRoot = "C:\tmp\WinBlazeBench",
    [string[]]$Profiles = @("tiny", "fanout", "scale"),
    [int]$Runs = 3,
    [int]$TimeoutSeconds = 120,
    [string]$OutputPath = (Join-Path $PSScriptRoot "winblaze-release-medians.json"),
    [switch]$GenerateDatasets
)

$ErrorActionPreference = "Stop"

$Profiles = @($Profiles | ForEach-Object { $_ -split "," } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
$validProfiles = @("tiny", "small", "medium", "fanout", "fanout-large", "scale")
foreach ($profile in $Profiles) {
    if ($validProfiles -notcontains $profile) {
        throw "Unknown profile: $profile"
    }
}

$resolvedAppPath = (Resolve-Path -LiteralPath $AppPath).Path
$results = @()
foreach ($profile in $Profiles) {
    $suiteArgs = @{
        AppPath = $resolvedAppPath
        Size = $profile
        Runs = $Runs
        DatasetRoot = $DatasetRoot
        TimeoutSeconds = $TimeoutSeconds
    }
    if ($GenerateDatasets) {
        $suiteArgs.GenerateDataset = $true
    }

    $json = & (Join-Path $PSScriptRoot "run-ui-benchmark-suite.ps1") @suiteArgs
    $results += ($json | ConvertFrom-Json)
}

$record = [pscustomobject]@{
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    app_path = $resolvedAppPath
    dataset_root = $DatasetRoot
    profiles = $Profiles
    runs_per_profile = $Runs
    results = $results
}

$parent = Split-Path -Parent $OutputPath
if ($parent) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
$record | ConvertTo-Json -Depth 10 | Set-Content -Path $OutputPath -Encoding UTF8
$record | ConvertTo-Json -Depth 10
