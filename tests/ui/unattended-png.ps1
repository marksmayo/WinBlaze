param(
    [string]$AppPath='src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe',
    [string]$CacheDirectory=''
)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$app=(Resolve-Path -LiteralPath $AppPath).ProviderPath
$output=Join-Path $repo ('artifacts/ui-unattended-png-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $output | Out-Null
if(!$CacheDirectory) {
    $CacheDirectory=Join-Path $output 'appdata'
    $fixture=Join-Path $output 'fixture'
    New-Item -ItemType Directory -Path $CacheDirectory,$fixture | Out-Null
    foreach($size in @(4096,8192,16384)){[IO.File]::WriteAllBytes((Join-Path $fixture ($size.ToString()+'.bin')),(New-Object byte[] $size))}
    $scan=[Diagnostics.ProcessStartInfo]::new()
    $scan.FileName=Join-Path (Split-Path $app -Parent) 'winblaze-cli.exe'
    $scan.Arguments='--scan "{0}" "{1}"' -f $fixture,(Join-Path $output 'catalog.csv')
    $scan.UseShellExecute=$false
    $scan.CreateNoWindow=$true
    $scan.EnvironmentVariables['LOCALAPPDATA']=$CacheDirectory
    $scanner=[Diagnostics.Process]::Start($scan)
    try {
        if(!$scanner.WaitForExit(20000)){$scanner.Kill();throw 'Owned catalog fixture scan timed out'}
        if($scanner.ExitCode){throw 'Owned catalog fixture scan failed'}
    } finally {$scanner.Dispose()}
}
$cache=(Resolve-Path -LiteralPath $CacheDirectory).ProviderPath
if(!(Test-Path -LiteralPath (Join-Path $cache 'WinBlaze/index/winblaze.index.bin'))){throw 'Requires an existing persisted catalog fixture'}
$rows=@()
foreach($mode in @('treemap','flame','sunburst')) {
    $png=Join-Path $output ($mode+'.png')
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$app
    $start.Arguments='--{0}-png "{1}"' -f $mode,$png
    $start.UseShellExecute=$false
    $start.CreateNoWindow=$true
    $start.EnvironmentVariables['LOCALAPPDATA']=$cache
    $process=[Diagnostics.Process]::Start($start)
    try {
        if(!$process.WaitForExit(20000)){ $process.Kill();throw 'Owned PNG export timed out' }
        $bytes=if(Test-Path -LiteralPath $png){[IO.File]::ReadAllBytes($png)}else{@()}
        $valid=$bytes.Length -gt 24 -and [BitConverter]::ToString($bytes,0,8) -eq '89-50-4E-47-0D-0A-1A-0A'
        $width=if($valid){([int]$bytes[16]*16777216)+([int]$bytes[17]*65536)+([int]$bytes[18]*256)+$bytes[19]}else{0}
        $height=if($valid){([int]$bytes[20]*16777216)+([int]$bytes[21]*65536)+([int]$bytes[22]*256)+$bytes[23]}else{0}
        $rows += [pscustomobject]@{mode=$mode;exit_code=$process.ExitCode;valid_png=$valid;width=$width;height=$height;bytes=$bytes.Length;sha256=if($valid){(Get-FileHash -LiteralPath $png).Hash}else{$null}}
    } finally {$process.Dispose()}
}
[pscustomobject]@{app=$app;app_sha256=(Get-FileHash -LiteralPath $app).Hash;cache=$cache;results=$rows} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $output 'result.json') -Encoding UTF8
$rows | Format-Table | Out-String | Write-Output
if(@($rows | Where-Object {$_.exit_code -ne 0 -or !$_.valid_png -or $_.width -ne 1920 -or $_.height -ne 1080}).Count){throw "Unattended PNG export or normal shutdown failed; evidence=$output"}
if(@($rows.sha256 | Select-Object -Unique).Count -ne 3){throw 'Distinct visualization layouts produced identical PNGs'}
Write-Output "PASS: all three unattended PNG modes export distinct valid 1920x1080 images and exit normally; evidence=$output"
