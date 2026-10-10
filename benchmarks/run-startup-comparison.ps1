param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [string]$WinBlazeAppPath = (Join-Path $PSScriptRoot '../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'),
    [ValidateSet('WinBlaze','WinDirStat','WizTree')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree'),
    [string]$OutputPath = (Join-Path $PSScriptRoot 'current-startup-results.json'),
    [string]$HistoryPath = ''
)

# Initialization proxy: launch request to first idle GUI message queue.
# This does not establish that a window is visible, usable, or cache-loaded.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$definitions = @(
    @{ name = 'WinBlaze'; path = [IO.Path]::GetFullPath($WinBlazeAppPath); arguments = '' },
    @{ name = 'WinDirStat'; path = 'C:\Program Files\WinDirStat\WinDirStat.exe'; arguments = '' },
    @{ name = 'WizTree'; path = 'C:\Program Files\WizTree\WizTree64.exe'; arguments = '/admin=0' }
)
$rows = [Collections.Generic.List[object]]::new()
if ($Tools.Count -lt 3 -and (Test-Path -LiteralPath $OutputPath)) {
    foreach ($row in (Read-BenchmarkJson -Path $OutputPath)) {
        if ($row.tool -notin $Tools) { $rows.Add($row) }
    }
}
foreach ($tool in ($definitions | Where-Object { $_.name -in $Tools })) {
    $identityOptions=@{Tool=$tool.name;Executable=$tool.path}
    if($HistoryPath){$identityOptions.HistoryPath=$HistoryPath}
    $identity = Get-BenchmarkIdentity @identityOptions
    for ($run = 1; $run -le $Runs; $run++) {
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $options = @{ FilePath = $tool.path; WindowStyle = 'Hidden'; PassThru = $true }
        if ($tool.arguments) { $options.ArgumentList = $tool.arguments }
        $process = Start-Process @options
        try {
            $idle = $process.WaitForInputIdle(15000)
            $elapsed = $timer.Elapsed.TotalMilliseconds
            $process.Refresh()
            $row = [pscustomobject]@{
                tool = $tool.name; executable_path = $tool.path; run = $run; idle_reached = $idle
                binary = $identity
                scope = 'launch request to first idle GUI message queue; hidden window; not usable-window time'
                initialization_ms = [math]::Round($elapsed, 2)
                working_set_bytes_at_idle = $process.WorkingSet64
                peak_working_set_bytes_at_idle = $process.PeakWorkingSet64
                cpu_ms_at_idle = $process.TotalProcessorTime.TotalMilliseconds
                process_id = $process.Id; utc = [DateTime]::UtcNow.ToString('o')
            }
            $rows.Add($row)
            ConvertTo-Json -InputObject $rows.ToArray() | Set-Content $OutputPath
            Write-Output ("{0} startup proxy run {1}: {2:N0} ms (idle={3})" -f $tool.name, $run, $elapsed, $idle)
        } finally {
            if (!$process.HasExited) { Stop-Process -Id $process.Id }
        }
    }
}
