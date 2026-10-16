param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [string]$WinBlazeAppPath = (Join-Path $PSScriptRoot '../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'),
    [ValidateSet('WinBlaze','WinDirStat','WizTree','TreeSize Free','SpaceSniffer','Everything','Directory Report')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree'),
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
# Added products (scan-adapters.ps1). Everything gets a private instance with
# no volumes or folders, so its first-run NTFS indexing prompt never appears.
. (Join-Path $PSScriptRoot 'scan-adapters.ps1')
$everythingStartup = Join-Path $env:TEMP 'winblaze-everything-startup'
New-Item -ItemType Directory -Force -Path $everythingStartup | Out-Null
@('[Everything]','app_data=0','run_as_admin=0','allow_multiple_instances=1','auto_include_fixed_volumes=0','auto_include_removable_volumes=0',
  'auto_include_fixed_refs_volumes=0','auto_include_removable_refs_volumes=0','show_tray_icon=0','check_for_updates_on_startup=0') |
    Set-Content -LiteralPath (Join-Path $everythingStartup 'cfg.ini') -Encoding ASCII
$definitions += @(
    @{ name = 'TreeSize Free'; path = Get-BenchToolExecutable 'TreeSize Free'; arguments = '' },
    @{ name = 'SpaceSniffer'; path = Get-BenchToolExecutable 'SpaceSniffer'; arguments = '' },
    @{ name = 'Everything'; path = Get-BenchToolExecutable 'Everything'; arguments = "-instance WBStartup -config `"$everythingStartup\cfg.ini`" -db `"$everythingStartup\index.db`"" },
    @{ name = 'Directory Report'; path = Get-BenchToolExecutable 'Directory Report'; arguments = '' }
)
$rows = [Collections.Generic.List[object]]::new()
if (Test-Path -LiteralPath $OutputPath) {
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
