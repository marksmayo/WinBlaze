param([ValidateRange(10,100)][int]$Runs=10)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$audit=Join-Path $repo 'artifacts/parity-audit/interop'
New-Item -ItemType Directory -Path $audit -Force | Out-Null
$fixture=Join-Path $audit 'fixture'
New-Item -ItemType Directory -Path $fixture -Force | Out-Null
$file=Join-Path $fixture 'probe.txt'
[IO.File]::WriteAllText($file,'three')
$knownUtc=[datetime]::new(2020,1,2,3,4,5,[DateTimeKind]::Utc)
[IO.File]::SetLastWriteTimeUtc($file,$knownUtc)
$expected=([DateTimeOffset]$knownUtc).ToUnixTimeSeconds()
$wiz='C:\Program Files\WizTree\WizTree64.exe'
$wd='C:\Program Files\WinDirStat\WinDirStat.exe'
$native=Join-Path $repo 'target/release/examples/feature_catalog.exe'
$records=[Collections.Generic.List[object]]::new()
function Invoke-AuditProcess([string]$Exe,[string]$Arguments,[switch]$Native) {
    $info=[Diagnostics.ProcessStartInfo]::new($Exe,$Arguments)
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    $info.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
    if($Native){$info.EnvironmentVariables['LOCALAPPDATA']=Join-Path $audit 'appdata'}
    $process=[Diagnostics.Process]::Start($info)
    try {
        if(!$process.WaitForExit(30000)){$process.Kill();throw "Audit process timed out: $Exe"}
        return $process.ExitCode
    } finally {$process.Dispose()}
}
$wdSeed=Join-Path $audit 'windirstat.csv'
if((Invoke-AuditProcess $wd ('/SaveTo "{0}" "{1}"' -f $wdSeed,$fixture)) -ne 0){throw 'WinDirStat seed export failed'}
$wdRows=@(Import-Csv -LiteralPath $wdSeed)
$ownerSeed=Join-Path $audit 'windirstat-owner.csv'
$wdRows | Select-Object *,@{Name='Owner';Expression={'AUDIT\fixture'}} |
    Export-Csv -LiteralPath $ownerSeed -NoTypeInformation -Encoding utf8
$reorderedSeed=Join-Path $audit 'windirstat-reordered-owner.csv'
$wdRows | Select-Object @{Name='Owner';Expression={'AUDIT\fixture'}},Index,'Physical Size','WinDirStat Attributes','Logical Size','Last Change',Attributes,Folders,Files,Name |
    Export-Csv -LiteralPath $reorderedSeed -NoTypeInformation -Encoding utf8
