$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$folder=Join-Path $PSScriptRoot ('../artifacts/atomic-result-test-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $folder | Out-Null
$path=Join-Path $folder 'results.json'
try {
    Write-BenchmarkJson -Path $path -Value @{count=1}
    (Get-Item -LiteralPath $path).IsReadOnly=$true
    $failed=$false
    try {Write-BenchmarkJson -Path $path -Value @{count=2}} catch {$failed=$true}
    if(!$failed -or (Read-BenchmarkJson -Path $path).count -ne 1){throw 'A failed update did not preserve the prior JSON'}
    (Get-Item -LiteralPath $path).IsReadOnly=$false
    Write-BenchmarkJson -Path $path -Value @{count=3}
    if((Read-BenchmarkJson -Path $path).count -ne 3){throw 'Successful update not published'}
    if(@(Get-ChildItem -LiteralPath $folder -Filter '*.tmp').Count){throw 'Temporary result files leaked'}
    'PASS: atomic result updates preserve previous JSON on real write failure'
} finally {
    if(Test-Path -LiteralPath $path){(Get-Item -LiteralPath $path).IsReadOnly=$false}
    $resolved=[IO.Path]::GetFullPath($folder)
    $root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../artifacts'))+[IO.Path]::DirectorySeparatorChar
    if(!$resolved.StartsWith($root,[StringComparison]::OrdinalIgnoreCase)){throw 'Test directory outside artifacts'}
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
