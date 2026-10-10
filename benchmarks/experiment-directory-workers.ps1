param(
    [ValidateRange(10,100)][int]$Runs = 10,
    [ValidateRange(1,64)][int[]]$Workers = @(4,8,20),
    [ValidateSet('tiny','scale','full')][string]$Profile = 'full'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$measurementLock = Enter-BenchmarkMeasurementLock
try {
    $repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
    $exe = Join-Path $repo 'target/release/examples/directory_walk_benchmark.exe'
    $hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
    $target = if ($Profile -eq 'full') { 'C:\' } else { Join-Path $repo ('artifacts/benchmark-datasets/'+$Profile) }
    $oracle = if ($Profile -ne 'full') { Read-BenchmarkJson -Path "$target.manifest.json" } else { $null }
    $rows = [Collections.Generic.List[object]]::new()
    $output = Join-Path $PSScriptRoot "experiment-directory-workers-$Profile.json"
    # Rotate order each round to avoid giving one setting every earlier cache state.
    for ($run = 1; $run -le $Runs; $run++) {
        for ($slot = 0; $slot -lt $Workers.Count; $slot++) {
            $workersNow = $Workers[($slot + $run - 1) % $Workers.Count]
            $stdout = Join-Path $repo "artifacts/benchmark-exports/worker-$workersNow-$Profile-$run.json"
            $stderr = "$stdout.stderr"
            $argsText = if ($Profile -eq 'full') { "$target $workersNow" } else { '"{0}" {1}' -f $target,$workersNow }
            $started = [DateTime]::UtcNow
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $process = Start-Process -FilePath $exe -ArgumentList $argsText -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
            $peakWs = 0L; $peakPrivate = 0L; $cpuMs = 0.0; $samples = 0
            while (!$process.WaitForExit(25)) {
                if ($timer.Elapsed.TotalSeconds -gt 600) { Stop-Process -Id $process.Id; throw 'Worker experiment timed out.' }
                try {
                    $process.Refresh()
                    $peakWs = [math]::Max($peakWs,$process.PeakWorkingSet64)
                    $peakPrivate = [math]::Max($peakPrivate,$process.PrivateMemorySize64)
                    $cpuMs = Merge-BenchmarkCpuLowerBound $cpuMs $process.TotalProcessorTime.TotalMilliseconds
                    $samples++
                } catch { }
            }
            $processMs = $timer.Elapsed.TotalMilliseconds
            $process.WaitForExit()
            if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) { throw 'Worker experiment process failed.' }
            $scan = Get-Content -LiteralPath $stdout -Raw | ConvertFrom-Json
            if (!$scan.completed -or $scan.failed -or $scan.cancelled -or $scan.workers -ne $workersNow) { throw 'Worker experiment completion or configuration mismatch.' }
            if ($scan.files -ne $scan.summary_files -or $scan.directories -ne $scan.summary_directories) { throw 'Event totals differ from summary.' }
            if ($oracle -and ($scan.summary_files -ne $oracle.files -or $scan.summary_directories -ne $oracle.directories -or $scan.summary_bytes -ne $oracle.bytes -or $scan.issues -ne 0)) { throw 'Worker experiment differs from fixture oracle.' }
            if (!$oracle -and $scan.summary_files -lt 100000) { throw 'Full-volume experiment unexpectedly returned too few files.' }
            $rows.Add([pscustomobject]@{
                profile=$Profile; run=$run; workers=$workersNow; started_utc=$started.ToString('o'); sha256=$hash
                scope='directory-walk producer/controller only; controlled worker experiment, not a medal event'
                process_ms=$processMs; scan_ms=$scan.elapsed_ms; cpu_ms_sampled_lower_bound=$cpuMs
                peak_working_set_bytes_sampled=$peakWs; peak_private_bytes_sampled=$peakPrivate; sample_count=$samples
                files=$scan.summary_files; directories=$scan.summary_directories; bytes=$scan.summary_bytes; issues=$scan.issues
            })
            [IO.File]::WriteAllText($output,(ConvertTo-Json -InputObject $rows.ToArray() -Depth 5),[Text.UTF8Encoding]::new($false))
            Write-Output ("{0} workers={1} run={2}: {3:N0} ms, {4:N2} CPU-s, {5:N1} MiB, files={6}" -f $Profile,$workersNow,$run,$processMs,($cpuMs/1000),($peakWs/1MB),$scan.summary_files)
        }
    }
    if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $hash) { throw 'Experiment binary changed during measurement.' }
} finally { $measurementLock.ReleaseMutex(); $measurementLock.Dispose() }
