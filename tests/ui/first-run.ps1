param(
    [string]$AppPath = (Join-Path $PSScriptRoot "..\..\src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe"),
    [string]$LocalAppDataRoot = "C:\tmp\WinBlazeFirstRunLocalAppData"
)

$ErrorActionPreference = "Stop"

if (Test-Path -LiteralPath $LocalAppDataRoot) {
    Remove-Item -LiteralPath $LocalAppDataRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $LocalAppDataRoot | Out-Null

$previousLocalAppData = $env:LOCALAPPDATA
try {
    $env:LOCALAPPDATA = $LocalAppDataRoot
    & (Join-Path $PSScriptRoot "smoke.ps1") -AppPath $AppPath

    $indexPath = Join-Path $LocalAppDataRoot "WinBlaze\index\winblaze.index.bin"
    $eventsPath = Join-Path $LocalAppDataRoot "WinBlaze\logs\events.jsonl"
    if (-not (Test-Path -LiteralPath $indexPath)) {
        throw "First-run smoke did not create index snapshot: $indexPath"
    }
    if (-not (Test-Path -LiteralPath $eventsPath)) {
        throw "First-run smoke did not create events log: $eventsPath"
    }

    [pscustomobject]@{
        Result = "OK"
        LocalAppDataRoot = $LocalAppDataRoot
        IndexBytes = (Get-Item -LiteralPath $indexPath).Length
        EventsBytes = (Get-Item -LiteralPath $eventsPath).Length
    } | Format-List
}
finally {
    $env:LOCALAPPDATA = $previousLocalAppData
}
