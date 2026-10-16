# Dot-source: unattended scan + export adapters for the products added beside
# WinBlaze, WinDirStat and WizTree (see products.mjs). Each plan names the
# measured process, an optional companion process started right after it (a
# client that drives the export and then closes the measured process), the
# export files, and how to read the scanned root's totals for validation.
#   SpaceSniffer: the timed run exports "Simple file list" (one line per file;
#     its line count is the exact file count). SpaceSniffer applies one export
#     per scan, so folder count and a rounded on-disk size come from one untimed
#     "Summary" scan per target, cached for the remaining runs.
#   Everything: a fresh folder index of the target (standard users cannot
#     index NTFS volumes); es.exe exports every indexed item, then exits it.
#   Directory Report: settings file with AutoPrint (requires installation).
#   TreeSize Free: no command line export; startup + scan until its status bar
#     shows "N Files", then gui-ready-closer closes it (validated on file count).
$script:BenchToolRoot = Join-Path $env:LOCALAPPDATA 'WinBlazeBench'
$script:ScanAdapterTools = @('SpaceSniffer', 'Everything', 'Directory Report', 'TreeSize Free')

function Get-BenchToolExecutable([string]$Tool) {
    $override = @{ 'TreeSize Free' = $env:WINBLAZE_BENCH_TREESIZE; 'SpaceSniffer' = $env:WINBLAZE_BENCH_SPACESNIFFER;
        'Everything' = $env:WINBLAZE_BENCH_EVERYTHING; 'Directory Report' = $env:WINBLAZE_BENCH_DIRREPORT }[$Tool]
    if ($override) { return $override }
    switch ($Tool) {
        'TreeSize Free' { return Join-Path $script:BenchToolRoot 'TreeSizeFree\TreeSizeFree.exe' }
        'SpaceSniffer' { return Join-Path $script:BenchToolRoot 'SpaceSniffer\SpaceSniffer.exe' }
        'Everything' { return Join-Path $script:BenchToolRoot 'Everything\Everything.exe' }
        # Administrative MSI extract (no elevation); dr64 is the 64-bit build.
        'Directory Report' { return Join-Path $script:BenchToolRoot 'DirectoryReport\program files\AMB Software\Directory Report\dr64\dr64.exe' }
    }
    throw "Unknown benchmark tool '$Tool'"
}

function Test-ScanAdapterTool([string]$Tool) { return $Tool -in $script:ScanAdapterTools }

# Light native companions, compiled on first use from benchmarks/<name>.cs:
#   dismiss-dialog: clicks through per-launch notices.
#   gui-ready-closer: waits for a GUI-only product's ready status, records it,
#     then closes the window so the measured process exits.
function Get-BenchCompanion([string]$Name) {
    $exe = Join-Path $script:BenchToolRoot "bin\$Name.exe"
    $source = Join-Path $PSScriptRoot "$Name.cs"
    if (!(Test-Path -LiteralPath $exe) -or (Get-Item -LiteralPath $source).LastWriteTimeUtc -gt (Get-Item -LiteralPath $exe).LastWriteTimeUtc) {
        New-Item -ItemType Directory -Path (Split-Path $exe) -Force | Out-Null
        & "$env:WINDIR\Microsoft.NET\Framework64\v4.0.30319\csc.exe" /nologo /optimize /target:winexe "/out:$exe" $source | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Could not compile $Name.exe" }
    }
    return $exe
}
function Get-DialogDismisser { return Get-BenchCompanion 'dismiss-dialog' }

