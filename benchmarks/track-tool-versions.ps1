# Dot-source this file from runners. Each measurement carries its own binary identity.
function Merge-BenchmarkCpuLowerBound {
    param([double]$PreviousMilliseconds, [double]$ObservedMilliseconds)
    return [math]::Max($PreviousMilliseconds, $ObservedMilliseconds)
}
function Read-BenchmarkJson {
    param([string]$Path)
    $options = @{}
    if ((Get-Command ConvertFrom-Json).Parameters.ContainsKey('DateKind')) {
        $options.DateKind = 'String'
    }
    $parsed = Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json @options
    foreach ($item in $parsed) { Write-Output $item }
}
function Get-BenchmarkIdentity {
    param([string]$Tool, [string]$Executable, [string]$HistoryPath = (Join-Path $PSScriptRoot 'tool-version-history.json'))
    $item = Get-Item -LiteralPath $Executable
    $identity = [pscustomobject]@{
        tool = $Tool
        product_version = $item.VersionInfo.ProductVersion
        file_version = $item.VersionInfo.FileVersion
        executable_path = $item.FullName
        sha256 = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash
        native_dll_sha256 = $null
        identity_sha256 = $null
        observed_utc = [DateTime]::UtcNow.ToString('o')
        revision = (git -C (Join-Path $PSScriptRoot '..') rev-parse HEAD)
    }
    $identity.identity_sha256 = $identity.sha256
    $native = Join-Path $item.DirectoryName 'winblaze_native.dll'
    if ($item.Name -ieq 'WinBlaze.UI.exe' -and (Test-Path -LiteralPath $native)) {
        $identity.native_dll_sha256 = (Get-FileHash -LiteralPath $native -Algorithm SHA256).Hash
        $hash = [Security.Cryptography.SHA256]::Create()
        try {
            $bytes = [Text.Encoding]::UTF8.GetBytes(('{0}:{1}' -f $identity.sha256,$identity.native_dll_sha256))
            $identity.identity_sha256 = ([BitConverter]::ToString($hash.ComputeHash($bytes))).Replace('-','')
        } finally { $hash.Dispose() }
    }
    $history = [Collections.Generic.List[object]]::new()
    if (Test-Path -LiteralPath $historyPath) {
        foreach ($entry in (Read-BenchmarkJson -Path $historyPath)) { $history.Add($entry) }
    }
    $previous = @($history | Where-Object { $_.tool -eq $Tool }) | Select-Object -Last 1
    $previousIdentity = if ($previous.identity_sha256) { $previous.identity_sha256 } else { $previous.sha256 }
    if (!$previous -or $previousIdentity -ne $identity.identity_sha256) {
        if ($previous) { Write-Warning "$Tool binary changed: $($previous.product_version) -> $($identity.product_version). Preserve separate baselines." }
        $history.Add($identity)
        Write-BenchmarkJson -Path $historyPath -Value $history.ToArray()
    }
    return $identity
}

# Standard and administrator refreshes share one measurement slot in this Windows session.
function Get-BenchmarkMemoryPeakValue {
    param([long]$ObservedPeak, [int]$Samples)
    if ($Samples -gt 0 -and $ObservedPeak -gt 0) { return $ObservedPeak }
    return $null
}

function Enter-BenchmarkMeasurementLock {
    $mutex=[Threading.Mutex]::new($false, 'Local\WinBlazeBenchmarkMeasurements')
    try { $mutex.WaitOne() | Out-Null }
    catch [Threading.AbandonedMutexException] { } # The previous runner exited; ownership is granted.
    catch { $mutex.Dispose(); throw }
    return $mutex
}
function Write-BenchmarkJson {
    param([string]$Path,[object]$Value)
    $temporary=$Path+'.'+[Guid]::NewGuid().ToString('N')+'.tmp'
    try {
        $json=ConvertTo-Json -InputObject $Value -Depth 30
        [IO.File]::WriteAllText($temporary,$json,[Text.UTF8Encoding]::new($false))
        # Antivirus/indexing readers can briefly deny replacement. Never truncate
        # the previous result, and propagate persistent permission/write failures.
        for ($attempt=0; ; $attempt++) {
            try {
                if(Test-Path -LiteralPath $Path){[IO.File]::Replace($temporary,$Path,[NullString]::Value)}else{[IO.File]::Move($temporary,$Path)}
                break
            } catch [IO.IOException] {
                if ($attempt -ge 9) {throw}
                Start-Sleep -Milliseconds 100
            }
        }
    } finally {if(Test-Path -LiteralPath $temporary){Remove-Item -LiteralPath $temporary -Force}}
}
