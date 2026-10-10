param(
    [ValidateRange(10,100)][int]$Runs = 10,
    [string]$OutputPath = (Join-Path $PSScriptRoot 'current-startup-module-inventory.json')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$measurementLock = Enter-BenchmarkMeasurementLock
try {
    $definitions = @(
        @{name='WinBlaze';path=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'));arguments=''},
        @{name='WinDirStat';path='C:\Program Files\WinDirStat\WinDirStat.exe';arguments=''},
        @{name='WizTree';path='C:\Program Files\WizTree\WizTree64.exe';arguments='/admin=0'}
    )
    $rows = [Collections.Generic.List[object]]::new()
    foreach ($tool in $definitions) {
        $identity = Get-BenchmarkIdentity -Tool $tool.name -Executable $tool.path
        for ($run = 1; $run -le $Runs; $run++) {
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $options = @{FilePath=$tool.path;WindowStyle='Hidden';PassThru=$true}
            if ($tool.arguments) {$options.ArgumentList=$tool.arguments}
            $process = Start-Process @options
            try {
                if (!$process.WaitForInputIdle(15000)) {throw "$($tool.name) did not reach first idle"}
                $idleMs = $timer.Elapsed.TotalMilliseconds
                $process.Refresh()
                # Capture counters before module enumeration; enumeration can
                # take time and must not be interpreted as the idle boundary.
                $workingSet = $process.WorkingSet64
                $cpuMs = $process.TotalProcessorTime.TotalMilliseconds
                $snapshotStarted = $timer.Elapsed.TotalMilliseconds
                $modules = @($process.Modules | ForEach-Object {
                    [pscustomobject]@{
                        name=$_.ModuleName;path=$_.FileName
                        mapped_image_bytes=$_.ModuleMemorySize
                    }
                } | Sort-Object path)
                if (!$modules.Count) {throw "$($tool.name) returned no loaded modules"}
                $rows.Add([pscustomobject]@{
                    tool=$tool.name;run=$run;binary=$identity;process_id=$process.Id
                    observed_utc=[DateTime]::UtcNow.ToString('o')
                    idle_ms=$idleMs;cpu_ms_at_idle=$cpuMs;working_set_bytes_at_idle=$workingSet
                    snapshot_started_ms=$snapshotStarted;snapshot_finished_ms=$timer.Elapsed.TotalMilliseconds
                    scope='Diagnostic module enumeration after first idle; not a medal measurement. Image sizes are mapped address ranges, not resident memory or attributed CPU.'
                    modules=$modules
                })
                Write-BenchmarkJson -Path $OutputPath -Value $rows.ToArray()
                Write-Host "$($tool.name) module snapshot $run/${Runs}: $($modules.Count) modules"
            } finally {
                if (!$process.HasExited) {Stop-Process -Id $process.Id}
                $process.Dispose()
            }
        }
    }
    foreach ($tool in $definitions) {
        if (@($rows | Where-Object tool -eq $tool.name).Count -ne $Runs) {
            throw "Incomplete module inventory for $($tool.name)"
        }
    }
} finally {
    $measurementLock.ReleaseMutex()
    $measurementLock.Dispose()
}
