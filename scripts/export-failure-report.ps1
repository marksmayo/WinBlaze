param(
    [string]$OutputPath = (Join-Path $env:TEMP ("WinBlaze-failure-report-{0:yyyyMMdd-HHmmss}.zip" -f (Get-Date))),
    [int]$RecentMinutes = 30
)

$ErrorActionPreference = "Stop"

$workRoot = Join-Path $env:TEMP ("WinBlaze-failure-report-{0}" -f ([guid]::NewGuid().ToString("N")))
$reportRoot = Join-Path $workRoot "WinBlaze-failure-report"
New-Item -ItemType Directory -Force -Path $reportRoot | Out-Null

try {
    $startupLog = Join-Path $env:TEMP "WinBlaze-startup.log"
    if (Test-Path -LiteralPath $startupLog) {
        Copy-Item -LiteralPath $startupLog -Destination (Join-Path $reportRoot "WinBlaze-startup.log")
    }

    $logRoot = Join-Path $env:LOCALAPPDATA "WinBlaze\logs"
    if (Test-Path -LiteralPath $logRoot) {
        Copy-Item -LiteralPath $logRoot -Destination (Join-Path $reportRoot "logs") -Recurse -Force
    }

    $indexRoot = Join-Path $env:LOCALAPPDATA "WinBlaze\index"
    if (Test-Path -LiteralPath $indexRoot) {
        Get-ChildItem -LiteralPath $indexRoot -File -ErrorAction SilentlyContinue |
            Select-Object Name,Length,LastWriteTime |
            ConvertTo-Json |
            Set-Content -Path (Join-Path $reportRoot "index-files.json") -Encoding UTF8
    }

    Get-Process WinBlaze.UI -ErrorAction SilentlyContinue |
        Select-Object Id,ProcessName,MainWindowTitle,Responding,WorkingSet64,StartTime |
        ConvertTo-Json |
        Set-Content -Path (Join-Path $reportRoot "process.json") -Encoding UTF8

    Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = (Get-Date).AddMinutes(-1 * $RecentMinutes) } -ErrorAction SilentlyContinue |
        Where-Object { $_.ProviderName -eq "Application Error" -or $_.Message -like "*WinBlaze*" } |
        Select-Object TimeCreated,ProviderName,Id,Message |
        ConvertTo-Json -Depth 4 |
        Set-Content -Path (Join-Path $reportRoot "windows-application-events.json") -Encoding UTF8

    $summary = [pscustomobject]@{
        generated_utc = (Get-Date).ToUniversalTime().ToString("o")
        machine = $env:COMPUTERNAME
        user = $env:USERNAME
        recent_minutes = $RecentMinutes
        startup_log = $startupLog
        log_root = $logRoot
        index_root = $indexRoot
    }
    $summary | ConvertTo-Json |
        Set-Content -Path (Join-Path $reportRoot "summary.json") -Encoding UTF8

    if (Test-Path -LiteralPath $OutputPath) {
        Remove-Item -LiteralPath $OutputPath -Force
    }
    Compress-Archive -LiteralPath $reportRoot -DestinationPath $OutputPath -Force
    [pscustomobject]@{
        Result = "OK"
        OutputPath = $OutputPath
    } | Format-List
}
finally {
    if (Test-Path -LiteralPath $workRoot) {
        Remove-Item -LiteralPath $workRoot -Recurse -Force
    }
}
