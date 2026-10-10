param([Parameter(Mandatory)][string]$DeviceId,[ValidateRange(10,100)][int]$Runs=10)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$measurementLock=Enter-BenchmarkMeasurementLock
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$exe=Join-Path $repo 'artifacts/portable-devices/portable-devices.exe'
$batch=Join-Path $repo ('artifacts/mtp-validation-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $batch | Out-Null
$identity=Get-BenchmarkIdentity -Tool 'WinBlaze feature portable-devices' -Executable $exe
$rows=[Collections.Generic.List[object]]::new()
try {
 for($run=1;$run -le $Runs;$run++) {
  $output=Join-Path $batch ("device-$run.csv")
  $start=[Diagnostics.ProcessStartInfo]::new($exe)
  $start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
  foreach($arg in @('--scan',$DeviceId,$output)){$start.ArgumentList.Add($arg)}
  $timer=[Diagnostics.Stopwatch]::StartNew();$process=[Diagnostics.Process]::Start($start)
  $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
  $samples=0;$cpu=$null;$peak=$null;$private=$null
  while(!$process.WaitForExit(10)) {
   if($timer.Elapsed.TotalMinutes -gt 10){$process.Kill();throw 'Device scan exceeded ten minutes'}
   try{$process.Refresh();$cpu=Merge-BenchmarkCpuLowerBound $cpu $process.TotalProcessorTime.TotalMilliseconds;$peak=[Math]::Max([long]$peak,$process.PeakWorkingSet64);$private=[Math]::Max([long]$private,$process.PrivateMemorySize64);$samples++}catch{}
  }
  $elapsed=$timer.Elapsed.TotalMilliseconds
  $message=$stdout.GetAwaiter().GetResult();$errorText=$stderr.GetAwaiter().GetResult()
  if($process.ExitCode){throw "Device scan failed: $($process.ExitCode); $errorText"}
  $stats=$message | ConvertFrom-Json
  $records=@(Import-Csv -LiteralPath $output)
  $files=@($records | Where-Object Kind -eq File)
  if(!$files.Count -or $records[0].Path -notlike 'MTP:*' -or [uint64]$records[0].Files -ne $files.Count -or [uint64]$stats.files -ne $files.Count -or @($records | Where-Object {[uint64]$_.AllocatedBytes -ne 0}).Count){throw 'Device CSV metadata/count validation failed'}
  if([uint64]$records[0].LogicalBytes -ne [uint64]$stats.logical_bytes){throw 'Device root totals disagree'}
  $rows.Add(@{tool='WinBlaze';job='wb-mtp-live';operation='mtp-metadata-scan';run=$run;requested_runs=$Runs;status='measured';scope='Native adapter startup + connected WPD device metadata walk + CSV snapshot; no file-content downloads or GUI';binary=$identity;started_utc=[DateTime]::UtcNow.ToString('o');elevation='standard user token';cache='uncontrolled; sequential device repeats';elapsed_ms=$elapsed;cpu_ms_sampled_lower_bound=$cpu;peak_working_set_bytes_sampled=$peak;peak_private_bytes_sampled=$private;sample_count=$samples;output_bytes=(Get-Item $output).Length;validation=@{verified=$true;files=$files.Count;logical_bytes=$stats.logical_bytes;errors=$stats.errors;unknown_sizes=$stats.unknown_sizes;note='Real-device metadata and internal totals validated; no independent complete device-file inventory ground truth.'}})
  Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-portable-device-results.json') -Value $rows.ToArray()
  Write-Output "MTP $run/$Runs : $([Math]::Round($elapsed)) ms; $($files.Count) files; skipped/errors $($stats.errors)"
 }
} finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
