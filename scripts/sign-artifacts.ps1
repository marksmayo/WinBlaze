param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",
    [string]$Platform = "x64",
    [string]$CertificatePath = $env:WINBLAZE_SIGNING_CERT_PATH,
    [string]$CertificatePassword = $env:WINBLAZE_SIGNING_CERT_PASSWORD,
    [string]$CertificateThumbprint = $env:WINBLAZE_SIGNING_THUMBPRINT,
    [string]$TimestampUrl = $(if ($env:WINBLAZE_TIMESTAMP_URL) { $env:WINBLAZE_TIMESTAMP_URL } else { "http://timestamp.digicert.com" }),
    [string[]]$Files,
    [switch]$IncludeInstaller,
    [switch]$VerifyOnly
)

$ErrorActionPreference = "Stop"

function Find-SignTool {
    $candidates = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
        "${env:ProgramFiles}\Windows Kits\10\bin"
    )

    foreach ($root in $candidates) {
        if (-not (Test-Path -LiteralPath $root)) {
            continue
        }
        $tool = Get-ChildItem -LiteralPath $root -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -like "*\x64\signtool.exe" } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($tool) {
            return $tool.FullName
        }
    }

    $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    return $null
}

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$buildDir = Join-Path $repoRoot "src\WinBlaze.UI\bin\$Platform\$Configuration"
if (-not $Files -or $Files.Count -eq 0) {
    $Files = @(
        (Join-Path $buildDir "WinBlaze.UI.exe"),
        (Join-Path $buildDir "winblaze_native.dll")
    )
    if ($IncludeInstaller) {
        $installerRoot = Join-Path $repoRoot "artifacts\installer"
        if (Test-Path -LiteralPath $installerRoot -PathType Container) {
            $Files += Get-ChildItem -LiteralPath $installerRoot -Filter "*.msi" -File |
                Sort-Object LastWriteTimeUtc -Descending |
                Select-Object -ExpandProperty FullName
        }
    }
}

$resolvedFiles = @()
foreach ($file in $Files) {
    $resolvedFiles += (Resolve-Path -LiteralPath $file).Path
}

$signTool = Find-SignTool
if ([string]::IsNullOrWhiteSpace($signTool)) {
    throw "signtool.exe was not found. Install the Windows SDK signing tools."
}

foreach ($file in $resolvedFiles) {
    if (-not $VerifyOnly) {
        $args = @("sign", "/fd", "SHA256", "/tr", $TimestampUrl, "/td", "SHA256")
        if (-not [string]::IsNullOrWhiteSpace($CertificatePath)) {
            $args += @("/f", (Resolve-Path -LiteralPath $CertificatePath).Path)
            if (-not [string]::IsNullOrWhiteSpace($CertificatePassword)) {
                $args += @("/p", $CertificatePassword)
            }
        } elseif (-not [string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
            $args += @("/sha1", $CertificateThumbprint)
        } else {
            throw "No signing certificate configured. Set WINBLAZE_SIGNING_CERT_PATH or WINBLAZE_SIGNING_THUMBPRINT."
        }
        $args += $file
        & $signTool @args
        if ($LASTEXITCODE -ne 0) {
            throw "signtool sign failed for $file with exit code $LASTEXITCODE."
        }
    }

    & $signTool verify /pa /v $file
    if ($LASTEXITCODE -ne 0) {
        throw "signtool verify failed for $file with exit code $LASTEXITCODE."
    }
}

[pscustomobject]@{
    configuration = $Configuration
    platform = $Platform
    signed = -not [bool]$VerifyOnly
    verified_files = $resolvedFiles
    signtool_path = $signTool
} | ConvertTo-Json
