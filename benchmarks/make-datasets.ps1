param(
    [string]$Root = "C:\tmp\WinBlazeBench",
    [ValidateSet("tiny", "small", "medium", "fanout", "fanout-large", "scale")]
    [string]$Size = "small",
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

$profiles = @{
    tiny = @{
        TopDirs = 3
        SubDirs = 3
        FilesPerDir = 8
        BytesPerFile = 1024
    }
    small = @{
        TopDirs = 8
        SubDirs = 8
        FilesPerDir = 24
        BytesPerFile = 4096
    }
    medium = @{
        TopDirs = 16
        SubDirs = 12
        FilesPerDir = 48
        BytesPerFile = 8192
    }
    fanout = @{
        TopDirs = 1
        SubDirs = 1
        FilesPerDir = 2048
        BytesPerFile = 512
    }
    "fanout-large" = @{
        TopDirs = 1
        SubDirs = 1
        FilesPerDir = 8192
        BytesPerFile = 256
    }
    scale = @{
        TopDirs = 32
        SubDirs = 16
        FilesPerDir = 32
        BytesPerFile = 0
    }
}

function New-DeterministicBytes {
    param(
        [int]$Length,
        [int]$Seed
    )

    $bytes = New-Object byte[] $Length
    for ($index = 0; $index -lt $Length; $index++) {
        $bytes[$index] = [byte](($Seed + ($index * 31)) % 251)
    }
    return ,$bytes
}

$profile = $profiles[$Size]
$datasetRoot = Join-Path $Root $Size

if ($Clean -and (Test-Path -LiteralPath $datasetRoot)) {
    Remove-Item -LiteralPath $datasetRoot -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $datasetRoot | Out-Null

$fileCount = 0
$directoryCount = 1
$totalBytes = 0
for ($top = 0; $top -lt $profile.TopDirs; $top++) {
    $topPath = Join-Path $datasetRoot ("dir-{0:D3}" -f $top)
    New-Item -ItemType Directory -Force -Path $topPath | Out-Null
    $directoryCount++

    for ($sub = 0; $sub -lt $profile.SubDirs; $sub++) {
        $subPath = Join-Path $topPath ("sub-{0:D3}" -f $sub)
        New-Item -ItemType Directory -Force -Path $subPath | Out-Null
        $directoryCount++

        for ($file = 0; $file -lt $profile.FilesPerDir; $file++) {
            $seed = ($top * 100000) + ($sub * 1000) + $file
            $path = Join-Path $subPath ("file-{0:D3}-{1:D3}-{2:D4}.bin" -f $top, $sub, $file)
            [System.IO.File]::WriteAllBytes($path, (New-DeterministicBytes -Length $profile.BytesPerFile -Seed $seed))
            $fileCount++
            $totalBytes += $profile.BytesPerFile
        }
    }
}

$manifest = [pscustomobject]@{
    name = $Size
    root = $datasetRoot
    directories = $directoryCount
    files = $fileCount
    bytes = $totalBytes
    bytes_per_file = $profile.BytesPerFile
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
}

New-Item -ItemType Directory -Force -Path $Root | Out-Null
$manifestPath = Join-Path $Root "$Size.manifest.json"
$manifest | ConvertTo-Json | Set-Content -Path $manifestPath -Encoding UTF8
$manifest | Format-List
