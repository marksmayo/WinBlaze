$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'feature-benchmark-lib.ps1')
$work=Join-Path $PSScriptRoot ('../artifacts/validator-'+[DateTime]::UtcNow.Ticks)
New-Item -ItemType Directory -Path $work | Out-Null
$fixture=[pscustomobject]@{files=1;logical_bytes=3;extensions=@{bin=1};duplicate_names=@('a.bin')}
$stdout=Join-Path $work 'stats.json'
'{"files":1,"logical_bytes":3,"load_ms":1,"operation_ms":2}' | Set-Content $stdout -Encoding utf8
$output=Join-Path $work 'detailed.json'
'{"schema_version":1,"entries":[{"Kind":"Directory","Files":1,"LogicalBytes":3},{"Kind":"File","Path":"C:\\fixture\\a.bin","LogicalBytes":3,"Owner":"Fixture","VolumeIdentity":"1","FileIdentity":"2","LinkCount":1}]}' |
    Set-Content $output -Encoding utf8
$result=Test-FeatureOutput 'detailed-json-native' $output $stdout $fixture
if(!$result.verified){throw 'Valid detailed JSON was rejected'}
$invalid=Get-Content $output -Raw | ConvertFrom-Json
$invalid.entries[1].Owner=$null
$invalid | ConvertTo-Json -Depth 6 | Set-Content $output -Encoding utf8
$refused=$false
try{$null=Test-FeatureOutput 'detailed-json-native' $output $stdout $fixture}catch{$refused=$true}
if(!$refused){throw 'Missing owner passed detailed metadata validation'}
Write-Output 'PASS: detailed JSON workload validates the full catalog and refuses missing metadata'
