param([Parameter(Mandatory)][string]$DumpPath,[ValidateRange(10,100)][int]$Runs=10)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$measurementLock=Enter-BenchmarkMeasurementLock
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$exe=Join-Path $repo 'target/release/examples/feature_catalog.exe'
$batch=Join-Path $repo ('artifacts/mft-reopen-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $batch | Out-Null
$identity=Get-BenchmarkIdentity -Tool 'WinBlaze feature feature_catalog' -Executable $exe
$rows=[Collections.Generic.List[object]]::new()
try {
 for($run=1;$run -le $Runs;$run++) {
  $output=Join-Path $batch ("reopened-$run.csv")
  $start=[Diagnostics.ProcessStartInfo]::new($exe)
  $start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
  foreach($arg in @('mft-import','MFT:\',$DumpPath,$output)){$start.ArgumentList.Add($arg)}
  $timer=[Diagnostics.Stopwatch]::StartNew();$process=[Diagnostics.Process]::Start($start)
  $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
  $samples=0;$cpu=$null;$peak=$null;$private=$null
  while(!$process.WaitForExit(10)) {
   if($timer.Elapsed.TotalMinutes -gt 10){$process.Kill();throw 'MFT reopen exceeded ten minutes'}
   try{$process.Refresh();$cpu=Merge-BenchmarkCpuLowerBound $cpu $process.TotalProcessorTime.TotalMilliseconds;$peak=[Math]::Max([long]$peak,$process.PeakWorkingSet64);$private=[Math]::Max([long]$private,$process.PrivateMemorySize64);$samples++}catch{}
  }
  $elapsed=$timer.Elapsed.TotalMilliseconds
  $message=$stdout.GetAwaiter().GetResult();$errorText=$stderr.GetAwaiter().GetResult()
  if($process.ExitCode){throw "MFT reopen failed: $($process.ExitCode); $errorText"}
  $stats=$message | ConvertFrom-Json
  $root=@(Get-Content -LiteralPath $output -TotalCount 2 | ConvertFrom-Csv)[0]
  if(!$stats.files -or $root.Path -ne 'MFT:\' -or [uint64]$root.Files -ne [uint64]$stats.files -or [uint64]$root.LogicalBytes -ne [uint64]$stats.logical_bytes){throw 'MFT reopen did not preserve its reconstructed catalog totals'}
  $rows.Add(@{tool='WinBlaze';job='wb-mft-reopen';operation='raw-mft-reopen-csv';run=$run;requested_runs=$Runs;status='measured';scope='Native adapter startup + real raw MFT dump parsing/catalog construction + complete CSV export; no raw volume access or GUI';binary=$identity;started_utc=[DateTime]::UtcNow.ToString('o');elevation='administrator token runner; offline operation';cache='uncontrolled; sequential repeats';elapsed_ms=$elapsed;cpu_ms_sampled_lower_bound=$cpu;peak_working_set_bytes_sampled=$peak;peak_private_bytes_sampled=$private;sample_count=$samples;output_bytes=(Get-Item $output).Length;validation=@{verified=$true;files=$stats.files;logical_bytes=$stats.logical_bytes;note='Positive real dump reopen and catalog totals; no independent record-by-record completeness proof.'}})
  Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-mft-reopen-results.json') -Value $rows.ToArray()
  if($run -gt 1){Remove-Item -LiteralPath $output}
  Write-Output "MFT reopen $run/$Runs : $([Math]::Round($elapsed)) ms; $($stats.files) files"
 }
} finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
