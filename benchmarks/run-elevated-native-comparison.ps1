param([ValidateRange(1,100)][int]$Runs = 10)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
# Elevated PowerShell can keep its process working directory at System32 even
# after Set-Location. Synchronize it for the retained caller's atomic JSON writes.
[IO.Directory]::SetCurrentDirectory($repo)
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (!$admin) { throw 'Administrator token required for the native MFT pipeline comparison.' }
$exe = Join-Path $repo 'target/release/examples/ffi_scan_repro.exe'
$identity = Get-BenchmarkIdentity -Tool 'WinBlaze native pipeline' -Executable $exe
$rows = [Collections.Generic.List[object]]::new()
for ($run = 1; $run -le $Runs; $run++) {
    $stdout = Join-Path $repo "artifacts/benchmark-exports/native-elevated-$run.stdout"
    $stderr = Join-Path $repo "artifacts/benchmark-exports/native-elevated-$run.stderr"
    $p = Start-Process -FilePath $exe -ArgumentList 'C:\' -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $peak = 0L; $cpu = 0.0; $samples = 0
    while (!$p.WaitForExit(25)) {
        if ($timer.Elapsed.TotalSeconds -gt 360) { Stop-Process -Id $p.Id; throw 'Native pipeline timed out.' }
        try { $p.Refresh(); $peak = [math]::Max($peak, $p.PeakWorkingSet64); $cpu = Merge-BenchmarkCpuLowerBound $cpu $p.TotalProcessorTime.TotalMilliseconds; $samples++ } catch { }
    }
    $p.WaitForExit()
    $lines = Get-Content -LiteralPath $stdout
    $scan = $lines[0] | ConvertFrom-Json
    if ($scan.completed -ne 1) { throw 'Native pipeline did not complete.' }
    $rootLine = $lines | Where-Object { $_ -like 'tree root:*' } | Select-Object -First 1
    if ($rootLine -notmatch 'physical=(\d+) logical=(\d+) files=(\d+) items=(\d+)') { throw 'Native pipeline did not expose aggregate tree totals.' }
    $physical = [long]$Matches[1]; $logical = [long]$Matches[2]; $files = [long]$Matches[3]; $items = [long]$Matches[4]
    $rows.Add([pscustomobject]@{tool='WinBlaze native pipeline';run=$run;profile='full';binary=$identity;elevation='administrator token';scope='native scan through Completed callback; includes index publication, excludes deferred snapshot write and UI/export; process time/resources include the later write and tree queries';scan_ms=$scan.completed_ms;summary_ms=$scan.summary_ms;process_ms=$timer.Elapsed.TotalMilliseconds;files=$files;items=$items;logical_bytes=$logical;allocated_bytes=$physical;emitted_file_callbacks=$scan.files;emitted_directory_callbacks=$scan.directories;callback_note='Callback counts are capped; aggregate counts come from the tree root.';tree_rollup_verified=(@($lines | Where-Object { $_ -like 'tree rollup check: OK*' }).Count -gt 0);peak_working_set_bytes_sampled=$peak;cpu_ms_sampled_lower_bound=$cpu;sample_count=$samples;utc=[DateTime]::UtcNow.ToString('o')})
    ConvertTo-Json -InputObject $rows.ToArray() -Depth 8 | Set-Content (Join-Path $PSScriptRoot 'current-elevated-native-results.json') -Encoding UTF8
    # The reproduction example prints real root-child names; retain aggregates only.
    Remove-Item -LiteralPath $stdout
    Remove-Item -LiteralPath $stderr
}
