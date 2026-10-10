#requires -Version 7.0
param(
    [Parameter(Mandatory=$true)][string]$BuildDirectory,
    [string]$OutputRoot = 'artifacts/os-xaml-package-contract'
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$build = [IO.Path]::GetFullPath($BuildDirectory, $repo)
$output = [IO.Path]::GetFullPath($OutputRoot, $repo)
$json = & (Join-Path $repo 'scripts/package-portable.ps1') -Configuration Release -UiHost OsXaml -BuildDirectory $build -OutputRoot $output
$package = $json | ConvertFrom-Json
$unpacked = Join-Path $output ('unpacked-' + [guid]::NewGuid().ToString('N'))
Expand-Archive -LiteralPath $package.zip -DestinationPath $unpacked
foreach ($file in @('WinBlaze.UI.exe', 'winblaze_native.dll', 'winblaze-cli.exe', 'winblaze-updater.exe')) {
    if ((Get-FileHash (Join-Path $build $file)).Hash -ne (Get-FileHash (Join-Path $unpacked $file)).Hash) {
        throw "Portable ZIP changed or lost $file"
    }
}
$fixture = Join-Path $output ('fixture-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'example.txt') -Value 'portable contract'
$previous = $env:LOCALAPPDATA
try {
    $env:LOCALAPPDATA = Join-Path $output ('appdata-' + [guid]::NewGuid().ToString('N'))
    & (Join-Path $unpacked 'winblaze-cli.exe') --scan $fixture (Join-Path $output 'scan.csv')
    if ($LASTEXITCODE -ne 0) { throw 'Portable CLI failed' }
    $rows = @(Import-Csv (Join-Path $output 'scan.csv'))
    if ($rows[0].Files -ne 1) { throw 'Portable CLI lost fixture files' }
} finally { $env:LOCALAPPDATA = $previous }
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'window-icons.ps1') -AppPath (Join-Path $unpacked 'WinBlaze.UI.exe')
if ($LASTEXITCODE -ne 0) { throw 'Extracted portable UI failed' }
& (Join-Path $PSScriptRoot '../native/cli-console-policy.ps1') -AppPath (Join-Path $unpacked 'winblaze-cli.exe') -OutputDirectory (Join-Path $output 'cli-console-contract')
if ($LASTEXITCODE -ne 0) { throw 'Extracted portable CLI lost its console contract' }
Write-Output 'PASS: portable ZIP preserves all four binaries and its extracted UI and CLI work'
