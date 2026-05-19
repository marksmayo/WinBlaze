param(
    [string]$CertificateThumbprint = $env:WINBLAZE_SIGNING_THUMBPRINT
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

$signTool = Find-SignTool
$cert = $null
if (-not [string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
    $cert = Get-ChildItem Cert:\CurrentUser\My,Cert:\LocalMachine\My -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $CertificateThumbprint } |
        Select-Object -First 1
}

$result = [pscustomobject]@{
    signtool_found = -not [string]::IsNullOrWhiteSpace($signTool)
    signtool_path = $signTool
    thumbprint_configured = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
    certificate_found = $null -ne $cert
    certificate_subject = if ($cert) { $cert.Subject } else { $null }
    certificate_path_configured = -not [string]::IsNullOrWhiteSpace($env:WINBLAZE_SIGNING_CERT_PATH)
    timestamp_url_configured = -not [string]::IsNullOrWhiteSpace($env:WINBLAZE_TIMESTAMP_URL)
    signing_ready = (-not [string]::IsNullOrWhiteSpace($signTool)) -and
        (
            ((-not [string]::IsNullOrWhiteSpace($CertificateThumbprint)) -and ($null -ne $cert)) -or
            (-not [string]::IsNullOrWhiteSpace($env:WINBLAZE_SIGNING_CERT_PATH))
        )
}

$result | ConvertTo-Json

if (-not $result.signtool_found) {
    throw "signtool.exe was not found. Install the Windows SDK signing tools."
}
