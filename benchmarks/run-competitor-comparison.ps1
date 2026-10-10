[CmdletBinding()]
param(
    [string[]]$Profiles = @('tiny', 'scale', 'full'),
    [ValidateRange(1,100)][int]$Runs = 10,
    [ValidateRange(1,3600)][int]$TimeoutSeconds = 600,
    [string]$OutputPath = (Join-Path $PSScriptRoot 'current-competitor-results.json'),
    [switch]$AppendResults,
    [ValidateSet('WinBlaze', 'WinDirStat', 'WizTree')]
    [string[]]$Tools = @('WinBlaze', 'WinDirStat', 'WizTree'),
    [int]$RunOffset = 0,
    [string]$WinBlazeCliPath = (Join-Path $PSScriptRoot '../target/release/winblaze-cli.exe'),
    [string]$HistoryPath = '',
    [scriptblock]$OnToolComplete,
    [string]$BatchPolicyPath = (Join-Path $PSScriptRoot 'current-benchmark-batch.json')
)

# Sequential standard-user scan/export benchmarks. Elapsed includes process
# startup, scan, and CSV serialization; it is NOT a pure scan/UI-ready metric.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
# The retained administrator worker already calls this fixed output path.
# Read the controller's explicit batch policy here so it can reuse baselines
# without restarting the elevated worker or asking for another UAC approval.
$elevatedPath=Join-Path $PSScriptRoot 'current-elevated-competitor-results.json'
$batchPath=$BatchPolicyPath
$resolvedOutputPath=if([IO.Path]::IsPathRooted($OutputPath)){$OutputPath}else{Join-Path (Get-Location).ProviderPath $OutputPath}
if([IO.Path]::GetFullPath($resolvedOutputPath) -eq $elevatedPath -and (Test-Path -LiteralPath $batchPath)) {
    $controller=Read-BenchmarkJson $batchPath
    if($controller.state -in @('running','awaiting-administrator') -and $controller.competitor_refresh_policy -eq 'reuse-complete-version-baselines-v1') {
        . (Join-Path $PSScriptRoot 'competitor-refresh-policy.ps1')
        $required=@(Get-InstalledBenchmarkRefreshTools)
        $Tools=@($Tools | Where-Object {$_ -in $controller.selected_tools -or $_ -in $required})
        if(!$Tools.Count){Write-Output 'Reusing complete version-matched administrator competitor baselines.';return}
        $AppendResults=$true
        # Replace only the product being refreshed; preserve the other baseline.
        $prior=@(Read-BenchmarkJson $OutputPath | Where-Object {$_.tool -notin $Tools})
        Write-BenchmarkJson -Path $OutputPath -Value $prior
    }
}
$exports = Join-Path $repo 'artifacts/benchmark-exports'
New-Item -ItemType Directory -Force -Path $exports | Out-Null
$results = [Collections.Generic.List[object]]::new()
if ($AppendResults -and (Test-Path -LiteralPath $OutputPath)) {
    $savedRows = Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json
    foreach ($saved in $savedRows) { $results.Add($saved) }
}
foreach ($profile in $Profiles) {
    $target = if ($profile -eq 'full') { 'C:\' } else { Join-Path $repo "artifacts/benchmark-datasets/$profile" }
    foreach ($tool in $Tools) {
        $exe = if ($tool -eq 'WinBlaze') { [IO.Path]::GetFullPath($WinBlazeCliPath) } elseif ($tool -eq 'WinDirStat') { 'C:\Program Files\WinDirStat\WinDirStat.exe' } else { 'C:\Program Files\WizTree\WizTree64.exe' }
        $identityName = if ($tool -eq 'WinBlaze') { 'WinBlaze scan/export' } else { $tool }
        $identityOptions = @{Tool=$identityName;Executable=$exe}
        if($HistoryPath){$identityOptions.HistoryPath=$HistoryPath}
        $identity = Get-BenchmarkIdentity @identityOptions
        for ($iteration = 1; $iteration -le $Runs; $iteration++) {
            $run = $iteration + $RunOffset
            $csv = Join-Path $exports "$tool-$profile-$run.csv"
            if ($tool -eq 'WinBlaze') {
                # C: is drive-relative, not a volume root. A bare C:\ avoids
                # the Windows quoted-trailing-backslash argument pitfall.
                $argsText = if ($profile -eq 'full') { 'C:\ "{0}"' -f $csv } else { '"{0}" "{1}"' -f $target, $csv }
            } elseif ($tool -eq 'WinDirStat') {
                $argsText = '/SaveTo "{0}" "{1}"' -f $csv, $target.TrimEnd('\')
            } else {
                # Avoid a trailing backslash immediately before a quoted arg.
                $wizTarget = $target.TrimEnd('\')
                $argsText = '"{0}" /admin=0 /export="{1}"' -f $wizTarget, $csv
            }
            $started = [DateTime]::UtcNow
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $process = Start-Process -FilePath $exe -ArgumentList $argsText -WindowStyle Hidden -PassThru
            $peakWs = 0L; $cpuMs = 0.0; $privateBytes = 0L; $sampleCount = 0
            # Sample before the first wait too: a valid short scan/export can
            # finish inside that first 25 ms interval. Keep the same periodic
            # sampling interval and retain only counters actually observed.
            do {
                if ($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds) {
                    Stop-Process -Id $process.Id
                    throw "$tool $profile run $run timed out; owned benchmark process stopped."
                }
                try {
                    $process.Refresh()
                    $peakWs = [math]::Max($peakWs, $process.PeakWorkingSet64)
                    $privateBytes = [math]::Max($privateBytes, $process.PrivateMemorySize64)
                    $cpuMs = Merge-BenchmarkCpuLowerBound $cpuMs $process.TotalProcessorTime.TotalMilliseconds
                    $sampleCount++
                } catch { }
            } while (!$process.WaitForExit(25))
            $elapsed = $timer.Elapsed.TotalMilliseconds
            $process.WaitForExit()
            if (($null -ne $process.ExitCode -and $process.ExitCode -ne 0) -or !(Test-Path -LiteralPath $csv)) {
                throw "$tool did not produce a successful export (exit $($process.ExitCode))."
            }
            $reader = [IO.StreamReader]::new($csv)
            try {
                if ($tool -eq 'WizTree') { $null = $reader.ReadLine() }
                $header = $reader.ReadLine()
                $line = $reader.ReadLine()
                $root = @($header, $line) | ConvertFrom-Csv
            } finally { $reader.Dispose() }
            if ($tool -eq 'WinBlaze' -and ![string]::Equals($root.Path.TrimEnd('\'), $target.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
                throw "WinBlaze exported a different root than the requested workload; result rejected."
            }
            $row = [pscustomobject]@{
                tool = $tool; profile = $profile; run = $run
                binary = $identity
                started_utc = $started.ToString('o'); process_id = $process.Id
                scope = 'process startup + scan + CSV export (no GUI rendering)'
                elevation = if (([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { 'administrator token' } else { 'standard user token' }
                cache = 'uncontrolled first observation; subsequent runs warmed'
                elapsed_ms = [math]::Round($elapsed, 2)
                cpu_ms_sampled_lower_bound = if ($sampleCount) { [math]::Round($cpuMs, 2) } else { $null }
                peak_working_set_bytes_sampled = Get-BenchmarkMemoryPeakValue $peakWs $sampleCount
                peak_private_bytes_sampled = Get-BenchmarkMemoryPeakValue $privateBytes $sampleCount
                sample_count = $sampleCount
                timeout_seconds = $TimeoutSeconds
                export_bytes = (Get-Item -LiteralPath $csv).Length
                files = [long]$root.Files
                folders_excluding_root = if ($tool -eq 'WinBlaze') { [long]$root.Directories } else { [long]$root.Folders }
                logical_bytes = if ($tool -eq 'WinBlaze') { [long]$root.LogicalBytes } elseif ($tool -eq 'WinDirStat') { [long]$root.'Logical Size' } else { [long]$root.Size }
                allocated_bytes = if ($tool -eq 'WinBlaze') { [long]$root.AllocatedBytes } elseif ($tool -eq 'WinDirStat') { [long]$root.'Physical Size' } else { [long]$root.Allocated }
            }
            $results.Add($row)
            ConvertTo-Json -InputObject $results.ToArray() -Depth 6 | Set-Content -LiteralPath $OutputPath
            Write-Output ("{0} {1} run {2}: {3:N0} ms; {4} files; peak {5:N1} MiB" -f $tool, $profile, $run, $elapsed, $row.files, ($peakWs / 1MB))
            # Full-volume exports contain personal paths and can be large.
            # Keep aggregate evidence only; delete this exact generated file.
            if ($profile -eq 'full') { Remove-Item -LiteralPath $csv }
        }
        # Publish only after the full validated cohort, outside timed work.
        if ($OnToolComplete) { & $OnToolComplete }
    }
}
