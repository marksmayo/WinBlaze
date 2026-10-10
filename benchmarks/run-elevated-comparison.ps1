param([ValidateRange(1,100)][int]$Runs = 10, [switch]$SkipCompetitors)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$statusPath = Join-Path $PSScriptRoot 'current-elevated-status.json'
function Write-Status([string]$State, [string]$Detail) {
    @{ state = $State; detail = $Detail; utc = [DateTime]::UtcNow.ToString('o') } |
        ConvertTo-Json | Set-Content -LiteralPath $statusPath -Encoding UTF8
}
try {
    $admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (!$admin) { throw 'Administrator token required.' }
    Write-Status 'running' 'Elevated MFT phases, then sequential full-volume competitor exports.'
    . (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
    $exe = Join-Path $repo 'target/release/examples/mft_phase_bench.exe'
    $identity = Get-BenchmarkIdentity -Tool 'WinBlaze MFT phases' -Executable $exe
    $rows = [Collections.Generic.List[object]]::new()
    for ($run = 1; $run -le $Runs; $run++) {
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $stdout=Join-Path $repo "artifacts/benchmark-exports/mft-$run.stdout"
        $stderr=Join-Path $repo "artifacts/benchmark-exports/mft-$run.stderr"
        $process=Start-Process -FilePath $exe -ArgumentList 'C:\' -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $peak=0L; $cpu=0.0; $samples=0
        while(!$process.WaitForExit(25)){
            if($timer.Elapsed.TotalSeconds -gt 300){Stop-Process -Id $process.Id;throw 'MFT phase benchmark timed out.'}
            try {$process.Refresh();$peak=[Math]::Max($peak,$process.WorkingSet64);$cpu=Merge-BenchmarkCpuLowerBound $cpu $process.TotalProcessorTime.TotalMilliseconds;$samples++} catch {}
        }
        $process.WaitForExit()
        if ($null -ne $process.ExitCode -and $process.ExitCode -ne 0) { throw 'MFT phase benchmark failed; see aggregate status and stderr.' }
        $raw=Get-Content -LiteralPath $stdout -Raw
        Remove-Item -LiteralPath $stdout
        $rows.Add([pscustomobject]@{ run = $run; elevation = 'administrator token'; binary = $identity; process_ms = $timer.Elapsed.TotalMilliseconds; phases = ($raw | ConvertFrom-Json); peak_working_set_bytes_sampled = if($samples){$peak}else{$null}; cpu_ms_sampled_lower_bound = if($samples){$cpu}else{$null}; sample_count=$samples; scope = 'raw MFT phase decomposition; repeated layers, no index or GUI; process resources span all repeated phase experiments' })
        ConvertTo-Json -InputObject $rows.ToArray() -Depth 12 | Set-Content (Join-Path $PSScriptRoot 'current-elevated-mft-results.json') -Encoding UTF8
    }
    if (!$SkipCompetitors) { & (Join-Path $PSScriptRoot 'run-competitor-comparison.ps1') -Profiles full -Runs $Runs -Tools WinDirStat,WizTree -OutputPath (Join-Path $PSScriptRoot 'current-elevated-competitor-results.json') }
    Write-Status 'complete' 'Administrator-token MFT phase and competitor export measurements completed.'
} catch {
    Write-Status 'failed' $_.Exception.Message
    throw
}
