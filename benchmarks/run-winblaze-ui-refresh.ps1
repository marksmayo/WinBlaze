param([ValidateRange(1,100)][int]$Runs=10,[string[]]$Profiles=@('tiny','small','medium','fanout','fanout-large','scale','full'),[switch]$AppendGui,[switch]$SkipCache)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$app=Join-Path $repo 'src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'
$identity=Get-BenchmarkIdentity -Tool WinBlaze -Executable $app
$root=Join-Path $repo ('artifacts/refresh-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Path $root -Force | Out-Null
$gui=[Collections.Generic.List[object]]::new()
if ($AppendGui -and (Test-Path (Join-Path $PSScriptRoot 'current-gui-results.json'))) {
 foreach($row in (Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-gui-results.json'))) {
  if($row.profile -notin $Profiles){$gui.Add($row)}
 }
}
foreach($profile in $Profiles){
 if($profile -ne 'full' -and !(Test-Path (Join-Path $repo "artifacts/benchmark-datasets/$profile.manifest.json"))){
  & (Join-Path $PSScriptRoot 'make-datasets.ps1') -Root (Join-Path $repo 'artifacts/benchmark-datasets') -Size $profile | Out-Null
 }
 for($run=1;$run -le $Runs;$run++){
  $raw=& (Join-Path $PSScriptRoot 'run-ui-benchmark.ps1') -AppPath $app -Size $profile -DatasetRoot (Join-Path $repo 'artifacts/benchmark-datasets') -TimeoutSeconds 300
  $r=($raw | Where-Object {$_ -is [string]}) -join "`n" | ConvertFrom-Json
  $gui.Add([pscustomobject]@{tool='WinBlaze current GUI';profile=$profile;scenario="$profile-$run";run=$run;binary=$identity;utc=[DateTime]::UtcNow.ToString('o');scan_ms=$r.scan_duration_ms;observed_completion_ms=$r.elapsed_ms;sampled_peak_working_set_bytes_through_observation=$r.sampled_peak_working_set_bytes_through_observation;sampled_cpu_ms_through_observation=$r.sampled_cpu_ms_through_observation;files=$r.expected_files;cache_directory=$r.cache_directory;cache_retained=$true;last_input_ms=$r.last_input_ms;peak_frame_ms=$r.peak_frame_ms;peak_flush_ms=$r.peak_flush_ms;scope='app-reported GUI scan; resources sampled through scan/frame/treemap observation';included_in_summary=$true})
  if ($profile -eq 'full') {
   foreach($previous in ($gui | Where-Object { $_.profile -eq 'full' -and $_.cache_retained -and $_.cache_directory -ne $r.cache_directory })) {
    $cacheRoot=[IO.Path]::GetFullPath((Join-Path $repo 'artifacts/benchmark-datasets'))+[IO.Path]::DirectorySeparatorChar
    $cacheDirectory=[IO.Path]::GetFullPath($previous.cache_directory)
    if(!$cacheDirectory.StartsWith($cacheRoot,[StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($cacheDirectory) -notmatch '^appdata-[0-9a-f]{32}$'){throw 'Refusing to prune cache outside owned benchmark directories'}
    $cacheFile=Join-Path $cacheDirectory 'WinBlaze/index/winblaze.index.bin'
    if(Test-Path -LiteralPath $cacheFile){Remove-Item -LiteralPath $cacheFile}
    $previous.cache_retained=$false
   }
  }
  Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-gui-results.json') -Value $gui.ToArray()
  Write-Host "GUI $profile $run/$Runs : $($r.scan_duration_ms) ms"
 }
 & node (Join-Path $PSScriptRoot 'render-report.mjs') | Out-Null
}
$fullCache=($gui | Where-Object profile -eq 'full' | Select-Object -Last 1).cache_directory
$cache=[Collections.Generic.List[object]]::new()
if(!$SkipCache){for($run=1;$run -le $Runs;$run++){
 $r=(& (Join-Path $PSScriptRoot 'run-ui-cache-load.ps1') -AppPath $app -LocalAppData $fullCache -TimeoutSeconds 60 | Where-Object {$_ -is [string]}) -join "`n" | ConvertFrom-Json
 $cache.Add([pscustomobject]@{run=$run;binary=$identity;native_decode_ms=$r.cache_decode_ms;cpu_ms_through_observation=$r.cpu_ms_through_observation;working_set_bytes_at_observation=$r.working_set_bytes_at_observation;reported_read_ms=$r.cache_read_ms;entries=$r.cache_entries;cache_bytes_label=$r.cache_bytes_label;observed_completion_ms=$r.elapsed_ms;scope='existing full-drive snapshot; launch to accessibility observation includes diagnostics/treemap work'})
 Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-cache-results.json') -Value $cache.ToArray()
 Write-Host "Cache $run/$Runs : $($r.cache_decode_ms) ms decode"
}}
$incremental=[Collections.Generic.List[object]]::new()
foreach($mutation in @('add','remove','modify')){
 for($run=1;$run -le $Runs;$run++){
  $fixture=Join-Path $root "$mutation-$run"
  $resolved=[IO.Path]::GetFullPath($fixture)
  if(!$resolved.StartsWith($repo+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Fixture must remain inside this repository.'}
  $raw=& (Join-Path $PSScriptRoot 'run-ui-incremental-benchmark.ps1') -AppPath $app -Size tiny -Mutation $mutation -DatasetRoot $resolved -GenerateDataset
  $r=($raw | Where-Object {$_ -is [string]}) -join "`n" | ConvertFrom-Json
  $incremental.Add([pscustomobject]@{run=$run;mutation=$mutation;binary=$identity;scan_ms=$r.scan_duration_ms;observed_completion_ms=$r.incremental_elapsed_ms;correctness=$r.incremental_correctness;scope='owned tiny fixture; initial scan followed by one mutation'})
  Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-incremental-results.json') -Value $incremental.ToArray()
  Write-Host "Incremental $mutation $run/$Runs : $($r.scan_duration_ms) ms"
 }
}
& node (Join-Path $PSScriptRoot 'render-report.mjs')
