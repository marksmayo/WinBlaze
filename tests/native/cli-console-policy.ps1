#requires -Version 7.0
param(
    [string]$AppPath='target/release/winblaze-cli.exe',
    [string]$OutputDirectory=('artifacts/cli-console-policy-'+[Guid]::NewGuid().ToString('N'))
)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Set-Location -LiteralPath $repo
$executable=(Resolve-Path -LiteralPath $AppPath).Path
$output=[IO.Path]::GetFullPath($OutputDirectory,$repo)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$bytes=[IO.File]::ReadAllBytes($executable)
$pe=[BitConverter]::ToInt32($bytes,0x3c)
$subsystem=[BitConverter]::ToUInt16($bytes,$pe+24+68)
if($subsystem -ne 3){throw 'The existing CLI must retain the console PE subsystem and shell waiting semantics'}
$manifestTool=Get-ChildItem -Path 'C:/Program Files (x86)/Windows Kits/10/bin/*/x64/mt.exe' | Sort-Object FullName -Descending | Select-Object -First 1
$manifest=Join-Path $output 'embedded.manifest'
& $manifestTool.FullName '-nologo' ('-inputresource:'+$executable+';#1') ('-out:'+$manifest) *> (Join-Path $output 'manifest-extraction.log')
if($LASTEXITCODE -or !(Test-Path -LiteralPath $manifest)){throw 'CLI lacks an embedded detached-console allocation policy'}
[xml]$xml=Get-Content -LiteralPath $manifest -Raw
$policy=$xml.SelectSingleNode("//*[local-name()='consoleAllocationPolicy' and namespace-uri()='http://schemas.microsoft.com/SMI/2024/WindowsSettings']")
if(!$policy -or $policy.InnerText -ne 'detached'){throw 'CLI must request the supported detached console allocation policy'}

$fixture=Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
foreach($index in 1..3){[IO.File]::WriteAllBytes((Join-Path $fixture ('file-'+$index)),[byte[]](1..$index))}
$rows=[Collections.Generic.List[object]]::new()
foreach($scenario in @('valid','usage-error')) {
    $start=[Diagnostics.ProcessStartInfo]::new('cmd.exe')
    $start.Arguments='/d /q'
    $start.UseShellExecute=$false
    $start.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardInput=$true
    $start.RedirectStandardOutput=$true
    $start.RedirectStandardError=$true
    $process=[Diagnostics.Process]::new()
    $process.StartInfo=$start
    if(!$process.Start()){throw 'Owned terminal launch failed'}
    $ownedHandle=$process.Handle
    $command='"'+$executable+'"'
    $csv=Join-Path $output 'terminal.csv'
    if($scenario -eq 'valid'){$command+=' "'+$fixture+'" "'+$csv+'"'}
    $process.StandardInput.WriteLine($command)
    $process.StandardInput.WriteLine('echo AFTER_CLI ERRORLEVEL=%ERRORLEVEL%')
    $process.StandardInput.WriteLine('exit')
    $process.StandardInput.Close()
    if(!$process.WaitForExit(15000)){$process.Kill();throw 'Owned terminal timed out'}
    $stdout=$process.StandardOutput.ReadToEnd()
    $stderr=$process.StandardError.ReadToEnd()
    $status=$process.ExitCode
    $rows.Add([pscustomobject]@{scenario=$scenario;stdout=$stdout;stderr=$stderr;exit_code=$status})
    $process.Dispose()
    if($scenario -eq 'valid') {
        $completion=$stdout.IndexOf('{"scan_export_ms":')
        $marker=$stdout.IndexOf('AFTER_CLI ERRORLEVEL=0')
        if($status -ne 0 -or $completion -lt 0 -or $marker -lt $completion){throw 'Terminal must wait for completion and retain stdout/status0'}
        $export=Get-Content -LiteralPath $csv -TotalCount 2 | ConvertFrom-Csv
        if($export.Files -ne 3 -or $export.LogicalBytes -ne 6){throw 'Terminal export lost fixture records/bytes'}
    } elseif($status -ne 2 -or !$stdout.Contains('AFTER_CLI ERRORLEVEL=2') -or !$stderr.Contains('usage: winblaze-cli')) {
        throw 'Terminal must preserve usage stderr and exit status2'
    }
}
$rows.ToArray() | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'terminal-results.json')
Write-Output ('PASS: embedded detached policy retains console subsystem, synchronous CMD/stdout, full export and usage stderr/status2; evidence='+$output)
