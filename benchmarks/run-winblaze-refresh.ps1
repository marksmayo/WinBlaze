param([ValidateRange(1,100)][int]$Runs=10,[switch]$SkipStandard,[switch]$SkipUi,[switch]$IncludeElevated,[switch]$SkipBuild)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location $repo
$measurementLock=Enter-BenchmarkMeasurementLock
$batch=Join-Path $PSScriptRoot ('runs/refresh-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Force -Path $batch | Out-Null
Get-ChildItem $PSScriptRoot -Filter 'current-*.json' | Copy-Item -Destination $batch
$manifest=@{batch_id=Split-Path $batch -Leaf;started_utc=[DateTime]::UtcNow.ToString('o');requested_runs=$Runs;state='running';profiles=@('tiny','scale','fanout-large','full');scope='WinBlaze-only refresh; comparator observations retained from prior dated batch';competitor_refresh_policy='reuse-complete-version-baselines-v1';selected_tools=@('WinBlaze');cache='uncontrolled; no claim of cold cache';elevation='standard user token';error=$null}
function Publish {
 Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-benchmark-batch.json') -Value $manifest
 & node (Join-Path $PSScriptRoot 'render-report.mjs') | Out-Null
 if($LASTEXITCODE -ne 0){throw 'Report rendering failed.'}
}
try {
 if(!$SkipBuild){
  & cargo +1.96.0 build --release --offline -p winblaze-native -p winblaze-scanner -p winblaze-index --examples
  if($LASTEXITCODE -ne 0){throw 'Release benchmark build failed.'}
  & cargo +1.96.0 build --release --offline -p winblaze-native
  if($LASTEXITCODE -ne 0){throw 'Native DLL build failed.'}
  @'
import os, subprocess
r = subprocess.run([r'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe', 'src/WinBlaze.UI/WinBlaze.UI.vcxproj', '/p:Configuration=Release', '/p:Platform=x64', '/m', '/nologo', '/v:minimal'], env=dict(os.environ))
if r.returncode: raise SystemExit(r.returncode)
for project in ['tests/native/file-actions.vcxproj', 'tests/native/directory-watcher.vcxproj', 'tests/native/virtual-disks.vcxproj', 'tests/native/shell-context.vcxproj', 'tests/native/portable-devices.vcxproj', 'tests/native/treemap.vcxproj', 'tests/native/search-input.vcxproj', 'tests/native/color-resources.vcxproj', 'tests/native/permission-view.vcxproj', 'tests/native/parity-layouts.vcxproj']:
    r = subprocess.run([r'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\amd64\MSBuild.exe', project, '/p:Configuration=Release', '/p:Platform=x64', '/m', '/nologo', '/v:minimal'], env=dict(os.environ))
    if r.returncode: raise SystemExit(r.returncode)
'@ | python -
  if($LASTEXITCODE -ne 0){throw 'GUI build failed.'}
 }
 foreach($size in @('tiny','small','medium','fanout','fanout-large','scale')) {
  if(!(Test-Path (Join-Path $repo "artifacts/benchmark-datasets/$size.manifest.json"))) {
   & (Join-Path $PSScriptRoot 'make-datasets.ps1') -Root (Join-Path $repo 'artifacts/benchmark-datasets') -Size $size | Out-Null
  }
 }
 Publish
 if(!$SkipStandard){
  $saved=@(Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-workflow-results.json') | Where-Object tool -ne WinBlaze)
  $temp=Join-Path $batch 'winblaze-workflow.json'
  & (Join-Path $PSScriptRoot 'run-competitor-comparison.ps1') -Tools WinBlaze -Runs $Runs -Profiles $manifest.profiles -OutputPath $temp
  $fresh=@(Read-BenchmarkJson -Path $temp)
  Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-workflow-results.json') -Value @($saved+$fresh)
  Publish
  & (Join-Path $PSScriptRoot 'run-startup-comparison.ps1') -Tools WinBlaze -Runs $Runs
  Publish
  & (Join-Path $PSScriptRoot 'run-current-engine-comparison.ps1') -Runs $Runs -Profiles tiny,small,medium,fanout,fanout-large,scale,full
  Publish
  & (Join-Path $PSScriptRoot 'run-feature-comparison.ps1') -Tools WinBlaze -Runs $Runs
  $featureRows=@(Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-feature-performance-results.json') | Where-Object { $_.tool -eq 'WinBlaze' -and $_.operation -ne 'raw-mft-dump' })
  $featureManifest=Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-feature-performance-batch.json')
  $expectedJobs=@($featureManifest.jobs | Where-Object {$_.tool -eq 'WinBlaze' -and $_.kind -ne 'mft'})
  if($featureRows.Count -ne $expectedJobs.Count*$Runs -or @($featureRows | Where-Object status -ne measured).Count){throw 'WinBlaze feature batch is incomplete or failed output validation.'}
  foreach($job in $expectedJobs){if(@($featureRows | Where-Object job -eq $job.id).Count -ne $Runs){throw "Incomplete feature job: $($job.id)"}}
  Publish
 }
 if(!$SkipUi){
  & (Join-Path $PSScriptRoot 'run-winblaze-ui-refresh.ps1') -Runs $Runs
  & (Join-Path $PSScriptRoot 'run-ui-ready-comparison.ps1') -Runs $Runs
  Publish
 }
 $memoryIdentity=Get-BenchmarkIdentity -Tool 'WinBlaze index memory' -Executable (Join-Path $repo 'target/release/examples/index_memory.exe')
 $memory=[Collections.Generic.List[object]]::new()
 foreach($files in @(10000,100000,1000000)){
  for($run=1;$run -le $Runs;$run++){
   $raw=& (Join-Path $repo 'target/release/examples/index_memory.exe') $files
   if($LASTEXITCODE -ne 0){throw 'Index memory benchmark failed.'}
   $r=$raw | ConvertFrom-Json
   $memory.Add([pscustomobject]@{run=$run;files=$files;binary=$memoryIdentity;metrics=$r})
  }
 }
 ConvertTo-Json -InputObject $memory.ToArray() -Depth 12 | Set-Content (Join-Path $PSScriptRoot 'current-index-memory-results.json') -Encoding UTF8
 if($IncludeElevated){
  & (Join-Path $PSScriptRoot 'run-elevated-comparison.ps1') -Runs $Runs -SkipCompetitors
  & (Join-Path $PSScriptRoot 'run-elevated-native-comparison.ps1') -Runs $Runs
 }
 & (Join-Path $PSScriptRoot 'run-public-operation-benchmarks.ps1') -Runs $Runs
 & node (Join-Path $PSScriptRoot 'summarize-optimizations.mjs') | Out-Null
 $manifest.state='complete';$manifest.completed_utc=[DateTime]::UtcNow.ToString('o');Publish
} catch {$manifest.state='failed';$manifest.error=$_.Exception.Message;Publish;throw}

finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