$localizedSeed=Join-Path $audit 'windirstat-localized.csv'
$localizedLines=[IO.File]::ReadAllLines($ownerSeed)
$localizedLines[0]='"Name","Dateien","Ordner","Logische Größe","Physische Größe","Attribute","Letzte Änderung","WinDirStat-Attribute","Index","Besitzer"'
[IO.File]::WriteAllLines($localizedSeed,$localizedLines,[Text.UTF8Encoding]::new($false))
$wdJson=Join-Path $audit 'windirstat.json'
if((Invoke-AuditProcess $wd ('/SaveTo "{0}" "{1}"' -f $wdJson,$fixture)) -ne 0){throw 'WinDirStat JSON seed export failed'}
$localizedJson=Join-Path $audit 'windirstat-localized.json'
$translated=@{'Files'='Dateien';'Folders'='Ordner';'Logical Size'='Logische Größe';'Physical Size'='Physische Größe';'Attributes'='Attribute';'Last Change'='Letzte Änderung';'WinDirStat Attributes'='WinDirStat-Attribute';'Owner'='Besitzer'}
$localizedRecords=@(Get-Content $wdJson -Raw | ConvertFrom-Json -AsHashtable | ForEach-Object {
    $row=[ordered]@{}
    foreach($name in $_.Keys){$caption=if($translated.ContainsKey($name)){$translated[$name]}else{$name};$row[$caption]=$_[$name]}
    $row
})
ConvertTo-Json -InputObject $localizedRecords -Depth 8 | Set-Content $localizedJson -Encoding utf8
$cli=Join-Path $repo 'target/release/winblaze-cli.exe'
$wbSeed=Join-Path $audit 'winblaze.csv'
if((Invoke-AuditProcess $cli ('"{0}" "{1}"' -f $fixture,$wbSeed) -Native) -ne 0){throw 'Own scan seed failed'}
$tsv=Join-Path $audit 'winblaze.tsv'
if((Invoke-AuditProcess $native ('tsv-export "{0}" "{1}"' -f $fixture,$tsv) -Native) -ne 0){throw 'Own TSV seed failed'}
for($run=1;$run -le $Runs;$run++){
    $json=Join-Path $audit "wiztree-extension-$run.json"
    $code=Invoke-AuditProcess $wiz ('"{0}" /admin=0 /export="{1}"' -f $fixture,$json)
    $text=Get-Content -LiteralPath $json -Raw
    $isJson=$true
    try{$null=$text | ConvertFrom-Json}catch{$isJson=$false}
    if($code -ne 0 -or $isJson -or !$text.Contains('File Name')){throw 'Unexpected WizTree .json-extension export'}
    $records.Add(@{run=$run;case='WizTree .json export filename';result='CSV content, not JSON';exit_code=$code})

    foreach($mode in @('local','utc')){
        $csv=Join-Path $audit "wiztree-$mode-$run.csv"
        $utcFlag=if($mode -eq 'utc'){1}else{0}
        $code=Invoke-AuditProcess $wiz ('"{0}" /admin=0 /export="{1}" /exportUTCTime={2}' -f $fixture,$csv,$utcFlag)
        if($code -ne 0){throw 'WizTree timestamp seed failed'}
        $output=Join-Path $audit "winblaze-$mode-$run.csv"
        $operation=if($mode -eq 'utc'){'csv-import-utc'}else{'csv-import'}
        $code=Invoke-AuditProcess $native ('{3} "{0}" "{1}" "{2}"' -f $fixture,$csv,$output,$operation) -Native
        if($code -ne 0){throw 'WinBlaze timestamp import failed'}
        $probe=Import-Csv -LiteralPath $output | Where-Object {$_.Kind -eq 'File' -and $_.Path.EndsWith('\probe.txt')}
        if(@($probe).Count -ne 1){throw 'Imported probe missing'}
        $actual=[long]$probe.ModifiedUnixSeconds
        if($actual -ne $expected){throw 'Explicit timestamp convention did not preserve UTC'}
        $records.Add(@{run=$run;case="WizTree $mode timestamp import";expected_utc=$expected;actual_utc=$actual;delta_seconds=$actual-$expected;result=if($actual -eq $expected){'preserved'}else{'timezone shift'}})
    }
    foreach($case in @(
        @{name='WinDirStat optional Owner CSV';seed=$ownerSeed;owner=$true;operation='csv-import-json'},
        @{name='WinDirStat reordered Owner CSV';seed=$reorderedSeed;owner=$true;operation='csv-import-json'},
        @{name='Localized fixed-order WinDirStat CSV';seed=$localizedSeed;owner=$true;operation='csv-import-json'},
        @{name='WinDirStat JSON import';seed=$wdJson;owner=$false;operation='json-import'},
        @{name='Localized fixed-order WinDirStat JSON';seed=$localizedJson;owner=$false;operation='json-import'},
        @{name='WinBlaze TSV roundtrip';seed=$tsv;owner=$false;operation='tsv-import'}
    )) {
        $extension=if($case.owner){'json'}else{'csv'}
        $output=Join-Path $audit ("format-"+$case.operation+"-"+[guid]::NewGuid().ToString('N')+".$extension")
        $code=Invoke-AuditProcess $native ('{3} "{0}" "{1}" "{2}"' -f $fixture,$case.seed,$output,$case.operation) -Native
        if($code -ne 0){throw ($case.name+' was rejected')}
        if($case.owner) {
            $probe=@((Get-Content $output -Raw | ConvertFrom-Json).entries | Where-Object {$_.Kind -eq 'File' -and $_.Path.EndsWith('\probe.txt')})
            if($probe.Count -ne 1 -or $probe[0].Owner -ne 'AUDIT\fixture'){throw 'Optional owner metadata was lost'}
        } else {
            $probe=@(Import-Csv -LiteralPath $output | Where-Object {$_.Kind -eq 'File' -and $_.Path.EndsWith('\probe.txt')})
            if($probe.Count -ne 1){throw 'Imported format lost the known file'}
        }
        if([long]$probe[0].ModifiedUnixSeconds -ne $expected){throw 'Format conversion changed the UTC timestamp'}
        if([uint64]$probe[0].LogicalBytes -ne 5){throw 'Format conversion changed the file size'}
        $records.Add(@{run=$run;case=$case.name;result='preserved';exit_code=$code;owner_retained=$case.owner})
    }
}
$result=@{
    runs=$Runs;generated_utc=[datetime]::UtcNow.ToString('o');scope='Functional format probes, not performance measurements'
    known_file_utc=$knownUtc.ToString('o');timezone=(Get-TimeZone).Id
    identities=@($wiz,$wd,$native | ForEach-Object {@{path=$_;sha256=(Get-FileHash -LiteralPath $_).Hash;version=(Get-Item -LiteralPath $_).VersionInfo.ProductVersion}})
    records=$records.ToArray()
}
$result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'current-parity-interop-validation.json') -Encoding utf8
Write-Output "Verified $($records.Count) functional format probes, n=$Runs per case."
