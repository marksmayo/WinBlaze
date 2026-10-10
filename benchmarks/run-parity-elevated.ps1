param([ValidateRange(1,100)][int]$Runs=10)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$administrator=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (!$administrator) {throw 'This helper requires the administrator token requested by the Windows prompt.'}
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location -LiteralPath $repo
Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-parity-elevated-status.json') -Value @{state='running';requested_runs=$Runs;utc=[DateTime]::UtcNow.ToString('o');detail='Administrator prompt accepted; elevated n=10 measurements running.'}
try {
    & (Join-Path $PSScriptRoot 'run-elevated-winblaze-refresh.ps1') -Runs $Runs
    if ($LASTEXITCODE -ne 0) {throw 'Elevated WinBlaze pipeline refresh failed.'}
    & (Join-Path $PSScriptRoot 'run-feature-comparison.ps1') -OnlyMft -Runs $Runs -KeepMftSample
    $rows=@(Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-feature-mft-results.json'))
    foreach ($job in @('wb-mft','wiz-mft')) {
        $valid=@($rows | Where-Object {$_.job -eq $job -and $_.status -eq 'measured' -and $_.validation.verified})
        if ($valid.Count -ne $Runs) {throw "Incomplete validated MFT job: $job"}
    }
    $manifest=Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-feature-mft-batch.json')
    $dump=Join-Path $repo ("artifacts/feature-benchmarks/$($manifest.batch_id)/wb-mft-1.mft")
    & (Join-Path $PSScriptRoot 'run-mft-reopen-validation.ps1') -DumpPath $dump -Runs $Runs
    if($LASTEXITCODE -ne 0){throw 'Positive MFT dump reopen validation failed'}
    $features=Read-BenchmarkJson -Path (Join-Path $PSScriptRoot 'feature-comparison.json')
    $raw=$features.rows | Where-Object feature -eq 'Raw MFT dump export'
    $raw.note='UI and shipped CLI stream initialized MFT data including unused records. Live administrator n=10 WinBlaze and WizTree dumps passed NTFS signature/output checks; record-by-record completeness is not established.'
    $raw.performance.coverage=$raw.note
    Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'feature-comparison.json') -Value $features
    & node (Join-Path $PSScriptRoot 'render-report.mjs')
    if ($LASTEXITCODE -ne 0) {throw 'Final elevated report rendering failed.'}
    Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-parity-elevated-status.json') -Value @{state='complete';requested_runs=$Runs;utc=[DateTime]::UtcNow.ToString('o');detail='Current WinBlaze native/MFT pipeline and both raw MFT exports validated.'}
} catch {
    Write-BenchmarkJson -Path (Join-Path $PSScriptRoot 'current-parity-elevated-status.json') -Value @{state='failed';requested_runs=$Runs;utc=[DateTime]::UtcNow.ToString('o');detail=$_.Exception.Message}
    exit 1
}
