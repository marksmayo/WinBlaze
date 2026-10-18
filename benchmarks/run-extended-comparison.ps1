[CmdletBinding()]
param(
    # Dataset names from make-extended-datasets.py, or 'full' (C:\), 'windows' (C:\Windows), 'profile' (user profile).
    [Parameter(Mandatory)][string[]]$Targets,
    [ValidateRange(1,100)][int]$Runs = 10,
    [ValidateSet('WinBlaze','WinDirStat','WizTree','SpaceSniffer','Everything','Directory Report','TreeSize Free')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree'),
    [int]$TimeoutSeconds = 900,
    [string]$OutputPath = (Join-Path $PSScriptRoot 'current-extended-medal-results.json'),
    [string]$HistoryPath = '',
    # Keep existing rows for the selected targets/tools and continue from the next run number.
    [switch]$Resume,
    # Fixture (manifest) targets are capped at this limit, below -TimeoutSeconds.
    [int]$FixtureTimeoutSeconds = 300,
    # Only cohorts that timed out: replace the timeout row with one more run
    # (limit -TimeoutSeconds, fixtures included), so a slow product records a
    # value instead of a failure. Implies -Resume.
    [switch]$RetryTimedOut
)
# Whole-process startup + scan + CSV export for the extended medal events,
# measured with exact kernel counters (process-measure.ps1). Rows for the
# selected targets/tools are replaced; all other rows are kept.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
. (Join-Path $PSScriptRoot 'process-measure.ps1')
. (Join-Path $PSScriptRoot 'scan-adapters.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$exports = Join-Path $repo 'artifacts/benchmark-exports'
New-Item -ItemType Directory -Force -Path $exports | Out-Null
$executables = @{
    WinBlaze = [IO.Path]::GetFullPath((Join-Path $repo 'target/release/winblaze-cli.exe'))
    WinDirStat = if ($env:WINBLAZE_BENCH_WINDIRSTAT) { $env:WINBLAZE_BENCH_WINDIRSTAT } else { 'C:\Program Files\WinDirStat\WinDirStat.exe' }
    WizTree = if ($env:WINBLAZE_BENCH_WIZTREE) { $env:WINBLAZE_BENCH_WIZTREE } else { 'C:\Program Files\WizTree\WizTree64.exe' }
}
foreach ($tool in $Tools) { if (Test-ScanAdapterTool $tool) { $executables[$tool] = Get-BenchToolExecutable $tool } }
function Resolve-Target([string]$name) {
    switch ($name) {
        'full' { return @{ path = 'C:\'; manifest = $null } }
        'windows' { return @{ path = $env:SystemRoot; manifest = $null } }
        'profile' { return @{ path = $env:USERPROFILE; manifest = $null } }
        # Personal OneDrive: folders managed by the provider are cloud placeholders.
        'onedrive' {
            $path = if ($env:WINBLAZE_BENCH_ONEDRIVE) { $env:WINBLAZE_BENCH_ONEDRIVE } else { Join-Path $env:USERPROFILE 'OneDrive' }
            if (!(Test-Path -LiteralPath $path)) { throw "OneDrive folder $path is missing; set WINBLAZE_BENCH_ONEDRIVE" }
            return @{ path = $path; manifest = $null }
        }
        # Any large, inactive Git working tree (stable counts across runs).
        'gitrepo' {
            if (!$env:WINBLAZE_BENCH_GITREPO -or !(Test-Path -LiteralPath (Join-Path $env:WINBLAZE_BENCH_GITREPO '.git'))) { throw 'Set WINBLAZE_BENCH_GITREPO to an inactive Git working tree.' }
            return @{ path = [IO.Path]::GetFullPath($env:WINBLAZE_BENCH_GITREPO); manifest = $null }
        }
        default {
            $root = Join-Path $repo "artifacts/extended-datasets/$name"
            $manifest = "$root.manifest.json"
            if (!(Test-Path -LiteralPath $manifest)) { throw "Dataset $name is missing; run make-extended-datasets.py $name" }
            return @{ path = [IO.Path]::GetFullPath($root); manifest = (Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json) }
        }
    }
}
# Conditions (<target>@<condition>): background load for the whole cohort.
function Start-Load([string]$condition, [string]$targetPath) {
    $generator = Join-Path $PSScriptRoot 'load-generators.py'
    switch ($condition) {
        '' { return @() }
        'memlimit' { return @() }
        'cpuload' { return @(1..[math]::Max(1, [int]([Environment]::ProcessorCount / 2)) | ForEach-Object { Start-Process python -ArgumentList '-I', "`"$generator`"", 'cpu' -WindowStyle Hidden -PassThru }) }
        'diskbusy' { return @(Start-Process python -ArgumentList '-I', "`"$generator`"", 'disk', "`"$(Join-Path $repo 'artifacts/busy')`"" -WindowStyle Hidden -PassThru) }
        'changing' {
            $churn = Join-Path $targetPath '_churn'
            $writer = Start-Process python -ArgumentList '-I', "`"$generator`"", 'churn', "`"$churn`"" -WindowStyle Hidden -PassThru
            # Every product must scan while the writer is active: wait until it has written.
            $deadline = [DateTime]::UtcNow.AddSeconds(60)
            while (@(Get-ChildItem -LiteralPath $churn -ErrorAction SilentlyContinue).Count -lt 500) {
                if ([DateTime]::UtcNow -gt $deadline) { Stop-Process -Id $writer.Id -Force; throw 'Churn writer did not start' }
                Start-Sleep -Milliseconds 100
            }
            return @($writer)
        }
        default { throw "Unknown condition $condition" }
    }
}
function Stop-Load($processes, [string]$condition, [string]$targetPath) {
    foreach ($process in $processes) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue; $process.WaitForExit() }
    if ($condition -eq 'diskbusy') { Remove-Item -LiteralPath (Join-Path $repo 'artifacts/busy') -Recurse -Force -ErrorAction SilentlyContinue }
    if ($condition -eq 'changing') { Remove-Item -LiteralPath (Join-Path $targetPath '_churn') -Recurse -Force -ErrorAction SilentlyContinue }
}
function Get-Arguments([string]$tool, [string]$target, [string]$csv) {
    $bare = $target.TrimEnd('\')
    switch ($tool) {
        # A bare drive root avoids the quoted-trailing-backslash pitfall.
        'WinBlaze' { if ($bare -match '^[A-Za-z]:$') { return ('{0}\ "{1}"' -f $bare, $csv) } else { return ('"{0}" "{1}"' -f $bare, $csv) } }
        'WinDirStat' { return ('/SaveTo "{0}" "{1}"' -f $csv, $bare) }
        'WizTree' { return ('"{0}" /admin=0 /export="{1}"' -f $bare, $csv) }
    }
}
function Read-RootTotals([string]$tool, [string]$csv) {
    $reader = [IO.StreamReader]::new($csv)
    try {
        if ($tool -eq 'WizTree') { $null = $reader.ReadLine() }  # Banner line.
        $root = @($reader.ReadLine(), $reader.ReadLine()) | ConvertFrom-Csv
    } finally { $reader.Dispose() }
    switch ($tool) {
        'WinBlaze' { return @{ files = [long]$root.Files; folders = [long]$root.Directories; logical = [long]$root.LogicalBytes; allocated = [long]$root.AllocatedBytes } }
        'WinDirStat' { return @{ files = [long]$root.Files; folders = [long]$root.Folders; logical = [long]$root.'Logical Size'; allocated = [long]$root.'Physical Size' } }
        'WizTree' { return @{ files = [long]$root.Files; folders = [long]$root.Folders; logical = [long]$root.Size; allocated = [long]$root.Allocated } }
    }
}
$elevation = if (([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { 'administrator token' } else { 'standard user token' }
$rows = [Collections.Generic.List[object]]::new()
if (Test-Path -LiteralPath $OutputPath) {
    foreach ($row in @((Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json) | ForEach-Object { $_ })) {
        if ($Resume -or $RetryTimedOut -or !($row.target -in $Targets -and $row.tool -in $Tools)) { $rows.Add($row) }
    }
}
foreach ($targetName in $Targets) {
    $baseName, $condition = $targetName -split '@', 2
    if (!$condition) { $condition = '' }
    $target = Resolve-Target $baseName
    # A live writer changes counts while scanning; completion is the requirement.
    if ($condition -eq 'changing') { $target.manifest = $null }
    $memoryLimit = if ($condition -eq 'memlimit') { 1GB } else { 0 }
    foreach ($tool in $Tools) {
        $identityOptions = @{ Tool = $(if ($tool -eq 'WinBlaze') { 'WinBlaze scan/export' } else { $tool }); Executable = $executables[$tool] }
        if ($HistoryPath) { $identityOptions.HistoryPath = $HistoryPath }
        $identity = Get-BenchmarkIdentity @identityOptions
        $existing = @($rows | Where-Object { $_.target -eq $targetName -and $_.tool -eq $tool })
        $timedOut = @($existing | Where-Object { $_.timed_out })
        if ($RetryTimedOut) {
            if (!$timedOut.Count) { continue }
            foreach ($row in $timedOut) { [void]$rows.Remove($row) }
            $existing = @($existing | Where-Object { !$_.timed_out })
        } elseif ($timedOut.Count) {
            # A recorded failure is that cohort's result; resuming does not retry it.
            continue
        }
        $first = 1 + $existing.Count
        $last = if ($RetryTimedOut) { $first } else { $Runs }
        $load = Start-Load $condition $target.path
        try {
        for ($run = $first; $run -le $last; $run++) {
            $csv = Join-Path $exports ("ext-{0}-{1}-{2}.csv" -f $tool, $targetName, $run)
            Remove-Item -LiteralPath $csv -ErrorAction SilentlyContinue
            $plan = if (Test-ScanAdapterTool $tool) { New-ScanPlan $tool $target.path $csv } else { $null }
            $started = [DateTime]::UtcNow
            # A product that never finishes a fixture is a failed result, not a
            # stopped benchmark: every product gets the same limit, and a timeout
            # is recorded and skips that product's remaining runs here.
            $limit = if ($target.manifest -and !$RetryTimedOut) { [math]::Min($TimeoutSeconds, $FixtureTimeoutSeconds) } else { $TimeoutSeconds }
            try {
                if ($plan) {
                    $measured = Invoke-MeasuredProcess -Executable $plan.Executable -Arguments $plan.Arguments -CompanionExecutable $plan.CompanionExecutable -CompanionArguments $plan.CompanionArguments -TimeoutSeconds $limit -MemoryLimitBytes $memoryLimit -MeasureEnergy:($baseName -eq 'full')
                } else {
                    $measured = Invoke-MeasuredProcess -Executable $executables[$tool] -Arguments (Get-Arguments $tool $target.path $csv) -TimeoutSeconds $limit -MemoryLimitBytes $memoryLimit -MeasureEnergy:($baseName -eq 'full')
                }
            } catch {
                # Only a timeout is a product result; a launch failure (bad
                # path, missing binary) is a broken benchmark and must stop it.
                if ($_.Exception.Message -notmatch ' timed out after \d+ s$') { throw }
                if ($plan) { Remove-ScanOutputs $plan } else { Remove-Item -LiteralPath $csv -ErrorAction SilentlyContinue }
                $rows.Add([pscustomobject][ordered]@{ target = $targetName; tool = $tool; run = $run; binary = $identity
                    started_utc = $started.ToString('o'); elevation = $elevation; condition = $condition
                    scope = 'process startup + scan + CSV export (no GUI); exact kernel counters'
                    exit_code = -1; timed_out = $true; timeout_seconds = $limit; failure = $_.Exception.Message })
                Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
                Write-Output ("{0} {1} run {2}: FAILED ({3})" -f $tool, $targetName, $run, $_.Exception.Message)
                break
            }
            if ($measured.exit_code -ne 0 -or !(Test-Path -LiteralPath $csv)) { if ($plan) { Remove-ScanOutputs $plan }; throw "$tool $targetName run ${run}: no successful export (exit $($measured.exit_code))" }
            $totals = if ($plan) { Read-ScanTotals $plan } else { Read-RootTotals $tool $csv }
            $row = [ordered]@{
                target = $targetName; tool = $tool; run = $run; binary = $identity
                started_utc = $started.ToString('o'); elevation = $elevation
                scope = 'process startup + scan + CSV export (no GUI); exact kernel counters'; condition = $condition
                export_bytes = (Get-Item -LiteralPath $csv).Length
                files = $totals.files; folders_excluding_root = $totals.folders
                logical_bytes = $totals.logical; allocated_bytes = $totals.allocated
                size_precision = if ($totals.size_precision) { $totals.size_precision } else { 'exact' }
                manifest = if ($target.manifest) { [ordered]@{ files = $target.manifest.files; directories = $target.manifest.directories; bytes = $target.manifest.bytes } } else { $null }
            }
            # The report shows a retried cohort's value with its run count.
            if ($RetryTimedOut) { $row.retry_after_timeout = $true; $row.timeout_seconds = $TimeoutSeconds }
            foreach ($property in $measured.PSObject.Properties) { $row[$property.Name] = $property.Value }
            $rows.Add([pscustomobject]$row)
            Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
            Write-Output ("{0} {1} run {2}: {3:N0} ms; {4} files; read {5:N1} MiB; peak {6:N1} MiB" -f $tool, $targetName, $run, $measured.elapsed_ms, $totals.files, ($measured.read_bytes / 1MB), ($measured.peak_working_set_bytes / 1MB))
            # Large and full-drive exports can hold personal paths; keep aggregates only.
            if ($plan) { Remove-ScanOutputs $plan } else { Remove-Item -LiteralPath $csv }
        }
        } finally { Stop-Load $load $condition $target.path }
    }
}
