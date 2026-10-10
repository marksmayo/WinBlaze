param([ValidateRange(10,100)][int]$Runs=10, [ValidateSet('scale','tiny')][string]$Profile='scale')
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$exe=Join-Path $repo 'target/release/winblaze-cli.exe'
$target=Join-Path $repo ('artifacts/benchmark-datasets/'+$Profile)
$directory=Join-Path $repo ('artifacts/empty-scale-process/'+[guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$rows=[Collections.Generic.List[object]]::new()
$measurementLock=Enter-BenchmarkMeasurementLock
try {
    $identity=Get-BenchmarkIdentity -Tool ('WinBlaze '+$Profile+' phase diagnostic') -Executable $exe
    for($run=1;$run -le $Runs;$run++) {
        $csv=Join-Path $directory ('run-'+$run+'.csv')
        $phasePath=Join-Path $directory ('run-'+$run+'-phases.json')
        $start=[Diagnostics.ProcessStartInfo]::new()
        $start.FileName=$exe
        $start.Arguments='"{0}" "{1}"' -f $target,$csv
        $start.UseShellExecute=$false
        $start.CreateNoWindow=$true
        $start.RedirectStandardOutput=$true
        $start.RedirectStandardError=$true
        $start.EnvironmentVariables['WINBLAZE_CLI_PHASES']=$phasePath
        $timer=[Diagnostics.Stopwatch]::StartNew()
        $process=[Diagnostics.Process]::Start($start)
        $launchMs=$timer.Elapsed.TotalMilliseconds
        try {
            if(!$process.WaitForExit(300000)){ $process.Kill();throw 'Diagnostic scan timed out' }
            $wallMs=$timer.Elapsed.TotalMilliseconds
            $stdout=$process.StandardOutput.ReadToEnd()
            $stderr=$process.StandardError.ReadToEnd()
            if($process.ExitCode){throw "Diagnostic scan failed: $stderr"}
            $metrics=$stdout | ConvertFrom-Json
            $phases=Get-Content -LiteralPath $phasePath -Raw | ConvertFrom-Json
            $summary=Get-Content -LiteralPath $csv -TotalCount 2 | ConvertFrom-Csv
            $expectedFiles=if($Profile -eq 'tiny'){72}else{16384}
            $expectedBytes=if($Profile -eq 'tiny'){73728}else{0}
            if([long]$summary.Files -ne $expectedFiles -or [long]$summary.LogicalBytes -ne $expectedBytes){throw 'Incorrect fixture output'}
            $rows.Add([pscustomobject]@{run=$run;binary=$identity;launch_request_ms=$launchMs;process_wall_ms=$wallMs;scan_export_ms=$metrics.scan_export_ms;phases=$phases;cpu_ms=$process.TotalProcessorTime.TotalMilliseconds;scope='DIAGNOSTIC ONLY: redirected stdout, direct Process.Start, opt-in phase file, no sampled-resource loop; not medal evidence';csv=$csv})
            Write-Output "Diagnostic $run/$Runs wall=$([math]::Round($wallMs,2)) ms internal=$($metrics.scan_export_ms) ms"
        } finally {$process.Dispose()}
    }
    Write-BenchmarkJson -Path (Join-Path $PSScriptRoot ($Profile+'-process-diagnostic.json')) -Value $rows.ToArray()
} finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
