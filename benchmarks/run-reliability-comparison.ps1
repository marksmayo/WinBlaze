[CmdletBinding()]
param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [ValidateRange(1,100)][int]$ScansPerRun = 10,
    [ValidateSet('WinBlaze','WinDirStat','WizTree','SpaceSniffer','Everything','Directory Report','TreeSize Free')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree'),
    [int]$TimeoutSeconds = 120,
    [string]$OutputPath = '',
    [string]$HistoryPath = ''
)
# Crash-free rate (EXTENDED_MEDALS_PLAN.md #70): $Runs x $ScansPerRun standard-user
# scan/export runs per product, rotating over the hostile extended datasets (deep
# paths, junction maze, access-denied subtree, alternate data streams, Unicode
# names). A scan succeeds when the process exits within the timeout with exit code
# 0 and writes an export with the root row; anything else (crash, hang, missing or
# empty output) is a failure. Each row reports one run's success percentage.
$ErrorActionPreference = 'Stop'
if (!$OutputPath) { $OutputPath = Join-Path $PSScriptRoot 'current-reliability-results.json' }
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$datasets = @('deep','junctions','denied','ads','unicode') | ForEach-Object {
    $path = [IO.Path]::GetFullPath((Join-Path $repo "artifacts/extended-datasets/$_"))
    if (!(Test-Path -LiteralPath $path)) { throw "Dataset $_ is missing" }
    $path
}
$executables = @{
    WinBlaze = [IO.Path]::GetFullPath((Join-Path $repo 'target/release/winblaze-cli.exe'))
    WinDirStat = if ($env:WINBLAZE_BENCH_WINDIRSTAT) { $env:WINBLAZE_BENCH_WINDIRSTAT } else { 'C:\Program Files\WinDirStat\WinDirStat.exe' }
    WizTree = if ($env:WINBLAZE_BENCH_WIZTREE) { $env:WINBLAZE_BENCH_WIZTREE } else { 'C:\Program Files\WizTree\WizTree64.exe' }
}
. (Join-Path $PSScriptRoot 'scan-adapters.ps1')
foreach ($tool in $Tools) { if (Test-ScanAdapterTool $tool) { $executables[$tool] = Get-BenchToolExecutable $tool } }
$work = Join-Path $repo 'artifacts/reliability'
New-Item -ItemType Directory -Force -Path $work | Out-Null

function Invoke-Scan([string]$tool, [string]$target, [string]$csv) {
    if (Test-ScanAdapterTool $tool) {
        $plan = New-ScanPlan $tool $target $csv
        $companion = $null
        try {
            $process = Start-Process -FilePath $plan.Executable -ArgumentList $plan.Arguments -PassThru -WindowStyle Hidden
            $companion = if ($plan.CompanionExecutable) { Start-Process -FilePath $plan.CompanionExecutable -ArgumentList $plan.CompanionArguments -WindowStyle Hidden -PassThru }
            if (!$process.WaitForExit($TimeoutSeconds * 1000)) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue; return 'timeout' }
            if ($process.ExitCode -ne 0) { return "exit $($process.ExitCode)" }
            if (!(Test-Path -LiteralPath $csv)) { return 'no output' }
            # Each adapter's totals reader knows its output format (a root row,
            # a summary block or a status line); unreadable totals fail the scan.
            try { $totals = Read-ScanTotals $plan } catch { return 'unreadable output' }
            if (!($totals.files -ge 0)) { return 'no file count' }
            return 'ok'
        } finally {
            # The whole tree: a cmd.exe companion's children (es.exe) outlive it otherwise.
            if ($companion -and !$companion.HasExited) { & taskkill.exe /PID $companion.Id /T /F 2>&1 | Out-Null }
            Remove-ScanOutputs $plan
        }
    }
    $argumentText = switch ($tool) {
        'WinBlaze' { '"{0}" "{1}"' -f $target, $csv }
        'WinDirStat' { '/SaveTo "{0}" "{1}"' -f $csv, $target }
        'WizTree' { '"{0}" /admin=0 /export="{1}"' -f $target, $csv }
    }
    $process = Start-Process -FilePath $executables[$tool] -ArgumentList $argumentText -PassThru -WindowStyle Hidden
    if (!$process.WaitForExit($TimeoutSeconds * 1000)) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        return 'timeout'
    }
    if ($process.ExitCode -ne 0) { return "exit $($process.ExitCode)" }
    if (!(Test-Path -LiteralPath $csv)) { return 'no output' }
    $root = $target.TrimEnd('\')
    $head = Get-Content -LiteralPath $csv -TotalCount 4 -Encoding UTF8
    if (!($head | Where-Object { $_ -like "*$root*" })) { return 'root row missing' }
    return 'ok'
}

$rows = [Collections.Generic.List[object]]::new()
if (Test-Path -LiteralPath $OutputPath) {
    foreach ($row in @((Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json) | ForEach-Object { $_ })) {
        if (!($row.tool -in $Tools)) { $rows.Add($row) }
    }
}
foreach ($tool in $Tools) {
    $identityOptions = @{ Tool = $tool; Executable = $executables[$tool] }
    if ($HistoryPath) { $identityOptions.HistoryPath = $HistoryPath }
    $identity = Get-BenchmarkIdentity @identityOptions
    for ($run = 1; $run -le $Runs; $run++) {
        $outcomes = for ($scan = 0; $scan -lt $ScansPerRun; $scan++) {
            $target = $datasets[$scan % $datasets.Count]
            $csv = Join-Path $work "$tool-$run-$scan.csv"
            Remove-Item -LiteralPath $csv -ErrorAction SilentlyContinue
            $outcome = Invoke-Scan $tool $target $csv
            Remove-Item -LiteralPath $csv -ErrorAction SilentlyContinue
            [pscustomobject]@{ dataset = [IO.Path]::GetFileName($target); outcome = $outcome }
        }
        $ok = @($outcomes | Where-Object outcome -eq 'ok').Count
        $rows.Add([pscustomobject][ordered]@{
            tool = $tool; run = $run; binary = $identity; started_utc = [DateTime]::UtcNow.ToString('o')
            scope = "$ScansPerRun standard-user scan/export runs rotating over deep, junctions, denied, ads and unicode"
            scans = $ScansPerRun; succeeded = $ok; success_percent = [math]::Round(100.0 * $ok / $ScansPerRun, 2); failure_percent = [math]::Round(100.0 * ($ScansPerRun - $ok) / $ScansPerRun, 2)
            failures = @($outcomes | Where-Object outcome -ne 'ok')
        })
        Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
        Write-Output ("{0} reliability run {1}: {2}/{3} scans succeeded" -f $tool, $run, $ok, $ScansPerRun)
    }
}
