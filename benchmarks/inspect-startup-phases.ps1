param([ValidateRange(10,100)][int]$Runs=10)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$measurementLock=Enter-BenchmarkMeasurementLock
try {
    $exe=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'))
    $identity=Get-BenchmarkIdentity -Tool 'WinBlaze startup phases' -Executable $exe
    $output=Join-Path $PSScriptRoot 'current-startup-phase-inventory.json'
    $rows=[Collections.Generic.List[object]]::new()
    for($run=1;$run -le $Runs;$run++) {
        $directory=Join-Path $PSScriptRoot ('../artifacts/startup-phase-inventory/'+[guid]::NewGuid())
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
        $tracePath=Join-Path $directory 'WinBlaze-startup.log'
        $origin=[Diagnostics.Stopwatch]::GetTimestamp()
        $process=Start-Process -FilePath $exe -WindowStyle Hidden -PassThru -Environment @{
            WINBLAZE_STARTUP_TRACE='1';TEMP=[IO.Path]::GetFullPath($directory);TMP=[IO.Path]::GetFullPath($directory)
        }
        try {
            if(!$process.WaitForInputIdle(15000)){throw 'First idle was not reached'}
            $idle=[Diagnostics.Stopwatch]::GetTimestamp()
            $wait=[Diagnostics.Stopwatch]::StartNew()
            do {
                $phases=@()
                if(Test-Path -LiteralPath $tracePath) {
                    # A writer may still be appending the last line; retry it.
                    try {$phases=@(Get-Content -LiteralPath $tracePath | ConvertFrom-Json)} catch {$phases=@()}
                }
                $ready=@($phases | Where-Object message -eq 'App::OnLaunched forced HWND maximized').Count -gt 0
                if(!$ready){Start-Sleep -Milliseconds 20}
            } while(!$ready -and $wait.Elapsed.TotalSeconds -lt 15 -and !$process.HasExited)
            if(!$ready){throw 'Complete startup trace was not observed'}
            foreach($phase in $phases) {
                if($phase.pid -ne $process.Id -or $phase.qpc_frequency -ne [Diagnostics.Stopwatch]::Frequency) {
                    throw 'Trace and parent clock/process identities do not match'
                }
            }
            $mapped=@($phases | ForEach-Object {
                [pscustomobject]@{
                    message=$_.message;qpc=$_.qpc
                    since_launch_ms=($_.qpc-$origin)*1000.0/[Diagnostics.Stopwatch]::Frequency
                    before_first_idle=$_.qpc -le $idle
                }
            })
            $rows.Add([pscustomobject]@{
                run=$run;binary=$identity;process_id=$process.Id
                launch_qpc=$origin;idle_qpc=$idle;frequency=[Diagnostics.Stopwatch]::Frequency
                observed_utc=[DateTime]::UtcNow.ToString('o');phases=$mapped
                scope='Diagnostic tracing enabled; synchronous log writes perturb startup. Phase ordering locates candidates; these timings do not enter medals.'
            })
            Write-BenchmarkJson -Path $output -Value $rows.ToArray()
            Write-Host "Startup phase trace $run/${Runs}: $($phases.Count) phases"
        } finally {
            if(!$process.HasExited){Stop-Process -Id $process.Id}
            $process.Dispose()
        }
    }
} finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
