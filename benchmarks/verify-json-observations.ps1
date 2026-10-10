$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'track-tool-versions.ps1')
$path = Join-Path $PSScriptRoot ('../artifacts/json-array-test-' + [Guid]::NewGuid().ToString('N') + '.json')
try {
    '[{"tool":"WinBlaze","run":1},{"tool":"WizTree","run":1},{"tool":"WizTree","run":2}]' | Set-Content -LiteralPath $path -Encoding UTF8
    $rows = @(Read-BenchmarkJson -Path $path | Where-Object tool -eq WizTree)
    if ($rows.Count -ne 2 -or $rows[0].run -ne 1 -or $rows[1].run -ne 2) { throw 'JSON reader must emit separate observations for pipeline filtering on both PowerShell 5.1 and 7.' }
    Write-Host 'PASS: JSON observation arrays are enumerated consistently.'
} finally { if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path } }
