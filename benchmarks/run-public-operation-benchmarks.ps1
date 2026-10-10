param([ValidateRange(1,100)][int]$Runs=10,[long[]]$Sizes=@(100000,1000000))
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$exe=Join-Path $repo 'target/release/examples/performance_operations.exe'
$identity=Get-BenchmarkIdentity -Tool 'WinBlaze public operations' -Executable $exe
$rows=[Collections.Generic.List[object]]::new()
foreach($size in $Sizes){for($run=1;$run -le $Runs;$run++){
 $out=Join-Path $repo "artifacts/benchmark-exports/operations-$size-$run.json"
 $p=Start-Process -FilePath $exe -ArgumentList $size -PassThru -WindowStyle Hidden -RedirectStandardOutput $out
 $timer=[Diagnostics.Stopwatch]::StartNew();$peak=0L;$cpu=0.0;$samples=0
 while(!$p.HasExited){$p.Refresh();if(!$p.HasExited){$peak=[Math]::Max($peak,$p.WorkingSet64);$cpu=Merge-BenchmarkCpuLowerBound $cpu $p.TotalProcessorTime.TotalMilliseconds;$samples++};Start-Sleep -Milliseconds 25}
 if($null -ne $p.ExitCode -and $p.ExitCode -ne 0){throw 'Public operation benchmark failed.'}
 $metrics=Get-Content $out -Raw | ConvertFrom-Json
 if(!$metrics.verified -or $metrics.files -ne $size){throw 'Public operation output failed correctness validation.'}
 $rows.Add([pscustomobject]@{run=$run;files=$size;binary=$identity;metrics=$metrics;elapsed_ms=$timer.Elapsed.TotalMilliseconds;peak_working_set_bytes_sampled=if($samples){$peak}else{$null};cpu_ms_sampled_lower_bound=if($samples){$cpu}else{$null};sample_count=$samples;scope='operation timers exclude fixture construction; process resources include fixture and all operations'})
 ConvertTo-Json -InputObject $rows.ToArray() -Depth 12 | Set-Content (Join-Path $PSScriptRoot 'current-public-operation-results.json') -Encoding UTF8
 Write-Host "Public operations $size $run/$Runs validated"
}}
