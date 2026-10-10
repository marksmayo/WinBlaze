param([ValidateRange(1,100)][int]$Runs = 10, [ValidateSet('tiny','small','medium','fanout','fanout-large','scale','full')][string[]]$Profiles = @('tiny', 'scale', 'full'),
    [string]$OutputPath = (Join-Path $PSScriptRoot 'current-engine-results.json'))

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
# Rebuild the diagnostic executable before acquiring its identity or starting
# a timed observation. A native CLI build does not rebuild scanner examples.
& cargo +1.96.0 build --release --offline --manifest-path (Join-Path $repo 'Cargo.toml') -p winblaze-scanner --example directory_walk_benchmark
if ($LASTEXITCODE -ne 0) { throw 'Current engine diagnostic build failed.' }
$exe = Join-Path $repo 'target/release/examples/directory_walk_benchmark.exe'
$output = $OutputPath
$identity = Get-BenchmarkIdentity -Tool 'WinBlaze engine' -Executable $exe
$rows = [Collections.Generic.List[object]]::new()
foreach ($profile in $Profiles) {
    $target = if ($profile -eq 'full') { 'C:\' } else { Join-Path $repo "artifacts/benchmark-datasets/$profile" }
    if (!(Test-Path -LiteralPath $target -PathType Container)) {throw 'Requested engine dataset does not exist.'}
    $expected=if ($profile -eq 'full') {$null} else {Read-BenchmarkJson -Path (Join-Path $repo "artifacts/benchmark-datasets/$profile.manifest.json")}
    for ($run = 1; $run -le $Runs; $run++) {
        $stdout = Join-Path $repo "artifacts/benchmark-exports/engine-$profile-$run.json"
        $stderr = Join-Path $repo "artifacts/benchmark-exports/engine-$profile-$run.stderr"
        $argsText = if ($profile -eq 'full') { $target } else { '"{0}"' -f $target }
        $started = [DateTime]::UtcNow
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $exe -ArgumentList $argsText -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $peakWs = 0L; $cpuMs = 0.0; $sampleCount = 0
        while (!$process.WaitForExit(25)) {
            if ($timer.Elapsed.TotalSeconds -gt 300) { Stop-Process -Id $process.Id; throw 'Engine benchmark timed out.' }
            try {
                $process.Refresh()
                $peakWs = [math]::Max($peakWs, $process.PeakWorkingSet64)
                $cpuMs = Merge-BenchmarkCpuLowerBound $cpuMs $process.TotalProcessorTime.TotalMilliseconds
                $sampleCount++
            } catch { }
        }
        $processMs = $timer.Elapsed.TotalMilliseconds
        $process.WaitForExit()
        if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) { throw "Engine exited with $($process.ExitCode)" }
        $scan = Get-Content -LiteralPath $stdout -Raw | ConvertFrom-Json
        if (!$scan.completed -or $scan.failed -or $scan.cancelled) { throw 'Engine scan did not complete successfully.' }
        if ($expected -and ($scan.summary_files -ne $expected.files -or $scan.summary_directories -ne $expected.directories -or $scan.summary_bytes -ne $expected.bytes -or $scan.issues -ne 0)) {throw 'Engine totals or issues differ from the generated fixture oracle.'}
        # Keep aggregate measurements; omit personal paths from recent issues.
        $row = [pscustomobject]@{
            tool = 'WinBlaze current engine'; profile = $profile; run = $run
            binary = $identity; started_utc = $started.ToString('o')
            revision = $identity.revision; rust = '1.96.0'; build = 'release'
            scope = 'directory-walk producer/controller; no index, GUI, or CSV export'
            scan_ms = $scan.elapsed_ms; process_ms = [math]::Round($processMs, 2)
            first_event_ms = $scan.first_event_ms; summary_ms = $scan.summary_ms; completed_ms = $scan.completed_ms
            files = $scan.summary_files; directories_including_root = $scan.summary_directories
            logical_bytes = $scan.summary_bytes; issues = $scan.issues; issues_by_kind = $scan.issues_by_kind
            files_per_second = $scan.files_per_second
            sample_count = $sampleCount
            peak_working_set_bytes_sampled = if ($sampleCount) { $peakWs } else { $null }
            cpu_ms_sampled_lower_bound = if ($sampleCount) { $cpuMs } else { $null }
        }
        $rows.Add($row)
        $json = ConvertTo-Json -InputObject $rows.ToArray() -Depth 6
        for ($attempt = 0; ; $attempt++) {
            try {
                [IO.File]::WriteAllText($output, $json, [Text.UTF8Encoding]::new($false))
                break
            } catch [IO.IOException] {
                if ($attempt -ge 19) { throw }
                Start-Sleep -Milliseconds 100
            }
        }
        Write-Output ("{0} run {1}: {2} ms; {3} files; {4} issues" -f $profile, $run, $scan.elapsed_ms, $scan.summary_files, $scan.issues)
    }
}
