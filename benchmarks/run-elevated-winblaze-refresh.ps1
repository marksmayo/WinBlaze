param([ValidateRange(1,100)][int]$Runs=10,[switch]$SkipNative,[int]$AfterProcessId=0)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
if($AfterProcessId){while(Get-Process -Id $AfterProcessId -ErrorAction SilentlyContinue){Start-Sleep -Seconds 2}}
$measurementLock=Enter-BenchmarkMeasurementLock
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location $repo
$env:LOCALAPPDATA=Join-Path $repo 'artifacts/elevated-benchmark-appdata'
New-Item -ItemType Directory -Path $env:LOCALAPPDATA -Force | Out-Null
try {
 & (Join-Path $PSScriptRoot 'run-elevated-comparison.ps1') -Runs $Runs -SkipCompetitors
 if(!$SkipNative){ & (Join-Path $PSScriptRoot 'run-elevated-native-comparison.ps1') -Runs $Runs }
 & node (Join-Path $PSScriptRoot 'render-report.mjs')
 if($LASTEXITCODE -ne 0){throw 'Report rendering failed.'}
 @{state='complete';detail='Current-build WinBlaze MFT phase and native pipeline refresh completed with 10 runs each; comparator observations retained.';utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'current-elevated-status.json') -Encoding UTF8
} catch {
 @{state='failed';detail=$_.Exception.Message;utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'current-elevated-status.json') -Encoding UTF8
 exit 1
}

finally {$measurementLock.ReleaseMutex();$measurementLock.Dispose()}