# Quote a path for a command line; a bare drive root avoids the quoted
# trailing-backslash pitfall.
function ConvertTo-ScanArgument([string]$Path) {
    $bare = $Path.TrimEnd('\')
    if ($bare -match '^[A-Za-z]:$') { return "$bare\" }
    return '"' + $bare + '"'
}

function New-ScanPlan([Parameter(Mandatory)][string]$Tool, [Parameter(Mandatory)][string]$Target, [Parameter(Mandatory)][string]$Export) {
    $exe = Get-BenchToolExecutable $Tool
    $targetArgument = ConvertTo-ScanArgument $Target
    switch ($Tool) {
        'SpaceSniffer' {
            Remove-Item -LiteralPath $Export -ErrorAction SilentlyContinue
            return [pscustomobject]@{ Tool = $Tool; Executable = $exe; Target = $Target; Export = $Export; Outputs = @($Export)
                Arguments = "scan $targetArgument export `"Simple file list`" `"$Export`" autoclose"
                CompanionExecutable = $null; CompanionArguments = $null }
        }
        'Everything' {
            $work = "$Export.everything"
            if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
            New-Item -ItemType Directory -Path $work | Out-Null
            Remove-Item -LiteralPath $Export -ErrorAction SilentlyContinue
            $instance = 'WBB' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
            # Unquoted: Everything keeps a quoted value's backslash escapes verbatim.
            $folder = $Target.TrimEnd('\') + $(if ($Target.TrimEnd('\') -match '^[A-Za-z]:$') { '\' } else { '' })
            if ($folder -match '[,"]') { throw "Everything folder paths cannot contain commas or quotes: $folder" }
            @(
                '[Everything]', 'app_data=0', 'run_as_admin=0', 'allow_multiple_instances=1',
                'auto_include_fixed_volumes=0', 'auto_include_removable_volumes=0',
                'auto_include_fixed_refs_volumes=0', 'auto_include_removable_refs_volumes=0',
                "folders=$folder", 'folder_monitor_changes=0', 'folder_update_rescan_asap=1',
                'index_size=1', 'index_attributes=1', 'index_folder_size=1',
                'show_tray_icon=0', 'check_for_updates_on_startup=0'
            ) | Set-Content -LiteralPath (Join-Path $work 'cfg.ini') -Encoding ASCII
            $es = Join-Path (Split-Path $exe) 'es.exe'
            $client = "`"`"$es`" -instance $instance -timeout 3600000 -size -attributes -export-csv `"$Export`" & `"$es`" -instance $instance -exit`""
            return [pscustomobject]@{ Tool = $Tool; Executable = $exe; Target = $Target; Export = $Export; Outputs = @($Export)
                Arguments = "-instance $instance -config `"$work\cfg.ini`" -db `"$work\index.db`" -startup"
                CompanionExecutable = "$env:SystemRoot\System32\cmd.exe"; CompanionArguments = "/d /c $client" }
        }
        'TreeSize Free' {
            # No command line export: the boundary is startup + scan until the
            # status bar shows "N Files", then the companion closes the window
            # (the process exit ends the timing). Validated on file count.
            Remove-Item -LiteralPath $Export -ErrorAction SilentlyContinue
            return [pscustomobject]@{ Tool = $Tool; Executable = $exe; Target = $Target; Export = $Export; Outputs = @($Export)
                Arguments = $targetArgument
                CompanionExecutable = Get-BenchCompanion 'gui-ready-closer'
                CompanionArguments = "3600 TreeSizeFree TMainForm `"^\s*[\d,.]+ Files`" `"$Export`"" }
        }
        'Directory Report' {
            # Batch mode: a saved settings file (UTF-16, as the program writes
            # it) whose Auto Print section writes the main report as CSV and
            # exits. directory-report-settings.txt keeps names, sizes, dates and
            # attributes; per-file hashes, owners, link counts and media/document
            # metadata are off (each reads file contents or opens every file).
            Remove-Item -LiteralPath $Export -ErrorAction SilentlyContinue
            $settings = "$Export.settings.txt"
            $template = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'directory-report-settings.txt') -Encoding UTF8
            $template.Replace('{{TARGET}}', $Target.TrimEnd('\') + $(if ($Target.TrimEnd('\') -match '^[A-Za-z]:$') { '\' } else { '' })).Replace('{{REPORT}}', $Export) |
                Set-Content -LiteralPath $settings -Encoding Unicode
            # Every launch shows the trial notice; the companion clicks OK.
            return [pscustomobject]@{ Tool = $Tool; Executable = $exe; Target = $Target; Export = $Export; Outputs = @($Export, $settings)
                Arguments = "`"$settings`""
                CompanionExecutable = Get-DialogDismisser; CompanionArguments = "3600 `"Directory Report Licensing|OK`"" }
        }
    }
    throw "No scan adapter for '$Tool'"
}

# Bytes from SpaceSniffer's rounded size text such as "288KB" or "1.5GB".
function ConvertFrom-SpaceSnifferSize([string]$Text) {
    if ($Text -notmatch '([\d.,]+)\s*([KMGT]?B)') { return $null }
    $value = [double]::Parse(($Matches[1] -replace ',', ''), [Globalization.CultureInfo]::InvariantCulture)
    $scale = @{ B = 1; KB = 1KB; MB = 1MB; GB = 1GB; TB = 1TB }[$Matches[2]]
    return [long]($value * $scale)
}

$script:SpaceSnifferSummaries = @{}
# One untimed Summary scan per target: folder count and rounded on-disk size.
function Get-SpaceSnifferSummary([string]$Executable, [string]$Target) {
    if ($script:SpaceSnifferSummaries.ContainsKey($Target)) { return $script:SpaceSnifferSummaries[$Target] }
    $out = Join-Path $env:TEMP ('winblaze-ss-summary-' + [Guid]::NewGuid().ToString('N') + '.txt')
    $process = Start-Process -FilePath $Executable -ArgumentList "scan $(ConvertTo-ScanArgument $Target) export `"Summary`" `"$out`" autoclose" -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(3600000)) { Stop-Process -Id $process.Id -Force; throw 'SpaceSniffer summary scan timed out' }
    try {
        $text = Get-Content -LiteralPath $out
        $field = { param($name) ($text | Where-Object { $_ -match "^$name\s*:" } | Select-Object -First 1) -replace '^[^:]*:\s*', '' }
        $summary = [pscustomobject]@{ files = [long](& $field 'Files'); folders = [long](& $field 'Folders'); logical = ConvertFrom-SpaceSnifferSize (& $field 'Total size') }
    } finally { Remove-Item -LiteralPath $out -ErrorAction SilentlyContinue }
    $script:SpaceSnifferSummaries[$Target] = $summary
    return $summary
}

# Root totals in the runners' common shape. size_precision 'files-only' marks
# totals without logical bytes; validation then checks the file count only.
function Read-ScanTotals([Parameter(Mandatory)]$Plan) {
    foreach ($output in $Plan.Outputs) { if (!(Test-Path -LiteralPath $output)) { throw "$($Plan.Tool) did not write $output" } }
    switch ($Plan.Tool) {
        'SpaceSniffer' {
            $files = 0L
            $reader = [IO.StreamReader]::new($Plan.Export)
            try { while ($null -ne ($line = $reader.ReadLine())) { if ($line) { $files++ } } } finally { $reader.Dispose() }
            $summary = Get-SpaceSnifferSummary $Plan.Executable $Plan.Target
            # SpaceSniffer's summary total is the on-disk (allocated) size.
            return [pscustomobject]@{ files = $files; folders = $summary.folders; logical = $null
                allocated = $summary.logical; size_precision = 'files-only'; summary_files = $summary.files }
        }
        'Everything' {
            $files = 0L; $folders = 0L; $logical = 0L
            $root = $Plan.Target.TrimEnd('\') + '\'
            $reader = [IO.StreamReader]::new($Plan.Export)
            try {
                $null = $reader.ReadLine()
                while ($null -ne ($line = $reader.ReadLine())) {
                    # "path",size,attributes - the path is the only quoted field.
                    $end = $line.LastIndexOf('",')
                    if ($end -lt 1) { continue }
                    $path = $line.Substring(1, $end - 1)
                    $parts = $line.Substring($end + 2).Split(',')
                    $attributes = [long]$parts[1]
                    if ($attributes -band 0x10) { if ($path -ne $root) { $folders++ } }
                    else { $files++; if ($parts[0]) { $logical += [long]$parts[0] } }
                }
            } finally { $reader.Dispose() }
            return [pscustomobject]@{ files = $files; folders = $folders; logical = $logical; allocated = $null; size_precision = 'exact' }
        }
        'TreeSize Free' {
            # Line 1: window caption; line 2: status-bar parts ("... | N Files | ...").
            $status = @(Get-Content -LiteralPath $Plan.Export -Encoding UTF8)[1]
            if ($status -notmatch '(?:^|\|)\s*([\d,.]+) Files') { throw "TreeSize Free status has no file count: $status" }
            return [pscustomobject]@{ files = [long]($Matches[1] -replace '[,.]', ''); folders = $null; logical = $null; allocated = $null; size_precision = 'files-only' }
        }
        'Directory Report' {
            # The root's summary block follows its quoted path line:
            #   "N files in all subdirectories"," M directories in all subdirectories"
            #   "B bytes including all subdirectories"
            #   "A actual bytes including all subdirectories"
            $root = '"' + $Plan.Target.TrimEnd('\') + $(if ($Plan.Target.TrimEnd('\') -match '^[A-Za-z]:$') { '\' } else { '' }) + '"'
            $number = { param($line, $label) if ($line -match "`"([\d,]+) $label") { [long]($Matches[1] -replace ',', '') } }
            $totals = [ordered]@{}
            $reader = [IO.StreamReader]::new($Plan.Export)
            try {
                while ($null -ne ($line = $reader.ReadLine())) { if ($line -eq $root) { break } }
                for ($i = 0; $i -lt 8 -and $null -ne ($line = $reader.ReadLine()); $i++) {
                    if ($line -match 'files in all subdirectories') {
                        $totals.files = & $number $line 'files in all'; $totals.folders = if ($line -match '([\d,]+) directories in all') { [long]($Matches[1] -replace ',', '') } }
                    elseif ($line -match '^"[\d,]+ actual bytes including') { $totals.allocated = & $number $line 'actual bytes' }
                    elseif ($line -match '^"[\d,]+ bytes including') { $totals.logical = & $number $line 'bytes including' }
                }
            } finally { $reader.Dispose() }
            if ($null -eq $totals.files -or $null -eq $totals.logical) { throw "Directory Report summary for $root not found" }
            return [pscustomobject]@{ files = $totals.files; folders = $totals.folders; logical = $totals.logical; allocated = $totals.allocated; size_precision = 'exact' }
        }
    }
    throw "No totals reader for '$($Plan.Tool)'"
}

# Exported files can hold personal paths (full drive); keep aggregates only.
function Remove-ScanOutputs([Parameter(Mandatory)]$Plan) {
    foreach ($output in $Plan.Outputs) { Remove-Item -LiteralPath $output -ErrorAction SilentlyContinue }
    $work = "$($Plan.Export).everything"
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
}
