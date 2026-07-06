param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",
    [string]$Platform = "x64",
    [string]$MsBuildPath = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe",
    [switch]$SkipUiSmoke,
    [switch]$SkipNegativeUiSmoke,
    [switch]$AutoSkipUiSmokeIfUnavailable,
    [switch]$SkipBenchmarks,
    [switch]$SkipPackage,
    [switch]$RunBenchmarkBudgets,
    [switch]$RecordCompetitors,
    [switch]$CheckSigning,
    [switch]$CheckInstaller,
    [switch]$PackageInstaller
)

$ErrorActionPreference = "Stop"

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Command
    )

    Write-Host "== $Name =="
    & $Command
}

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$projectPath = Join-Path $repoRoot "src\WinBlaze.UI\WinBlaze.UI.vcxproj"
$appPath = Join-Path $repoRoot "src\WinBlaze.UI\bin\$Platform\$Configuration\WinBlaze.UI.exe"
$budgetPath = Join-Path $repoRoot "benchmarks\performance-budgets.json"
if ($Configuration -eq "Release") {
    $budgetPath = Join-Path $repoRoot "benchmarks\performance-budgets.release.json"
}
$budgets = Get-Content -LiteralPath $budgetPath -Raw | ConvertFrom-Json
$tinyBudget = $budgets.profiles.tiny

# Lint gates first (fast, and mirror the CI "Rust checks" job) so formatting
# and clippy failures surface locally instead of only in Actions.
Invoke-Step "Rust format" {
    cargo fmt --all --check
}

Invoke-Step "Rust clippy" {
    cargo clippy --workspace --all-targets --all-features -- -D warnings
}

Invoke-Step "Rust tests" {
    cargo test -q
}

Invoke-Step "Rust examples" {
    cargo test -q --examples
}

Invoke-Step "WinUI build" {
    & $MsBuildPath $projectPath /p:Configuration=$Configuration /p:Platform=$Platform /m /nologo /v:minimal
}

if (-not $SkipUiSmoke) {
    $script:uiPrereqs = $null
    Invoke-Step "UI automation prerequisites" {
        $json = powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "tests\ui\check-prereqs.ps1") -AppPath $appPath
        $json
        $script:uiPrereqs = $json | ConvertFrom-Json
    }

    if ($AutoSkipUiSmokeIfUnavailable -and -not $script:uiPrereqs.ui_smoke_ready) {
        Write-Host "Skipping UI smoke because UI Automation prerequisites are unavailable."
        $SkipUiSmoke = $true
        $SkipNegativeUiSmoke = $true
    }
}

if (-not $SkipUiSmoke) {
    Invoke-Step "UI smoke" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "tests\ui\smoke.ps1") -AppPath $appPath
    }
}

if (-not $SkipNegativeUiSmoke) {
    Invoke-Step "Negative UI smoke" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "tests\ui\negative.ps1") -AppPath $appPath
    }
}

if (-not $SkipBenchmarks) {
    Invoke-Step "Tiny UI benchmark" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "benchmarks\run-ui-benchmark.ps1") -AppPath $appPath -Size tiny -GenerateDataset -MaxElapsedMs ([int]$tinyBudget.max_elapsed_ms) -MaxWorkingSetMb ([int]$tinyBudget.max_working_set_mb) -MaxPeakFrameMs ([int]$tinyBudget.max_peak_frame_ms) -MaxPeakFlushMs ([int]$tinyBudget.max_peak_flush_ms)
    }

    if ($RunBenchmarkBudgets) {
        Invoke-Step "Budgeted UI benchmarks" {
            powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "benchmarks\run-baseline-set.ps1") -AppPath $appPath -Profiles tiny,fanout,fanout-large,scale -GenerateDatasets -EnforceBudgets -BudgetPath $budgetPath -TimeoutSeconds 120
        }
    }
}

if ($RecordCompetitors) {
    Invoke-Step "Competitor inventory" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "benchmarks\record-competitor-baselines.ps1") -Size tiny
    }
}

if ($CheckSigning) {
    Invoke-Step "Signing prerequisites" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\check-signing-prereqs.ps1")
    }
}

if ($CheckInstaller) {
    Invoke-Step "Installer prerequisites" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\check-installer-prereqs.ps1")
    }

    Invoke-Step "Installer staging validation" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\package-installer.ps1") -Configuration $Configuration -Platform $Platform -ValidateOnly
    }
}

if (-not $SkipPackage) {
    Invoke-Step "Portable package" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\package-portable.ps1") -Configuration $Configuration -Platform $Platform -Clean
    }
}

if ($PackageInstaller) {
    Invoke-Step "Installer package" {
        powershell.exe -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\package-installer.ps1") -Configuration $Configuration -Platform $Platform
    }
}

Write-Host "Local check completed."
