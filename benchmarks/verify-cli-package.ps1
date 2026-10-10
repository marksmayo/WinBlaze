$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$cli=Join-Path $repo 'artifacts/portable/WinBlaze-Release-x64-portable/winblaze-cli.exe'
if(!(Test-Path -LiteralPath $cli)){throw 'RED: shipped portable package lacks the unattended scan/analysis CLI'}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead((Join-Path $repo 'artifacts/portable/WinBlaze-Release-x64-portable.zip'))
try{if(!@($zip.Entries|Where-Object {$_.Name -eq 'winblaze-cli.exe'}).Count){throw 'CLI absent from ZIP'}}finally{$zip.Dispose()}
$work=Join-Path $repo ('artifacts/packaged-cli-'+[guid]::NewGuid().ToString('N'));$data=Join-Path $work 'data';New-Item -ItemType Directory -Force $data|Out-Null
[IO.File]::WriteAllText((Join-Path $data 'fixture.txt'),'packaged CLI')
$previous=$env:LOCALAPPDATA
try{$env:LOCALAPPDATA=Join-Path $work 'appdata'; & $cli --scan $data (Join-Path $work 'scan.csv');if($LASTEXITCODE -ne 0){throw 'Packaged CLI failed'};$rows=Import-Csv (Join-Path $work 'scan.csv');if($rows[0].Files -ne 1){throw 'Packaged scan lost files'}}finally{$env:LOCALAPPDATA=$previous}
Write-Output 'PASS: portable stage and ZIP include a working unattended scan CLI'
