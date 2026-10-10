#requires -Version 7.0
param([string]$OutputDirectory = 'artifacts/os-xaml-build-contract')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
& (Join-Path $repo 'scripts/build-os-xaml-ui.ps1') -OutputDirectory $OutputDirectory
if ($LASTEXITCODE -ne 0) { throw 'OS XAML build failed' }
$output = [IO.Path]::GetFullPath($OutputDirectory, $repo)
$inventory = Get-Content (Join-Path $output 'build-inventory.json') -Raw | ConvertFrom-Json
foreach ($buildInput in $inventory.inputs) {
    if ((Get-FileHash -LiteralPath (Join-Path $repo $buildInput.path)).Hash -ne $buildInput.sha256) {
        throw "Build used stale repository input: $($buildInput.path)"
    }
}
$app = Join-Path $output 'full-os-candidate/WinBlaze.UI.exe'
if ((Get-FileHash -LiteralPath $app).Hash -ne $inventory.ui_sha256) {
    throw 'Built app does not match its inventory'
}
& python (Join-Path $repo 'benchmarks/verify-startup-import-budget.py') $app
if ($LASTEXITCODE -ne 0) { throw 'Built app lost shipping shell first-use imports' }
foreach ($dependency in $inventory.delay_load_dependencies) {
    & python (Join-Path $repo 'benchmarks/verify-startup-imports.py') $dependency $app
    if ($LASTEXITCODE -ne 0) { throw "Built app lost first-use linkage: $dependency" }
}
foreach ($check in @('window-icons', 'path-ux')) {
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        (Join-Path $PSScriptRoot "$check.ps1"), '-AppPath', $app)
    if ($check -eq 'path-ux') { $arguments += @('-Case', 'window') }
    & powershell.exe @arguments
    if ($LASTEXITCODE -ne 0) { throw "Built app failed $check" }
}
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'path-ux.ps1') -AppPath $app -Case expand -RequireVisibleTree
if ($LASTEXITCODE -ne 0) { throw 'Built app lost usable folder expansion' }
& (Join-Path $PSScriptRoot 'os-xaml-focus.ps1') -BuildDirectory $output -OutputDirectory (Join-Path $output 'focus-contract')
if ($LASTEXITCODE -ne 0) { throw 'OS XAML host focus contract failed' }
& (Join-Path $PSScriptRoot 'os-xaml-lazy-panels.ps1') -BuildDirectory $output -OutputDirectory (Join-Path $output 'lazy-panel-contract')
if ($LASTEXITCODE -ne 0) { throw 'OS XAML lazy panel contract failed' }
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'unattended-png.ps1') -AppPath $app
if ($LASTEXITCODE -ne 0) { throw 'Built app lost unattended PNG export or normal shutdown' }
& (Join-Path $PSScriptRoot '../native/cli-console-policy.ps1') -AppPath (Join-Path $output 'full-os-candidate/winblaze-cli.exe') -OutputDirectory (Join-Path $output 'cli-console-contract')
if ($LASTEXITCODE -ne 0) { throw 'Built CLI lost its console allocation or terminal contract' }
Write-Output 'PASS: current repository inputs build an app with icons and usable restored scan controls'
