param(
    [ValidateRange(1,100)][int]$Runs = 10,
    [int]$TimeoutSeconds = 30,
    [switch]$OnlyMft,
    [switch]$IncludeMft,
    [switch]$KeepMftSample,
    [switch]$OnlyWinBlazeScanCsv,
    [string[]]$OnlyWinBlazeJobs = @(),
    [string]$OutputPath = '',
    [ValidateSet('WinBlaze','WinDirStat','WizTree')][string[]]$Tools = @('WinBlaze','WinDirStat','WizTree')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'feature-benchmark-lib.ps1')
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
. (Join-Path $PSScriptRoot 'process-telemetry.ps1')
if($OnlyWinBlazeScanCsv) {
    if($OnlyMft -or $IncludeMft){throw 'Scan/CSV repair cannot include an MFT run'}
    $Tools=@('WinBlaze')
}
if($OnlyWinBlazeJobs.Count) {
    if($OnlyMft -or $IncludeMft -or $OnlyWinBlazeScanCsv){throw 'Selected WinBlaze jobs cannot be combined with other repair modes'}
    $Tools=@('WinBlaze')
}
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$id = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$batch = Join-Path $PSScriptRoot "runs/features-$id"
$artifacts = Join-Path $repo "artifacts/feature-benchmarks/$id"
New-Item -ItemType Directory -Force -Path $batch,$artifacts | Out-Null
$current = Join-Path $PSScriptRoot $(if ($OnlyMft) {'current-feature-mft-results.json'} else {'current-feature-performance-results.json'})
if (!$OutputPath) { $OutputPath = Join-Path $batch 'results.json' }
$administrator = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$manifest = [ordered]@{batch_id=$id;requested_runs=$Runs;state='running';started_utc=[DateTime]::UtcNow.ToString('o');cache='uncontrolled; sequential repeats';sample_interval_ms=10;elevation=if ($administrator) {'administrator token'} else {'standard user token'};fixture=$null}
$results = [Collections.Generic.List[object]]::new()
if (!$OnlyMft -and $Tools.Count -lt 3 -and (Test-Path -LiteralPath $current)) {
    foreach ($row in (Read-BenchmarkJson -Path $current)) {
        if ($row.tool -notin $Tools -or ($OnlyWinBlazeScanCsv -and $row.job -ne 'wb-scan-csv') -or ($OnlyWinBlazeJobs.Count -and $row.job -notin $OnlyWinBlazeJobs)) { $results.Add($row) }
    }
}
function Publish-Features {
    Write-FeatureJson $OutputPath $results.ToArray()
    Copy-Item -LiteralPath $OutputPath -Destination $current
    Write-FeatureJson (Join-Path $batch 'manifest.json') $manifest
    Write-FeatureJson (Join-Path $PSScriptRoot $(if ($OnlyMft) {'current-feature-mft-batch.json'} else {'current-feature-performance-batch.json'})) $manifest
    & node (Join-Path $PSScriptRoot 'render-report.mjs')
    if ($LASTEXITCODE -ne 0) { throw 'Feature report generation failed.' }
}
$measurementLock=Enter-BenchmarkMeasurementLock
try {
$root = Join-Path $artifacts 'dataset'
$featureCache = Join-Path $artifacts 'appdata'
$fixture = if ($OnlyMft) { $null } else { New-FeatureFixture $root }
$manifest.fixture = $fixture
if ($fixture) { Write-FeatureJson (Join-Path $batch 'fixture.json') $fixture }
$wb = Join-Path $repo 'target/release/winblaze-cli.exe'
$catalog = Join-Path $repo 'target/release/examples/feature_catalog.exe'
$wd = 'C:/Program Files/WinDirStat/WinDirStat.exe'
$wiz = 'C:/Program Files/WizTree/WizTree64.exe'
$wizSeed = Join-Path $artifacts 'wiz-scan-csv-1.csv'
$wbSeed = Join-Path $artifacts 'wb-scan-csv-1.csv'
$wdSeed = Join-Path $artifacts 'windirstat-import-seed.csv'
$wdJsonSeed = Join-Path $artifacts 'windirstat-import-seed.json'
$wizUtcSeed = Join-Path $artifacts 'wiztree-utc-import-seed.csv'
$wbJsonSeed = Join-Path $artifacts 'wb-json-1.json'
$wbTsvSeed = Join-Path $artifacts 'wb-tsv-1.tsv'
$wizImportSeed = Join-Path $artifacts 'wiztree-import-seed.csv'
$ui = Join-Path $repo 'src/WinBlaze.UI/bin/x64/Release/WinBlaze.UI.exe'
$seedIdentities=@{}
if (!$OnlyMft -and !$OnlyWinBlazeScanCsv -and 'WinBlaze' -in $Tools) {
    # Foreign CSV seed creation is outside the timed import workloads.
    foreach ($seed in @(@{exe=$wd;args=('/SaveTo "{0}" "{1}"' -f $wdSeed,$root);kind='csv-wd';out=$wdSeed;tool='WinDirStat'},@{exe=$wiz;args=('"{0}" /admin=0 /export="{1}"' -f $root,$wizImportSeed);kind='csv-wiz';out=$wizImportSeed;tool='WizTree'},@{exe=$wd;args=('/SaveTo "{0}" "{1}"' -f $wdJsonSeed,$root);kind='json';out=$wdJsonSeed;tool='WinDirStat'},@{exe=$wiz;args=('"{0}" /admin=0 /export="{1}" /exportUTCTime=1' -f $root,$wizUtcSeed);kind='csv-wiz';out=$wizUtcSeed;tool='WizTree'})) {
        $seedName=$seed.tool
        $seedIdentities[$seedName]=Get-BenchmarkIdentity -Tool $seedName -Executable $seed.exe
        $info=[Diagnostics.ProcessStartInfo]::new(); $info.FileName=$seed.exe; $info.Arguments=$seed.args; $info.UseShellExecute=$false; $info.CreateNoWindow=$true
        $child=[Diagnostics.Process]::Start($info)
        try {if (!$child.WaitForExit(30000)) {$child.Kill(); throw 'External import seed creation timed out'}; if ($child.ExitCode -ne 0) {throw 'External import seed creation failed'}} finally {$child.Dispose()}
        $null=Test-FeatureOutput $seed.kind $seed.out '' $fixture
    }
}

$jobs = @(
    @{id='wb-scan-csv';tool='WinBlaze';operation='scan-csv-fixture';kind='csv-wb';exe=$wb;ext='csv';scope='Startup + scan/index/persistence + CSV export; duplicate fixture';args={param($out) '"{0}" "{1}"' -f $root,$out}},
    @{id='wb-cache-load';tool='WinBlaze';operation='cache-load';kind='cache-load';exe=$catalog;ext='csv';scope='Startup + persisted native catalog decode; no GUI';args={param($out) 'cache-load "{0}" "{1}"' -f $root,$out}},
    @{id='wb-cached-csv';tool='WinBlaze';operation='cached-csv';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + complete CSV export; no scan/GUI';args={param($out) 'csv-export "{0}" "{1}"' -f $root,$out}},
    @{id='wb-json';tool='WinBlaze';operation='json-export';kind='json-native';exe=$catalog;ext='json';scope='Startup + persisted catalog decode + full JSON export; no scan/GUI';args={param($out) 'json-export "{0}" "{1}"' -f $root,$out}},
    @{id='wb-detailed-csv';tool='WinBlaze';operation='detailed-csv-export';kind='detailed-csv-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + current owner/physical identity queries + fourteen-column CSV export; no content reads/GUI';args={param($out) 'detailed-csv "{0}" "{1}"' -f $root,$out}},
    @{id='wb-flame-layout';tool='WinBlaze';operation='flame-layout';kind='flame-layout-native';exe=(Join-Path $repo 'artifacts/parity-layouts/parity-layouts.exe');ext='json';scope='Native adapter startup + generated 4097-node hierarchy + 100 weighted flame layouts + proportional-span checks + JSON; excludes GPU/UI rendering';args={param($out) 'flame "{0}"' -f $out}},
    @{id='wb-sunburst-layout';tool='WinBlaze';operation='sunburst-layout';kind='sunburst-layout-native';exe=(Join-Path $repo 'artifacts/parity-layouts/parity-layouts.exe');ext='json';scope='Native adapter startup + generated 4097-node hierarchy + 100 weighted sunburst layouts + angular-span checks + JSON; excludes GPU/UI rendering';args={param($out) 'sunburst "{0}"' -f $out}},
    @{id='wb-permission-controls';tool='WinBlaze';operation='permission-account-controls';kind='permission-controls-native';exe=(Join-Path $repo 'artifacts/parity-layouts/parity-layouts.exe');ext='json';scope='Native adapter startup + 100 passes over 4096 generated ACL-view rows + compiled-regex account exclusion/highlighting/ACE-rights filtering + checked counts + JSON; excludes filesystem ACL queries/GUI';args={param($out) 'permissions "{0}"' -f $out}},
    @{id='wb-detailed-json';tool='WinBlaze';operation='detailed-json-export';kind='detailed-json-native';exe=$catalog;ext='json';scope='Startup + catalog decode + current owner/physical identity queries + detailed JSON export; no content reads/GUI';args={param($out) 'detailed-json "{0}" "{1}"' -f $root,$out}},
    @{id='wb-tsv';tool='WinBlaze';operation='tsv-export';kind='tsv-native';exe=$catalog;ext='tsv';scope='Startup + catalog decode + complete quoted TSV export; no scan/GUI';args={param($out) 'tsv-export "{0}" "{1}"' -f $root,$out}},
    @{id='wb-import-json';tool='WinBlaze';operation='json-import-export';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + own JSON import/catalog build + complete CSV re-export; no scan/GUI';args={param($out) 'json-import "{0}" "{1}" "{2}"' -f $root,$wbJsonSeed,$out}},
    @{id='wb-import-wd-json';tool='WinBlaze';operation='windirstat-json-import';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + installed WinDirStat JSON import/catalog build + complete CSV re-export; no scan/GUI';args={param($out) 'json-import "{0}" "{1}" "{2}"' -f $root,$wdJsonSeed,$out}},
    @{id='wb-import-tsv';tool='WinBlaze';operation='tsv-import-export';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + own TSV import/catalog build + complete CSV re-export; no scan/GUI';args={param($out) 'tsv-import "{0}" "{1}" "{2}"' -f $root,$wbTsvSeed,$out}},
    @{id='wb-import-wiz-utc';tool='WinBlaze';operation='wiztree-utc-csv-import';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + installed WizTree explicit UTC CSV import + complete CSV re-export; no scan/GUI';args={param($out) 'csv-import-utc "{0}" "{1}" "{2}"' -f $root,$wizUtcSeed,$out}},
    @{id='wb-multiple-roots';tool='WinBlaze';operation='multiple-root-scan-csv';kind='multiple-roots-native';exe=$catalog;ext='csv';scope='Startup + two disjoint 32-file folder scans + combined catalog build/physical metadata reconciliation + complete CSV export; metadata-decoy subset, 64 files; no GUI';args={param($out) 'multiple-roots "WinBlaze:\Multiple roots" "{0}" "{1}"' -f ((Join-Path $root 'left')+[Environment]::NewLine+(Join-Path $root 'right')),$out}},
    @{id='wb-regex';tool='WinBlaze';operation='regex-search';kind='regex-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + complete-catalog regex filename search + matching-row CSV; no scan/GUI';args={param($out) 'regex-search "{0}" "{1}"' -f $root,$out}},
    @{id='wb-allocated-search';tool='WinBlaze';operation='allocated-size-search';kind='allocated-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + allocated-size bound search + matching-row CSV; no scan/GUI';args={param($out) 'allocated-search "{0}" "{1}"' -f $root,$out}},
    @{id='wb-storage-cost';tool='WinBlaze';operation='storage-cost-estimate';kind='storage-cost-native';exe=$catalog;ext='json';scope='Startup + catalog decode + allocated bytes/UTC age storage-tier cost estimate + JSON; no scan/GUI';args={param($out) 'storage-cost "{0}" "{1}"' -f $root,$out}},
    @{id='wb-duplicates-sha256';tool='WinBlaze';operation='cached-duplicates-sha256';kind='duplicates-sha256-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + SHA256 hashing + byte-for-byte duplicate verification + digest CSV report; no scan/GUI';args={param($out) 'duplicates-sha256 "{0}" "{1}"' -f $root,$out}},
    @{id='wb-duplicates-md5';tool='WinBlaze';operation='cached-duplicates-md5';kind='duplicates-md5-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + MD5 hashing + byte-for-byte duplicate verification + digest CSV report; no scan/GUI';args={param($out) 'duplicates-md5 "{0}" "{1}"' -f $root,$out}},
    @{id='wb-duplicates-sha1';tool='WinBlaze';operation='cached-duplicates-sha1';kind='duplicates-sha1-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + SHA1 hashing + byte-for-byte duplicate verification + digest CSV report; no scan/GUI';args={param($out) 'duplicates-sha1 "{0}" "{1}"' -f $root,$out}},
    @{id='wb-duplicates-sha512';tool='WinBlaze';operation='cached-duplicates-sha512';kind='duplicates-sha512-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + SHA512 hashing + byte-for-byte duplicate verification + digest CSV report; no scan/GUI';args={param($out) 'duplicates-sha512 "{0}" "{1}"' -f $root,$out}},
    @{id='wb-hardlinks';tool='WinBlaze';operation='hardlink-consolidation';kind='hardlink-native';exe=(Join-Path $repo 'target/release/examples/feature_maintenance.exe');ext='fixture';scope='Native adapter startup + owned identical 2MiB file creation + locked byte-verified hard-link consolidation + byte/identity checks + cleanup; no GUI';args={param($out) 'hardlinks "{0}"' -f $out}},
    @{id='wb-compression';tool='WinBlaze';operation='ntfs-compression';kind='compression-native';exe=(Join-Path $repo 'target/release/examples/feature_maintenance.exe');ext='fixture';scope='Native adapter startup + owned 2MiB zero-file creation + compression/allocation checks + decompression/byte checks + cleanup; no GUI';args={param($out) 'compression "{0}"' -f $out}},
    @{id='wb-sparsify';tool='WinBlaze';operation='ntfs-sparsification';kind='sparsify-native';exe=(Join-Path $repo 'target/release/examples/feature_maintenance.exe');ext='fixture';scope='Native adapter startup + owned mixed 2MiB file creation + verified-zero sparse conversion + byte/allocation checks + cleanup; no GUI';args={param($out) 'sparsify "{0}"' -f $out}},
    @{id='wb-vhd';tool='WinBlaze';operation='vhd-zero-block-compaction';kind='vhd-native';exe=(Join-Path $repo 'artifacts/virtual-disks/virtual-disks.exe');ext='vhdx';scope='Native adapter startup + invalid-disk refusal + owned empty 16MiB dynamic VHDX creation + detached zero-block compaction/size validation + cleanup; no mounting/GUI';args={param($out) '"{0}"' -f $out}},
    @{id='wb-filter';tool='WinBlaze';operation='filtered-export';kind='filtered-native';exe=$catalog;ext='csv';scope='Startup + persisted catalog decode + *.bin files-only CSV export; no scan/GUI';args={param($out) 'filtered-export "{0}" "{1}"' -f $root,$out}},
    @{id='wb-extensions' ;tool='WinBlaze';operation='extension-summary';kind='extensions-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + native extension API; no scan/GUI/CSV';args={param($out) 'extensions "{0}" "{1}"' -f $root,$out}},
    @{id='wb-import';tool='WinBlaze';operation='csv-import-export';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + own-format CSV import + catalog build + complete CSV re-export; no scan/GUI';args={param($out) 'csv-import "{0}" "{1}" "{2}"' -f $root,$wbSeed,$out}},
    @{id='wb-import-wd';tool='WinBlaze';operation='windirstat-csv-import';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + WinDirStat CSV import + catalog build + complete CSV re-export; no scan/GUI';args={param($out) 'csv-import "{0}" "{1}" "{2}"' -f $root,$wdSeed,$out}},
    @{id='wb-import-wiz';tool='WinBlaze';operation='wiztree-csv-import';kind='csv-native';exe=$catalog;ext='csv';scope='Startup + WizTree CSV import + catalog build + complete CSV re-export; local dates use this PC timezone; no scan/GUI';args={param($out) 'csv-import "{0}" "{1}" "{2}"' -f $root,$wizImportSeed,$out}},
    @{id='wb-advanced-filter';tool='WinBlaze';operation='advanced-filtered-export';kind='advanced-filter-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + *.bin;unique-???.txt + exactly 1MiB logical size + UTC date bounds CSV export; no scan/GUI';args={param($out) 'advanced-filter "{0}" "{1}"' -f $root,$out}},
    @{id='wb-largest';tool='WinBlaze';operation='largest-files';kind='largest-native';exe=$catalog;ext='csv';scope='Startup + catalog decode + every largest-file page (50 per page) + CSV serialization; no scan/GUI';args={param($out) 'largest-files "{0}" "{1}"' -f $root,$out}},
    @{id='wb-flame-png';tool='WinBlaze';operation='flame-png';kind='png';exe=$ui;ext='png';scope='GUI startup + persisted catalog decode + native tree queries + flame layout/GPU rendering + 1920x1080 PNG serialization; no scan';args={param($out) '--flame-png "{0}"' -f $out}},
    @{id='wb-sunburst-png';tool='WinBlaze';operation='sunburst-png';kind='png';exe=$ui;ext='png';scope='GUI startup + persisted catalog decode + native tree queries + sunburst sector layout/GPU rendering + 1920x1080 PNG serialization; no scan';args={param($out) '--sunburst-png "{0}"' -f $out}},
    @{id='wb-png';tool='WinBlaze';operation='treemap-png';kind='png';exe=$ui;ext='png';scope='GUI startup + persisted catalog decode + native tree/extension queries + GPU treemap layout + 1920x1080 PNG serialization; no scan';args={param($out) '--treemap-png "{0}"' -f $out}},
    @{id='wb-duplicates';tool='WinBlaze';operation='content-duplicates';kind='duplicates-native';exe=$wb;ext='csv';scope='Startup + scan/index/persistence + byte-for-byte duplicate verification + CSV report; no GUI';args={param($out) '--duplicates "{0}" "{1}"' -f $root,$out}},
    @{id='wb-permissions';tool='WinBlaze';operation='permission-export';kind='permissions-native';exe=$wb;ext='csv';scope='Startup + scan/index/persistence + owner/group/DACL reads + SDDL CSV report; no GUI';args={param($out) '--permissions "{0}" "{1}"' -f $root,$out}},
    @{id='wb-actions';tool='WinBlaze';operation='file-actions';kind='actions-native';exe=(Join-Path $repo 'artifacts/file-actions/file-actions.exe');ext='fixture';scope='Native adapter startup + owned fixture creation + collision refusal + file move/recycle + folder rename + batch file/folder move/recycle; excludes GUI confirmation';args={param($out) '"{0}"' -f $out}},
    @{id='wb-watch';tool='WinBlaze';operation='change-monitoring';kind='watch-native';exe=(Join-Path $repo 'artifacts/directory-watcher/directory-watcher.exe');ext='fixture';scope='Native adapter startup + subscription + add/modify/remove notifications + 100ms ignored-write observation; excludes GUI rescan';args={param($out) '"{0}"' -f $out}},
    @{id='wb-mft';tool='WinBlaze';operation='raw-mft-dump';kind='mft';exe=$wb;ext='mft';scope='Startup + raw C: NTFS MFT dump to local SSD; administrator token; no scan tree/GUI';args={param($out) '--dump-mft C:\ "{0}"' -f $out}},
    @{id='wd-scan-csv';tool='WinDirStat';operation='scan-csv-fixture';kind='csv-wd';exe=$wd;ext='csv';scope='Startup + scan + CSV export; duplicate fixture';args={param($out) '/SaveTo "{0}" "{1}"' -f $out,$root}},
    @{id='wd-duplicates';tool='WinDirStat';operation='content-duplicates';kind='duplicates';exe=$wd;ext='csv';scope='Startup + scan + content-duplicate detection + CSV report; saved hash settings';args={param($out) '/SaveDupesTo "{0}" "{1}"' -f $out,$root}},
    @{id='wd-json';tool='WinDirStat';operation='json-export';kind='json';exe=$wd;ext='json';scope='Startup + scan + JSON export';args={param($out) '/SaveTo "{0}" "{1}"' -f $out,$root}},
    @{id='wd-permissions';tool='WinDirStat';operation='permission-export';kind='permissions';exe=$wd;ext='csv';scope='Startup + scan + accessible ACL reads + CSV report; saved settings';args={param($out) '/SavePermsTo "{0}" "{1}"' -f $out,$root}},
    @{id='wiz-scan-csv';tool='WizTree';operation='scan-csv-fixture';kind='csv-wiz';exe=$wiz;ext='csv';scope='Startup + scan + CSV export; duplicate fixture';args={param($out) '"{0}" /admin=0 /export="{1}"' -f $root,$out}},
    @{id='wiz-filter';tool='WizTree';operation='filtered-export';kind='filtered';exe=$wiz;ext='csv';scope='Startup + scan + *.bin filter + files-only CSV export';args={param($out) '"{0}" /admin=0 /export="{1}" /filter="*.bin" /filterfullpath=0 /exportfolders=0' -f $root,$out}},
    @{id='wiz-extensions';tool='WizTree';operation='extension-summary';kind='extensions-wiz';exe=$wiz;ext='csv';scope='Startup + scan + file-type CSV export';args={param($out) '"{0}" /admin=0 /exportfiletypes="{1}"' -f $root,$out}},
    @{id='wiz-png';tool='WizTree';operation='treemap-png';kind='png';exe=$wiz;ext='png';scope='Startup + scan + treemap layout + 1920x1080 PNG serialization';args={param($out) '"{0}" /admin=0 /treemapimagefile="{1}" /treemapimagewidth=1920 /treemapimageheight=1080 /treemapimagefreespace=0' -f $root,$out}},
    @{id='wiz-import';tool='WizTree';operation='csv-import-export';kind='csv-wiz';exe=$wiz;ext='csv';scope='Startup + own-format CSV import + CSV re-export; includes serialization';args={param($out) '"{0}" /admin=0 /export="{1}"' -f $wizSeed,$out}},
    @{id='wiz-mft';tool='WizTree';operation='raw-mft-dump';kind='mft';exe=$wiz;ext='mft';scope='Startup + raw C: NTFS MFT dump to local SSD; administrator token; no scan tree/GUI';args={param($out) 'C: /admin=1 /dumpmftfile="{0}"' -f $out}}
)
if ($OnlyMft) { $jobs = @($jobs | Where-Object kind -eq 'mft') }
$manifest['selected_tools']=$Tools
$manifest['only_winblaze_scan_csv']=[bool]$OnlyWinBlazeScanCsv
$manifest['only_winblaze_jobs']=$OnlyWinBlazeJobs
foreach($selectedJob in $OnlyWinBlazeJobs) {
    if(!@($jobs | Where-Object {$_.id -eq $selectedJob -and $_.tool -eq 'WinBlaze' -and $_.kind -ne 'mft'}).Count){throw "Unknown or unsupported WinBlaze repair job: $selectedJob"}
}
$manifest['jobs']=@($jobs | ForEach-Object {
    [pscustomobject]@{id=$_.id;tool=$_.tool;kind=$_.kind;operation=$_.operation;scope=$_.scope}
})
foreach ($job in ($jobs | Where-Object { $_.tool -in $Tools -and (!$OnlyWinBlazeScanCsv -or $_.id -eq 'wb-scan-csv') -and (!$OnlyWinBlazeJobs.Count -or $_.id -in $OnlyWinBlazeJobs) })) {
        $identityName = if ($job.tool -eq 'WinBlaze') { 'WinBlaze feature ' + [IO.Path]::GetFileNameWithoutExtension($job.exe) } else { $job.tool }
        $identity = Get-BenchmarkIdentity -Tool $identityName -Executable $job.exe
        if ($job.kind -eq 'mft' -and (!($OnlyMft -or $IncludeMft) -or !$administrator)) {
            $results.Add([pscustomobject]@{tool=$job.tool;job=$job.id;operation=$job.operation;status='requires-administrator';scope=$job.scope;requested_runs=$Runs;binary=$identity;note='Run -OnlyMft from an administrator terminal, or -IncludeMft with that token.'})
            Publish-Features
            continue
        }
        for ($run = 1; $run -le $Runs; $run++) {
            $out = Join-Path $artifacts "$($job.id)-$run.$($job.ext)"
            $stdout = Join-Path $artifacts "$($job.id)-$run.stdout"
            $stderr = Join-Path $artifacts "$($job.id)-$run.stderr"
            $row = [ordered]@{tool=$job.tool;job=$job.id;operation=$job.operation;run=$run;requested_runs=$Runs;status='failed';scope=$job.scope;profile=if ($job.kind -eq 'mft') {'live C: MFT'} elseif($job.kind -in @('hardlink-native','compression-native','sparsify-native')) {'owned-maintenance-2MiB'} elseif($job.kind -eq 'multiple-roots-native') {'duplicate-fixture-left-right-64-files'} elseif($job.kind -eq 'vhd-native') {'owned-empty-dynamic-VHDX-16MiB'} elseif($job.kind -in @('flame-layout-native','sunburst-layout-native')) {'generated-hierarchy-4097x100'} elseif($job.kind -eq 'permission-controls-native') {'generated-ACL-view-4096x100'} else {'duplicate-fixture-v1'};binary=$identity;started_utc=[DateTime]::UtcNow.ToString('o');elevation=$manifest.elevation;cache=$manifest.cache;sample_count=0;elapsed_ms=$null;cpu_ms_sampled_lower_bound=$null;peak_working_set_bytes_sampled=$null;peak_private_bytes_sampled=$null;output_bytes=$null;validation=$null;note=$null}
            if ($job.id -in @('wb-import-wd','wb-import-wiz','wb-import-wd-json','wb-import-wiz-utc')) {$sourceName=if ($job.id -in @('wb-import-wd','wb-import-wd-json')) {'WinDirStat'}else{'WizTree'}; $row['input_export_binary']=$seedIdentities[$sourceName]}
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $process = $null
            try {
                $arguments = & $job.args $out
                $startInfo = [Diagnostics.ProcessStartInfo]::new()
                $startInfo.FileName=$job.exe; $startInfo.Arguments=$arguments
                $startInfo.UseShellExecute=$false; $startInfo.CreateNoWindow=$true
                $startInfo.RedirectStandardOutput=$true; $startInfo.RedirectStandardError=$true
                # Isolate WinBlaze's benchmark snapshot without changing the parent environment.
                if ($job.tool -eq 'WinBlaze') { $startInfo.EnvironmentVariables['LOCALAPPDATA']=$featureCache }
                $process = [Diagnostics.Process]::new(); $process.StartInfo=$startInfo
                if (!$process.Start()) { throw 'Feature process did not start.' }
                $ownedProcessHandle=$process.Handle
                $telemetry=@{sample_count=0;cpu_ms=$null;peak_working_set_bytes=$null;private_bytes=$null}
                Add-BenchmarkProcessTelemetry $telemetry (Get-BenchmarkProcessTelemetry $process)
                $stdoutTask=$process.StandardOutput.ReadToEndAsync(); $stderrTask=$process.StandardError.ReadToEndAsync()
                while (!$process.WaitForExit(10)) {
                    $limit = if ($job.kind -eq 'mft') { 300 } else { $TimeoutSeconds }
                    if ($timer.Elapsed.TotalSeconds -gt $limit) { throw 'No unattended completion within timeout; adapter requires investigation.' }
                    Add-BenchmarkProcessTelemetry $telemetry (Get-BenchmarkProcessTelemetry $process)
                }
                $row.elapsed_ms = [Math]::Round($timer.Elapsed.TotalMilliseconds,2)
                $process.WaitForExit()
                $row['process_exit_code']=$process.ExitCode
                Add-BenchmarkProcessTelemetry $telemetry (Get-BenchmarkProcessTelemetry $process)
                [IO.File]::WriteAllText($stdout,$stdoutTask.GetAwaiter().GetResult())
                [IO.File]::WriteAllText($stderr,$stderrTask.GetAwaiter().GetResult())
                if ($null -eq $row.process_exit_code) {throw 'Feature process terminal exit status is unavailable.'}
                if ($row.process_exit_code -ne 0) { throw "Feature process exited with $($row.process_exit_code)." }
                $row.validation = Test-FeatureOutput $job.kind $out $stdout $fixture
                $row.sample_count=$telemetry.sample_count
                $row.cpu_ms_sampled_lower_bound=$telemetry.cpu_ms
                $row.peak_working_set_bytes_sampled=$telemetry.peak_working_set_bytes
                $row.peak_private_bytes_sampled=$telemetry.private_bytes
                $row['telemetry_method']='Owned kernel handle: immediate capture, 10 ms live polls, completion counters; private memory is the maximum observed current PrivateUsage.'
                if (Test-Path -LiteralPath $out -PathType Leaf) { $row.output_bytes=(Get-Item -LiteralPath $out).Length }
                $row.status = 'measured'
                Write-Output ("{0} {1} {2}/{3}: {4:N0} ms; validated" -f $job.tool,$job.operation,$run,$Runs,$row.elapsed_ms)
            } catch {
                if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id }
                $row.note = $_.Exception.Message
                $row['failure_position']=$_.InvocationInfo.PositionMessage
                Write-Warning "$($job.id): $($row.note)"
            }
            $results.Add([pscustomobject]$row)
            # Raw MFT/ACL exports can contain private metadata. Retain aggregates only.
            if (($job.kind -in @('permissions','permissions-native') -or ($job.kind -eq 'mft' -and !($KeepMftSample -and $job.id -eq 'wb-mft' -and $run -eq 1))) -and (Test-Path -LiteralPath $out)) { Remove-Item -LiteralPath $out }
            Publish-Features
            if ($row.status -ne 'measured') { break }
        }
    }
    $manifest.state = if (@($results | Where-Object status -eq failed).Count) {'complete-with-failures'} else {'complete'}
    $manifest['completed_utc'] = [DateTime]::UtcNow.ToString('o')
    Publish-Features
} catch { $manifest.state='failed'; $manifest['error']=$_.Exception.Message; Publish-Features; throw }

finally { $measurementLock.ReleaseMutex(); $measurementLock.Dispose() }
