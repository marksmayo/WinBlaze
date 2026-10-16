[CmdletBinding()]
param([string]$OutputPath = '', [string]$HistoryPath = '')
# On-disk footprint (#50): bytes of the files each product needs to run its
# 64-bit GUI. WinBlaze: the shipped OS XAML host's required files
# (scripts/package-portable.ps1); WinDirStat: x64\WinDirStat.exe; WizTree:
# WizTree64.exe. Deterministic, so one row per product per binary identity.
$ErrorActionPreference = 'Stop'
if (!$OutputPath) { $OutputPath = Join-Path $PSScriptRoot 'current-footprint-results.json' }
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$wds = if ($env:WINBLAZE_BENCH_WINDIRSTAT) { $env:WINBLAZE_BENCH_WINDIRSTAT } else { 'C:\Program Files\WinDirStat\WinDirStat.exe' }
$wiz = if ($env:WINBLAZE_BENCH_WIZTREE) { $env:WINBLAZE_BENCH_WIZTREE } else { 'C:\Program Files\WizTree\WizTree64.exe' }
$ui = Join-Path $repo 'src/WinBlaze.UI/bin/x64/Release'
$products = [ordered]@{
    WinBlaze = @{ identity = Join-Path $ui 'WinBlaze.UI.exe'; files = @((Join-Path $ui 'WinBlaze.UI.exe'), (Join-Path $ui 'winblaze_native.dll')) }
    WinDirStat = @{ identity = $wds; files = @($wds) }
    WizTree = @{ identity = $wiz; files = @($wiz) }
}
# Added products: the executable each needs for its 64-bit GUI (Everything's
# language pack, es.exe and TreeSize's optional context-menu files excluded).
. (Join-Path $PSScriptRoot 'scan-adapters.ps1')
foreach ($tool in 'TreeSize Free', 'SpaceSniffer', 'Everything', 'Directory Report') {
    $exe = Get-BenchToolExecutable $tool
    if (Test-Path -LiteralPath $exe) { $products[$tool] = @{ identity = $exe; files = @($exe) } }
}
# Ten measurements per product (re-reading the files each time) satisfy the
# report's n=10 rule; the values are deterministic.
$rows = foreach ($tool in $products.Keys) {
    $options = @{ Tool = $tool; Executable = $products[$tool].identity }
    if ($HistoryPath) { $options.HistoryPath = $HistoryPath }
    $identity = Get-BenchmarkIdentity @options
    foreach ($run in 1..10) {
    [pscustomobject][ordered]@{
        tool = $tool; run = $run; binary = $identity; started_utc = [DateTime]::UtcNow.ToString('o')
        scope = 'Bytes of the files required to run the 64-bit GUI (excludes optional languages and 32-bit builds)'
        files = @($products[$tool].files | ForEach-Object { [IO.Path]::GetFileName($_) })
        bytes = [long](($products[$tool].files | ForEach-Object { (Get-Item -LiteralPath $_).Length } | Measure-Object -Sum).Sum)
    }
    }
}
Write-BenchmarkJson -Path $OutputPath -Value @($rows)
$rows | Where-Object run -eq 1 | ForEach-Object { '{0}: {1:N2} MiB ({2})' -f $_.tool, ($_.bytes / 1MB), ($_.files -join ', ') }
