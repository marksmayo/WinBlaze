. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$ErrorActionPreference = 'Stop'

function Write-FeatureJson {
    param([string]$Path,[object]$Value)
    Write-BenchmarkJson -Path $Path -Value $Value
}

function New-FeatureFixture {
    param([string]$Root)
    New-Item -ItemType Directory -Force -Path $Root | Out-Null
    $random = [Random]::new(104729)
    $duplicates = [Collections.Generic.List[string]]::new()
    $bytes = New-Object byte[] 1048576
    for ($group = 0; $group -lt 32; $group++) {
        $random.NextBytes($bytes)
        for ($copy = 0; $copy -lt 4; $copy++) {
            $name = 'group-{0:D3}-copy-{1}.bin' -f $group,$copy
            [IO.File]::WriteAllBytes((Join-Path $Root $name),$bytes)
            $duplicates.Add($name)
        }
        $random.NextBytes($bytes)
        [IO.File]::WriteAllBytes((Join-Path $Root ('unique-{0:D3}.txt' -f $group)),$bytes)
        # Same name, size and date, but different contents: metadata false positives.
        foreach ($side in @('left','right')) {
            $directory = Join-Path $Root $side
            New-Item -ItemType Directory -Force -Path $directory | Out-Null
            $random.NextBytes($bytes)
            $path = Join-Path $directory ('decoy-{0:D3}.dat' -f $group)
            [IO.File]::WriteAllBytes($path,$bytes)
            [IO.File]::SetLastWriteTimeUtc($path,[DateTime]::Parse('2026-01-01T00:00:00Z').ToUniversalTime())
        }
    }
    $files = @(Get-ChildItem -LiteralPath $Root -Recurse -File)
    if ($files.Count -ne 224) { throw 'Fixture contains unexpected files.' }
    return [pscustomobject]@{schema=1;files=224;logical_bytes=234881024;duplicate_groups=32;duplicate_files=128;metadata_decoy_pairs=32;duplicate_names=$duplicates.ToArray();extensions=@{bin=128;txt=32;dat=64};generation='seed 104729, 1 MiB files; each true group has four byte-identical copies; decoys differ by contents'}
}

function Read-FeatureCsv {
    param([string]$Path,[switch]$WizTree)
    $lines = [IO.File]::ReadAllLines($Path)
    if ($WizTree) { $lines = @($lines | Select-Object -Skip 1) }
    return @($lines | ConvertFrom-Csv)
}

