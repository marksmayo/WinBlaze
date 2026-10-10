$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$work = Join-Path $PSScriptRoot ('../artifacts/identity-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory -Path $work | Out-Null
$exe = Join-Path $work 'WinBlaze.UI.exe'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe') -Destination $exe
$dll = Join-Path $work 'winblaze_native.dll'
$history = Join-Path $work 'history.json'
Set-Content -LiteralPath $dll -Value 'first backend'
$first = Get-BenchmarkIdentity -Tool WinBlaze -Executable $exe -HistoryPath $history
Set-Content -LiteralPath $dll -Value 'changed backend'
$second = Get-BenchmarkIdentity -Tool WinBlaze -Executable $exe -HistoryPath $history
if ($first.sha256 -ne $second.sha256) { throw 'Frontend hash must retain its literal executable meaning.' }
if (!$first.identity_sha256 -or $first.identity_sha256 -eq $second.identity_sha256) { throw 'RED: changing only the native DLL must change the benchmark identity.' }
$entries = Get-Content -LiteralPath $history -Raw | ConvertFrom-Json
if ($entries.Count -ne 2) { throw 'Backend changes must produce separate version-history entries.' }
Write-Output 'PASS: frontend + backend binary identities remain separate.'