function Test-FeatureOutput {
    param([string]$Kind,[string]$Path,[string]$Stdout,[object]$Fixture)
    if ($Kind -in @('cache-load','extensions-native','csv-native','json-native','filtered-native','advanced-filter-native','largest-native','detailed-json-native','tsv-native','regex-native','allocated-native','storage-cost-native')) {
        $stats = Get-Content -LiteralPath $Stdout -Raw | ConvertFrom-Json
        if ($stats.files -ne $Fixture.files -or $stats.logical_bytes -ne $Fixture.logical_bytes) { throw 'Native catalog totals differ from fixture.' }
        if ($Kind -eq 'extensions-native' -and $stats.extension_files -ne $Fixture.files) { throw 'Extension aggregation lost files.' }
        if ($Kind -eq 'csv-native') {
            $rows = Read-FeatureCsv $Path
            if (@($rows | Where-Object Kind -eq File).Count -ne $Fixture.files -or ($rows | Where-Object Kind -eq File | Measure-Object LogicalBytes -Sum).Sum -ne $Fixture.logical_bytes) { throw 'CSV export lost files.' }
        }
        if ($Kind -in @('json-native','detailed-json-native')) {
            $decoded = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
            $files = @($decoded.entries | Where-Object Kind -eq File)
            if ($decoded.schema_version -ne 1 -or $decoded.entries[0].Files -ne $Fixture.files -or $decoded.entries[0].LogicalBytes -ne $Fixture.logical_bytes -or $files.Count -ne $Fixture.files -or ($files | Measure-Object LogicalBytes -Sum).Sum -ne $Fixture.logical_bytes) { throw 'Native JSON schema or complete catalog totals differ from fixture.' }
        }
        if ($Kind -eq 'detailed-json-native') {
            if (@($files | Where-Object {!$_.Owner -or !$_.VolumeIdentity -or !$_.FileIdentity -or $_.LinkCount -lt 1}).Count) {
                throw 'Detailed JSON lost current owner or physical identities.'
            }
            foreach($file in $files) {
                $number=[uint64]::Parse($file.VolumeIdentity)
                if([Numerics.BigInteger]::Parse($file.FileIdentity) -le 0){throw 'Invalid physical file identity'}
            }
        }
        if ($Kind -eq 'tsv-native') {
            $files=@(Import-Csv -LiteralPath $Path -Delimiter ([char]9) | Where-Object Kind -eq File)
            if($files.Count -ne $Fixture.files -or ($files | Measure-Object LogicalBytes -Sum).Sum -ne $Fixture.logical_bytes){
                throw 'TSV export lost files or logical bytes.'
            }
        }
        if ($Kind -in @('regex-native','allocated-native')) {
            $files=@(Read-FeatureCsv $Path)
            $expected=if($Kind -eq 'regex-native'){$Fixture.extensions.bin}else{$Fixture.files}
            if($files.Count -ne $expected -or @($files.Path | Sort-Object -Unique).Count -ne $expected){throw 'Search lost or duplicated matches'}
            if($Kind -eq 'regex-native') {
                $names=@($files | ForEach-Object {[IO.Path]::GetFileName($_.Path)})
                if(@(Compare-Object ($Fixture.duplicate_names | Sort-Object) ($names | Sort-Object)).Count){throw 'Regex did not select exactly the known .bin files'}
            } elseif(@($files | Where-Object {[uint64]$_.AllocatedBytes -lt 1}).Count){throw 'Allocated bound included zero-allocation files'}
        }
        if ($Kind -eq 'storage-cost-native') {
            $estimate=Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
            if(@($estimate.files).Count -ne 3 -or ($estimate.files | Measure-Object -Sum).Sum -ne $Fixture.files -or
                $estimate.files[2] -ne $Fixture.files -or $estimate.unknown_dates -ne 0){throw 'Storage tier counts differ from known modified-date fixture'}
            $allocated=($estimate.allocated_bytes | Measure-Object -Sum).Sum
            if([Math]::Abs($estimate.all_hot_cost-$allocated/1073741824) -gt 1e-10 -or
                [Math]::Abs($estimate.monthly_cost-$allocated/1073741824*0.25) -gt 1e-10){throw 'Storage cost differs from explicit allocation/prices'}
        }
        if ($Kind -eq 'filtered-native') {
            $files = @(Read-FeatureCsv $Path)
            if ($files.Count -ne $Fixture.extensions.bin -or @($files | Where-Object { $_.Kind -ne 'File' -or $_.Path -notlike '*.bin' }).Count) { throw 'Native filtered export did not select exactly the .bin files.' }
            $names = @($files | ForEach-Object { [IO.Path]::GetFileName($_.Path) })
            if (@(Compare-Object ($Fixture.duplicate_names | Sort-Object) ($names | Sort-Object)).Count -or ($files | Measure-Object LogicalBytes -Sum).Sum -ne $Fixture.extensions.bin * 1048576) { throw 'Native filtered export lost or duplicated an expected file.' }

        }
        if ($Kind -eq 'advanced-filter-native') {
            $files=@(Read-FeatureCsv $Path)
            if ($files.Count -ne 160 -or @($files | Where-Object { $_.Kind -ne 'File' -or ($_.Path -notlike '*.bin' -and $_.Path -notlike '*unique-???.txt') -or [uint64]$_.LogicalBytes -ne 1048576 -or [int64]$_.ModifiedUnixSeconds -lt 1767225600 -or [int64]$_.ModifiedUnixSeconds -gt 4102444799 }).Count) {throw 'Advanced filter did not combine pattern, size and date predicates'}
            if (@($files.Path | Sort-Object -Unique).Count -ne 160) {throw 'Advanced filter duplicated rows'}
        }
        if ($Kind -eq 'largest-native') {
            $files=@(Read-FeatureCsv $Path)
            if ($files.Count -ne $Fixture.files -or @($files.Path | Sort-Object -Unique).Count -ne $Fixture.files -or ($files | Measure-Object LogicalBytes -Sum).Sum -ne $Fixture.logical_bytes) {throw 'Largest-file paging lost, duplicated or altered files'}
            for ($i=1;$i -lt $files.Count;$i++) {if ([uint64]$files[$i].AllocatedBytes -gt [uint64]$files[$i-1].AllocatedBytes) {throw 'Largest files not sorted by allocation'}}
        }
        return @{files=$stats.files;load_ms=$stats.load_ms;operation_ms=$stats.operation_ms;verified=$true}
    }
    if($Kind -eq 'multiple-roots-native') {
        $stats=Get-Content -LiteralPath $Stdout -Raw | ConvertFrom-Json
        $rows=@(Read-FeatureCsv $Path)
        $files=@($rows | Where-Object Kind -eq File)
        $roots=@($rows | Where-Object {$_.Kind -eq 'Directory' -and ($_.Path.EndsWith('\left') -or $_.Path.EndsWith('\right'))})
        if($stats.files -ne 64 -or $stats.logical_bytes -ne 67108864 -or $files.Count -ne 64 -or
            ($files | Measure-Object LogicalBytes -Sum).Sum -ne 67108864 -or @($files.Path | Sort-Object -Unique).Count -ne 64 -or
            $roots.Count -ne 2 -or @($files | Where-Object {$_.Path -notmatch '\\(left|right)\\decoy-\d{3}\.dat$'}).Count){
            throw 'Combined-root scan lost original paths or changed the known 64-file subset'
        }
        return @{verified=$true;files=64;roots=2;note='Native preparation includes both scans; not presented as a cache-decode phase.'}
    }
    if($Kind -in @('hardlink-native','compression-native','sparsify-native')) {
        $checked=Get-Content -LiteralPath $Stdout -Raw | ConvertFrom-Json
        $expected=@{'hardlink-native'='hardlinks';'compression-native'='compression';'sparsify-native'='sparsify'}[$Kind]
        if($checked.action -ne $expected -or !$checked.verified -or !$checked.bytes_preserved -or $checked.logical_bytes -ne 2097152){
            throw 'Owned maintenance fixture did not verify its selected operation and bytes'
        }
        if($Kind -eq 'hardlink-native') {
            if(!$checked.shared_identity){throw 'Hard-link consolidation did not produce one shared physical identity'}
        } elseif($checked.after_allocated -ge $checked.before_allocated) {throw 'Maintenance did not reduce allocation in the compressible/sparse fixture'}
        if($Kind -eq 'compression-native' -and !$checked.reversible){throw 'Compression did not verify decompression'}
        return @{verified=$true;logical_bytes=$checked.logical_bytes;note='Native action plus owned-fixture creation, verification and cleanup; GUI excluded.'}
    }
    if($Kind -eq 'vhd-native') {
        $message=Get-Content -LiteralPath $Stdout -Raw
        if([string]::IsNullOrWhiteSpace($message)){throw 'Native operation adapter produced no completion proof.'}
        if(!$message.Contains('PASS: real empty dynamic VHDX compacts without mounting or growing') -or
            !$message.Contains('PASS: invalid virtual-disk content is refused without changing the owned file')){
            throw 'VHD adapter did not positively verify owned empty-disk compaction and invalid-disk refusal'
        }
        return @{verified=$true;note='Owned empty dynamic VHDX only; populated disks and GUI interaction excluded.'}
    }
    if($Kind -in @('flame-layout-native','sunburst-layout-native','permission-controls-native')) {
        $checked=Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
        $expected=@{'flame-layout-native'='flame';'sunburst-layout-native'='sunburst';'permission-controls-native'='permissions'}[$Kind]
        $records=if($Kind -eq 'permission-controls-native'){409600}else{409700}
        if(!$checked.verified -or $checked.operation -ne $expected -or $checked.repeats -ne 100 -or $checked.records_checked -ne $records){throw 'Parity view fixture did not validate its operation and counts'}
        return @{verified=$true;records_checked=$records;note='Generated view-model geometry/control workload; GPU/UI and disk reads excluded.'}
    }
    if($Kind -eq 'detailed-csv-native') {
        $rows=Read-FeatureCsv $Path
        $files=@($rows | Where-Object Kind -eq 'File')
        if($files.Count -ne $Fixture.files -or $rows[0].Files -ne $Fixture.files -or $rows[0].LogicalBytes -ne $Fixture.logical_bytes){throw 'Detailed CSV totals differ from fixture'}
        if(@($files | Where-Object {!$_.Owner -or !$_.FileIdentity -or !$_.VolumeIdentity -or !$_.CreatedUnixSeconds -or !$_.Attributes}).Count){throw 'Detailed CSV lacks actual owner, timestamps, attributes or physical identities'}
        return @{verified=$true;files=$files.Count;columns=14;note='Current owner/physical identities checked on owned filesystem fixture.'}
    }
    if ($Kind -in @('actions-native','watch-native')) {
        $message=Get-Content -LiteralPath $Stdout -Raw
        $expected=if ($Kind -eq 'actions-native') {'PASS: move without overwrite, file-only recycle, source removed after success'} else {'PASS: add/modify/remove/rename history with observation times, ignored internal writes'}
        if (!$message.Contains($expected)) {throw 'Native operation adapter did not verify its actions.'}
        return @{verified=$true;note='Real native component operations on isolated generated files; GUI interaction excluded.'}
    }
    if ($Kind -eq 'duplicates-native' -or $Kind -like 'duplicates-*-native') {
        $rows=Read-FeatureCsv $Path
        $names=@($rows | ForEach-Object {[IO.Path]::GetFileName($_.Path)})
        $groups=@($rows | Group-Object Group)
        if ($rows.Count -ne $Fixture.duplicate_files -or @(Compare-Object ($Fixture.duplicate_names | Sort-Object) ($names | Sort-Object)).Count -or $groups.Count -ne $Fixture.duplicate_groups -or @($groups | Where-Object Count -ne 4).Count -or @($rows | Where-Object {$_.Status -ne 'verified' -or [uint64]$_.LogicalBytes -ne 1048576}).Count) {throw 'Verified duplicate report differs from byte-content ground truth.'}
        if($Kind -ne 'duplicates-native') {
            $algorithm=$Kind.Split('-')[1]
            $length=@{sha256=64;md5=32;sha1=40;sha512=128}[$algorithm]
            if(!$length -or @($rows | Where-Object {$_.Digest -notmatch ('^[a-f0-9]{'+$length+'}$')}).Count){
                throw 'Selected content-hash algorithm did not emit its expected digest representation'
            }
            foreach($group in $groups){if(@($group.Group.Digest | Sort-Object -Unique).Count -ne 1){throw 'Verified group contains inconsistent digests'}}
            $stats=Get-Content -LiteralPath $Stdout -Raw | ConvertFrom-Json
            if($stats.files -ne $Fixture.files -or $stats.logical_bytes -ne $Fixture.logical_bytes){throw 'Cached duplicate catalog differs from fixture'}
            return @{verified=$true;files=$rows.Count;groups=$groups.Count;algorithm=$algorithm;load_ms=$stats.load_ms;operation_ms=$stats.operation_ms}
        }
        return @{verified=$true;duplicate_groups=$groups.Count;duplicate_files=$rows.Count;metadata_false_positives=0;fixture_file_bytes=1048576}
    }
    if ($Kind -eq 'permissions-native') {
        $rows=Read-FeatureCsv $Path
        if ($rows.Count -ne $Fixture.files+3 -or @($rows.Path | Sort-Object -Unique).Count -ne $Fixture.files+3) {throw 'ACL report did not cover every fixture file and directory.'}
        foreach ($row in $rows) {
            if ($row.Status -ne 'ok' -or $row.SDDL -notmatch 'O:' -or $row.SDDL -notmatch 'G:' -or $row.SDDL -notmatch 'D:') {throw 'Missing owner, group or discretionary ACL.'}
            $descriptor=[Security.AccessControl.RawSecurityDescriptor]::new($row.SDDL)
            if (!$descriptor.Owner -or !$descriptor.Group -or !$descriptor.DiscretionaryAcl) {throw 'Invalid permission descriptor.'}
        }
        return @{verified=$true;permission_rows=$rows.Count;note='Owner/group/DACL SDDL parses for every fixture entry; SACL auditing excluded.'}
    }
    if (!(Test-Path -LiteralPath $Path) -or (Get-Item -LiteralPath $Path).Length -eq 0) { throw 'Output is missing or empty.' }
    switch ($Kind) {
        'csv-wb' {
            $rows = Read-FeatureCsv $Path
            if ($rows[0].Files -ne $Fixture.files -or $rows[0].LogicalBytes -ne $Fixture.logical_bytes -or @($rows | Where-Object Kind -eq File).Count -ne $Fixture.files) { throw 'WinBlaze scan CSV differs from fixture.' }
            return @{verified=$true;files=$Fixture.files}
        }
        'csv-wd' {
            $rows = Read-FeatureCsv $Path
            if ($rows[0].Files -ne $Fixture.files -or $rows[0].'Logical Size' -ne $Fixture.logical_bytes) { throw 'WinDirStat scan CSV differs from fixture.' }
            return @{verified=$true;files=$Fixture.files}
        }
        'duplicates' {
            $rows = Read-FeatureCsv $Path
            $names = @($rows | ForEach-Object { [IO.Path]::GetFileName($_.Name) })
            if ($rows.Count -ne $Fixture.duplicate_files -or @(Compare-Object ($Fixture.duplicate_names | Sort-Object) ($names | Sort-Object)).Count) { throw 'Duplicate report missed true copies or included unique/metadata-decoy files.' }
            $groups = @($rows | Group-Object 'Hash Prefix')
            if ($groups.Count -ne $Fixture.duplicate_groups -or @($groups | Where-Object Count -ne 4).Count) { throw 'Duplicate groups do not match ground truth.' }
            if (@($rows | Where-Object { $_.'Sampled hash' -eq 'true' }).Count) { throw 'Sampled hashes cannot establish content equality.' }
            return @{verified=$true;duplicate_groups=$groups.Count;duplicate_files=$rows.Count;metadata_false_positives=0;fixture_file_bytes=1048576}
        }
        'json' {
            $decoded = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
            $rows = @($decoded)
            if ($rows.Count -lt $Fixture.files -or $rows[0].Files -ne $Fixture.files -or $rows[0].'Logical Size' -ne $Fixture.logical_bytes) { throw 'JSON scan totals differ from fixture.' }
            return @{verified=$true;files=$rows[0].Files;records=$rows.Count}
        }
        'permissions' {
            $rows = Read-FeatureCsv $Path
            if (!$rows.Count -or !($rows[0].PSObject.Properties.Name -contains 'Account') -or !($rows[0].PSObject.Properties.Name -contains 'Rights')) { throw 'Permission report has no validated ACL schema.' }
            return @{verified=$true;permission_rows=$rows.Count;note='Report schema/nonempty entries checked; not a complete ACL correctness proof.'}
        }
        'png' {
            $bytes = [IO.File]::ReadAllBytes($Path)
            if ($bytes.Length -lt 24 -or [BitConverter]::ToString($bytes,0,8) -ne '89-50-4E-47-0D-0A-1A-0A' -or [Text.Encoding]::ASCII.GetString($bytes,12,4) -ne 'IHDR') { throw 'Treemap export is not a PNG.' }
            $width = $bytes[16]*16777216+$bytes[17]*65536+$bytes[18]*256+$bytes[19]
            $height = $bytes[20]*16777216+$bytes[21]*65536+$bytes[22]*256+$bytes[23]
            if ($width -ne 1920 -or $height -ne 1080) { throw 'Treemap PNG dimensions differ from requested workload.' }
            Add-Type -AssemblyName System.Drawing
            $image=[Drawing.Bitmap]::new($Path)
            try {
                $colors=[Collections.Generic.HashSet[int]]::new()
                for ($x=48;$x -lt $width;$x+=96) {for ($y=27;$y -lt $height;$y+=54) {$null=$colors.Add($image.GetPixel($x,$y).ToArgb())}}
                if ($colors.Count -lt 2) {throw 'Treemap PNG is blank or uniform'}
            } finally {$image.Dispose()}
            return @{verified=$true;width=$width;height=$height;sampled_colors=$colors.Count;note='Decoded PNG dimensions and nonblank pixel samples checked; geometry correctness covered separately.'}
        }
        'mft' {
            $stream = [IO.File]::OpenRead($Path)
            try { $header = New-Object byte[] 4096; $count = $stream.Read($header,0,$header.Length) }
            finally { $stream.Dispose() }
            if ((Get-Item -LiteralPath $Path).Length -lt 1024 -or $count -lt 4 -or ![Text.Encoding]::ASCII.GetString($header).Contains('FILE')) { throw 'MFT output lacks an NTFS FILE record signature.' }
            return @{verified=$true;note='NTFS signature/nonempty dump checked, not record-by-record completeness.'}
        }
        default {
            $rows = Read-FeatureCsv $Path -WizTree
            if ($Kind -eq 'filtered') {
                if ($rows.Count -ne $Fixture.extensions.bin -or @($rows | Where-Object { $_.'File Name' -notlike '*.bin' }).Count) { throw 'Filtered export did not select exactly the expected .bin files.' }
                return @{verified=$true;files=$rows.Count;filter='*.bin'}
            }
            if ($Kind -eq 'extensions-wiz') {
                if ($rows.Count -ne 3 -or ($rows | Measure-Object Files -Sum).Sum -ne $Fixture.files) { throw 'File-type aggregation differs from fixture.' }
                return @{verified=$true;extensions=$rows.Count;files=$Fixture.files}
            }
            if (!$rows.Count -or $rows[0].Files -ne $Fixture.files -or $rows[0].Size -ne $Fixture.logical_bytes) { throw 'WizTree scan/import totals differ from fixture.' }
            return @{verified=$true;files=$rows[0].Files}
        }
    }
}
